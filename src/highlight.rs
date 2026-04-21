//! HyperList syntax coloring, matching the HyperList TUI 256-color scheme.
//!
//! Color roles (from HyperList color-scheme):
//!   Red (196)     Properties (Name: ), dates, multi-line +, change markup
//!   Green (46)    Qualifiers [...], checkboxes, state/transition, semicolons
//!   Blue (33)     Operators (ALL-CAPS ending in colon-space), e.g. AND:
//!   Magenta (165) References <...>, SKIP, END
//!   Cyan (51)     Parentheses (...), quoted strings "..."
//!   Yellow (226)  Substitutions {...}
//!   Orange (208)  Hash tags #tag

use crust::style;

const C_PROPERTY: u8 = 196;
const C_QUALIFIER: u8 = 46;
const C_OPERATOR: u8 = 33;
const C_REFERENCE: u8 = 165;
const C_COMMENT: u8 = 51;      // parentheses
const C_STRING: u8 = 51;       // quoted strings
const C_SUBST: u8 = 226;
const C_TAG: u8 = 208;

/// Colorize a single line of HyperList content (markup preserved).
pub fn colorize_line(line: &str) -> String {
    if line.is_empty() { return String::new(); }
    let mut out = String::with_capacity(line.len() * 2);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    // Check for the operator at line start: ALL-CAPS word ending with ": ".
    // Highlight the whole "OPERATOR: " prefix.
    if let Some(op_end) = detect_operator(&chars) {
        let prefix: String = chars[..op_end].iter().collect();
        out.push_str(&style::bold(&style::fg(&prefix, C_OPERATOR)));
        i = op_end;
    } else if let Some(prop_end) = detect_property_chain(&chars) {
        // Properties like "Name: value" or chained "09.03: Silje: Opening".
        // Color the key chain (everything up to and including the last
        // colon-space before the value).
        let prefix: String = chars[..prop_end].iter().collect();
        out.push_str(&style::fg(&prefix, C_PROPERTY));
        i = prop_end;
    }

    // Continuation marker handling: line that starts with "+ " after stripping
    // leading indent is already stripped by the parser, but a stray "+" at the
    // very start still gets highlighted red.
    if i == 0 && !chars.is_empty() && chars[0] == '+' {
        out.push_str(&style::fg("+", C_PROPERTY));
        i = 1;
    }

    while i < chars.len() {
        let c = chars[i];
        // Qualifiers [...]
        if c == '[' {
            if let Some(end) = find_matching(&chars, i, '[', ']') {
                let span: String = chars[i..=end].iter().collect();
                out.push_str(&style::fg(&span, C_QUALIFIER));
                i = end + 1;
                continue;
            }
        }
        // References <...> or <<...>>
        if c == '<' {
            if let Some(end) = find_matching(&chars, i, '<', '>') {
                let span: String = chars[i..=end].iter().collect();
                out.push_str(&style::fg(&span, C_REFERENCE));
                i = end + 1;
                continue;
            }
        }
        // Substitutions {...}
        if c == '{' {
            if let Some(end) = find_matching(&chars, i, '{', '}') {
                let span: String = chars[i..=end].iter().collect();
                out.push_str(&style::fg(&span, C_SUBST));
                i = end + 1;
                continue;
            }
        }
        // Parentheses (...)
        if c == '(' {
            if let Some(end) = find_matching(&chars, i, '(', ')') {
                let span: String = chars[i..=end].iter().collect();
                out.push_str(&style::fg(&span, C_COMMENT));
                i = end + 1;
                continue;
            }
        }
        // Quoted strings "..."
        if c == '"' {
            if let Some(end) = chars[i+1..].iter().position(|&x| x == '"') {
                let span: String = chars[i..=i + 1 + end].iter().collect();
                out.push_str(&style::fg(&span, C_STRING));
                i = i + 2 + end;
                continue;
            }
        }
        // Tags: #word
        if c == '#' && i + 1 < chars.len() && chars[i + 1].is_alphanumeric() {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '-') {
                j += 1;
            }
            let span: String = chars[i..j].iter().collect();
            out.push_str(&style::fg(&span, C_TAG));
            i = j;
            continue;
        }
        // Semicolons (state/transition separator)
        if c == ';' {
            out.push_str(&style::fg(";", C_QUALIFIER));
            i += 1;
            continue;
        }
        // Inline formatting: *bold*, /italic/, _underline_
        if c == '*' && i + 1 < chars.len() && chars[i + 1] != ' ' {
            if let Some(end) = chars[i+1..].iter().position(|&x| x == '*') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&style::bold(&inner));
                i = i + 2 + end;
                continue;
            }
        }
        if c == '_' && i + 1 < chars.len() && chars[i + 1] != ' ' {
            if let Some(end) = chars[i+1..].iter().position(|&x| x == '_') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&style::underline(&inner));
                i = i + 2 + end;
                continue;
            }
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] != ' ' && chars[i + 1] != '/' {
            if let Some(end) = chars[i+1..].iter().position(|&x| x == '/') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&style::italic(&inner));
                i = i + 2 + end;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Detect an operator at line start: ALL-CAPS word followed by ": " (colon + space).
/// Returns the character index just past the trailing space on match.
fn detect_operator(chars: &[char]) -> Option<usize> {
    let mut i = 0;
    let mut seen_upper = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_uppercase() || c == '-' || c == '_' || c.is_ascii_digit() {
            if c.is_ascii_uppercase() { seen_upper = true; }
            i += 1;
        } else { break; }
    }
    if !seen_upper || i == 0 { return None; }
    // Require ": " (colon-space) immediately after the word.
    if i + 1 < chars.len() && chars[i] == ':' && chars[i + 1] == ' ' {
        Some(i + 2)
    } else { None }
}

/// Detect a property prefix at the start: one or more "key: " chains before
/// the line content begins. Matches patterns like `Name: value` or
/// `09.03: Silje: Opening` (colors the whole "09.03: Silje: " prefix).
fn detect_property_chain(chars: &[char]) -> Option<usize> {
    let mut i = 0;
    let mut last_match = 0;
    loop {
        let segment_start = i;
        while i < chars.len() {
            let c = chars[i];
            if c == '\n' || c == '[' || c == '<' || c == '(' || c == '{' || c == '"' { return None; }
            if c == ':' { break; }
            i += 1;
        }
        if i >= chars.len() || chars[i] != ':' { break; }
        // need ": " (colon followed by space or end)
        if i + 1 < chars.len() && chars[i + 1] != ' ' { return None; }
        if i == segment_start { return None; }
        // This segment qualifies; advance past the space.
        i += if i + 1 < chars.len() { 2 } else { 1 };
        last_match = i;
        // Peek: if next chars look like another key (no space at pos 0,
        // letters/digits, eventually another ": "), continue the chain.
        let mut look = i;
        let mut found_another = false;
        while look < chars.len() {
            let c = chars[look];
            if c == ':' && look + 1 < chars.len() && chars[look + 1] == ' ' {
                found_another = true; break;
            }
            if c == ' ' || c == '\n' || c == '[' || c == '<' || c == '(' || c == '{' { break; }
            look += 1;
        }
        if !found_another { break; }
    }
    if last_match > 0 { Some(last_match) } else { None }
}

fn find_matching(chars: &[char], start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for i in start..chars.len() {
        if chars[i] == open { depth += 1; }
        else if chars[i] == close {
            depth -= 1;
            if depth == 0 { return Some(i); }
        }
    }
    None
}
