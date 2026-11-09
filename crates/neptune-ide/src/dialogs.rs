//! The command palette and every dialog box.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Align2, Color32, FontId, Id, Key, Layout, Margin, Order, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Ui, Vec2};

use crate::actions::{self, Action};
use crate::app::{App, Palette};
use crate::procs;
use crate::theme::{Theme, THEMES};
use crate::ui_util::{self, fuzzy, paint_icon, primary_button, row, Icon};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    Open,
    Folder,
    SaveAs(u64),
}

pub struct FileDialog {
    pub mode: FileMode,
    pub dir: PathBuf,
    pub dir_text: String,
    pub name: String,
    pub all_files: bool,
    pub entries: Vec<(PathBuf, bool)>,
    pub selected: Option<PathBuf>,
    pub overwrite: Option<PathBuf>,
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map_or_else(|| PathBuf::from("."), PathBuf::from)
}

impl FileDialog {
    pub fn new(mode: FileMode, start: Option<PathBuf>, name: &str) -> Self {
        let dir = start.filter(|p| p.is_dir()).unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| home()));
        let mut d = FileDialog { mode, dir: PathBuf::new(), dir_text: String::new(), name: name.to_string(), all_files: false, entries: Vec::new(), selected: None, overwrite: None };
        d.enter(dir);
        d
    }

    fn enter(&mut self, dir: PathBuf) {
        self.dir_text = dir.display().to_string();
        self.dir = dir;
        self.selected = None;
        self.reload();
    }

    fn reload(&mut self) {
        let mut list: Vec<(PathBuf, bool)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| (e.path(), e.path().is_dir()))
            .filter(|(p, d)| {
                let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                !name.starts_with('.') && (*d || (self.mode != FileMode::Folder && (self.all_files || procs::is_source(p))))
            })
            .collect();
        list.sort_by_key(|(p, d)| (!*d, p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
        self.entries = list;
    }
}

fn drives() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'A'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).filter(|p| p.exists()).collect()
    } else {
        vec![PathBuf::from("/")]
    }
}

pub enum PromptKind {
    CommandLine,
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
}

pub enum Dialog {
    File(FileDialog),
    CloseUnsaved { pane: usize, index: usize, id: u64 },
    Reload(u64),
    Prompt { title: String, value: String, kind: PromptKind, focus: bool },
    Delete(PathBuf),
    Build { out: String, console: bool },
    Settings,
    Shortcuts { filter: String },
    About,
}

enum Choice {
    Act(Action),
    Open(PathBuf),
    Line(usize),
    Symbol(usize),
}

fn dim_background(ctx: &egui::Context, id: &str, th: &Theme) -> bool {
    let screen = ctx.content_rect();
    let mut clicked = false;
    egui::Area::new(Id::unique(id)).order(Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let resp = ui.allocate_rect(screen, Sense::click());
        ui.painter().rect_filled(screen, 0.0, if th.dark { Color32::from_black_alpha(110) } else { Color32::from_black_alpha(60) });
        clicked = resp.clicked();
    });
    clicked
}

fn window(ctx: &egui::Context, th: &Theme, title: &str, size: Vec2, add: impl FnOnce(&mut Ui)) {
    let screen = ctx.content_rect();
    let pos = screen.center() - size * 0.5;
    egui::Area::new(Id::unique(("dialog", title))).order(Order::Foreground).fixed_pos(Pos2::new(pos.x, pos.y.max(30.0))).show(ctx, |ui| {
        egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.line)).show(ui, |ui| {
            ui.set_width(size.x);
            let (bar, _) = ui.allocate_exact_size(Vec2::new(size.x, 30.0), Sense::hover());
            ui.painter().rect_filled(bar, 0.0, th.rail);
            ui.painter().rect_filled(Rect::from_min_size(bar.min, Vec2::new(size.x, 2.0)), 0.0, th.accent);
            ui.painter().text(Pos2::new(bar.left() + 12.0, bar.center().y + 1.0), Align2::LEFT_CENTER, title, FontId::proportional(13.5), th.text);
            egui::Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
                ui.set_width(size.x - 28.0);
                add(ui);
            });
        });
    });
}

impl App {
    pub fn show_palette(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        let th = self.th;
        if dim_background(ctx, "palette-back", th) {
            self.palette.open = false;
            self.focus_editor = true;
            return;
        }
        let q = self.palette.query.clone();
        let mut items: Vec<(u32, String, String, Choice)> = Vec::new();
        if let Some(rest) = q.strip_prefix('>') {
            let rest = rest.trim();
            for i in actions::ALL {
                let full = format!("{}: {}", i.group, i.title);
                if self.action_enabled(i.action) {
                    if let Some(s) = fuzzy(rest, i.title).or_else(|| fuzzy(rest, &full)) {
                        items.push((s, full, actions::shortcut_text(i.action), Choice::Act(i.action)));
                    }
                }
            }
        } else if let Some(rest) = q.strip_prefix(':') {
            if let Some(d) = self.active() {
                let total = d.buf.line_count();
                let target = rest.trim().parse::<usize>().ok().filter(|n| *n >= 1);
                match target {
                    Some(n) => items.push((0, format!("Go to line {}", n.min(total)), format!("of {total}"), Choice::Line(n.min(total) - 1))),
                    None => items.push((0, format!("Type a line number from 1 to {total}"), String::new(), Choice::Line(d.buf.primary().head.line))),
                }
            }
        } else if let Some(rest) = q.strip_prefix('@') {
            if let Some(d) = self.active() {
                for x in &d.decls {
                    if let Some(s) = fuzzy(rest.trim(), &x.name) {
                        items.push((s, x.name.clone(), format!("line {}", x.line + 1), Choice::Symbol(x.line)));
                    }
                }
            }
        } else {
            let root = self.folder.clone();
            let label = |p: &Path| root.as_deref().and_then(|r| p.strip_prefix(r).ok()).unwrap_or(p).display().to_string();
            let mut seen = std::collections::HashSet::new();
            let open: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
            let recent: Vec<PathBuf> = self.recent.iter().map(PathBuf::from).collect();
            for p in open.iter().chain(recent.iter()) {
                let shown = label(p);
                if let Some(s) = fuzzy(&q, &shown) {
                    if seen.insert(p.clone()) {
                        items.push((s.saturating_sub(1), shown, "recent".into(), Choice::Open(p.clone())));
                    }
                }
            }
            for p in self.file_index.iter().filter(|p| procs::is_source(p)) {
                if seen.contains(p) {
                    continue;
                }
                let shown = label(p);
                let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                if let Some(s) = fuzzy(&q, &name).or_else(|| fuzzy(&q, &shown).map(|s| s + 50)) {
                    items.push((s, shown, String::new(), Choice::Open(p.clone())));
                }
            }
            if items.is_empty() && q.is_empty() {
                items.push((0, "Type > for commands, @ for symbols, : for a line number".into(), String::new(), Choice::Act(Action::Palette)));
            }
        }
        items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())));
        items.truncate(120);
        let n = items.len();
        if n > 0 {
            self.palette.selected = self.palette.selected.min(n - 1);
        }
        let (mut close, mut chosen) = (false, None);
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown) && n > 0 {
                self.palette.selected = (self.palette.selected + 1) % n;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp) && n > 0 {
                self.palette.selected = (self.palette.selected + n - 1) % n;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                close = true;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Enter) {
                chosen = Some(self.palette.selected);
            }
        });
        let screen = ctx.content_rect();
        let width = 620.0f32.min(screen.width() - 40.0);
        let pos = Pos2::new(screen.center().x - width * 0.5, screen.top() + 70.0);
        let selected = self.palette.selected;
        egui::Area::new(Id::unique("palette")).order(Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.accent)).inner_margin(Margin::same(6)).show(ui, |ui| {
                ui.set_width(width);
                let r = ui.add(TextEdit::singleline(&mut self.palette.query).id(Id::unique("palette-box")).hint_text("Search files. > commands, @ symbols, : line").desired_width(f32::INFINITY).font(FontId::proportional(15.0)));
                if self.palette.focus {
                    r.request_focus();
                    // the caret goes after any prefix already typed in
                    if let Some(mut state) = TextEdit::load_state(ctx, r.id) {
                        let end = egui::text::CCursor::new(self.palette.query.chars().count());
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(ctx, r.id);
                    }
                    self.palette.focus = false;
                }
                if r.changed() {
                    self.palette.selected = 0;
                }
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, (_, title, right, _)) in items.iter().enumerate() {
                        let (r, resp) = row(ui, th, 26.0, i == selected);
                        if i == selected {
                            resp.scroll_to_me(None);
                            ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(2.0, r.height())), 0.0, th.accent);
                        }
                        ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, title, FontId::proportional(13.5), th.text);
                        ui.painter().text(Pos2::new(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, right, FontId::proportional(12.0), th.dim);
                        if resp.clicked() {
                            chosen = Some(i);
                        }
                    }
                });
            });
        });
        if let Some(i) = chosen {
            if let Some((_, _, _, c)) = items.into_iter().nth(i) {
                self.palette = Palette { open: false, query: String::new(), selected: 0, focus: false };
                self.focus_editor = true;
                match c {
                    Choice::Act(Action::Palette) => {}
                    Choice::Act(a) => self.queue.push(a),
                    Choice::Open(p) => {
                        self.open_path(&p);
                    }
                    Choice::Line(l) => self.goto_pos_in_active(l, 0),
                    Choice::Symbol(l) => self.goto_pos_in_active(l, 0),
                }
            }
        } else if close {
            self.palette.open = false;
            self.focus_editor = true;
        }
    }

    pub fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let th = self.th;
        dim_background(ctx, "dialog-back", th);
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        let keep = match &mut dialog {
            Dialog::File(f) => self.file_dialog(ctx, f, esc),
            Dialog::CloseUnsaved { pane, index, id } => self.close_unsaved(ctx, *pane, *index, *id, esc),
            Dialog::Reload(id) => self.reload_prompt(ctx, *id, esc),
            Dialog::Prompt { title, value, kind, focus } => self.prompt(ctx, title, value, kind, focus, esc),
            Dialog::Delete(p) => self.delete_prompt(ctx, p, esc),
            Dialog::Build { out, console } => self.build_dialog(ctx, out, console, esc),
            Dialog::Settings => self.settings_dialog(ctx, esc),
            Dialog::Shortcuts { filter } => self.shortcuts_dialog(ctx, filter, esc),
            Dialog::About => self.about_dialog(ctx, esc),
        };
        if keep && self.dialog.is_none() {
            self.dialog = Some(dialog);
        }
        if !keep {
            self.focus_editor = true;
        }
    }

    fn file_dialog(&mut self, ctx: &egui::Context, f: &mut FileDialog, esc: bool) -> bool {
        let th = self.th;
        let title = match f.mode {
            FileMode::Open => "Open File",
            FileMode::Folder => "Open Folder",
            FileMode::SaveAs(_) => "Save As",
        };
        let (mut result, mut cancel, mut go_to) = (None, esc, None);
        window(ctx, th, title, Vec2::new(680.0, 500.0), |ui| {
            ui.horizontal(|ui| {
                if ui_util::icon_button(ui, th, Icon::Undo, "Up one folder", f.dir.parent().is_some(), false).clicked() {
                    go_to = f.dir.parent().map(Path::to_path_buf);
                }
                if ui_util::icon_button(ui, th, Icon::Folder, "Home folder", true, false).clicked() {
                    go_to = Some(home());
                }
                for d in drives() {
                    let label = d.display().to_string();
                    if ui.small_button(label.trim_end_matches('\\')).clicked() {
                        go_to = Some(d);
                    }
                }
                let r = ui.add(TextEdit::singleline(&mut f.dir_text).desired_width(f32::INFINITY));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && Path::new(&f.dir_text).is_dir() {
                    go_to = Some(PathBuf::from(&f.dir_text));
                }
            });
            ui.add_space(6.0);
            egui::Frame::new().fill(th.bg).stroke(Stroke::new(1.0, th.line)).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, false]).max_height(340.0).show_rows(ui, 23.0, f.entries.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        let (p, is_dir) = f.entries[i].clone();
                        let (r, resp) = row(ui, th, 23.0, f.selected.as_ref() == Some(&p));
                        paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 15.0, r.center().y), Vec2::splat(14.0)), if is_dir { Icon::Folder } else { Icon::File }, if is_dir { th.warning } else { th.accent });
                        let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        ui.painter().text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.5), th.text);
                        if resp.double_clicked() {
                            if is_dir {
                                go_to = Some(p.clone());
                            } else {
                                result = Some(p.clone());
                            }
                        } else if resp.clicked() {
                            f.selected = Some(p.clone());
                            if !is_dir {
                                f.name = name;
                            }
                        }
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if f.mode != FileMode::Folder {
                    ui.label("Name");
                    let r = ui.add(TextEdit::singleline(&mut f.name).desired_width(ui.available_width() - 250.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !f.name.is_empty() {
                        result = Some(f.dir.join(&f.name));
                    }
                    if ui.checkbox(&mut f.all_files, "All files").changed() {
                        f.reload();
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ok = match f.mode {
                        FileMode::Open => "Open",
                        FileMode::Folder => "Use This Folder",
                        FileMode::SaveAs(_) => "Save",
                    };
                    if primary_button(ui, th, ok).clicked() {
                        match f.mode {
                            FileMode::Folder => result = Some(f.selected.clone().filter(|p| p.is_dir()).unwrap_or_else(|| f.dir.clone())),
                            _ if !f.name.is_empty() => result = Some(f.dir.join(&f.name)),
                            _ => {}
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if let Some(d) = go_to {
            f.enter(d);
        }
        if let Some(p) = f.overwrite.clone() {
            let mut answer = None;
            let area_size = Vec2::new(420.0, 110.0);
            let screen = ctx.content_rect();
            egui::Area::new(Id::unique("overwrite")).order(Order::Tooltip).fixed_pos(screen.center() - area_size * 0.5).show(ctx, |ui| {
                egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.warning)).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(area_size.x);
                    ui.label(format!("{} already exists. Replace it?", p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, th, "Replace").clicked() {
                            answer = Some(true);
                        }
                        if ui.button("Keep It").clicked() {
                            answer = Some(false);
                        }
                    });
                });
            });
            match answer {
                Some(true) => {
                    if let FileMode::SaveAs(id) = f.mode {
                        self.finish_save_as(id, p);
                    }
                    return false;
                }
                Some(false) => f.overwrite = None,
                None => {}
            }
            return true;
        }
        if let Some(p) = result {
            match f.mode {
                FileMode::Open => {
                    self.open_path(&p);
                }
                FileMode::Folder => self.set_folder(p),
                FileMode::SaveAs(id) => {
                    let p = if p.extension().is_none() { p.with_extension("nb") } else { p };
                    let same = self.doc(id).and_then(|d| d.path.as_ref()) == Some(&p);
                    if p.exists() && !same {
                        f.overwrite = Some(p);
                        return true;
                    }
                    self.finish_save_as(id, p);
                }
            }
            return false;
        }
        !cancel
    }

    fn close_unsaved(&mut self, ctx: &egui::Context, pane: usize, index: usize, id: u64, esc: bool) -> bool {
        let th = self.th;
        let name = self.doc(id).map_or(String::new(), |d| d.title());
        let mut answer = if esc { Some(0) } else { None };
        window(ctx, th, "Unsaved Changes", Vec2::new(430.0, 130.0), |ui| {
            ui.label(format!("Save your changes to {name}?"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_button(ui, th, "Save").clicked() {
                    answer = Some(1);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(2);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(0);
                }
            });
        });
        match answer {
            Some(1) => {
                if self.save(id) {
                    self.close_tab(pane, index);
                    false
                } else {
                    // no name yet: the Save As box takes over
                    self.dialog.is_some()
                }
            }
            Some(2) => {
                self.close_tab(pane, index);
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    fn reload_prompt(&mut self, ctx: &egui::Context, id: u64, esc: bool) -> bool {
        let th = self.th;
        let name = self.doc(id).map_or(String::new(), |d| d.title());
        let mut answer = if esc { Some(false) } else { None };
        window(ctx, th, "File Changed on Disk", Vec2::new(470.0, 130.0), |ui| {
            ui.label(format!("{name} was changed by another program, and you have unsaved edits here."));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Load Version on Disk").clicked() {
                    answer = Some(true);
                }
                if primary_button(ui, th, "Keep My Edits").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if let Some(d) = self.doc_mut(id) {
                    let _ = d.reload();
                }
                false
            }
            Some(false) => false,
            None => true,
        }
    }

    fn prompt(&mut self, ctx: &egui::Context, title: &str, value: &mut String, kind: &PromptKind, focus: &mut bool, esc: bool) -> bool {
        let th = self.th;
        let (mut ok, mut cancel) = (false, esc);
        window(ctx, th, title, Vec2::new(460.0, 120.0), |ui| {
            let r = ui.add(TextEdit::singleline(value).desired_width(f32::INFINITY));
            if *focus {
                r.request_focus();
                *focus = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                ok = true;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_button(ui, th, "OK").clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if ok {
            let v = value.trim().to_string();
            match kind {
                PromptKind::CommandLine => self.command_line = v,
                PromptKind::NewFile(dir) if !v.is_empty() => {
                    let p = dir.join(&v);
                    if !p.exists() {
                        if let Err(e) = std::fs::write(&p, "") {
                            self.say(format!("Cannot create {v}: {e}"), true);
                        }
                    }
                    self.tree_cache.clear();
                    self.open_path(&p);
                    if let Some(f) = self.folder.clone() {
                        procs::index_files(f, self.post.clone());
                    }
                }
                PromptKind::NewFolder(dir) if !v.is_empty() => {
                    if let Err(e) = std::fs::create_dir_all(dir.join(&v)) {
                        self.say(format!("Cannot create {v}: {e}"), true);
                    }
                    self.tree_cache.clear();
                }
                PromptKind::Rename(old) if !v.is_empty() => {
                    let new = old.with_file_name(&v);
                    match std::fs::rename(old, &new) {
                        Ok(()) => {
                            for d in &mut self.docs {
                                if d.path.as_ref() == Some(old) {
                                    d.path = Some(new.clone());
                                    d.lsp_open = false;
                                }
                            }
                        }
                        Err(e) => self.say(format!("Cannot rename: {e}"), true),
                    }
                    self.tree_cache.clear();
                }
                _ => {}
            }
            return false;
        }
        !cancel
    }

    fn delete_prompt(&mut self, ctx: &egui::Context, p: &Path, esc: bool) -> bool {
        let th = self.th;
        let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let mut answer = if esc { Some(false) } else { None };
        let what = if p.is_dir() { "this folder and everything in it" } else { "this file" };
        window(ctx, th, "Delete", Vec2::new(440.0, 130.0), |ui| {
            ui.label(format!("Permanently delete {what}?"));
            ui.label(RichText::new(&name).color(th.warning));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(th.error)).clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                let res = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
                if let Err(e) = res {
                    self.say(format!("Cannot delete {name}: {e}"), true);
                }
                self.tree_cache.clear();
                false
            }
            Some(false) => false,
            None => true,
        }
    }

    fn build_dialog(&mut self, ctx: &egui::Context, out: &mut String, console: &mut bool, esc: bool) -> bool {
        let th = self.th;
        let (mut ok, mut cancel) = (false, esc);
        window(ctx, th, "Create Executable", Vec2::new(500.0, 170.0), |ui| {
            ui.label("Output folder (empty: dist/<program name> beside the program)");
            ui.add(TextEdit::singleline(out).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.checkbox(console, "Keep a console window (Windows)");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_button(ui, th, "Build").clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if ok {
            self.build(out.clone(), *console);
            return false;
        }
        !cancel
    }

    fn settings_dialog(&mut self, ctx: &egui::Context, esc: bool) -> bool {
        let th = self.th;
        let mut close = esc;
        let defaults = crate::settings::Settings::default();
        window(ctx, th, "Settings", Vec2::new(620.0, 540.0), |ui| {
            egui::ScrollArea::vertical().max_height(450.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(RichText::new("THEME").size(11.0).strong().color(th.dim));
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    for t in THEMES {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(138.0, 64.0), Sense::click());
                        ui.painter().rect_filled(r, 0.0, t.bg);
                        ui.painter().rect_stroke(r, 0.0, Stroke::new(if self.st.theme == t.name { 2.0 } else { 1.0 }, if self.st.theme == t.name { th.accent } else { th.line }), egui::StrokeKind::Inside);
                        for (i, c) in [t.keyword, t.command, t.string, t.number].iter().enumerate() {
                            let y = r.top() + 10.0 + i as f32 * 7.0;
                            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left() + 10.0 + i as f32 * 8.0, y), Vec2::new(48.0 - i as f32 * 6.0, 3.0)), 0.0, *c);
                        }
                        ui.painter().text(Pos2::new(r.left() + 10.0, r.bottom() - 11.0), Align2::LEFT_CENTER, t.name, FontId::proportional(12.0), t.text);
                        if resp.clicked() {
                            self.st.theme = t.name.to_string();
                        }
                    }
                });
                ui.add_space(12.0);
                egui::Grid::new("settings-grid").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                    ui.label("Editor font size");
                    ui.add(egui::Slider::new(&mut self.st.font_size, 8.0..=40.0).step_by(1.0));
                    ui.end_row();
                    ui.label("Interface scale");
                    ui.add(egui::Slider::new(&mut self.st.ui_scale, 0.7..=2.5).step_by(0.05));
                    ui.end_row();
                    ui.label("Tab width");
                    ui.add(egui::Slider::new(&mut self.st.tab_size, 1..=16));
                    ui.end_row();
                    ui.label("neptune program");
                    ui.add(TextEdit::singleline(&mut self.st.neptune_path).hint_text("found automatically").desired_width(280.0));
                    ui.end_row();
                });
                ui.add_space(12.0);
                ui.columns(2, |c| {
                    c[0].label(RichText::new("EDITING").size(11.0).strong().color(th.dim));
                    c[0].checkbox(&mut self.st.autocap, "Capitalise keywords and commands");
                    c[0].checkbox(&mut self.st.auto_indent, "Indent new lines by block");
                    c[0].checkbox(&mut self.st.auto_pairs, "Close brackets and quotes");
                    c[0].checkbox(&mut self.st.completion, "Suggest as you type");
                    c[0].checkbox(&mut self.st.use_spaces, "Indent with spaces");
                    c[0].checkbox(&mut self.st.trim_on_save, "Trim trailing spaces on save");
                    c[0].checkbox(&mut self.st.save_before_run, "Save all files before running");
                    c[1].label(RichText::new("DISPLAY").size(11.0).strong().color(th.dim));
                    c[1].checkbox(&mut self.st.minimap, "Minimap");
                    c[1].checkbox(&mut self.st.indent_guides, "Indent guides");
                    c[1].checkbox(&mut self.st.whitespace, "Show whitespace");
                    c[1].checkbox(&mut self.st.inline_errors, "Show errors at the end of the line");
                    c[1].checkbox(&mut self.st.caret_blink, "Blinking caret");
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_button(ui, th, "Done").clicked() {
                    close = true;
                }
                if ui.button("Restore Defaults").clicked() {
                    self.st = defaults;
                }
            });
        });
        if close && self.lsp.is_none() {
            self.lsp_retry = Some(std::time::Instant::now());
        }
        !close
    }

    fn shortcuts_dialog(&mut self, ctx: &egui::Context, filter: &mut String, esc: bool) -> bool {
        let th = self.th;
        let mut close = esc;
        window(ctx, th, "Keyboard Shortcuts", Vec2::new(560.0, 560.0), |ui| {
            ui.add(TextEdit::singleline(filter).hint_text("Filter").desired_width(f32::INFINITY));
            ui.add_space(6.0);
            let f = filter.to_lowercase();
            egui::ScrollArea::vertical().max_height(450.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in actions::ALL.iter().filter(|i| i.keys.is_some() && (f.is_empty() || i.title.to_lowercase().contains(&f))) {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 23.0), Sense::hover());
                    ui.painter().text(Pos2::new(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, i.title, FontId::proportional(13.0), th.text);
                    ui.painter().text(Pos2::new(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, actions::shortcut_text(i.action), FontId::monospace(12.0), th.accent);
                }
            });
            ui.add_space(6.0);
            if primary_button(ui, th, "Close").clicked() {
                close = true;
            }
        });
        !close
    }

    fn about_dialog(&mut self, ctx: &egui::Context, esc: bool) -> bool {
        let th = self.th;
        let mut close = esc;
        window(ctx, th, "About", Vec2::new(420.0, 170.0), |ui| {
            ui.label(RichText::new("Neptune IDE").size(20.0).strong());
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label(RichText::new("The editor for NeptuneBASIC, in the spirit of the Blitz3D IDE.").color(th.dim));
            ui.add_space(10.0);
            if primary_button(ui, th, "Close").clicked() {
                close = true;
            }
        });
        !close
    }
}
