use proc_macro2::{Spacing, TokenStream, TokenTree};

// Safety budgets for recursive AST structures in syn and FileModel.
// These limits prevent stack overflow in both debug and release builds.
const MAX_GROUP_DEPTH: usize = 256;
const MAX_GENERIC_DEPTH: usize = 64;
const MAX_OPERATOR_CHAIN: usize = 500;
const DEEP_PARSE_ERROR: &str = "source nesting exceeds parser safety limit";

fn deep_parse_error(limit: usize) -> String {
    format!("{DEEP_PARSE_ERROR} of {limit}")
}

#[derive(Clone, Copy)]
enum ClosureState {
    None,
    Parameters(usize),
    Body(usize),
}

#[derive(Default)]
struct FrameState {
    generic_depth: usize,
    operator_chain: usize,
    prev_punct: Option<(char, Spacing)>,
    expects_operand: bool,
}

pub(crate) fn parse_file(source: &str) -> Result<syn::File, String> {
    let tokens = source
        .parse::<TokenStream>()
        .map_err(|error| error.to_string())?;
    check_parse_nesting(&tokens)?;
    syn::parse2(tokens).map_err(|error| error.to_string())
}

fn check_parse_nesting(tokens: &TokenStream) -> Result<(), String> {
    // Walk token groups with an explicit stack. A recursive walk could repeat
    // the same stack-overflow failure that this check prevents.
    let mut streams = vec![tokens.clone().into_iter()];
    let mut frames = vec![FrameState::default()];
    let mut group_depth: usize = 0;
    let mut closure_state = ClosureState::None;

    while !streams.is_empty() {
        let token = streams.last_mut().and_then(Iterator::next);
        if let Some(token) = token {
            process_token(
                token,
                &mut streams,
                &mut frames,
                &mut group_depth,
                &mut closure_state,
            )?;
        } else {
            close_stream(&mut streams, &mut frames, &mut group_depth, &mut closure_state);
        }
    }

    Ok(())
}

fn process_token(
    token: TokenTree,
    streams: &mut Vec<proc_macro2::token_stream::IntoIter>,
    frames: &mut Vec<FrameState>,
    group_depth: &mut usize,
    closure_state: &mut ClosureState,
) -> Result<(), String> {
    match token {
        TokenTree::Group(group) => {
            *group_depth += 1;
            if *group_depth > MAX_GROUP_DEPTH {
                return Err(deep_parse_error(MAX_GROUP_DEPTH));
            }
            if let Some(frame) = frames.last_mut() {
                frame.prev_punct = None;
                frame.expects_operand = false;
            }
            streams.push(group.stream().into_iter());
            frames.push(FrameState {
                expects_operand: true,
                ..FrameState::default()
            });
            reset_closure_body(closure_state);
        }
        TokenTree::Ident(ident) => {
            let frame = frames.last_mut().unwrap();
            frame.prev_punct = None;
            let text = ident.to_string();
            if matches!(
                text.as_str(),
                "let"
                    | "fn"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "mod"
                    | "use"
                    | "const"
                    | "static"
                    | "return"
                    | "match"
                    | "while"
                    | "for"
                    | "loop"
            ) {
                frame.operator_chain = 0;
                frame.generic_depth = 0;
                frame.expects_operand = true;
            } else {
                frame.expects_operand = matches!(text.as_str(), "async" | "move");
            }
            process_identifier(&text, closure_state);
        }
        TokenTree::Literal(_) => {
            let frame = frames.last_mut().unwrap();
            frame.prev_punct = None;
            frame.expects_operand = false;
            reset_closure(closure_state);
        }
        TokenTree::Punct(punct) => {
            process_punct(punct, frames.last_mut().unwrap(), closure_state)?;
        }
    }
    Ok(())
}

fn record_operator(frame: &mut FrameState) -> Result<(), String> {
    frame.operator_chain += 1;
    if frame.operator_chain > MAX_OPERATOR_CHAIN {
        Err(deep_parse_error(MAX_OPERATOR_CHAIN))
    } else {
        Ok(())
    }
}

fn handle_separator(ch: char, frame: &mut FrameState, closure_state: &mut ClosureState) -> bool {
    if ch == ';' {
        frame.operator_chain = 0;
        frame.generic_depth = 0;
        frame.expects_operand = true;
        reset_closure(closure_state);
        true
    } else if ch == ',' {
        frame.operator_chain = 0;
        frame.expects_operand = true;
        if frame.generic_depth == 0 {
            reset_closure(closure_state);
        }
        true
    } else {
        false
    }
}

fn handle_pipe(
    prev: Option<(char, Spacing)>,
    frame: &mut FrameState,
    closure_state: &mut ClosureState,
) -> Result<bool, String> {
    if prev == Some(('|', Spacing::Joint)) {
        *closure_state = ClosureState::None;
        frame.generic_depth = 0;
        frame.expects_operand = true;
        record_operator(frame)?;
        return Ok(true);
    }
    if frame.expects_operand || !matches!(closure_state, ClosureState::None) {
        if process_closure_pipe(closure_state)? {
            return Err(deep_parse_error(MAX_GROUP_DEPTH));
        }
        return Ok(true);
    }
    *closure_state = ClosureState::None;
    frame.expects_operand = true;
    record_operator(frame)?;
    Ok(true)
}

fn handle_compound_punct(
    prev: Option<(char, Spacing)>,
    ch: char,
    frame: &mut FrameState,
) -> Result<bool, String> {
    let Some((prev_ch, Spacing::Joint)) = prev else {
        return Ok(false);
    };

    if handle_compound_syntax(prev_ch, ch, frame)? {
        return Ok(true);
    }
    handle_compound_assign(prev_ch, ch, frame)
}

fn handle_compound_syntax(
    prev_ch: char,
    ch: char,
    frame: &mut FrameState,
) -> Result<bool, String> {
    match (prev_ch, ch) {
        (':', ':') | ('.', '.') => Ok(true),
        ('-', '>') => {
            frame.operator_chain = frame.operator_chain.saturating_sub(1);
            frame.expects_operand = true;
            Ok(true)
        }
        ('=', '>') => {
            frame.operator_chain = 0;
            frame.expects_operand = true;
            Ok(true)
        }
        ('&', '&') | ('=', '=') | ('!', '=') => {
            frame.generic_depth = 0;
            frame.expects_operand = true;
            Ok(true)
        }
        ('<', '=') | ('>', '=') => {
            frame.generic_depth = frame.generic_depth.saturating_sub(1);
            frame.expects_operand = true;
            Ok(true)
        }
        ('<', '<') => {
            frame.generic_depth += 1;
            if frame.generic_depth > MAX_GENERIC_DEPTH {
                return Err(deep_parse_error(MAX_GENERIC_DEPTH));
            }
            frame.expects_operand = true;
            Ok(true)
        }
        ('>', '>') => {
            frame.generic_depth = frame.generic_depth.saturating_sub(1);
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn handle_compound_assign(
    prev_ch: char,
    ch: char,
    frame: &mut FrameState,
) -> Result<bool, String> {
    if ch == '=' && matches!(prev_ch, '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|') {
        frame.operator_chain = 0;
        frame.expects_operand = true;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn handle_angle_bracket(ch: char, frame: &mut FrameState) -> Result<bool, String> {
    if ch == '<' {
        frame.generic_depth += 1;
        if frame.generic_depth > MAX_GENERIC_DEPTH {
            return Err(deep_parse_error(MAX_GENERIC_DEPTH));
        }
        record_operator(frame)?;
        frame.expects_operand = true;
        Ok(true)
    } else if ch == '>' {
        if frame.generic_depth > 0 {
            frame.generic_depth -= 1;
            frame.expects_operand = false;
        } else {
            record_operator(frame)?;
            frame.expects_operand = true;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

fn handle_standard_punct(
    ch: char,
    spacing: Spacing,
    frame: &mut FrameState,
) -> Result<(), String> {
    if ch == '=' && spacing == Spacing::Alone {
        frame.operator_chain = 0;
        frame.generic_depth = 0;
        frame.expects_operand = true;
    } else if ch == ':' {
        frame.expects_operand = true;
    } else if matches!(ch, '+' | '-' | '*' | '/' | '%' | '^' | '&' | '!' | '.') {
        record_operator(frame)?;
        frame.expects_operand = true;
    }
    Ok(())
}

fn process_punct(
    punct: proc_macro2::Punct,
    frame: &mut FrameState,
    closure_state: &mut ClosureState,
) -> Result<(), String> {
    let ch = punct.as_char();
    let spacing = punct.spacing();
    let prev = frame.prev_punct.take();
    frame.prev_punct = Some((ch, spacing));

    if handle_separator(ch, frame, closure_state) {
        return Ok(());
    }
    if ch == '|' && handle_pipe(prev, frame, closure_state)? {
        return Ok(());
    }

    process_punctuation(closure_state);

    if handle_compound_punct(prev, ch, frame)? {
        return Ok(());
    }
    if handle_angle_bracket(ch, frame)? {
        return Ok(());
    }
    handle_standard_punct(ch, spacing, frame)
}

fn process_closure_pipe(closure_state: &mut ClosureState) -> Result<bool, String> {
    match closure_state {
        ClosureState::None => *closure_state = ClosureState::Parameters(1),
        ClosureState::Parameters(depth) => *closure_state = ClosureState::Body(*depth),
        ClosureState::Body(depth) => {
            let next_depth = depth.saturating_add(1);
            if next_depth > MAX_GROUP_DEPTH {
                return Ok(true);
            }
            *closure_state = ClosureState::Parameters(next_depth);
        }
    }
    Ok(false)
}

fn process_identifier(ident: &str, closure_state: &mut ClosureState) {
    if matches!(closure_state, ClosureState::Parameters(_)) {
        return;
    }
    if matches!(closure_state, ClosureState::Body(_)) && matches!(ident, "async" | "move") {
        return;
    }
    reset_closure(closure_state);
}

fn process_punctuation(closure_state: &mut ClosureState) {
    if matches!(closure_state, ClosureState::Parameters(_)) {
        return;
    }
    reset_closure(closure_state);
}

fn reset_closure_body(closure_state: &mut ClosureState) {
    if matches!(closure_state, ClosureState::Body(_)) {
        *closure_state = ClosureState::None;
    }
}

fn reset_closure(closure_state: &mut ClosureState) {
    reset_closure_body(closure_state);
}

fn close_stream(
    streams: &mut Vec<proc_macro2::token_stream::IntoIter>,
    frames: &mut Vec<FrameState>,
    group_depth: &mut usize,
    closure_state: &mut ClosureState,
) {
    streams.pop();
    frames.pop();
    *group_depth = group_depth.saturating_sub(1);
    if let Some(frame) = frames.last_mut() {
        frame.prev_punct = None;
    }
    reset_closure_body(closure_state);
}

#[cfg(test)]
mod tests {
    use super::parse_file;

    fn nested_parentheses(depth: usize) -> String {
        let mut source = String::from("fn main() { ");
        source.extend(std::iter::repeat_n('(', depth));
        source.push('1');
        source.extend(std::iter::repeat_n(')', depth));
        source.push_str("; }\n");
        source
    }

    fn nested_struct_literals(depth: usize) -> String {
        let mut source =
            String::from("struct S { field: Option<Box<S>> }\nfn main() { let value = ");
        source.extend(std::iter::repeat_n("S { field: Some(", depth));
        source.push_str("None");
        source.extend(std::iter::repeat_n(") }", depth));
        source.push_str("; }\n");
        source
    }

    fn nested_if_blocks(depth: usize) -> String {
        let mut source = String::from("fn main() { ");
        source.extend(std::iter::repeat_n("if true { ", depth));
        source.push_str("1;");
        source.extend(std::iter::repeat_n(" }", depth));
        source.push_str(" }\n");
        source
    }

    #[test]
    fn rejects_deep_parentheses_before_syn_parses() {
        assert_eq!(
            parse_file(&nested_parentheses(2683)).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 256")
        );
    }

    #[test]
    fn rejects_deep_closure_chain_before_syn_parses() {
        let mut source = String::from("fn main() { let value = ");
        source.extend(std::iter::repeat_n("|value| ", 1952));
        source.push_str("value; }\n");

        assert_eq!(
            parse_file(&source).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 256")
        );
    }

    #[test]
    fn rejects_deep_struct_literals_before_syn_parses() {
        assert_eq!(
            parse_file(&nested_struct_literals(914)).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 256")
        );
    }

    #[test]
    fn rejects_deep_if_blocks_before_syn_parses() {
        assert_eq!(
            parse_file(&nested_if_blocks(2000)).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 256")
        );
    }

    #[test]
    fn rejects_deep_generics_before_syn_parses() {
        let mut source = String::from("fn f() -> ");
        source.extend(std::iter::repeat_n("Option<", 200));
        source.push_str("i32");
        source.extend(std::iter::repeat_n('>', 200));
        source.push_str(" { None }\n");

        assert_eq!(
            parse_file(&source).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 64")
        );
    }

    #[test]
    fn rejects_deep_binary_operator_chain_before_syn_parses() {
        let mut source = String::from("fn f() -> bool { ");
        for i in 0..1000 {
            if i > 0 {
                source.push_str(" && ");
            }
            source.push_str("true");
        }
        source.push_str(" }\n");

        assert_eq!(
            parse_file(&source).err().as_deref(),
            Some("source nesting exceeds parser safety limit of 500")
        );
    }

    #[test]
    fn parses_shallow_nesting() {
        assert!(parse_file(&nested_parentheses(32)).is_ok());
    }
}
