//! HyperList parser (HyperList 2.6 format).
//!
//! Indentation with tabs or asterisks, one item per physical line (multi-line
//! continuations start with +). A parsed item keeps its indent level, the
//! raw text (with formatting preserved), a folded flag, and children.

#[derive(Clone, Debug, Default)]
pub struct Item {
    /// Zero-based indent depth.
    pub depth: usize,
    /// Original line text with indentation stripped (markup preserved).
    pub text: String,
    /// True when this item's subtree should render collapsed.
    pub folded: bool,
    /// True for continuation lines (starting with `+`); these don't add depth.
    pub continuation: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub items: Vec<Item>,
    /// Optional config from the first line if wrapped in `((...))`.
    pub config: Option<String>,
}

pub fn parse(source: &str) -> Document {
    let mut doc = Document::default();
    let mut lines = source.lines();
    let Some(first) = lines.next() else { return doc; };
    let remaining_text: String;

    let first_trim = first.trim_start();
    if first_trim.starts_with("((") && first_trim.ends_with("))") && first_trim.len() >= 4 {
        doc.config = Some(first_trim[2..first_trim.len() - 2].to_string());
        remaining_text = lines.collect::<Vec<_>>().join("\n");
    } else {
        remaining_text = std::iter::once(first).chain(lines).collect::<Vec<_>>().join("\n");
    }

    for line in remaining_text.lines() {
        if line.trim().is_empty() {
            // Blank line: push a zero-depth blank item so it's preserved in
            // the render (outline feel depends on it).
            doc.items.push(Item { depth: 0, text: String::new(), folded: false, continuation: false });
            continue;
        }
        let (depth, rest) = leading_indent(line);
        let continuation = rest.starts_with('+');
        let text = if continuation { rest[1..].trim_start().to_string() } else { rest.to_string() };
        doc.items.push(Item { depth, text, folded: false, continuation });
    }
    doc
}

/// Count leading tabs or `*`s as one indent level each; a mix is allowed
/// because HyperList historically used either.
fn leading_indent(line: &str) -> (usize, &str) {
    let mut depth = 0usize;
    let mut idx = 0;
    for c in line.chars() {
        match c {
            '\t' => { depth += 1; idx += 1; }
            '*' => { depth += 1; idx += c.len_utf8(); }
            ' ' => { idx += 1; }   // Skip spaces (soft-tab indentation)
            _ => break,
        }
    }
    (depth, &line[idx..])
}

/// Serialize the document back to text. Uses tabs for indentation so the
/// result round-trips with Vim and the Ruby TUI.
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    if let Some(cfg) = &doc.config {
        out.push_str("((");
        out.push_str(cfg);
        out.push_str("))\n");
    }
    for it in &doc.items {
        if it.text.is_empty() {
            out.push('\n');
            continue;
        }
        for _ in 0..it.depth { out.push('\t'); }
        if it.continuation { out.push('+'); out.push(' '); }
        out.push_str(&it.text);
        out.push('\n');
    }
    out
}

/// Return indexes of items that are direct children of `parent_idx` (depth
/// parent.depth+1, between parent and next item at depth <= parent.depth).
#[allow(dead_code)]
pub fn children(items: &[Item], parent_idx: usize) -> Vec<usize> {
    let Some(parent) = items.get(parent_idx) else { return Vec::new() };
    let pdepth = parent.depth;
    let mut out = Vec::new();
    for (i, it) in items.iter().enumerate().skip(parent_idx + 1) {
        if it.depth <= pdepth && !it.text.is_empty() { break; }
        if it.depth == pdepth + 1 { out.push(i); }
    }
    out
}

/// Index of the last item in the subtree rooted at `idx`.
pub fn last_descendant(items: &[Item], idx: usize) -> usize {
    let Some(root) = items.get(idx) else { return idx };
    let rdepth = root.depth;
    let mut last = idx;
    for (i, it) in items.iter().enumerate().skip(idx + 1) {
        if it.text.is_empty() { last = i; continue; }
        if it.depth <= rdepth { break; }
        last = i;
    }
    last
}
