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
            "W" => {
                if let Err(e) = app.save() { app.footer_say(&format!(" Save failed: {}", e), 196); }
                else { app.footer_say(" Saved", 46); }
                app.render_footer();
            }
            "o" => { app.open_file_prompt(); app.render_all(); }
            _ => {}
        }
    }
    Crust::cleanup();
    Crust::clear_screen();
}

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
        let info = format!(" hyper v{}  {}{}  ({} items)",
            VERSION, name, dirty, self.doc.items.len());
        self.header.say(&style::bold(&info));
    }

    fn render_footer(&mut self) {
        if let Some((ref msg, color)) = self.status {
            self.footer.say(&style::fg(msg, color));
        } else {
            let hint = " j/k:Move  h/l:Parent/Child  SPACE:Fold  1-9:Level  z/Z:CollapseAll/ExpandAll  o:Open  W:Save  ?:Help  q:Quit";
            self.footer.say(&style::fg(hint, 245));
        }
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
            let styled = if vis_i == self.visible_idx {
                format!("\x1b[48;5;{}m{}\x1b[0m", 236, row)
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
            out.push(i);
            if self.doc.items[i].folded && self.has_children(i) {
                skip_until = Some(last_descendant(&self.doc.items, i));
            }
        }
        out
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

    fn show_help(&mut self) {
        let help = "\n  \
            hyper — HyperList terminal viewer\n\n  \
            KEYS\n  \
              j / DOWN       Move down (visible items)\n  \
              k / UP         Move up\n  \
              h / LEFT       Jump to parent\n  \
              l / RIGHT      Jump to first child (unfolds)\n  \
              PgUP / PgDOWN  Page\n  \
              g / HOME       First item\n  \
              G / END        Last item\n  \
              SPACE          Toggle fold on current\n  \
              1..9           Fold all at level N\n  \
              z / Z          Collapse / expand all\n  \
              o              Open a .hl file\n  \
              W              Save current file\n  \
              ? / q          Help / Quit\n\n  \
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
