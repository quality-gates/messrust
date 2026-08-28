//! Source comment directives for suppressing individual findings.

use std::collections::HashMap;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static DIRECTIVE_VISITS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectiveKind {
    DisableNextLine,
    Disable,
    Enable,
}

#[derive(Debug)]
struct Directive {
    line: usize,
    kind: DirectiveKind,
    rules: Vec<String>,
}

#[derive(Debug, Default)]
struct RuleSuppressions {
    intervals: Vec<(usize, usize)>,
    lines: Vec<usize>,
}

/// Suppression state indexed by the lower-case rule name of a finding.
#[derive(Debug, Default)]
pub struct Suppressions {
    by_rule: HashMap<String, RuleSuppressions>,
}

impl Suppressions {
    pub fn from_source(source: &str) -> Self {
        let directives = scan_directives(source);
        let mut active = HashMap::new();
        let mut by_rule: HashMap<String, RuleSuppressions> = HashMap::new();
        let mut index = 0;

        while index < directives.len() {
            let line = directives[index].line;
            let end = directives[index..]
                .iter()
                .position(|directive| directive.line != line)
                .map(|offset| index + offset)
                .unwrap_or(directives.len());
            let line_directives = &directives[index..end];
            apply_enables(line, line_directives, &mut active, &mut by_rule);
            apply_disables(line, line_directives, &mut active, &mut by_rule);
            index = end;
        }

        for (rule, start) in active {
            by_rule
                .entry(rule)
                .or_default()
                .intervals
                .push((start, usize::MAX));
        }
        for suppression in by_rule.values_mut() {
            suppression.lines.sort_unstable();
            suppression.lines.dedup();
        }

        Self { by_rule }
    }

    pub fn contains(&self, line: usize, rule: &str) -> bool {
        let key = rule.to_ascii_lowercase();
        let Some(suppression) = self.by_rule.get(&key) else {
            return false;
        };
        if suppression.lines.binary_search(&line).is_ok() {
            return true;
        }
        let index = suppression
            .intervals
            .partition_point(|(start, _)| *start <= line);
        index > 0 && suppression.intervals[index - 1].1 >= line
    }
}

fn apply_enables(
    line: usize,
    directives: &[Directive],
    active: &mut HashMap<String, usize>,
    by_rule: &mut HashMap<String, RuleSuppressions>,
) {
    for directive in directives {
        #[cfg(test)]
        DIRECTIVE_VISITS.with(|visits| visits.set(visits.get() + 1));
        if directive.kind != DirectiveKind::Enable {
            continue;
        }
        for rule in &directive.rules {
            let Some(start) = active.remove(rule) else {
                continue;
            };
            let interval_end = line.saturating_sub(1);
            if start <= interval_end {
                by_rule
                    .entry(rule.clone())
                    .or_default()
                    .intervals
                    .push((start, interval_end));
            }
        }
    }
}

fn apply_disables(
    line: usize,
    directives: &[Directive],
    active: &mut HashMap<String, usize>,
    by_rule: &mut HashMap<String, RuleSuppressions>,
) {
    for directive in directives {
        #[cfg(test)]
        DIRECTIVE_VISITS.with(|visits| visits.set(visits.get() + 1));
        match directive.kind {
            DirectiveKind::Disable => {
                for rule in &directive.rules {
                    active.entry(rule.clone()).or_insert(line + 1);
                }
            }
            DirectiveKind::DisableNextLine => {
                for rule in &directive.rules {
                    by_rule
                        .entry(rule.clone())
                        .or_default()
                        .lines
                        .push(line + 1);
                }
            }
            DirectiveKind::Enable => {}
        }
    }
}

fn scan_directives(source: &str) -> Vec<Directive> {
    DirectiveScanner::new(source).scan()
}

struct DirectiveScanner<'a> {
    source: &'a str,
    index: usize,
    line: usize,
    directives: Vec<Directive>,
}

enum SourceToken {
    LineComment,
    BlockComment,
    Newline,
    String,
    Character,
    PossibleRawString,
    Other,
}

impl<'a> DirectiveScanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            index: 0,
            line: 1,
            directives: Vec::new(),
        }
    }

    fn scan(mut self) -> Vec<Directive> {
        while self.index < self.source.len() {
            self.scan_next();
        }
        self.directives
    }

    fn scan_next(&mut self) {
        match source_token(self.source.as_bytes(), self.index) {
            SourceToken::LineComment => self.scan_line_comment(),
            SourceToken::BlockComment => self.scan_block_comment(),
            SourceToken::Newline => self.scan_newline(),
            SourceToken::String => self.skip_quoted_string(b'"'),
            SourceToken::Character => self.scan_character(),
            SourceToken::PossibleRawString => self.scan_possible_raw_string(),
            SourceToken::Other => self.index += 1,
        }
    }

    fn scan_line_comment(&mut self) {
        let start = self.index + 2;
        let mut end = start;
        while end < self.source.len() && self.source.as_bytes()[end] != b'\n' {
            end += 1;
        }
        add_directive(&mut self.directives, self.line, &self.source[start..end]);
        self.index = end;
    }

    fn scan_block_comment(&mut self) {
        let (end, comment) = scan_block_comment(self.source, self.index + 2);
        for (offset, text) in comment.lines().enumerate() {
            add_directive(&mut self.directives, self.line + offset, text);
        }
        self.line += comment.bytes().filter(|byte| *byte == b'\n').count();
        self.index = end;
    }

    fn scan_newline(&mut self) {
        self.line += 1;
        self.index += 1;
    }

    fn scan_character(&mut self) {
        let bytes = self.source.as_bytes();
        let end = if looks_like_char_literal(bytes, self.index) {
            skip_quoted(bytes, self.index + 1, b'\'')
        } else {
            self.index + 1
        };
        self.line += newline_count(&bytes[self.index..end]);
        self.index = end;
    }

    fn scan_possible_raw_string(&mut self) {
        let bytes = self.source.as_bytes();
        if let Some(end) = skip_raw_or_byte_string(bytes, self.index) {
            self.line += newline_count(&bytes[self.index..end]);
            self.index = end;
        } else {
            self.index += 1;
        }
    }

    fn skip_quoted_string(&mut self, quote: u8) {
        let bytes = self.source.as_bytes();
        let end = skip_quoted(bytes, self.index + 1, quote);
        self.line += newline_count(&bytes[self.index..end]);
        self.index = end;
    }
}

fn source_token(bytes: &[u8], index: usize) -> SourceToken {
    match bytes[index] {
        b'/' => slash_token(bytes, index),
        b'\n' => SourceToken::Newline,
        b'"' => SourceToken::String,
        b'\'' => SourceToken::Character,
        b'r' | b'b' => SourceToken::PossibleRawString,
        _ => SourceToken::Other,
    }
}

fn slash_token(bytes: &[u8], index: usize) -> SourceToken {
    match bytes.get(index + 1) {
        Some(b'/') => SourceToken::LineComment,
        Some(b'*') => SourceToken::BlockComment,
        _ => SourceToken::Other,
    }
}

fn newline_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn scan_block_comment(source: &str, start: usize) -> (usize, String) {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut depth = 1;
    let mut end = bytes.len();
    while index + 1 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'*' {
            depth += 1;
            index += 2;
        } else if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            depth -= 1;
            if depth == 0 {
                end = index;
                index += 2;
                break;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    (index, source[start..end].to_string())
}

fn add_directive(directives: &mut Vec<Directive>, line: usize, comment: &str) {
    let text = comment.trim();
    let lower = text.to_ascii_lowercase();
    let (kind, rest) = if let Some(rest) = command_rest(&lower, "messrust-disable-next-line") {
        (DirectiveKind::DisableNextLine, rest)
    } else if let Some(rest) = command_rest(&lower, "messrust-disable") {
        (DirectiveKind::Disable, rest)
    } else if let Some(rest) = command_rest(&lower, "messrust-enable") {
        (DirectiveKind::Enable, rest)
    } else {
        return;
    };

    let rules: Vec<String> = rest
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .map(str::trim)
        .filter(|rule| valid_rule_name(rule))
        .map(str::to_string)
        .collect();
    if !rules.is_empty() {
        directives.push(Directive { line, kind, rules });
    }
}

fn command_rest<'a>(text: &'a str, command: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(command)?;
    if rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_whitespace() || character == ',')
    {
        Some(rest)
    } else {
        None
    }
}

fn valid_rule_name(rule: &str) -> bool {
    let mut chars = rule.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

fn skip_quoted(bytes: &[u8], mut index: usize, quote: u8) -> usize {
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = index.saturating_add(2),
            c if c == quote => return index + 1,
            _ => index += 1,
        }
    }
    index
}

fn looks_like_char_literal(bytes: &[u8], index: usize) -> bool {
    let Some(mut end) = bytes.get(index + 1).copied() else {
        return false;
    };
    if end == b'\\' {
        end = bytes.get(index + 2).copied().unwrap_or_default();
        if end == 0 {
            return false;
        }
    } else if end == b'\n' || end == b'\r' {
        return false;
    }
    let mut cursor = index + 2;
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        if bytes[cursor] == b'\\' {
            cursor = cursor.saturating_add(2);
        } else if bytes[cursor] == b'\'' {
            return true;
        } else {
            cursor += 1;
        }
    }
    false
}

fn skip_raw_or_byte_string(bytes: &[u8], index: usize) -> Option<usize> {
    let quote = string_prefix_end(bytes, index)?;
    if bytes.get(quote) == Some(&b'"') {
        return Some(skip_quoted(bytes, quote + 1, b'"'));
    }
    skip_raw_string(bytes, quote + 1)
}

fn string_prefix_end(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes[index] {
        b'r' => Some(index),
        b'b' if matches!(bytes.get(index + 1), Some(b'"') | Some(b'r')) => Some(index + 1),
        _ => None,
    }
}

fn skip_raw_string(bytes: &[u8], quote: usize) -> Option<usize> {
    let mut hashes = 0;
    while bytes.get(quote + hashes) == Some(&b'#') {
        hashes += 1;
    }
    let quote = quote + hashes;
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' && has_hashes(bytes, cursor + 1, hashes) {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn has_hashes(bytes: &[u8], start: usize, count: usize) -> bool {
    (0..count).all(|offset| bytes.get(start + offset) == Some(&b'#'))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{Suppressions, DIRECTIVE_VISITS};

    #[test]
    fn supports_next_line_regions_and_case_insensitive_names() {
        let source = "// messrust-disable-next-line longvariable\nfn first() {}\n// messrust-disable ShortMethodName\nfn second() {}\n// messrust-enable shortmethodname\nfn third() {}\n";
        let suppressions = Suppressions::from_source(source);
        assert!(suppressions.contains(2, "LongVariable"));
        assert!(suppressions.contains(4, "shortmethodname"));
        assert!(!suppressions.contains(6, "ShortMethodName"));
    }

    #[test]
    fn ignores_directive_text_in_strings_and_accepts_block_comments() {
        let source = "let text = \"// messrust-disable LongVariable\";\n/* messrust-disable-next-line LongVariable */\nlet value = 1;\n";
        let suppressions = Suppressions::from_source(source);
        assert!(!suppressions.contains(1, "LongVariable"));
        assert!(suppressions.contains(3, "LongVariable"));
    }

    #[test]
    fn keeps_line_numbers_after_multiline_strings_and_rejects_bad_commands() {
        let source = "let text = r#\"first\n// messrust-disable LongVariable\nlast\"#;\n// messrust-disable-next-line LongVariable\nlet value = 1;\n// messrust-disable-next-linefoo\nlet other = 2;\n";
        let suppressions = Suppressions::from_source(source);
        assert!(suppressions.contains(5, "LongVariable"));
        assert!(!suppressions.contains(7, "foo"));
    }

    #[test]
    fn one_active_rule_uses_one_interval_for_many_lines() {
        let mut source = String::from("// messrust-disable LongVariable\n");
        source.extend(std::iter::repeat_n("let value = 1;\n", 10_000));

        let suppressions = Suppressions::from_source(&source);
        let rule = &suppressions.by_rule["longvariable"];

        assert_eq!(rule.intervals, vec![(2, usize::MAX)]);
        assert!(rule.lines.is_empty());
    }

    #[test]
    fn many_directives_create_one_interval_for_each_active_region() {
        DIRECTIVE_VISITS.with(|visits| visits.set(0));
        let mut source = String::new();
        for _ in 0..5_000 {
            source.push_str("// messrust-disable LongVariable\n");
            source.push_str("let suppressed = 1;\n");
            source.push_str("// messrust-enable LongVariable\n");
            source.push_str("let active = 1;\n");
        }

        let suppressions = Suppressions::from_source(&source);
        let rule = &suppressions.by_rule["longvariable"];

        assert_eq!(rule.intervals.len(), 5_000);
        assert_eq!(rule.intervals[0], (2, 2));
        assert_eq!(rule.intervals[4_999], (19_998, 19_998));
        assert!(rule.lines.is_empty());
        assert_eq!(DIRECTIVE_VISITS.with(Cell::get), 20_000);
    }
}
