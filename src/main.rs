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
            _ => {}
        }
    }
    Crust::cleanup();
    Crust::clear_screen();
}

#[derive(Copy, Clone)]
enum FilterMode { Show, Hide }

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
