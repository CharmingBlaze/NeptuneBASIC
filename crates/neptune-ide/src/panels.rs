//! The side bar, the command reference and the bottom dock.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2};

use crate::actions::Action;
use crate::app::{App, DockTab, SideTab, Var};
use crate::buffer::Pos;
use crate::dialogs::{Dialog, PromptKind};
use crate::lang::DeclKind;
use crate::theme::Theme;
use crate::ui_util::{self, icon_button, paint_icon, panel_title, row, Icon};

const ROW: f32 = 22.0;

fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn prop(size: f32) -> FontId {
    FontId::proportional(size)
}

fn header(ui: &mut Ui, th: &Theme, title: &str, add: impl FnOnce(&mut Ui)) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let mut c = ui.new_child(UiBuilder::new().max_rect(r.shrink2(Vec2::new(8.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    panel_title(&mut c, th, title);
    c.with_layout(Layout::right_to_left(Align::Center), add);
}

impl App {
    pub fn side_panel(&mut self, ui: &mut Ui) {
        match self.side {
            SideTab::Explorer => self.explorer(ui),
            SideTab::Outline => self.outline(ui),
            SideTab::Search => self.search_panel(ui),
        }
    }

    fn entries(&mut self, dir: &Path) -> Vec<(PathBuf, bool)> {
        if let Some(e) = self.tree_cache.get(dir) {
            return e.clone();
        }
        let mut list: Vec<(PathBuf, bool)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                !n.starts_with('.') && n != "target"
            })
            .map(|e| (e.path(), e.path().is_dir()))
            .collect();
        list.sort_by_key(|(p, d)| (!*d, p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
        self.tree_cache.insert(dir.to_path_buf(), list.clone());
        list
    }

    fn flatten(&mut self, dir: &Path, depth: usize, out: &mut Vec<(PathBuf, bool, usize)>) {
        for (p, is_dir) in self.entries(dir) {
            let open = is_dir && self.tree_open.contains(&p);
            out.push((p.clone(), is_dir, depth));
            if open {
                self.flatten(&p, depth + 1, out);
            }
        }
    }

    fn explorer(&mut self, ui: &mut Ui) {
        let th = self.th;
        let Some(root) = self.folder.clone() else {
            header(ui, th, "Files", |_| {});
            egui::Frame::new().inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.label(RichText::new("No folder is open.").color(th.dim));
                ui.add_space(6.0);
                if ui_util::primary_button(ui, th, "Open Folder").clicked() {
                    self.queue.push(Action::OpenFolder);
                }
            });
            return;
        };
        if self.tree_time.elapsed().as_secs() >= 3 {
            self.tree_time = std::time::Instant::now();
            self.tree_cache.clear();
        }
        let name = root.file_name().map_or(root.display().to_string(), |n| n.to_string_lossy().into_owned());
        let mut new_file = false;
        let mut new_dir = false;
        let mut refresh = false;
        header(ui, th, &name, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            refresh = icon_button(ui, th, Icon::Refresh, "Refresh", true, false).clicked();
            new_dir = icon_button(ui, th, Icon::Folder, "New Folder", true, false).clicked();
            new_file = icon_button(ui, th, Icon::Plus, "New File", true, false).clicked();
        });
        if refresh {
            self.tree_cache.clear();
            crate::procs::index_files(root.clone(), self.post.clone());
        }
        let target = self.tree_selected.as_ref().map(|p| if p.is_dir() { p.clone() } else { p.parent().unwrap_or(&root).to_path_buf() }).unwrap_or_else(|| root.clone());
        if new_file {
            self.dialog = Some(Dialog::Prompt { title: "New file name".into(), value: "program.nb".into(), kind: PromptKind::NewFile(target.clone()), focus: true });
        }
        if new_dir {
            self.dialog = Some(Dialog::Prompt { title: "New folder name".into(), value: String::new(), kind: PromptKind::NewFolder(target), focus: true });
        }
        let mut rows = Vec::new();
        self.flatten(&root, 0, &mut rows);
        let active_path = self.active().and_then(|d| d.path.clone());
        let mut open = None;
        let mut toggle = None;
        let mut dialog = None;
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("tree").auto_shrink([false, false]).show_rows(ui, ROW, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (p, is_dir, depth) in &rows[range] {
                let selected = active_path.as_ref() == Some(p);
                let (r, resp) = row(ui, th, ROW, selected);
                let x = r.left() + 8.0 + *depth as f32 * 14.0;
                let y = r.center().y;
                if *is_dir {
                    let ic = if self.tree_open.contains(p) { Icon::Down } else { Icon::Right };
                    paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(x + 5.0, y), Vec2::splat(11.0)), ic, th.dim);
                }
                let source = crate::procs::is_source(p);
                let icon = if *is_dir { Icon::Folder } else { Icon::File };
                paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(x + 19.0, y), Vec2::splat(13.0)), icon, if *is_dir { th.warning } else if source { th.accent } else { th.dim });
                let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                ui.painter().text(Pos2::new(x + 31.0, y), Align2::LEFT_CENTER, &name, prop(13.0), if selected || source || *is_dir { th.text } else { th.dim });
                if resp.clicked() {
                    pick = Some(p.clone());
                    if *is_dir {
                        toggle = Some(p.clone());
                    } else {
                        open = Some(p.clone());
                    }
                }
                resp.context_menu(|ui| {
                    ui.set_min_width(200.0);
                    let dir = if *is_dir { p.clone() } else { p.parent().unwrap_or(&root).to_path_buf() };
                    if ui_util::check_item(ui, "New File...", "", None, true) {
                        dialog = Some(Dialog::Prompt { title: "New file name".into(), value: "program.nb".into(), kind: PromptKind::NewFile(dir.clone()), focus: true });
                    }
                    if ui_util::check_item(ui, "New Folder...", "", None, true) {
                        dialog = Some(Dialog::Prompt { title: "New folder name".into(), value: String::new(), kind: PromptKind::NewFolder(dir), focus: true });
                    }
                    ui.separator();
                    if ui_util::check_item(ui, "Rename...", "", None, true) {
                        dialog = Some(Dialog::Prompt { title: "Rename to".into(), value: name.clone(), kind: PromptKind::Rename(p.clone()), focus: true });
                    }
                    if ui_util::check_item(ui, "Delete...", "", None, true) {
                        dialog = Some(Dialog::Delete(p.clone()));
                    }
                    ui.separator();
                    if ui_util::check_item(ui, "Copy Path", "", None, true) {
                        ui.ctx().copy_text(p.display().to_string());
                    }
                });
            }
        });
        if dialog.is_some() {
            self.dialog = dialog;
        }
        if pick.is_some() {
            self.tree_selected = pick;
        }
        if let Some(p) = toggle {
            if !self.tree_open.remove(&p) {
                self.tree_open.insert(p);
            }
        }
        if let Some(p) = open {
            self.open_path(&p);
        }
    }

    fn outline(&mut self, ui: &mut Ui) {
        let th = self.th;
        header(ui, th, "Outline", |_| {});
        egui::Frame::new().inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
            ui.add(TextEdit::singleline(&mut self.outline_filter).hint_text("Filter").desired_width(f32::INFINITY));
        });
        ui.add_space(4.0);
        let Some(d) = self.active() else {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Open a program to see its functions.").color(th.dim));
            });
            return;
        };
        let id = d.id;
        let caret = d.buf.primary().head.line;
        let f = self.outline_filter.to_lowercase();
        let current = d.decls.iter().rev().find(|x| x.line <= caret).map(|x| x.line);
        let groups = [
            ("Functions", &[DeclKind::Function, DeclKind::Method][..], th.function, "F"),
            ("Types", &[DeclKind::Type, DeclKind::Enum][..], th.ty, "T"),
            ("Labels", &[DeclKind::Label][..], th.label, "L"),
            ("Globals", &[DeclKind::Global][..], th.keyword, "G"),
            ("Constants", &[DeclKind::Const][..], th.constant, "C"),
        ];
        enum Row {
            Group(&'static str, usize),
            Item(usize, Color32, &'static str),
        }
        let mut rows = Vec::new();
        for (title, kinds, colour, tag) in groups {
            let members: Vec<usize> = d.decls.iter().enumerate().filter(|(_, x)| kinds.contains(&x.kind) && (f.is_empty() || x.name.to_lowercase().contains(&f))).map(|(i, _)| i).collect();
            if !members.is_empty() {
                rows.push(Row::Group(title, members.len()));
                rows.extend(members.into_iter().map(|i| Row::Item(i, colour, tag)));
            }
        }
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("outline").auto_shrink([false, false]).show_rows(ui, ROW, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for item in &rows[range] {
                match item {
                    Row::Group(t, n) => {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
                        ui.painter().text(Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, format!("{} ({n})", t.to_uppercase()), prop(11.0), th.dim);
                    }
                    Row::Item(i, colour, tag) => {
                        let x = &d.decls[*i];
                        let (r, resp) = row(ui, th, ROW, current == Some(x.line));
                        let b = Rect::from_center_size(Pos2::new(r.left() + 18.0, r.center().y), Vec2::splat(14.0));
                        ui.painter().rect_stroke(b, 0.0, Stroke::new(1.0, colour.gamma_multiply(0.55)), egui::StrokeKind::Inside);
                        ui.painter().text(b.center(), Align2::CENTER_CENTER, *tag, mono(9.5), *colour);
                        ui.painter().text(Pos2::new(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, &x.name, mono(12.5), th.text);
                        ui.painter().text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, (x.line + 1).to_string(), prop(11.0), th.faint);
                        if resp.clicked() {
                            jump = Some(x.line);
                        }
                        resp.on_hover_text(&x.text);
                    }
                }
            }
        });
        if let Some(l) = jump {
            self.goto(id, Pos::new(l, 0), None);
        }
    }

    fn search_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        header(ui, th, "Find in Files", |_| {});
        let mut go = false;
        egui::Frame::new().inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
            let r = ui.add(TextEdit::singleline(&mut self.search_query).hint_text("Search the project").desired_width(f32::INFINITY));
            if self.search_focus {
                r.request_focus();
                self.search_focus = false;
            }
            go = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                go |= ui.selectable_label(self.search_case, "Aa").on_hover_text("Match case").clicked() && {
                    self.search_case = !self.search_case;
                    true
                };
                go |= ui.selectable_label(self.search_word, "W").on_hover_text("Whole word").clicked() && {
                    self.search_word = !self.search_word;
                    true
                };
                if self.search_busy {
                    ui.spinner();
                }
                ui.label(RichText::new(format!("{} results", self.search_hits.len())).size(12.0).color(th.dim));
            });
        });
        if go {
            self.start_project_search();
        }
        if self.folder.is_none() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Open a folder to search all of it.").size(12.0).color(th.dim));
            });
        }
        ui.add_space(4.0);
        // grouped by file
        enum R<'a> {
            File(&'a Path),
            Hit(usize),
        }
        let mut rows: Vec<R> = Vec::new();
        let mut last: Option<&Path> = None;
        for (i, h) in self.search_hits.iter().enumerate() {
            if last != Some(h.path.as_path()) {
                rows.push(R::File(&h.path));
                last = Some(&h.path);
            }
            rows.push(R::Hit(i));
        }
        let mut open = None;
        let folder = self.folder.clone();
        egui::ScrollArea::vertical().id_salt("found").auto_shrink([false, false]).show_rows(ui, ROW, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for r0 in &rows[range] {
                match r0 {
                    R::File(p) => {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
                        paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 14.0, r.center().y), Vec2::splat(13.0)), Icon::File, th.accent);
                        let shown = folder.as_deref().and_then(|f| p.strip_prefix(f).ok()).unwrap_or(p);
                        ui.painter().text(Pos2::new(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, shown.display().to_string(), prop(12.5), th.text);
                    }
                    R::Hit(i) => {
                        let h = &self.search_hits[*i];
                        let (r, resp) = row(ui, th, ROW, false);
                        ui.painter().text(Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, (h.line + 1).to_string(), mono(11.0), th.faint);
                        ui.painter().text(Pos2::new(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, h.text.trim(), mono(12.0), th.dim);
                        if resp.clicked() {
                            open = Some((h.path.clone(), h.line, h.col, h.len));
                        }
                    }
                }
            }
        });
        if let Some((p, l, c, n)) = open {
            if let Some(id) = self.open_path(&p) {
                self.goto(id, Pos::new(l, c), Some(Pos::new(l, c + n)));
            }
        }
    }

    pub fn reference_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        let full = ui.available_rect_before_wrap();
        ui.expand_to_include_rect(full);
        let mut close = false;
        header(ui, th, "Command Reference", |ui| {
            close = icon_button(ui, th, Icon::Close, "Close", true, false).clicked();
        });
        if close {
            self.show_ref = false;
        }
        egui::Frame::new().inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
            let r = ui.add(TextEdit::singleline(&mut self.ref_query).hint_text("Search commands").desired_width(f32::INFINITY));
            if self.ref_focus {
                r.request_focus();
                self.ref_focus = false;
            }
        });
        ui.add_space(4.0);
        let q = self.ref_query.trim().to_lowercase();
        let mut found: Vec<(u32, usize)> = self
            .known
            .help
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                let score = if q.is_empty() {
                    Some(0)
                } else {
                    ui_util::fuzzy(&q, &h.name).filter(|s| *s < 100).or_else(|| h.text.to_lowercase().contains(&q).then_some(300)).or_else(|| ui_util::fuzzy(&q, &h.name))
                };
                score.map(|s| (s, i))
            })
            .collect();
        if !q.is_empty() {
            found.sort();
        }
        let detail_h = if self.ref_selected.is_some() { 180.0 } else { 0.0 };
        let rect = ui.available_rect_before_wrap();
        let list_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - detail_h));
        let mut list = ui.new_child(UiBuilder::new().max_rect(list_rect).id_salt("ref-list"));
        let mut pick = None;
        let mut scroll = egui::ScrollArea::vertical().id_salt("ref").auto_shrink([false, false]);
        if self.ref_scroll_to {
            if let Some(i) = self.ref_selected {
                if let Some(at) = found.iter().position(|(_, x)| *x == i) {
                    scroll = scroll.vertical_scroll_offset((at as f32 * ROW - 60.0).max(0.0));
                }
            }
            self.ref_scroll_to = false;
        }
        scroll.show_rows(&mut list, ROW, found.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (_, i) in &found[range] {
                let h = &self.known.help[*i];
                let (r, resp) = row(ui, th, ROW, self.ref_selected == Some(*i));
                ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, &h.name, mono(12.5), th.command);
                ui.painter().text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, &h.category, prop(11.0), th.faint);
                if resp.clicked() {
                    pick = Some(*i);
                }
            }
        });
        if pick.is_some() {
            self.ref_selected = pick;
        }
        if let Some(i) = self.ref_selected {
            let area = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - detail_h), rect.max);
            ui.painter().hline(area.x_range(), area.top(), Stroke::new(1.0, th.line));
            let mut d = ui.new_child(UiBuilder::new().max_rect(area.shrink(10.0)).id_salt("ref-detail"));
            let h = &self.known.help[i];
            let mut insert = None;
            egui::ScrollArea::vertical().id_salt("ref-text").auto_shrink([false, false]).show(&mut d, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&h.name).size(16.0).strong().color(th.text));
                    if h.ext {
                        ui.label(RichText::new("EXTENSION").size(10.0).color(th.accent));
                    }
                });
                if !h.signature.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&h.signature).font(mono(12.5)).color(th.accent)).wrap().selectable(true));
                }
                ui.add(egui::Label::new(RichText::new(&h.text).size(13.5).color(th.text)).wrap().selectable(true));
                ui.label(RichText::new(&h.category).size(11.5).color(th.dim));
                ui.horizontal(|ui| {
                    if ui_util::primary_button(ui, th, "Insert at Caret").clicked() {
                        insert = Some(h.name.clone());
                    }
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(h.name.clone());
                    }
                });
            });
            if let Some(n) = insert {
                self.with_doc(|d| d.buf.insert(&n, crate::buffer::EditKind::Other));
                self.focus_editor = true;
            }
        }
    }

    pub fn dock_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        let full = ui.available_rect_before_wrap();
        ui.expand_to_include_rect(full);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, th.line));
        let mut c = ui.new_child(UiBuilder::new().max_rect(bar.shrink2(Vec2::new(4.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        c.spacing_mut().item_spacing.x = 0.0;
        let problems: usize = self.docs.iter().map(|d| d.diags.len()).sum();
        for (tab, title) in [
            (DockTab::Output, "Output".to_string()),
            (DockTab::Problems, if problems > 0 { format!("Problems ({problems})") } else { "Problems".to_string() }),
            (DockTab::Debug, "Debug".to_string()),
        ] {
            let w = c.fonts_mut(|f| f.layout_no_wrap(title.clone(), prop(13.0), th.text).size().x) + 22.0;
            let (r, resp) = c.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
            let on = self.dock == tab;
            if resp.hovered() && !on {
                c.painter().rect_filled(r, 0.0, th.hover);
            }
            c.painter().text(r.center(), Align2::CENTER_CENTER, &title, prop(13.0), if on { th.text } else { th.dim });
            if on {
                c.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 2.0), r.max), 0.0, th.accent);
            }
            if resp.clicked() {
                self.dock = tab;
            }
        }
        c.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if icon_button(ui, th, Icon::Close, "Hide  (Ctrl+J)", true, false).clicked() {
                self.show_dock = false;
            }
            if self.dock == DockTab::Output {
                if icon_button(ui, th, Icon::Copy, "Copy All", true, false).clicked() {
                    let text: Vec<&str> = self.output.iter().map(|l| l.text.as_str()).collect();
                    ui.ctx().copy_text(text.join("\n"));
                }
                if icon_button(ui, th, Icon::Refresh, "Clear", true, false).clicked() {
                    self.output.clear();
                }
            }
        });
        match self.dock {
            DockTab::Output => self.output_panel(ui),
            DockTab::Problems => self.problems_panel(ui),
            DockTab::Debug => self.debug_panel(ui),
        }
    }

    fn output_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        let needs_input = self.running.as_ref().is_some_and(|r| r.stdin.is_some());
        let rect = ui.available_rect_before_wrap();
        let list_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - if needs_input { 30.0 } else { 0.0 }));
        let mut list = ui.new_child(UiBuilder::new().max_rect(list_rect).id_salt("out-list"));
        let mut jump = None;
        egui::ScrollArea::both().id_salt("output").auto_shrink([false, false]).stick_to_bottom(true).show_rows(&mut list, 18.0, self.output.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for l in &self.output[range] {
                let colour = if l.loc.is_some() {
                    th.accent
                } else if l.error {
                    th.error
                } else {
                    th.text
                };
                let mut t = RichText::new(&l.text).font(mono(12.5)).color(colour);
                if l.loc.is_some() {
                    t = t.underline();
                }
                let label = egui::Label::new(t).extend().selectable(l.loc.is_none()).sense(if l.loc.is_some() { Sense::click() } else { Sense::hover() });
                let resp = ui.allocate_ui_with_layout(Vec2::new(ui.available_width().max(10.0), 18.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.add(label)
                }).inner;
                if l.loc.is_some() && resp.clicked() {
                    jump = l.loc.clone();
                }
            }
        });
        if let Some((p, line, col)) = jump {
            let path = self.resolve_output_path(&p);
            self.goto_file(&path, line.saturating_sub(1), col.saturating_sub(1));
        }
        if needs_input {
            let bar = Rect::from_min_max(Pos2::new(rect.left() + 8.0, rect.bottom() - 28.0), Pos2::new(rect.right() - 8.0, rect.bottom() - 3.0));
            let mut c = ui.new_child(UiBuilder::new().max_rect(bar).id_salt("out-input").layout(Layout::left_to_right(Align::Center)));
            let r = c.add(TextEdit::singleline(&mut self.out_input).hint_text("Type input for the program and press Enter").desired_width(f32::INFINITY).font(mono(12.5)));
            if r.lost_focus() && c.input(|i| i.key_pressed(egui::Key::Enter)) {
                let line = std::mem::take(&mut self.out_input);
                if let Some(run) = &mut self.running {
                    run.send_line(&line);
                }
                r.request_focus();
            }
        }
    }

    fn problems_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        let mut rows = Vec::new();
        for d in &self.docs {
            for x in &d.diags {
                rows.push((d.id, d.title(), x.start, x.error, x.message.clone()));
            }
        }
        rows.sort_by_key(|r| !r.3);
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                paint_icon(ui.painter(), Rect::from_min_size(ui.cursor().min, Vec2::splat(14.0)), Icon::Check, th.ok);
                ui.add_space(20.0);
                ui.label(RichText::new("No problems found").color(th.dim));
            });
            return;
        }
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("problems").auto_shrink([false, false]).show_rows(ui, ROW, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (id, name, at, err, msg) in &rows[range] {
                let (r, resp) = row(ui, th, ROW, false);
                let c = if *err { th.error } else { th.warning };
                paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::splat(13.0)), if *err { Icon::Error } else { Icon::Warning }, c);
                let first = msg.lines().next().unwrap_or("");
                let m = ui.painter().text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, first, prop(13.0), th.text);
                ui.painter().text(Pos2::new(m.right() + 14.0, r.center().y), Align2::LEFT_CENTER, format!("{name}:{}:{}", at.line + 1, at.col + 1), prop(12.0), th.dim);
                if resp.clicked() {
                    jump = Some((*id, *at));
                }
            }
        });
        if let Some((id, p)) = jump {
            self.goto(id, p, None);
        }
    }

    fn debug_panel(&mut self, ui: &mut Ui) {
        let th = self.th;
        if self.debug.session.is_none() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("Not debugging. Click in the margin to set a breakpoint, then press F5.").color(th.dim));
            });
            return;
        }
        if let Some(msg) = self.debug.exception.clone() {
            egui::Frame::new().fill(th.error.gamma_multiply(0.2)).inner_margin(Margin::symmetric(10, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(msg).color(th.error));
            });
        }
        if !self.debug.paused {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.spinner();
                ui.label(RichText::new("Program is running. Press F4 to pause.").color(th.dim));
            });
            return;
        }
        let mut take_frame = None;
        let mut want: Vec<u32> = Vec::new();
        let mut remove_watch = None;
        let mut add = None;
        let mut goto_bp = None;
        let mut open = std::mem::take(&mut self.debug.open);
        ui.columns(3, |cols| {
            // call stack
            header(&mut cols[0], th, "Call Stack", |_| {});
            egui::ScrollArea::vertical().id_salt("stack").auto_shrink([false, false]).show(&mut cols[0], |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, f) in self.debug.frames.iter().enumerate() {
                    let (r, resp) = row(ui, th, ROW, i == self.debug.frame);
                    ui.painter().text(Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &f.name, mono(12.5), th.function);
                    let file = Path::new(&f.path).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    ui.painter().text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, format!("{file}:{}", f.line), prop(11.5), th.dim);
                    if resp.clicked() {
                        take_frame = Some(i);
                    }
                }
            });
            // variables
            header(&mut cols[1], th, "Variables", |_| {});
            egui::ScrollArea::both().id_salt("vars").auto_shrink([false, false]).show(&mut cols[1], |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (name, reference) in &self.debug.scopes {
                    let key = name.clone();
                    let (r, resp) = row(ui, th, ROW, false);
                    let is_open = !open.contains(&format!("closed:{key}"));
                    paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 10.0, r.center().y), Vec2::splat(11.0)), if is_open { Icon::Down } else { Icon::Right }, th.dim);
                    ui.painter().text(Pos2::new(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, name.to_uppercase(), prop(11.0), th.dim);
                    if resp.clicked() {
                        let k = format!("closed:{key}");
                        if !open.remove(&k) {
                            open.insert(k);
                        }
                    }
                    if is_open {
                        var_tree(ui, th, self.debug.vars.get(reference).map_or(&[][..], Vec::as_slice), &self.debug.vars, &mut open, &key, 1, &mut want);
                    }
                }
            });
            // watches and breakpoints
            header(&mut cols[2], th, "Watch", |_| {});
            egui::ScrollArea::vertical().id_salt("watch").auto_shrink([false, false]).show(&mut cols[2], |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, w) in self.debug.watches.iter().enumerate() {
                    let (r, resp) = row(ui, th, ROW, false);
                    let n = ui.painter().text(Pos2::new(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &w.expr, mono(12.5), th.text);
                    ui.painter().text(Pos2::new(n.right() + 8.0, r.center().y), Align2::LEFT_CENTER, format!("= {}", w.value), mono(12.5), if w.failed { th.faint } else { th.number });
                    if resp.secondary_clicked() || resp.middle_clicked() {
                        remove_watch = Some(i);
                    }
                    resp.on_hover_text("Right-click to remove");
                }
                ui.add_space(4.0);
                let r = ui.add(TextEdit::singleline(&mut self.debug.new_watch).hint_text("Add an expression").desired_width(f32::INFINITY).font(mono(12.5)));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.debug.new_watch.trim().is_empty() {
                    add = Some(std::mem::take(&mut self.debug.new_watch));
                }
                ui.add_space(10.0);
                panel_title(ui, th, "Breakpoints");
                ui.add_space(3.0);
                for d in &self.docs {
                    for &l in &d.breakpoints {
                        let (r, resp) = row(ui, th, ROW, false);
                        ui.painter().rect_filled(Rect::from_center_size(Pos2::new(r.left() + 12.0, r.center().y), Vec2::splat(9.0)), 0.0, th.error);
                        ui.painter().text(Pos2::new(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, format!("{}:{}", d.title(), l + 1), prop(12.5), th.text);
                        if resp.clicked() {
                            goto_bp = Some((d.id, l));
                        }
                    }
                }
            });
        });
        self.debug.open = open;
        for r in want {
            self.request_vars(r);
        }
        if let Some(i) = take_frame {
            self.select_frame(i);
        }
        if let Some(i) = remove_watch {
            self.debug.watches.remove(i);
        }
        if let Some(e) = add {
            self.add_watch(e.trim().to_string());
        }
        if let Some((id, l)) = goto_bp {
            self.goto(id, Pos::new(l, 0), None);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn var_tree(ui: &mut Ui, th: &Theme, vars: &[Var], all: &std::collections::HashMap<u32, Vec<Var>>, open: &mut BTreeSet<String>, path: &str, depth: usize, want: &mut Vec<u32>) {
    if depth > 12 {
        return;
    }
    for v in vars {
        let key = format!("{path}/{}", v.name);
        let is_open = open.contains(&key);
        let (r, resp) = row(ui, th, ROW, false);
        let x = r.left() + 6.0 + depth as f32 * 12.0;
        if v.reference != 0 {
            paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(x + 4.0, r.center().y), Vec2::splat(11.0)), if is_open { Icon::Down } else { Icon::Right }, th.dim);
        }
        let n = ui.painter().text(Pos2::new(x + 14.0, r.center().y), Align2::LEFT_CENTER, &v.name, mono(12.5), th.text);
        ui.painter().text(Pos2::new(n.right() + 10.0, r.center().y), Align2::LEFT_CENTER, &v.value, mono(12.5), th.number);
        if v.reference != 0 && resp.clicked() {
            if is_open {
                open.remove(&key);
            } else {
                open.insert(key.clone());
            }
        }
        resp.context_menu(|ui| {
            if ui_util::check_item(ui, "Copy Value", "", None, true) {
                ui.ctx().copy_text(v.value.clone());
            }
        });
        if v.reference != 0 && is_open {
            match all.get(&v.reference) {
                Some(children) => var_tree(ui, th, children, all, open, &key, depth + 1, want),
                None => want.push(v.reference),
            }
        }
    }
}
