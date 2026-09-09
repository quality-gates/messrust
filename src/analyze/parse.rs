use proc_macro2::{TokenStream, TokenTree};

// This is an internal safety budget, not a Rust language rule. It keeps syn's
// recursive parser away from the process stack guard page.
const MAX_PARSE_NESTING: usize = 1024;
const DEEP_PARSE_ERROR: &str = "source nesting exceeds parser safety limit";

#[derive(Clone, Copy)]
enum ClosureState {
    None,
    Parameters(usize),
    Body(usize),
}

pub(crate) fn parse_file(source: &str) -> Result<syn::File, String> {
    let tokens = source
        .parse::<TokenStream>()
        .map_err(|error| error.to_string())?;
    if exceeds_parse_nesting(&tokens) {
        return Err(DEEP_PARSE_ERROR.to_string());
    }
    syn::parse2(tokens).map_err(|error| error.to_string())
}

fn exceeds_parse_nesting(tokens: &TokenStream) -> bool {
    // Walk token groups with an explicit stack. A recursive walk could repeat
    // the same stack-overflow failure that this check prevents.
    let mut streams = vec![tokens.clone().into_iter()];
    let mut group_depth: usize = 0;
    let mut closure_state = ClosureState::None;

    while !streams.is_empty() {
        let token = streams.last_mut().and_then(Iterator::next);
        if let Some(token) = token {
            if process_token(token, &mut streams, &mut group_depth, &mut closure_state) {
                return true;
            }
        } else {
            close_stream(&mut streams, &mut group_depth, &mut closure_state);
        }
    }

    false
}

fn process_token(
    token: TokenTree,
    streams: &mut Vec<proc_macro2::token_stream::IntoIter>,
    group_depth: &mut usize,
    closure_state: &mut ClosureState,
) -> bool {
    match token {
        TokenTree::Group(group) => process_group(group, streams, group_depth, closure_state),
        TokenTree::Punct(punct) if punct.as_char() == '|' => process_closure_pipe(closure_state),
        TokenTree::Ident(ident) => process_identifier(ident, closure_state),
        TokenTree::Punct(_) => process_punctuation(closure_state),
        _ => reset_closure(closure_state),
    }
}

fn process_group(
    group: proc_macro2::Group,
    streams: &mut Vec<proc_macro2::token_stream::IntoIter>,
    group_depth: &mut usize,
    closure_state: &mut ClosureState,
) -> bool {
    *group_depth += 1;
    if *group_depth > MAX_PARSE_NESTING {
        return true;
    }
    streams.push(group.stream().into_iter());
    reset_closure_body(closure_state);
    false
}

fn process_closure_pipe(closure_state: &mut ClosureState) -> bool {
    match closure_state {
        ClosureState::None => *closure_state = ClosureState::Parameters(1),
        ClosureState::Parameters(depth) => *closure_state = ClosureState::Body(*depth),
        ClosureState::Body(depth) => {
            let next_depth = depth.saturating_add(1);
            if next_depth > MAX_PARSE_NESTING {
                return true;
            }
            *closure_state = ClosureState::Parameters(next_depth);
        }
    }
    false
}

fn process_identifier(ident: proc_macro2::Ident, closure_state: &mut ClosureState) -> bool {
    if matches!(closure_state, ClosureState::Parameters(_)) {
        return false;
    }
    if matches!(closure_state, ClosureState::Body(_))
        && matches!(ident.to_string().as_str(), "async" | "move")
    {
        return false;
    }
    reset_closure(closure_state)
}

fn process_punctuation(closure_state: &mut ClosureState) -> bool {
    if matches!(closure_state, ClosureState::Parameters(_)) {
        return false;
    }
    reset_closure(closure_state)
}

fn reset_closure_body(closure_state: &mut ClosureState) {
    if matches!(closure_state, ClosureState::Body(_)) {
        *closure_state = ClosureState::None;
    }
}

fn reset_closure(closure_state: &mut ClosureState) -> bool {
    reset_closure_body(closure_state);
    false
}

fn close_stream(
    streams: &mut Vec<proc_macro2::token_stream::IntoIter>,
    group_depth: &mut usize,
    closure_state: &mut ClosureState,
) {
    streams.pop();
    *group_depth = group_depth.saturating_sub(1);
    reset_closure_body(closure_state);
}

#[cfg(test)]
mod tests {
    use super::{parse_file, DEEP_PARSE_ERROR};

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
            Some(DEEP_PARSE_ERROR)
        );
    }

    #[test]
    fn rejects_deep_closure_chain_before_syn_parses() {
        let mut source = String::from("fn main() { let value = ");
        source.extend(std::iter::repeat_n("|value| ", 1952));
        source.push_str("value; }\n");

        assert_eq!(parse_file(&source).err().as_deref(), Some(DEEP_PARSE_ERROR));
    }

    #[test]
    fn rejects_deep_struct_literals_before_syn_parses() {
        assert_eq!(
            parse_file(&nested_struct_literals(914)).err().as_deref(),
            Some(DEEP_PARSE_ERROR)
        );
    }

    #[test]
    fn rejects_deep_if_blocks_before_syn_parses() {
        assert_eq!(
            parse_file(&nested_if_blocks(2000)).err().as_deref(),
            Some(DEEP_PARSE_ERROR)
        );
    }

    #[test]
    fn parses_shallow_nesting() {
        assert!(parse_file(&nested_parentheses(32)).is_ok());
    }
}
