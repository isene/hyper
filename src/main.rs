mod export;
mod highlight;
mod parser;

use crust::{Crust, Input, Pane};
use crust::style;
use parser::{Document, Item, last_descendant};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INDENT_CHAR: &str = "  ";
const FOLD_MARK: &str = "\u{25B8}";   // ▸  (collapsed)
const OPEN_MARK: &str = "\u{25BE}";   // ▾  (expanded)
const LEAF_MARK: &str = " ";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let filename = args.get(1).cloned();

    Crust::init();
    let mut app = App::new();
    if let Some(f) = filename {
        app.load(&f);
    }
    app.render_all();

    loop {
        let Some(key) = Input::getchr(Some(5)) else { continue };
        match key.as_str() {
            "q" => { if app.dirty { let _ = app.save(); } break; }
            "Q" => break,
            "?" => app.show_help(),
            "j" | "DOWN" => { app.move_down(); app.render_all(); }
            "k" | "UP" => { app.move_up(); app.render_all(); }
            "h" | "LEFT" => { app.goto_parent(); app.render_all(); }
            "l" | "RIGHT" => { app.goto_first_child(); app.render_all(); }
            "PgDOWN" | "C-D" => { app.page_down(); app.render_all(); }
            "PgUP" | "C-U" => { app.page_up(); app.render_all(); }
            "HOME" | "g" => { app.goto_first(); app.render_all(); }
            "END" | "G" => { app.goto_last(); app.render_all(); }
            "SPACE" | " " => { app.toggle_fold(); app.render_all(); }
            "z" => { app.collapse_all(); app.render_all(); }
            "Z" => { app.expand_all(); app.render_all(); }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                let n: usize = key.parse().unwrap_or(0);
                app.fold_to_level(n);
                app.render_all();
            }
            // a=10 .. f=15 — vim plugin's \a..\f extension to fold-level keys.
            "a" | "b" | "c" | "d" | "e" | "f" => {
                let n = 10 + (key.as_bytes()[0] - b'a') as usize;
                app.fold_to_level(n);
                app.render_all();
            }
            "W" => {
                if let Err(e) = app.save() { app.footer_say(&format!(" Save failed: {}", e), 196); }
                else { app.footer_say(" Saved", 46); }
                app.render_footer();
            }
            "o" => { app.open_file_prompt(); app.render_all(); }
            "S" => { app.show_filter_prompt(FilterMode::Show); app.render_all(); }
            "H" => { app.show_filter_prompt(FilterMode::Hide); app.render_all(); }
            "F" => { app.filter_set = None; app.footer_say(" Filter cleared", 46); app.render_all(); }
            "*" => { app.toggle_highlight_branch(); app.render_all(); }
            "p" => { app.toggle_presentation(); app.render_all(); }
            "ENTER" | "\n" | "\r" | "C-M" | "C-J" | "r" => { app.goto_reference(); app.render_all(); }
            "t" => { app.goto_next_template(); app.render_all(); }
            "C" => { app.show_complexity(); }
            // Editing
            "i" => { app.edit_current(); app.render_all(); }
            "O" => { app.insert_relative(false); app.render_all(); }
            // Lower-case `o` is open-file; new-line-below uses `+` so it's
            // ergonomic on a numpad/plus key.
            "+" => { app.insert_relative(true); app.render_all(); }
            "D" => { app.delete_current(); app.render_all(); }
            "TAB" | "\t" => { app.indent_current(); app.render_all(); }
            "S-TAB" | "BTab" | "B-TAB" => { app.outdent_current(); app.render_all(); }
            "v" => { app.toggle_checkbox(false); app.render_all(); }
            "V" => { app.toggle_checkbox(true); app.render_all(); }
            "R" => { app.renumber_all(); app.render_all(); }
            // Exporters
            "M-h" => { app.export_to(ExportKind::Html); app.render_all(); }
            "M-l" => { app.export_to(ExportKind::Latex); app.render_all(); }
            "M-m" => { app.export_to(ExportKind::Markdown); app.render_all(); }
            // Encryption (lowercase = item, uppercase = whole file).
            "e" => { app.encrypt_line(); app.render_all(); }
            "E" => { app.encrypt_all(); app.render_all(); }
            "x" => { app.decrypt_line(); app.render_all(); }
            "X" => { app.decrypt_all(); app.render_all(); }
            // Calendar export of future-dated items.
            "M-g" => { app.export_calendar(); app.render_all(); }
            // Operator / property completion popup (Alt-c since Tab indents).
            "M-c" => { app.complete_at_cursor(); app.render_all(); }
            _ => {}
        }
    }
    Crust::cleanup();
    Crust::clear_screen();
}

#[derive(Copy, Clone)]
enum FilterMode { Show, Hide }

#[derive(Copy, Clone)]
enum ExportKind { Html, Latex, Markdown }

struct App {
    doc: Document,
    filename: Option<std::path::PathBuf>,
    dirty: bool,
    visible_idx: usize,
    cols: u16,
    rows: u16,
    header: Pane,
    main_p: Pane,
    footer: Pane,
    status: Option<(String, u8)>,
    /// Indexes (into doc.items) that pass the Show/Hide filter. None = no filter.
    filter_set: Option<std::collections::HashSet<usize>>,
    /// Highlight-branch root: while Some(idx), dim everything outside the
    /// subtree rooted at idx. Vim's <leader>h.
    highlight_root: Option<usize>,
    /// Presentation mode: only ancestors + current path are shown. The
    /// flag drives a stricter visibility filter than fold.
    presentation: bool,
}

impl App {
    fn new() -> Self {
        let (cols, rows) = Crust::terminal_size();
        let mut header = Pane::new(1, 1, cols, 1, 255, 236);
        header.wrap = false;
        let mut main_p = Pane::new(1, 2, cols, rows.saturating_sub(2), 252, 0);
        main_p.wrap = false;
        let mut footer = Pane::new(1, rows, cols, 1, 255, 236);
        footer.wrap = false;
        Self {
            doc: Document::default(),
            filename: None, dirty: false,
            visible_idx: 0, cols, rows, header, main_p, footer,
            status: None,
            filter_set: None,
            highlight_root: None,
            presentation: false,
        }
    }

    fn load(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                self.doc = parser::parse(&s);
                self.filename = Some(path.into());
                self.apply_initial_fold();
                self.dirty = false;
            }
            Err(e) => self.footer_say(&format!(" Load failed: {}", e), 196),
        }
    }

    fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = self.filename.clone() else {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "no file"));
        };
        std::fs::write(&path, parser::serialize(&self.doc))?;
        self.dirty = false;
        Ok(())
    }

    fn open_file_prompt(&mut self) {
        let path = self.footer.ask(" Open file: ", "");
        if path.trim().is_empty() { return; }
        self.load(path.trim());
    }

    fn apply_initial_fold(&mut self) {
        // Look for `fold_level=N` in the config line.
        if let Some(cfg) = &self.doc.config {
            if let Some(lvl_s) = cfg.split(',').map(|s| s.trim()).find(|s| s.starts_with("fold_level=")) {
                if let Some(lvl) = lvl_s[11..].parse::<usize>().ok() {
                    self.fold_to_level(lvl);
                }
            }
        }
    }

    fn render_all(&mut self) {
        self.render_header();
        self.render_main();
        self.render_footer();
    }

    fn render_header(&mut self) {
        let name = self.filename.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[no file]".into());
        let dirty = if self.dirty { " *" } else { "" };
        let info = format!(" {}{}  ({} items)", name, dirty, self.doc.items.len());
        let crumb = self.breadcrumb();
        let cols = self.cols as usize;
        let info_styled = style::bold(&info);
        let info_w = crust::display_width(&info_styled);
        if !crumb.is_empty() && info_w + 3 < cols {
            // Right-align breadcrumb after the filename info, separated by gap.
            let crumb_styled = style::fg(&crumb, 244);
            let crumb_w = crust::display_width(&crumb_styled);
            let line = if info_w + crumb_w + 2 <= cols {
                let gap = cols - info_w - crumb_w - 1;
                format!("{}{}{} ", info_styled, " ".repeat(gap), crumb_styled)
            } else {
                // Truncate breadcrumb to what fits.
                let avail = cols.saturating_sub(info_w + 2);
                let trimmed = truncate_display(&crumb, avail);
                let s = style::fg(&trimmed, 244);
                let pad = cols - info_w - crust::display_width(&s) - 1;
                format!("{}{}{} ", info_styled, " ".repeat(pad), s)
            };
            self.header.say(&line);
        } else {
            self.header.say(&info_styled);
        }
    }

    /// Build a `parent > grandparent > root`-style trail of the current item's
    /// ancestors. Empty when at depth 0 or no item is selected.
    fn breadcrumb(&self) -> String {
        let Some(idx) = self.current_item_idx() else { return String::new() };
        let cur_depth = self.doc.items[idx].depth;
        if cur_depth == 0 { return String::new() }
        let mut parts: Vec<String> = Vec::new();
        let mut want = cur_depth;
        let mut i = idx;
        while i > 0 && want > 0 {
            i -= 1;
            let it = &self.doc.items[i];
            if it.text.is_empty() { continue; }
            if it.depth + 1 == want {
                let snippet = first_n_chars(&strip_markup(&it.text), 40);
                parts.push(snippet);
                want -= 1;
            }
        }
        parts.reverse();
        parts.join(" \u{203A} ")
    }

    fn render_footer(&mut self) {
        // Left: status message if any, otherwise the hint line.
        // Right: OSC 8 hyperlink on "hyper vX" pointing at the canonical
        // HyperList homepage, right-aligned in the footer pane.
        let left: String = if let Some((ref msg, color)) = self.status {
            style::fg(msg, color)
        } else {
            let hint = " j/k:Move  h/l:Parent/Child  SPACE:Fold  1-9:Level  z/Z:CollapseAll/ExpandAll  o:Open  W:Save  ?:Help  q:Quit";
            style::fg(hint, 245)
        };
        // OSC 8 open + visible text + OSC 8 close. Kitty-style terminals
        // underline the linked text; others render it as plain text.
        let version_link = format!(
            "\x1b]8;;https://isene.org/hyperlist/\x1b\\hyper v{}\x1b]8;;\x1b\\",
            VERSION
        );
        let right = style::fg(&version_link, 245);

        let cols = self.cols as usize;
        let left_w = crust::display_width(&left);
        let right_w = crust::display_width(&right);
        // One trailing space on the right so the link doesn't butt against
        // the pane edge.
        let right_padded = format!("{} ", right);
        let right_w = right_w + 1;
        let line = if left_w + right_w + 1 <= cols {
            let gap = cols - left_w - right_w;
            format!("{}{}{}", left, " ".repeat(gap), right_padded)
        } else {
            // Not enough room — drop the left side so the version stays visible.
            let gap = cols.saturating_sub(right_w);
            format!("{}{}", " ".repeat(gap), right_padded)
        };
        self.footer.say(&line);
    }

    fn footer_say(&mut self, msg: &str, c: u8) {
        self.status = Some((msg.to_string(), c));
        self.render_footer();
    }

    fn render_main(&mut self) {
        let visible = self.visible_items();
        let mut lines = Vec::new();
        for (vis_i, &idx) in visible.iter().enumerate() {
            let it = &self.doc.items[idx];
            if it.text.is_empty() {
                lines.push(String::new());
                continue;
            }
            let indent = INDENT_CHAR.repeat(it.depth);
            let mark = if self.has_children(idx) {
                if it.folded { FOLD_MARK } else { OPEN_MARK }
            } else { LEAF_MARK };
            let body = highlight::colorize_line(&it.text);
            let prefix = if vis_i == self.visible_idx { "\u{2192} " } else { "  " };
            let row = format!("{}{}{} {}", prefix, indent, style::fg(mark, 244), body);
            let dimmed = self.highlight_root.is_some() && !self.in_highlight_subtree(idx);
            let styled = if vis_i == self.visible_idx {
                style::bg(&row, 236)
            } else if dimmed {
                style::fg(&format!("{}{}{} {}", prefix, indent, mark, strip_ansi(&body)), 240)
            } else { row };
            lines.push(styled);
        }
        if lines.is_empty() {
            lines.push(style::fg("  (empty - press 'o' to open a .hl file)", 245));
        }
        self.main_p.set_text(&lines.join("\n"));
        self.main_p.ix = scroll_offset(self.visible_idx, lines.len(), self.main_p.h as usize);
        self.main_p.full_refresh();
    }

    // --- Visibility + fold ---

    fn visible_items(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.doc.items.len());
        let mut skip_until: Option<usize> = None;
        for (i, _) in self.doc.items.iter().enumerate() {
            if let Some(end) = skip_until {
                if i <= end { continue; }
                skip_until = None;
            }
            // Show/Hide filter: hide items that aren't in the filter set
            // (and aren't ancestors of any filter-set item).
            if let Some(set) = &self.filter_set {
                if !set.contains(&i) { continue; }
            }
            out.push(i);
            if self.doc.items[i].folded && self.has_children(i) {
                skip_until = Some(last_descendant(&self.doc.items, i));
            }
        }
        out
    }

    /// Build the set of items to keep when filtering by `keyword` (case-
    /// insensitive substring). Includes matched items plus their ancestors
    /// so the structure stays navigable.
    fn build_filter_set(&self, keyword: &str, mode: FilterMode) -> std::collections::HashSet<usize> {
        use std::collections::HashSet;
        let kw = keyword.to_lowercase();
        let mut keep: HashSet<usize> = HashSet::new();
        for (i, it) in self.doc.items.iter().enumerate() {
            if it.text.is_empty() { continue; }
            let hit = it.text.to_lowercase().contains(&kw);
            let want = match mode { FilterMode::Show => hit, FilterMode::Hide => !hit };
            if want {
                keep.insert(i);
                // Add all ancestors so the path to this item stays visible.
                let depth = it.depth;
                let mut want_d = depth;
                let mut j = i;
                while j > 0 && want_d > 0 {
                    j -= 1;
                    let a = &self.doc.items[j];
                    if a.text.is_empty() { continue; }
                    if a.depth + 1 == want_d {
                        keep.insert(j);
                        want_d -= 1;
                    }
                }
            }
        }
        keep
    }

    fn show_filter_prompt(&mut self, mode: FilterMode) {
        let kw = self.footer.ask(
            match mode { FilterMode::Show => " Show items containing: ", FilterMode::Hide => " Hide items containing: " },
            "");
        if kw.trim().is_empty() {
            self.filter_set = None;
            self.footer_say(" Filter cleared", 46);
            return;
        }
        let set = self.build_filter_set(kw.trim(), mode);
        let n = set.len();
        self.filter_set = Some(set);
        self.visible_idx = 0;
        self.footer_say(&format!(" Filter active ({} items match)", n), 46);
    }

    fn has_children(&self, idx: usize) -> bool {
        let Some(cur) = self.doc.items.get(idx) else { return false; };
        let cdepth = cur.depth;
        self.doc.items.iter().skip(idx + 1).any(|it| {
            if it.text.is_empty() { return false; }
            it.depth > cdepth || it.depth <= cdepth && false
        }) && self.doc.items.get(idx + 1).map(|n| n.depth > cdepth).unwrap_or(false)
    }

    fn current_item_idx(&self) -> Option<usize> {
        self.visible_items().get(self.visible_idx).copied()
    }

    fn move_down(&mut self) {
        let n = self.visible_items().len();
        if self.visible_idx + 1 < n { self.visible_idx += 1; }
    }
    fn move_up(&mut self) { if self.visible_idx > 0 { self.visible_idx -= 1; } }
    fn page_down(&mut self) {
        let n = self.visible_items().len();
        let step = self.main_p.h as usize;
        self.visible_idx = (self.visible_idx + step).min(n.saturating_sub(1));
    }
    fn page_up(&mut self) {
        let step = self.main_p.h as usize;
        self.visible_idx = self.visible_idx.saturating_sub(step);
    }
    fn goto_first(&mut self) { self.visible_idx = 0; }
    fn goto_last(&mut self) {
        let n = self.visible_items().len();
        self.visible_idx = n.saturating_sub(1);
    }

    fn goto_parent(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        let cur = &self.doc.items[idx];
        if cur.depth == 0 { return; }
        let target_depth = cur.depth - 1;
        let mut parent = idx;
        while parent > 0 {
            parent -= 1;
            let it = &self.doc.items[parent];
            if !it.text.is_empty() && it.depth == target_depth {
                if let Some(pos) = self.visible_items().iter().position(|&i| i == parent) {
                    self.visible_idx = pos;
                }
                return;
            }
        }
    }

    fn goto_first_child(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if !self.has_children(idx) { return; }
        // Unfold so the first child is visible.
        if self.doc.items[idx].folded { self.doc.items[idx].folded = false; }
        if let Some(pos) = self.visible_items().iter().position(|&i| i == idx + 1) {
            self.visible_idx = pos;
        }
    }

    fn toggle_fold(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if self.has_children(idx) {
            self.doc.items[idx].folded = !self.doc.items[idx].folded;
        }
    }
    fn collapse_all(&mut self) {
        for it in &mut self.doc.items { if it.depth == 0 { it.folded = true; } }
    }
    fn expand_all(&mut self) {
        for it in &mut self.doc.items { it.folded = false; }
    }
    fn fold_to_level(&mut self, level: usize) {
        for it in &mut self.doc.items {
            it.folded = it.depth >= level;
        }
    }

    // ── Highlight current branch (vim \h) ──────────────────────────────
    fn toggle_highlight_branch(&mut self) {
        if self.highlight_root.is_some() {
            self.highlight_root = None;
            self.footer_say(" Highlight off", 46);
        } else if let Some(idx) = self.current_item_idx() {
            self.highlight_root = Some(idx);
            self.footer_say(" Highlight on (current branch)", 46);
        }
    }

    fn in_highlight_subtree(&self, idx: usize) -> bool {
        let Some(root) = self.highlight_root else { return true };
        if idx == root { return true; }
        let last = last_descendant(&self.doc.items, root);
        idx > root && idx <= last
    }

    // ── Presentation mode (vim g<DOWN>/g<UP>) ──────────────────────────
    fn toggle_presentation(&mut self) {
        self.presentation = !self.presentation;
        if self.presentation {
            // Collapse everything; visibility is then driven by ancestor
            // walk for current item.
            for it in &mut self.doc.items { it.folded = true; }
            self.expand_ancestors_of_current();
            self.footer_say(" Presentation mode ON (g to exit)", 46);
        } else {
            self.footer_say(" Presentation mode OFF", 46);
        }
    }

    fn expand_ancestors_of_current(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        let mut depth = self.doc.items[idx].depth;
        let mut j = idx;
        while j > 0 && depth > 0 {
            j -= 1;
            let a = &self.doc.items[j];
            if a.text.is_empty() { continue; }
            if a.depth + 1 == depth {
                self.doc.items[j].folded = false;
                depth -= 1;
            }
        }
        // Unfold the current item too so its first child shows on l.
        self.doc.items[idx].folded = false;
    }

    // ── Goto reference (vim gr / Enter) ────────────────────────────────
    fn goto_reference(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        let text = self.doc.items[idx].text.clone();
        // Find first <…> / <<…>> or <file:…>. Walk char-by-char tracking
        // angle-bracket depth so we can distinguish double-angle refs.
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                // Check for <<…>>
                let double = i + 1 < bytes.len() && bytes[i + 1] == b'<';
                let start = if double { i + 2 } else { i + 1 };
                let close_seq: &[u8] = if double { b">>" } else { b">" };
                let mut j = start;
                while j + close_seq.len() <= bytes.len() {
                    if &bytes[j..j + close_seq.len()] == close_seq { break; }
                    j += 1;
                }
                if j + close_seq.len() <= bytes.len() {
                    let inner = &text[start..j];
                    if let Some(rest) = inner.strip_prefix("file:") {
                        let path = rest.trim();
                        let expanded = expand_tilde(path);
                        let _ = std::process::Command::new("xdg-open").arg(&expanded).spawn();
                        self.footer_say(&format!(" Opened {}", expanded), 46);
                    } else {
                        // In-document reference: substring match against item text.
                        let target = inner.trim();
                        if let Some(found) = self.find_item_by_text(target) {
                            // Make sure the found item is visible, then jump.
                            self.unfold_path_to(found);
                            if let Some(pos) = self.visible_items().iter().position(|&i| i == found) {
                                self.visible_idx = pos;
                                self.footer_say(&format!(" Jumped to <{}>", target), 46);
                            } else {
                                self.footer_say(&format!(" Reference <{}> not visible", target), 196);
                            }
                        } else {
                            self.footer_say(&format!(" Reference <{}> not found", target), 196);
                        }
                    }
                    return;
                }
            }
            i += 1;
        }
        self.footer_say(" No reference on this line", 245);
    }

    fn find_item_by_text(&self, needle: &str) -> Option<usize> {
        // Prefer exact match first, then case-insensitive substring.
        for (i, it) in self.doc.items.iter().enumerate() {
            if it.text.trim() == needle { return Some(i); }
        }
        let lc = needle.to_lowercase();
        for (i, it) in self.doc.items.iter().enumerate() {
            if it.text.to_lowercase().contains(&lc) { return Some(i); }
        }
        None
    }

    fn unfold_path_to(&mut self, target: usize) {
        let target_depth = self.doc.items[target].depth;
        let mut want = target_depth;
        let mut i = target;
        while i > 0 && want > 0 {
            i -= 1;
            let a = &self.doc.items[i];
            if a.text.is_empty() { continue; }
            if a.depth + 1 == want {
                self.doc.items[i].folded = false;
                want -= 1;
            }
        }
    }

    // ── Goto next template element (vim <leader><SPACE>) ──────────────
    fn goto_next_template(&mut self) {
        let visible = self.visible_items();
        let start = self.visible_idx + 1;
        for (vi, &idx) in visible.iter().enumerate().skip(start) {
            if self.doc.items[idx].text.trim_end().ends_with('=') {
                self.visible_idx = vi;
                self.footer_say(" → next template element", 46);
                return;
            }
        }
        // Wrap to start.
        for (vi, &idx) in visible.iter().enumerate().take(start) {
            if self.doc.items[idx].text.trim_end().ends_with('=') {
                self.visible_idx = vi;
                self.footer_say(" → next template element (wrapped)", 46);
                return;
            }
        }
        self.footer_say(" No template elements (item ending in `=`)", 245);
    }

    // ── Complexity (vim \C) ────────────────────────────────────────────
    fn show_complexity(&mut self) {
        let n = self.doc.items.iter().filter(|it| !it.text.is_empty()).count();
        let max_d = self.doc.items.iter().filter(|it| !it.text.is_empty())
            .map(|it| it.depth).max().unwrap_or(0);
        let score = (n as f64) * (1.0 + (max_d as f64) / 10.0);
        let msg = format!(
            "\n  HyperList Complexity\n\n  \
              Items:        {}\n  \
              Max depth:    {}\n  \
              Score:        {:.1}\n  \
              (Score = items × (1 + max_depth/10))\n\n  \
              Press any key to close.",
            n, max_d, score);
        self.main_p.set_text(&msg);
        self.main_p.ix = 0;
        self.main_p.full_refresh();
        let _ = Input::getchr(None);
        self.render_main();
    }

    // ── Edit mode (vim i / O / o / D / Tab / Shift-Tab) ───────────────
    fn edit_current(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        let it = self.doc.items[idx].clone();
        if it.text.is_empty() {
            // Blank-line items aren't really editable; treat as insert-below.
            self.insert_relative(true);
            return;
        }
        let prompt = format!(" ✎ [d{}] ", it.depth);
        let new = self.footer.ask(&prompt, &it.text);
        // ask() leaves footer in command-input bg; reset by re-rendering.
        if new == it.text { return; }
        self.doc.items[idx].text = new;
        self.dirty = true;
    }

    /// Insert a blank item below (or above) the current cursor, at the same
    /// depth, and immediately open it for editing.
    fn insert_relative(&mut self, below: bool) {
        let Some(cur) = self.current_item_idx() else {
            // Empty document: just push a depth-0 item.
            self.doc.items.push(parser::Item { depth: 0, text: String::new(), folded: false, continuation: false });
            self.dirty = true;
            self.visible_idx = 0;
            self.edit_just_inserted(self.doc.items.len() - 1);
            return;
        };
        let depth = self.doc.items[cur].depth;
        // Insertion point: just after the cursor's subtree (below) or at the
        // cursor (above).
        let insert_at = if below {
            last_descendant(&self.doc.items, cur) + 1
        } else { cur };
        let new = parser::Item { depth, text: String::new(), folded: false, continuation: false };
        self.doc.items.insert(insert_at, new);
        self.dirty = true;
        self.edit_just_inserted(insert_at);
    }

    fn edit_just_inserted(&mut self, idx: usize) {
        // Move the visible cursor onto the newly inserted item, render so the
        // user sees it, then prompt for text.
        if let Some(pos) = self.visible_items().iter().position(|&i| i == idx) {
            self.visible_idx = pos;
        }
        self.render_all();
        let prompt = format!(" + [d{}] ", self.doc.items[idx].depth);
        let txt = self.footer.ask(&prompt, "");
        if txt.trim().is_empty() {
            // Empty input: drop the inserted item.
            self.doc.items.remove(idx);
            // Restore cursor to a sensible spot.
            let n = self.visible_items().len();
            if self.visible_idx >= n && n > 0 { self.visible_idx = n - 1; }
        } else {
            self.doc.items[idx].text = txt;
        }
    }

    /// Delete the current item AND its subtree (matches vim plugin: dd on a
    /// folded parent removes the whole branch).
    fn delete_current(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if self.doc.items[idx].text.is_empty() { return; }
        let last = last_descendant(&self.doc.items, idx);
        self.doc.items.drain(idx..=last);
        self.dirty = true;
        let n = self.visible_items().len();
        if self.visible_idx >= n && n > 0 { self.visible_idx = n - 1; }
    }

    /// Indent current item and its descendants by one level (vim Tab / <c-t>).
    fn indent_current(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if self.doc.items[idx].text.is_empty() { return; }
        let last = last_descendant(&self.doc.items, idx);
        for i in idx..=last {
            if !self.doc.items[i].text.is_empty() {
                self.doc.items[i].depth += 1;
            }
        }
        self.dirty = true;
    }

    /// Outdent current item and descendants (vim Shift-Tab / <c-d>). No-op when
    /// already at depth 0.
    fn outdent_current(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if self.doc.items[idx].text.is_empty() { return; }
        if self.doc.items[idx].depth == 0 { return; }
        let last = last_descendant(&self.doc.items, idx);
        for i in idx..=last {
            if !self.doc.items[i].text.is_empty() && self.doc.items[i].depth > 0 {
                self.doc.items[i].depth -= 1;
            }
        }
        self.dirty = true;
    }

    // ── Checkbox toggle (vim \v / \V) ──────────────────────────────────
    /// Cycle: empty → `[_]` → `[x]` → empty. With `dated`=true, completion
    /// pins a date stamp `(YYYY-MM-DD)` after the marker.
    fn toggle_checkbox(&mut self, dated: bool) {
        let Some(idx) = self.current_item_idx() else { return };
        let txt = self.doc.items[idx].text.clone();
        if txt.is_empty() { return; }
        let trimmed = txt.trim_start();
        let lead_ws = &txt[..txt.len() - trimmed.len()];
        let new_body: String = if let Some(rest) = trimmed.strip_prefix("[_] ") {
            // Empty box → checked.
            if dated {
                format!("[x] ({}) {}", today(), rest)
            } else {
                format!("[x] {}", rest)
            }
        } else if let Some(rest) = trimmed.strip_prefix("[x] ") {
            // Checked → unchecked (drop trailing date stamp if present).
            let cleaned = strip_date_stamp(rest);
            cleaned.to_string()
        } else if let Some(rest) = trimmed.strip_prefix("[X] ") {
            let cleaned = strip_date_stamp(rest);
            cleaned.to_string()
        } else {
            // No box yet → add empty.
            format!("[_] {}", trimmed)
        };
        self.doc.items[idx].text = format!("{}{}", lead_ws, new_body);
        self.dirty = true;
    }

    // ── Autonumbering + renumber (vim \an / \# / \R) ──────────────────
    /// Renumber the immediate children of `parent_idx` with `1.`, `2.`, …
    /// Recursively renumbers their subtrees too. If `parent_idx` is None,
    /// renumber depth-0 items.
    fn renumber_under(&mut self, parent_idx: Option<usize>) {
        let parent_depth = parent_idx.map(|i| self.doc.items[i].depth).unwrap_or(usize::MAX);
        let target_depth = if parent_idx.is_none() { 0 } else { parent_depth + 1 };
        // Range to walk: parent's subtree, or the whole doc.
        let (start, end) = match parent_idx {
            Some(p) => (p + 1, last_descendant(&self.doc.items, p) + 1),
            None => (0, self.doc.items.len()),
        };
        let mut counter = 1usize;
        let mut last_child: Option<usize> = None;
        let mut i = start;
        while i < end {
            if self.doc.items[i].text.is_empty() { i += 1; continue; }
            if self.doc.items[i].depth == target_depth {
                let new_text = replace_leading_number(&self.doc.items[i].text, counter);
                if new_text != self.doc.items[i].text {
                    self.doc.items[i].text = new_text;
                    self.dirty = true;
                }
                last_child = Some(i);
                counter += 1;
            } else if self.doc.items[i].depth > target_depth && last_child.is_some() {
                // We crossed into a sub-tree of a child; recurse from the
                // child via the immediate-children renumber.
                if let Some(child) = last_child {
                    self.renumber_under(Some(child));
                    // Skip to after that child's subtree.
                    i = last_descendant(&self.doc.items, child);
                }
            }
            i += 1;
        }
    }

    /// Renumber the entire document — every level under root.
    fn renumber_all(&mut self) {
        // Top-level pass.
        self.renumber_under(None);
        self.footer_say(" Renumbered all", 46);
    }

    // ── Export to HTML / LaTeX / Markdown ──────────────────────────────
    fn export_to(&mut self, kind: ExportKind) {
        let Some(path) = self.filename.clone() else {
            self.footer_say(" No file loaded — open one first", 196);
            return;
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("hyperlist").to_string();
        let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let (ext, body) = match kind {
            ExportKind::Html     => ("html", export::to_html(&self.doc, &stem)),
            ExportKind::Latex    => ("tex",  export::to_latex(&self.doc, &stem)),
            ExportKind::Markdown => ("md",   export::to_markdown(&self.doc, &stem)),
        };
        let out_path = parent.join(format!("{}.{}", stem, ext));
        match std::fs::write(&out_path, body) {
            Ok(_)  => self.footer_say(&format!(" Wrote {}", out_path.display()), 46),
            Err(e) => self.footer_say(&format!(" Export failed: {}", e), 196),
        }
    }

    // ── Encryption per-line / file (vim \z / \Z / \x / \X) ────────────
    fn encrypt_line(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        if self.doc.items[idx].text.is_empty() { return; }
        // Encrypt the subtree (item + descendants), matching vim's behavior
        // of encrypting "the current line including all sublevels if folded".
        let last = last_descendant(&self.doc.items, idx);
        let mut plain = String::new();
        for i in idx..=last {
            if !self.doc.items[i].text.is_empty() {
                for _ in 0..self.doc.items[i].depth { plain.push('\t'); }
                plain.push_str(&self.doc.items[i].text);
            }
            plain.push('\n');
        }
        match gpg_encrypt(&plain) {
            Ok(armored) => {
                // Replace the subtree with a single sentinel item carrying the
                // armored ciphertext on one logical line (newlines escaped).
                let payload = format!("⊟ENC: {}", armored.replace('\n', "\\n"));
                self.doc.items.drain(idx..=last);
                self.doc.items.insert(idx, parser::Item { depth: self.doc.items.get(idx).map(|x| x.depth).unwrap_or(0), text: payload, folded: false, continuation: false });
                self.dirty = true;
                self.footer_say(" Encrypted subtree", 46);
            }
            Err(e) => self.footer_say(&format!(" gpg encrypt failed: {}", e), 196),
        }
    }

    fn decrypt_line(&mut self) {
        let Some(idx) = self.current_item_idx() else { return };
        let txt = self.doc.items[idx].text.clone();
        let Some(payload) = txt.strip_prefix("⊟ENC: ") else {
            self.footer_say(" Not an encrypted item", 245);
            return;
        };
        let armored = payload.replace("\\n", "\n");
        match gpg_decrypt(&armored) {
            Ok(plain) => {
                // Re-parse the plaintext into items and splice them in.
                let parsed = parser::parse(&plain);
                self.doc.items.remove(idx);
                for (k, item) in parsed.items.into_iter().enumerate() {
                    self.doc.items.insert(idx + k, item);
                }
                self.dirty = true;
                self.footer_say(" Decrypted subtree", 46);
            }
            Err(e) => self.footer_say(&format!(" gpg decrypt failed: {}", e), 196),
        }
    }

    fn encrypt_all(&mut self) {
        let plain = parser::serialize(&self.doc);
        let Some(path) = self.filename.clone() else {
            self.footer_say(" No file — encrypt-all needs a save target", 196); return;
        };
        match gpg_encrypt(&plain) {
            Ok(armored) => {
                if let Err(e) = std::fs::write(&path, armored) {
                    self.footer_say(&format!(" Write failed: {}", e), 196);
                } else {
                    self.dirty = false;
                    self.doc.items.clear();
                    self.doc.items.push(parser::Item {
                        depth: 0,
                        text: format!("⊟ENC-FILE: {}", path.display()),
                        folded: false, continuation: false,
                    });
                    self.footer_say(&format!(" Encrypted file written to {}", path.display()), 46);
                }
            }
            Err(e) => self.footer_say(&format!(" gpg encrypt-all failed: {}", e), 196),
        }
    }

    fn decrypt_all(&mut self) {
        let Some(path) = self.filename.clone() else { self.footer_say(" No file loaded", 196); return; };
        let armored = match std::fs::read_to_string(&path) {
            Ok(s) => s, Err(e) => { self.footer_say(&format!(" Read failed: {}", e), 196); return; }
        };
        match gpg_decrypt(&armored) {
            Ok(plain) => {
                self.doc = parser::parse(&plain);
                self.dirty = true;
                self.footer_say(" Decrypted file", 46);
            }
            Err(e) => self.footer_say(&format!(" gpg decrypt-all failed: {}", e), 196),
        }
    }

    // ── Calendar export (vim \G) ───────────────────────────────────────
    fn export_calendar(&mut self) {
        let mut events = Vec::new();
        for (i, it) in self.doc.items.iter().enumerate() {
            if it.text.is_empty() { continue; }
            if let Some((y, m, d)) = scan_future_date(&it.text) {
                events.push((i, y, m, d, it.text.clone()));
            }
        }
        if events.is_empty() {
            self.footer_say(" No future-dated items found", 245);
            return;
        }
        let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".tock/incoming");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.footer_say(&format!(" mkdir failed: {}", e), 196);
            return;
        }
        let stamp = today();
        for (i, y, m, d, summary) in &events {
            let ics = build_ics(*y, *m, *d, summary);
            let fname = dir.join(format!("hyper_{}_{}_{:03}.ics", stem_of(self.filename.as_ref()), stamp.replace('-', ""), i));
            let _ = std::fs::write(fname, ics);
        }
        self.footer_say(&format!(" Wrote {} calendar event(s) to ~/.tock/incoming/", events.len()), 46);
    }

    // ── Operator/property completion (vim HyperListComplete) ──────────
    fn complete_at_cursor(&mut self) {
        // Show a popup listing operator keywords + property names seen in
        // the document. User picks one with arrows and Enter; selected
        // string is appended to the current item's text.
        let Some(idx) = self.current_item_idx() else { return };
        let mut suggestions: Vec<&'static str> = vec![
            "AND: ", "OR: ", "NOT: ", "EXAMPLE: ", "IF: ", "ELSE: ", "WHEN: ",
            "WHERE: ", "WHO: ", "WHY: ", "HOW: ", "DO: ", "USE: ", "BEFORE: ",
            "AFTER: ", "WHILE: ", "UNTIL: ", "TODO: ", "SKIP: ", "END: ",
        ];
        let mut props: std::collections::HashSet<String> = std::collections::HashSet::new();
        for it in &self.doc.items {
            if let Some(colon_pos) = it.text.find(": ") {
                let key = it.text[..colon_pos].trim();
                // Heuristic: not all-caps (operator), not empty, not punctuation.
                if !key.is_empty()
                    && !key.chars().all(|c| c.is_ascii_uppercase() || !c.is_alphabetic())
                    && key.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.')
                {
                    props.insert(format!("{}: ", key));
                }
            }
        }
        let mut props_vec: Vec<String> = props.into_iter().collect();
        props_vec.sort();
        let mut all: Vec<String> = suggestions.drain(..).map(String::from).collect();
        all.extend(props_vec);
        // Show as numbered popup; pressing 1-9 picks that suggestion.
        let mut popup = String::from("\n  Completion — pick a number:\n\n");
        for (n, s) in all.iter().take(20).enumerate() {
            popup.push_str(&format!("  {}: {}\n", (n + 1) % 10, s));
            if n == 9 { popup.push_str("\n"); }
        }
        popup.push_str("\n  Press number or any other key to cancel.");
        self.main_p.set_text(&popup);
        self.main_p.full_refresh();
        let key = Input::getchr(None).unwrap_or_default();
        let n: Option<usize> = match key.as_str() {
            "1" => Some(1), "2" => Some(2), "3" => Some(3), "4" => Some(4), "5" => Some(5),
            "6" => Some(6), "7" => Some(7), "8" => Some(8), "9" => Some(9), "0" => Some(10),
            _ => None,
        };
        if let Some(n) = n {
            if n - 1 < all.len() {
                let txt = all[n - 1].clone();
                let cur_text = self.doc.items[idx].text.clone();
                self.doc.items[idx].text = if cur_text.is_empty() { txt } else { format!("{} {}", cur_text, txt) };
                self.dirty = true;
            }
        }
        self.render_main();
    }

    fn show_help(&mut self) {
        let help = "\n  \
            hyper — HyperList terminal viewer\n\n  \
            NAVIGATION\n  \
              j / DOWN       Move down (visible items)\n  \
              k / UP         Move up\n  \
              h / LEFT       Jump to parent\n  \
              l / RIGHT      Jump to first child (unfolds)\n  \
              PgUP / PgDOWN  Page\n  \
              g / HOME       First item\n  \
              G / END        Last item\n  \
              ENTER / r      Goto reference under cursor (<ref> or <file:…>)\n  \
              t              Goto next template element (item ending in `=`)\n\n  \
            FOLDING & VIEWS\n  \
              SPACE          Toggle fold on current\n  \
              1..9 / a..f    Fold all at level N (1–15)\n  \
              z / Z          Collapse / expand all\n  \
              S / H          Show / Hide items by keyword\n  \
              F              Clear show/hide filter\n  \
              *              Toggle highlight current branch\n  \
              p              Toggle presentation mode (ancestors only)\n\n  \
            INFO\n  \
              C              Complexity score (items × depth)\n\n  \
            EDIT\n  \
              i              Edit current item\n  \
              O / +          New item above / below at same depth\n  \
              D              Delete current item (and subtree)\n  \
              Tab / S-Tab    Indent / outdent current subtree\n  \
              v / V          Toggle checkbox / toggle with date stamp\n  \
              R              Renumber whole document\n  \
              M-c            Operator / property completion popup\n\n  \
            EXPORT (Alt-key)\n  \
              M-h / M-l / M-m  Export to HTML / LaTeX / Markdown\n  \
              M-g              Calendar (future-dated items → ~/.tock/incoming/)\n\n  \
            ENCRYPTION (gpg --symmetric)\n  \
              e / E          Encrypt current subtree / whole file\n  \
              x / X          Decrypt current subtree / whole file\n\n  \
            FILES\n  \
              o              Open a .hl file\n  \
              W              Save current file\n  \
              ? / q / Q      Help / Quit (save) / Quit (no save)\n\n  \
            Format: HyperList 2.6 - https://isene.org/hyperlist/\n  \
            Press any key to close.";
        self.main_p.set_text(help);
        self.main_p.ix = 0;
        self.main_p.full_refresh();
        let _ = Input::getchr(None);
        self.render_main();
    }
}

fn scroll_offset(idx: usize, total: usize, h: usize) -> usize {
    if total <= h { return 0; }
    let half = h / 2;
    if idx < half { 0 }
    else if idx + half >= total { total - h }
    else { idx - half }
}

/// Strip HyperList markup (bold `*…*`, italic `/…/`, underline `_…_`) for
/// breadcrumb / popup display where ANSI styling isn't applied.
fn strip_markup(s: &str) -> String {
    // Remove leading checkbox tokens and common operator suffix colons so the
    // breadcrumb shows the topic word, not the structural metadata.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '*' || c == '/' || c == '_' { continue; }
        out.push(c);
    }
    out.trim().to_string()
}

fn first_n_chars(s: &str, n: usize) -> String {
    let mut count = 0;
    let mut out = String::with_capacity(s.len().min(n * 4));
    for c in s.chars() {
        if count >= n { out.push('…'); break; }
        out.push(c);
        count += 1;
    }
    out
}

/// Strip ANSI SGR escape sequences (CSI ... m) from a string. Used by the
/// branch-highlight dim path which re-colors a previously highlighted line.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // CSI: skip until letter byte (final 0x40-0x7E).
            i += 2;
            while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) { i += 1; }
            if i < bytes.len() { i += 1; }
        } else if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            // OSC: skip until ESC \ or BEL.
            i += 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 { i += 1; break; }
                if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' { i += 2; break; }
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn today() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    // Cheap proleptic Gregorian conversion. Days since 1970-01-01 (Thursday).
    let days = secs / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    // Algorithm from "Calendrical Calculations" — accurate for 1900..2100+.
    let mut year = 1970i64;
    loop {
        let leap = is_leap(year);
        let dy = if leap { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    while days < 0 {
        year -= 1;
        let leap = is_leap(year);
        days += if leap { 366 } else { 365 };
    }
    let dim: [u32; 12] = [31, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    let mut d = days as u32;
    while m < 12 && d >= dim[m] {
        d -= dim[m];
        m += 1;
    }
    (year, (m + 1) as u32, d + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Drop a `(YYYY-MM-DD)` suffix appended by `\V`-style toggles.
fn strip_date_stamp(s: &str) -> &str {
    let t = s.trim_start();
    if t.starts_with('(') && t.len() >= 12 {
        let close = t.find(')').unwrap_or(0);
        if close > 0 {
            let inside = &t[1..close];
            // Looks like a date if it's 8-10 chars with two `-`.
            if inside.matches('-').count() == 2 && inside.len() >= 8 && inside.len() <= 10 {
                return t[close + 1..].trim_start();
            }
        }
    }
    s
}

/// Replace any leading `N.` (where N is 1+ digits) with `<n>.`. If the line
/// has no leading number, prepend it.
fn replace_leading_number(line: &str, n: usize) -> String {
    let leading_ws_end = line.find(|c: char| !c.is_whitespace()).unwrap_or(line.len());
    let (ws, rest) = line.split_at(leading_ws_end);
    // Detect existing N. prefix.
    let mut num_end = 0;
    for (i, c) in rest.char_indices() {
        if c.is_ascii_digit() { num_end = i + 1; }
        else { break; }
    }
    if num_end > 0 && rest[num_end..].starts_with('.') {
        let after = &rest[num_end + 1..];
        let after = after.strip_prefix(' ').unwrap_or(after);
        format!("{}{}. {}", ws, n, after)
    } else {
        format!("{}{}. {}", ws, n, rest)
    }
}

// ── GPG helpers ────────────────────────────────────────────────────────
fn gpg_encrypt(plain: &str) -> std::io::Result<String> {
    use std::io::Write;
    let mut child = std::process::Command::new("gpg")
        .args(["--batch", "--armor", "--symmetric", "--pinentry-mode", "loopback", "--passphrase-fd", "0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    // Prompt user for passphrase via tty (no echo). Use rpassword-free
    // approach: read from /dev/tty with stty -echo. Quick hack: ask via
    // gpg's own pinentry by NOT using --batch — but that needs a TTY.
    // For now we reuse our own footer ask which does NOT mask input.
    // The user can configure ~/.gnupg/gpg-agent.conf to cache.
    let pass = read_passphrase_tty()?;
    {
        let stdin = child.stdin.as_mut().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no stdin"))?;
        writeln!(stdin, "{}", pass)?;
        stdin.write_all(plain.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other,
            format!("gpg exit: {}", String::from_utf8_lossy(&out.stderr))));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn gpg_decrypt(armored: &str) -> std::io::Result<String> {
    use std::io::Write;
    let pass = read_passphrase_tty()?;
    let mut child = std::process::Command::new("gpg")
        .args(["--batch", "--decrypt", "--pinentry-mode", "loopback", "--passphrase-fd", "0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        let stdin = child.stdin.as_mut().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no stdin"))?;
        writeln!(stdin, "{}", pass)?;
        stdin.write_all(armored.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other,
            format!("gpg exit: {}", String::from_utf8_lossy(&out.stderr))));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Read a passphrase from /dev/tty with echo disabled. Falls back to
/// reading without disabling echo if stty isn't available.
fn read_passphrase_tty() -> std::io::Result<String> {
    use std::io::{BufRead, BufReader, Write as _};
    // Disable echo via stty.
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let mut tty = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    write!(tty, "\nPassphrase: ")?;
    tty.flush().ok();
    let mut reader = BufReader::new(&tty);
    let mut pass = String::new();
    reader.read_line(&mut pass)?;
    let _ = std::process::Command::new("stty").arg("echo").status();
    writeln!(&tty)?;
    Ok(pass.trim_end_matches('\n').to_string())
}

// ── Calendar helpers ───────────────────────────────────────────────────
fn scan_future_date(text: &str) -> Option<(i64, u32, u32)> {
    // Match Nordic DD.MM.YYYY, ISO YYYY-MM-DD, EU DD/MM/YYYY.
    let bytes = text.as_bytes();
    let today = today();
    let (ty, tm, td) = parse_iso(&today)?;
    let mut i = 0;
    while i < bytes.len() {
        // Try ISO YYYY-MM-DD
        if i + 9 < bytes.len() && bytes[i].is_ascii_digit() {
            if let Some(d) = try_iso_at(text, i) { if is_future(d, (ty, tm, td)) { return Some(d); } }
            if let Some(d) = try_nordic_at(text, i) { if is_future(d, (ty, tm, td)) { return Some(d); } }
            if let Some(d) = try_eu_at(text, i) { if is_future(d, (ty, tm, td)) { return Some(d); } }
        }
        i += 1;
    }
    None
}

fn try_iso_at(s: &str, i: usize) -> Option<(i64, u32, u32)> {
    let b = s.as_bytes();
    if i + 9 >= b.len() { return None; }
    let yr: String = b[i..i+4].iter().map(|&c| c as char).collect();
    if b[i+4] != b'-' || b[i+7] != b'-' { return None; }
    let mo: String = b[i+5..i+7].iter().map(|&c| c as char).collect();
    let da: String = b[i+8..i+10].iter().map(|&c| c as char).collect();
    Some((yr.parse().ok()?, mo.parse().ok()?, da.parse().ok()?))
}

fn try_nordic_at(s: &str, i: usize) -> Option<(i64, u32, u32)> {
    let b = s.as_bytes();
    if i + 9 >= b.len() { return None; }
    if b[i+2] != b'.' || b[i+5] != b'.' { return None; }
    let da: String = b[i..i+2].iter().map(|&c| c as char).collect();
    let mo: String = b[i+3..i+5].iter().map(|&c| c as char).collect();
    let yr: String = b[i+6..i+10].iter().map(|&c| c as char).collect();
    Some((yr.parse().ok()?, mo.parse().ok()?, da.parse().ok()?))
}

fn try_eu_at(s: &str, i: usize) -> Option<(i64, u32, u32)> {
    let b = s.as_bytes();
    if i + 9 >= b.len() { return None; }
    if b[i+2] != b'/' || b[i+5] != b'/' { return None; }
    let da: String = b[i..i+2].iter().map(|&c| c as char).collect();
    let mo: String = b[i+3..i+5].iter().map(|&c| c as char).collect();
    let yr: String = b[i+6..i+10].iter().map(|&c| c as char).collect();
    Some((yr.parse().ok()?, mo.parse().ok()?, da.parse().ok()?))
}

fn parse_iso(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 { return None; }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

fn is_future(d: (i64, u32, u32), today: (i64, u32, u32)) -> bool {
    d > today
}

fn build_ics(y: i64, m: u32, d: u32, summary: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//hyper//hyperlist//EN\r\n\
        BEGIN:VEVENT\r\nUID:hyper-{:04}{:02}{:02}-{}@local\r\n\
        DTSTART;VALUE=DATE:{:04}{:02}{:02}\r\n\
        SUMMARY:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        y, m, d, summary.chars().take(8).collect::<String>().replace(' ', "_"),
        y, m, d, summary.replace(',', "\\,").replace(';', "\\;").lines().next().unwrap_or("")
    )
}

fn stem_of(p: Option<&std::path::PathBuf>) -> String {
    p.and_then(|p| p.file_stem().and_then(|s| s.to_str()))
        .unwrap_or("hyperlist").to_string()
}

/// Expand a leading `~` or `~/` to the user's home directory. Used by
/// `<file:~/foo>` references.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, rest);
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") { return home; }
    }
    path.to_string()
}

fn truncate_display(s: &str, max_w: usize) -> String {
    if max_w == 0 { return String::new(); }
    if crust::display_width(s) <= max_w { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut w = 0usize;
    for c in s.chars() {
        let cw = crust::display_width(&c.to_string());
        if w + cw + 1 > max_w { break; }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}
