//! The window: menus, toolbar, tabs, editor panes and status bar.

use eframe::egui::{self, Align, Align2, Color32, FontId, Id, Layout, Margin, Panel, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, ViewportCommand};

use crate::actions::{self, Action};
use crate::app::{App, DockTab, SideTab, Tab};
use crate::buffer::Caret;
use crate::editor::{self, Shared};
use crate::manual;
use crate::theme::{Theme, THEMES};
use crate::ui_util::{self, icon_button, icon_button_coloured, paint_icon, Icon};

fn flat(fill: Color32, x: i8, y: i8) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(Margin::symmetric(x, y))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.th.bg.to_normalized_gamma_f32()
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(c) = &mut self.capture {
            c.inject(raw);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_program();
        // written directly: the process is about to end
        crate::settings::store(&self.st, &self.session());
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

impl App {
    fn apply_settings(&mut self, ctx: &egui::Context) {
        if self.applied.as_ref() == Some(&self.st) {
            return;
        }
        self.th = Theme::by_name(&self.st.theme);
        let th = self.th;
        ctx.all_styles_mut(|s| th.apply(s));
        ctx.set_zoom_factor(self.st.ui_scale);
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            o.reduce_texture_memory = false;
        });
        self.applied = Some(self.st.clone());
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        let started = std::time::Instant::now();
        let ctx = ui.ctx().clone();
        self.pump();
        self.apply_settings(&ctx);
        self.handle_shortcuts(&ctx);
        for d in &mut self.docs {
            d.sync();
        }
        self.sync_lsp(&ctx);
        self.check_disk(&ctx);
        self.refresh_find();
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            if p.is_dir() {
                self.set_folder(p);
            } else {
                self.open_path(&p);
            }
        }
        ctx.input(|i| {
            if let Some(r) = i.viewport().inner_rect {
                let maxed = i.viewport().maximized.unwrap_or(false);
                self.maximized = maxed;
                if !maxed {
                    self.window_size = Some((r.width(), r.height()));
                }
            }
        });

        let th = self.th;
        Panel::top("menu").frame(flat(th.rail, 4, 0)).exact_size(26.0).show_separator_line(false).show(ui, |ui| self.menu_bar(ui));
        Panel::top("tools").frame(flat(th.panel, 6, 3)).exact_size(34.0).show_separator_line(false).show(ui, |ui| self.tool_bar(ui));
        Panel::bottom("status").frame(flat(self.status_colour(), 6, 0)).exact_size(24.0).show_separator_line(false).show(ui, |ui| self.status_bar(ui));
        Panel::left("rail").frame(flat(th.rail, 4, 6)).exact_size(36.0).show_separator_line(false).show(ui, |ui| self.rail(ui));
        if self.show_side {
            Panel::left("side").frame(flat(th.panel, 0, 0)).default_size(250.0).size_range(150.0..=560.0).show(ui, |ui| self.side_panel(ui));
        }
        if self.show_ref {
            Panel::right("reference").frame(flat(th.panel, 0, 0)).default_size(330.0).size_range(220.0..=640.0).show(ui, |ui| self.reference_panel(ui));
        }
        if self.show_dock {
            Panel::bottom("dock").frame(flat(th.panel, 0, 0)).default_size(210.0).size_range(90.0..=600.0).show(ui, |ui| self.dock_panel(ui));
        }
        egui::CentralPanel::default().frame(flat(th.bg, 0, 0)).show(ui, |ui| self.panes_ui(ui));

        self.show_palette(&ctx);
        self.show_dialog(&ctx);
        self.run_queue(&ctx);
        self.store_if_due();
        self.update_title(&ctx);
        if let Some(mut c) = self.capture.take() {
            c.timed(started.elapsed());
            let done = c.step(&ctx, self);
            if !done {
                self.capture = Some(c);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.active().map(|d| format!("{}{} - ", if d.buf.is_modified() { "* " } else { "" }, d.title())).unwrap_or_default();
        let title = format!("{name}Neptune IDE");
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn status_colour(&self) -> Color32 {
        if self.debug.paused {
            Color32::from_rgb(0xB8, 0x74, 0x10)
        } else if self.debug.session.is_some() || self.running.is_some() {
            self.th.active
        } else {
            self.th.rail
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().button_padding = Vec2::new(8.0, 2.0);
            let group = |ui: &mut Ui, this: &mut App, list: &[Option<Action>]| {
                for it in list {
                    match it {
                        Some(a) => {
                            if ui_util::menu_item(ui, *a, this.action_enabled(*a)) {
                                this.queue.push(*a);
                            }
                        }
                        None => {
                            ui.separator();
                        }
                    }
                }
            };
            use Action as A;
            ui.menu_button("File", |ui| {
                ui.set_min_width(280.0);
                group(ui, self, &[Some(A::New), Some(A::Open), Some(A::OpenFolder)]);
                ui.menu_button("Open Recent", |ui| {
                    ui.set_min_width(340.0);
                    if self.recent.is_empty() {
                        ui.label("No recent files");
                    }
                    for r in self.recent.clone() {
                        if ui_util::check_item(ui, &r, "", None, true) {
                            self.open_path(std::path::Path::new(&r));
                        }
                    }
                });
                ui.separator();
                group(ui, self, &[Some(A::Save), Some(A::SaveAs), Some(A::SaveAll), None, Some(A::Close), Some(A::CloseAll), None, Some(A::Settings), None, Some(A::Quit)]);
            });
            ui.menu_button("Edit", |ui| {
                ui.set_min_width(300.0);
                group(
                    ui,
                    self,
                    &[
                        Some(A::Undo),
                        Some(A::Redo),
                        None,
                        Some(A::Cut),
                        Some(A::Copy),
                        Some(A::Paste),
                        Some(A::SelectAll),
                        None,
                        Some(A::Find),
                        Some(A::FindNext),
                        Some(A::FindPrev),
                        Some(A::Replace),
                        Some(A::FindInFiles),
                        None,
                        Some(A::ToggleComment),
                        Some(A::Indent),
                        Some(A::Outdent),
                        Some(A::DuplicateLine),
                        Some(A::DeleteLine),
                        Some(A::MoveLineUp),
                        Some(A::MoveLineDown),
                        None,
                        Some(A::AddNextOccurrence),
                        Some(A::CaretAbove),
                        Some(A::CaretBelow),
                        None,
                        Some(A::UpperCase),
                        Some(A::LowerCase),
                        Some(A::TrimTrailing),
                        Some(A::FormatFile),
                        Some(A::RenameSymbol),
                    ],
                );
            });
            ui.menu_button("View", |ui| {
                ui.set_min_width(280.0);
                group(ui, self, &[Some(A::Palette), None, Some(A::ToggleSidebar), Some(A::ToggleBottom), Some(A::ToggleReference), Some(A::SplitEditor), None, Some(A::ZoomIn), Some(A::ZoomOut), Some(A::ZoomReset), None]);
                let mut toggle = |ui: &mut Ui, title: &str, keys: String, on: bool, a: A| {
                    if ui_util::check_item(ui, title, &keys, Some(on), true) {
                        self.queue.push(a);
                    }
                };
                toggle(ui, "Minimap", actions::shortcut_text(A::ToggleMinimap), self.st.minimap, A::ToggleMinimap);
                toggle(ui, "Whitespace", String::new(), self.st.whitespace, A::ToggleWhitespace);
                ui.separator();
                group(ui, self, &[Some(A::ToggleFold), Some(A::FoldAll), Some(A::UnfoldAll)]);
                ui.separator();
                ui.menu_button("Theme", |ui| {
                    ui.set_min_width(180.0);
                    for t in THEMES {
                        if ui_util::check_item(ui, t.name, "", Some(self.st.theme == t.name), true) {
                            self.st.theme = t.name.to_string();
                        }
                    }
                });
            });
            ui.menu_button("Go", |ui| {
                ui.set_min_width(280.0);
                group(ui, self, &[Some(A::QuickOpen), Some(A::GotoSymbol), Some(A::GotoLine), Some(A::GotoDefinition), None, Some(A::Back), Some(A::Forward), None, Some(A::NextProblem), Some(A::PrevProblem), None, Some(A::ToggleBookmark), Some(A::NextBookmark), Some(A::PrevBookmark)]);
            });
            ui.menu_button("Program", |ui| {
                ui.set_min_width(290.0);
                group(ui, self, &[Some(A::Run), Some(A::RunAgain), Some(A::RunNoDebug), Some(A::Stop), None, Some(A::Check), Some(A::Build), Some(A::Blender), Some(A::CommandLine), None]);
                if ui_util::check_item(ui, "Debug Enabled", "", Some(self.st.debug_enabled), true) {
                    self.queue.push(A::ToggleDebug);
                }
                ui.separator();
                group(ui, self, &[Some(A::Pause), Some(A::StepOver), Some(A::StepIn), Some(A::StepOut), None, Some(A::ToggleBreakpoint), Some(A::EditBreakpoint), Some(A::ClearBreakpoints)]);
            });
            ui.menu_button("Help", |ui| {
                ui.set_min_width(280.0);
                group(ui, self, &[Some(A::QuickHelp), Some(A::Reference), Some(A::Manual), None]);
                ui.menu_button("Examples", |ui| {
                    ui.set_min_width(240.0);
                    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                        for e in self.examples.clone() {
                            let name = e.file_stem().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                            if ui_util::check_item(ui, &name, "", None, true) {
                                self.open_path(&e);
                            }
                        }
                    });
                });
                ui.separator();
                group(ui, self, &[Some(A::Welcome), Some(A::Shortcuts), None, Some(A::About)]);
            });
        });
    }

    pub fn action_enabled(&self, a: Action) -> bool {
        use Action as A;
        let has_doc = self.active_id().is_some();
        let paused = self.debug.paused;
        match a {
            A::Undo => self.active().is_some_and(|d| d.buf.can_undo()),
            A::Redo => self.active().is_some_and(|d| d.buf.can_redo()),
            A::Save | A::SaveAs | A::Cut | A::Copy | A::Paste | A::SelectAll | A::Find | A::Replace | A::ToggleComment | A::Check | A::Build | A::RunNoDebug | A::FormatFile => has_doc,
            A::Stop => self.is_busy(),
            A::RunAgain => self.last_target.is_some() || has_doc,
            A::Pause => self.debug.session.is_some() && !paused,
            A::StepOver | A::StepIn | A::StepOut | A::Continue => paused,
            _ => true,
        }
    }

    fn tool_bar(&mut self, ui: &mut Ui) {
        let th = self.th;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            use Action as A;
            let tip = |a: A| {
                let k = actions::shortcut_text(a);
                let t = actions::info(a).title.trim_end_matches("...");
                if k.is_empty() {
                    t.to_string()
                } else {
                    format!("{t}  ({k})")
                }
            };
            for (icon, a) in [(Icon::New, A::New), (Icon::Open, A::Open), (Icon::Save, A::Save), (Icon::SaveAll, A::SaveAll)] {
                if icon_button(ui, th, icon, &tip(a), self.action_enabled(a), false).clicked() {
                    self.queue.push(a);
                }
            }
            sep(ui, th);
            for (icon, a) in [(Icon::Cut, A::Cut), (Icon::Copy, A::Copy), (Icon::Paste, A::Paste)] {
                if icon_button(ui, th, icon, actions::info(a).title, self.action_enabled(a), false).clicked() {
                    self.queue.push(a);
                }
            }
            sep(ui, th);
            for (icon, a) in [(Icon::Undo, A::Undo), (Icon::Redo, A::Redo), (Icon::Find, A::Find)] {
                if icon_button(ui, th, icon, &tip(a), self.action_enabled(a), false).clicked() {
                    self.queue.push(a);
                }
            }
            sep(ui, th);
            let run_tip = if self.debug.paused { "Continue  (F5)".to_string() } else { tip(A::Run) };
            if icon_button_coloured(ui, th, Icon::Run, &run_tip, true, false, th.ok).clicked() {
                self.queue.push(A::Run);
            }
            if icon_button_coloured(ui, th, Icon::RunAgain, &tip(A::RunAgain), self.action_enabled(A::RunAgain), false, th.ok).clicked() {
                self.queue.push(A::RunAgain);
            }
            if icon_button_coloured(ui, th, Icon::Stop, &tip(A::Stop), self.is_busy(), false, th.error).clicked() {
                self.queue.push(A::Stop);
            }
            if icon_button(ui, th, Icon::Bug, "Debug Enabled: run with the debugger", true, self.st.debug_enabled).clicked() {
                self.queue.push(A::ToggleDebug);
            }
            sep(ui, th);
            for (icon, a) in [(Icon::Pause, A::Pause), (Icon::StepOver, A::StepOver), (Icon::StepIn, A::StepIn), (Icon::StepOut, A::StepOut)] {
                if icon_button(ui, th, icon, &tip(a), self.action_enabled(a), false).clicked() {
                    self.queue.push(a);
                }
            }
            sep(ui, th);
            if icon_button(ui, th, Icon::Check, &tip(A::Check), self.action_enabled(A::Check), false).clicked() {
                self.queue.push(A::Check);
            }
            if icon_button(ui, th, Icon::Build, &tip(A::Build), self.action_enabled(A::Build), false).clicked() {
                self.queue.push(A::Build);
            }
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            egui::ComboBox::from_id_salt("quality")
                .width(84.0)
                .selected_text(if self.st.quality.is_empty() { "Default".to_string() } else { title_case(&self.st.quality) })
                .show_ui(ui, |ui| {
                    for (v, label) in [("", "Default"), ("low", "Low"), ("medium", "Medium"), ("high", "High"), ("ultra", "Ultra")] {
                        ui.selectable_value(&mut self.st.quality, v.to_string(), label);
                    }
                })
                .response
                .on_hover_text("Graphics quality for Run");

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                if icon_button(ui, th, Icon::Gear, &tip(A::Settings), true, false).clicked() {
                    self.queue.push(A::Settings);
                }
                if icon_button(ui, th, Icon::Book, &tip(A::ToggleReference), true, self.show_ref).clicked() {
                    self.queue.push(A::ToggleReference);
                }
                if icon_button(ui, th, Icon::Split, &tip(A::SplitEditor), true, self.panes.len() > 1).clicked() {
                    self.queue.push(A::SplitEditor);
                }
                ui.add_space(8.0);
                // looks like a search box; opens the quick-open palette
                let w = ui.available_width().clamp(120.0, 360.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
                ui.painter().rect_filled(r, 0.0, if resp.hovered() { th.hover } else { th.bg });
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, th.line), egui::StrokeKind::Inside);
                paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 13.0, r.center().y), Vec2::splat(12.0)), Icon::Search, th.dim);
                ui.painter().text(Pos2::new(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, format!("Search files and commands   {}", actions::shortcut_text(A::QuickOpen)), FontId::proportional(12.5), th.dim);
                if resp.clicked() {
                    self.queue.push(A::QuickOpen);
                }
            });
        });
    }

    fn rail(&mut self, ui: &mut Ui) {
        let th = self.th;
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.vertical_centered(|ui| {
            for (icon, tab, tip) in [(Icon::Explorer, SideTab::Explorer, "Files"), (Icon::Outline, SideTab::Outline, "Functions, types and labels"), (Icon::Search, SideTab::Search, "Find in Files")] {
                let on = self.show_side && self.side == tab;
                if icon_button(ui, th, icon, tip, true, on).clicked() {
                    if on {
                        self.show_side = false;
                    } else {
                        self.side = tab;
                        self.show_side = true;
                    }
                }
            }
            ui.add_space(8.0);
            if icon_button(ui, th, Icon::Book, "Manual", true, matches!(self.panes[self.focus_pane].current(), Some(Tab::Manual))).clicked() {
                self.queue.push(Action::Manual);
            }
            if icon_button(ui, th, Icon::Palette, "Command Palette", true, false).clicked() {
                self.queue.push(Action::Palette);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let th = self.th;
        let small = |t: String, c: Color32| RichText::new(t).size(12.0).color(c);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (errors, warnings): (usize, usize) = self.docs.iter().map(|d| (d.errors(), d.warnings())).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            let (r, resp) = ui.allocate_exact_size(Vec2::new(82.0, 22.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 0.0, th.hover);
            }
            let y = r.center().y;
            paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 10.0, y), Vec2::splat(13.0)), Icon::Error, if errors > 0 { th.error } else { th.dim });
            ui.painter().text(Pos2::new(r.left() + 20.0, y), Align2::LEFT_CENTER, errors.to_string(), FontId::proportional(12.0), th.text);
            paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 48.0, y), Vec2::splat(13.0)), Icon::Warning, if warnings > 0 { th.warning } else { th.dim });
            ui.painter().text(Pos2::new(r.left() + 58.0, y), Align2::LEFT_CENTER, warnings.to_string(), FontId::proportional(12.0), th.text);
            if resp.clicked() {
                self.dock = DockTab::Problems;
                self.show_dock = true;
            }
            if self.debug.paused {
                ui.label(small(format!("Paused: {}", self.debug.reason), Color32::WHITE));
            } else if self.debug.session.is_some() {
                ui.label(small("Debugging".into(), th.text));
            } else if self.running.is_some() {
                ui.label(small("Running".into(), th.text));
            }
            if let Some((_, sig)) = &self.help_tip {
                ui.label(RichText::new(sig).monospace().size(12.5).color(th.accent));
            } else if let Some((msg, when, err)) = &self.status {
                if when.elapsed().as_secs() < 8 {
                    ui.label(small(msg.clone(), if *err { th.error } else { th.text }));
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(8));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(small(if self.lsp_ready { "Neptune".into() } else { "Neptune (starting)".into() }, if self.lsp_ready { th.dim } else { th.warning }));
                if let Some(d) = self.active() {
                    ui.label(small(if d.latin1 { "Latin-1".into() } else { "UTF-8".into() }, th.dim));
                    ui.label(small(if d.crlf { "CRLF".into() } else { "LF".into() }, th.dim));
                    ui.label(small(if self.st.use_spaces { format!("Spaces: {}", self.st.tab_size) } else { format!("Tab: {}", self.st.tab_size) }, th.dim));
                    let c = d.buf.primary();
                    if d.buf.carets.len() > 1 {
                        ui.label(small(format!("{} carets", d.buf.carets.len()), th.accent));
                    }
                    let picked: usize = d.buf.carets.iter().filter(|k| k.has_selection()).map(|k: &Caret| crate::buffer::char_len(&d.buf.text_in(k.start(), k.end()))).sum();
                    if picked > 0 {
                        ui.label(small(format!("{picked} selected"), th.dim));
                    }
                    ui.label(small(format!("Ln {}, Col {}", c.head.line + 1, c.head.col + 1), th.text));
                }
            });
        });
    }

    fn panes_ui(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let n = self.panes.len();
        let w = rect.width() / n as f32;
        for i in 0..n {
            let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * w, rect.top()), Vec2::new(w, rect.height()));
            let mut child = ui.new_child(UiBuilder::new().max_rect(r).id_salt(("pane", i)));
            child.set_clip_rect(r);
            self.pane_ui(&mut child, i);
            if i > 0 {
                ui.painter().vline(r.left(), r.y_range(), Stroke::new(1.0, self.th.line));
            }
            if i >= self.panes.len() {
                break;
            }
        }
    }

    fn pane_ui(&mut self, ui: &mut Ui, index: usize) {
        self.tab_bar(ui, index);
        if index >= self.panes.len() {
            return;
        }
        match self.panes[index].current() {
            Some(Tab::Doc(id)) => {
                self.breadcrumbs(ui, id);
                self.editor_ui(ui, index, id);
                if self.focus_pane == index && self.find.open {
                    self.find_bar(ui);
                }
            }
            Some(Tab::Welcome) => self.welcome(ui),
            Some(Tab::Manual) => self.manual_ui(ui),
            None => self.welcome(ui),
        }
    }

    fn tab_bar(&mut self, ui: &mut Ui, pane: usize) {
        let th = self.th;
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
        ui.painter().rect_filled(bar, 0.0, th.panel);
        let mut child = ui.new_child(UiBuilder::new().max_rect(bar).layout(Layout::left_to_right(Align::Center)));
        let mut close = None;
        let mut select = None;
        let mut swap = None;
        let mut close_others = None;
        // bring the active tab into view when it changes, and only then, so
        // the strip can still be scrolled by hand
        let seen = ui.scope_id().with(("tab-seen", pane));
        let now = (self.panes[pane].active, self.panes[pane].tabs.len());
        let reveal = ui.ctx().data_mut(|d| {
            let changed = d.get_temp::<(usize, usize)>(seen) != Some(now);
            d.insert_temp(seen, now);
            changed
        });
        egui::ScrollArea::horizontal().id_salt(("tabs", pane)).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                for i in 0..self.panes[pane].tabs.len() {
                    let tab = self.panes[pane].tabs[i];
                    let (title, modified, err) = match tab {
                        Tab::Doc(id) => self.doc(id).map_or((String::new(), false, false), |d| (d.title(), d.buf.is_modified(), d.errors() > 0)),
                        Tab::Welcome => ("Welcome".into(), false, false),
                        Tab::Manual => ("Manual".into(), false, false),
                    };
                    let w = ui.fonts_mut(|f| f.layout_no_wrap(title.clone(), FontId::proportional(13.0), th.text).size().x) + 54.0;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click_and_drag());
                    let active = self.panes[pane].active == i;
                    let focused = active && self.focus_pane == pane;
                    if active && reveal {
                        resp.scroll_to_me(None);
                    }
                    ui.painter().rect_filled(r, 0.0, if active { th.bg } else if resp.hovered() { th.hover } else { th.panel });
                    if active {
                        ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, if focused { th.accent } else { th.faint });
                    }
                    ui.painter().vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0, th.line));
                    let icon_rect = Rect::from_center_size(Pos2::new(r.left() + 15.0, r.center().y), Vec2::splat(13.0));
                    let icon = if matches!(tab, Tab::Doc(_)) { Icon::File } else { Icon::Book };
                    paint_icon(ui.painter(), icon_rect, icon, if err { th.error } else if active { th.accent } else { th.dim });
                    ui.painter().text(Pos2::new(r.left() + 27.0, r.center().y), Align2::LEFT_CENTER, &title, FontId::proportional(13.0), if err { th.error } else if active { th.text } else { th.dim });
                    let xr = Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(18.0));
                    let over_x = ui.rect_contains_pointer(xr);
                    if over_x {
                        ui.painter().rect_filled(xr, 0.0, th.active);
                    }
                    if modified && !over_x {
                        ui.painter().rect_filled(Rect::from_center_size(xr.center(), Vec2::splat(7.0)), 0.0, th.accent);
                    } else if over_x || resp.hovered() || active {
                        paint_icon(ui.painter(), xr.shrink(4.0), Icon::Close, th.dim);
                    }
                    if resp.clicked() {
                        if over_x {
                            close = Some(i);
                        } else {
                            select = Some(i);
                        }
                    }
                    if resp.middle_clicked() {
                        close = Some(i);
                    }
                    if resp.dragged() {
                        if let Some(p) = resp.interact_pointer_pos() {
                            if p.x < r.left() && i > 0 {
                                swap = Some((i, i - 1));
                            } else if p.x > r.right() && i + 1 < self.panes[pane].tabs.len() {
                                swap = Some((i, i + 1));
                            }
                        }
                    }
                    resp.context_menu(|ui| {
                        ui.set_min_width(200.0);
                        if ui_util::check_item(ui, "Close", "", None, true) {
                            close = Some(i);
                        }
                        if ui_util::check_item(ui, "Close Others", "", None, true) {
                            close_others = Some(i);
                        }
                        if let Tab::Doc(id) = tab {
                            if let Some(p) = self.doc(id).and_then(|d| d.path.clone()) {
                                if ui_util::check_item(ui, "Copy Path", "", None, true) {
                                    ui.ctx().copy_text(p.display().to_string());
                                }
                            }
                        }
                    });
                }
            });
        });
        if let Some((a, b)) = swap {
            let p = &mut self.panes[pane];
            p.tabs.swap(a, b);
            if p.active == a {
                p.active = b;
            } else if p.active == b {
                p.active = a;
            }
        }
        if let Some(i) = select {
            self.panes[pane].active = i;
            self.focus_pane = pane;
            self.focus_editor = true;
            self.find.key.0 = 0;
        }
        if let Some(i) = close {
            self.request_close(pane, i);
        }
        if let Some(keep) = close_others {
            let kept = self.panes[pane].tabs[keep];
            for j in (0..self.panes[pane].tabs.len()).rev() {
                let tab = self.panes[pane].tabs[j];
                let unsaved = matches!(tab, Tab::Doc(id) if self.doc(id).is_some_and(|d| d.buf.is_modified()));
                if tab != kept && !unsaved {
                    self.close_tab(pane, j);
                }
            }
        }
    }

    fn breadcrumbs(&mut self, ui: &mut Ui, id: u64) {
        let th = self.th;
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
        let Some(d) = self.doc(id) else { return };
        ui.painter().rect_filled(r, 0.0, th.bg);
        ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, th.guide));
        let mut text = match (&d.path, &self.folder) {
            (Some(p), Some(f)) => p.strip_prefix(f).unwrap_or(p).display().to_string(),
            (Some(p), None) => p.display().to_string(),
            _ => d.title(),
        };
        text = text.replace(['\\', '/'], " / ").replace("  ", " ");
        let line = d.buf.primary().head.line;
        let scope = d.decls.iter().rev().find(|x| {
            x.line <= line && matches!(x.kind, crate::lang::DeclKind::Function | crate::lang::DeclKind::Method | crate::lang::DeclKind::Type) && d.fold_at(x.line).is_some_and(|(_, e)| line <= e)
        });
        let pos = ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(12.0), th.dim);
        if let Some(s) = scope {
            ui.painter().text(Pos2::new(pos.right() + 8.0, r.center().y), Align2::LEFT_CENTER, format!("> {}", s.name), FontId::proportional(12.0), th.accent);
        }
    }

    fn editor_ui(&mut self, ui: &mut Ui, pane: usize, id: u64) {
        let exec = self.exec_line(id);
        let Some(i) = self.docs.iter().position(|d| d.id == id) else { return };
        let is_focus_pane = self.focus_pane == pane;
        let mut outs = Vec::new();
        let (matches, current): (&[_], _) = if is_focus_pane && self.find.open { (&self.find.matches, self.find.current) } else { (&[], None) };
        let want_focus = is_focus_pane && self.focus_editor && !self.palette.open && self.dialog.is_none();
        let id_hash = egui::Id::unique(("editor", pane));
        {
            let view = {
                let p = &mut self.panes[pane];
                let pane_id = p.id;
                p.views.entry(id).or_insert_with(|| editor::View::new(pane_id))
            };
            let mut sh = Shared {
                st: &self.st,
                th: self.th,
                known: &self.known,
                comp_items: &self.comp_items,
                completion: &mut self.completion,
                exec_line: exec,
                matches,
                current_match: current,
                eval: &self.debug.hover,
                debugging: self.debug.paused,
                out: &mut outs,
                want_focus,
                read_only: false,
            };
            editor::show(ui, &mut self.docs[i], view, &mut sh);
        }
        if want_focus {
            self.focus_editor = false;
        }
        let focused = ui.memory(|m| m.has_focus(id_hash));
        if focused {
            if self.focus_pane != pane {
                self.focus_pane = pane;
                self.find.key.0 = 0;
            }
            self.editor_focused = true;
        } else if is_focus_pane {
            self.editor_focused = false;
        }
        if focused && self.help_tip.is_some() && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_)))) {
            self.help_tip = None;
        }
        let ctx = ui.ctx().clone();
        self.apply_outs(outs, &ctx);
    }

    fn find_bar(&mut self, ui: &mut Ui) {
        let th = self.th;
        let pane = ui.max_rect();
        let width = 430.0f32.min(pane.width() - 40.0);
        let right = ui.clip_rect().right() - 120.0;
        let top = ui.clip_rect().top() + 60.0;
        let mut close = false;
        let mut step = None;
        let mut do_replace = false;
        let mut do_all = false;
        egui::Area::new(Id::unique("find")).order(egui::Order::Foreground).fixed_pos(Pos2::new(right - width, top)).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.line)).inner_margin(Margin::same(5)).show(ui, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 4.0);
                ui.horizontal(|ui| {
                    let ic = if self.find.replace { Icon::Down } else { Icon::Right };
                    if icon_button(ui, th, ic, "Toggle Replace", true, false).clicked() {
                        self.find.replace = !self.find.replace;
                    }
                    let box_w = width - 216.0;
                    let r = ui.add(TextEdit::singleline(&mut self.find.query).id(Id::unique("find-box")).hint_text("Find").desired_width(box_w));
                    if self.find.focus {
                        r.request_focus();
                        self.find.focus = false;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        step = Some(!ui.input(|i| i.modifiers.shift));
                        r.request_focus();
                    }
                    if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                    let n = self.find.matches.len();
                    let info = match (self.find.current, n) {
                        (_, 0) if !self.find.query.is_empty() => "None".to_string(),
                        (Some(i), n) => format!("{}/{}", i + 1, n),
                        (None, 0) => String::new(),
                        (None, n) => n.to_string(),
                    };
                    let (r2, _) = ui.allocate_exact_size(Vec2::new(44.0, 22.0), Sense::hover());
                    ui.painter().text(r2.center(), Align2::CENTER_CENTER, info, FontId::proportional(11.5), if n == 0 && !self.find.query.is_empty() { th.error } else { th.dim });
                    if ui.selectable_label(self.find.case, RichText::new("Aa").size(12.0)).on_hover_text("Match case").clicked() {
                        self.find.case = !self.find.case;
                    }
                    if ui.selectable_label(self.find.word, RichText::new("W").size(12.0)).on_hover_text("Whole word").clicked() {
                        self.find.word = !self.find.word;
                    }
                    if icon_button(ui, th, Icon::Up, "Previous match  (Shift+F3)", n > 0, false).clicked() {
                        step = Some(false);
                    }
                    if icon_button(ui, th, Icon::Down, "Next match  (F3)", n > 0, false).clicked() {
                        step = Some(true);
                    }
                    if icon_button(ui, th, Icon::Close, "Close  (Esc)", true, false).clicked() {
                        close = true;
                    }
                });
                if self.find.replace {
                    ui.horizontal(|ui| {
                        ui.add_space(29.0);
                        ui.add(TextEdit::singleline(&mut self.find.replacement).hint_text("Replace").desired_width(width - 216.0));
                        if ui.button("Replace").clicked() {
                            do_replace = true;
                        }
                        if ui.button("All").clicked() {
                            do_all = true;
                        }
                    });
                }
            });
        });
        if let Some(fwd) = step {
            self.find_step(fwd);
        }
        if do_replace {
            self.replace_one();
        }
        if do_all {
            self.replace_all();
        }
        if close || (self.editor_focused && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.find.open = false;
            self.find.matches.clear();
            self.find.current = None;
            self.find.key.0 = 0;
            self.focus_editor = true;
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        let th = self.th;
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, th.bg);
        egui::ScrollArea::vertical().id_salt("welcome").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(48.0);
            let margin = ((ui.available_width() - 820.0) * 0.5).max(24.0);
            ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_max_width(820.0);
                    // the mark: a planet and its ring, built from squares
                    let (r, _) = ui.allocate_exact_size(Vec2::new(66.0, 54.0), Sense::hover());
                    let c = r.center();
                    let s = 8.0;
                    for (x, y) in [(-1, -2), (0, -2), (-2, -1), (-1, -1), (0, -1), (1, -1), (-2, 0), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1), (-2, 1)] {
                        let p = Pos2::new(c.x + x as f32 * s, c.y + y as f32 * s);
                        ui.painter().rect_filled(Rect::from_min_size(p, Vec2::splat(s - 1.0)), 0.0, th.accent);
                    }
                    for (x, y) in [(-4, 2), (-3, 1), (2, 0), (3, -1)] {
                        let p = Pos2::new(c.x + x as f32 * s, c.y + y as f32 * s);
                        ui.painter().rect_filled(Rect::from_min_size(p, Vec2::splat(s - 1.0)), 0.0, th.warning);
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("Neptune IDE").size(34.0).strong().color(th.text));
                    ui.label(RichText::new("Write it. Press F5. Play it.").size(15.0).color(th.dim));
                    ui.add_space(24.0);
                    ui.columns(2, |cols| {
                        cols[0].label(RichText::new("START").size(11.0).strong().color(th.dim));
                        cols[0].add_space(4.0);
                        for (icon, label, keys, a) in [
                            (Icon::New, "New program", "Ctrl+N", Action::New),
                            (Icon::Open, "Open file", "Ctrl+O", Action::Open),
                            (Icon::Folder, "Open folder", "Ctrl+Shift+O", Action::OpenFolder),
                            (Icon::Book, "Read the manual", "", Action::Manual),
                            (Icon::Palette, "All commands", "Ctrl+Shift+P", Action::Palette),
                            (Icon::Gear, "Settings", "Ctrl+,", Action::Settings),
                        ] {
                            if welcome_row(&mut cols[0], th, icon, label, keys).clicked() {
                                self.queue.push(a);
                            }
                        }
                        cols[0].add_space(16.0);
                        cols[0].label(RichText::new("RECENT").size(11.0).strong().color(th.dim));
                        cols[0].add_space(4.0);
                        if self.recent.is_empty() {
                            cols[0].label(RichText::new("Files you open will be listed here.").color(th.faint));
                        }
                        for r in self.recent.clone().into_iter().take(7) {
                            let name = std::path::Path::new(&r).file_name().map_or(r.clone(), |n| n.to_string_lossy().into_owned());
                            if welcome_row(&mut cols[0], th, Icon::File, &name, "").on_hover_text(&r).clicked() {
                                self.open_path(std::path::Path::new(&r));
                            }
                        }
                        cols[1].label(RichText::new("EXAMPLES").size(11.0).strong().color(th.dim));
                        cols[1].add_space(4.0);
                        for e in self.examples.clone().into_iter().take(16) {
                            let name = e.file_stem().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                            if welcome_row(&mut cols[1], th, Icon::Run, &name, "").clicked() {
                                self.open_path(&e);
                            }
                        }
                    });
                });
            });
        });
    }

    fn manual_ui(&mut self, ui: &mut Ui) {
        let th = self.th;
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, th.bg);
        let list = Rect::from_min_max(rect.min, Pos2::new(rect.left() + 240.0, rect.bottom()));
        let page = Rect::from_min_max(Pos2::new(list.right() + 1.0, rect.top()), rect.max);
        ui.painter().vline(list.right(), rect.y_range(), Stroke::new(1.0, th.line));
        let mut left = ui.new_child(UiBuilder::new().max_rect(list.shrink(6.0)));
        left.add(TextEdit::singleline(&mut self.manual_query).hint_text("Filter chapters").desired_width(f32::INFINITY));
        left.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("chapters").auto_shrink([false, false]).show(&mut left, |ui| {
            let q = self.manual_query.to_lowercase();
            for (i, (_, text)) in manual::CHAPTERS.iter().enumerate() {
                let title = manual::chapter_title(text);
                if !q.is_empty() && !title.to_lowercase().contains(&q) {
                    continue;
                }
                let (r, resp) = ui_util::row(ui, th, 24.0, i == self.manual_page);
                ui.painter().text(Pos2::new(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, title, FontId::proportional(13.0), if i == self.manual_page { th.text } else { th.dim });
                if resp.clicked() {
                    self.manual_page = i;
                }
            }
        });
        if self.manual_cache.as_ref().is_none_or(|(p, _)| *p != self.manual_page) {
            let (_, text) = manual::CHAPTERS[self.manual_page.min(manual::CHAPTERS.len() - 1)];
            self.manual_cache = Some((self.manual_page, manual::parse(text)));
        }
        let mut right = ui.new_child(UiBuilder::new().max_rect(page).id_salt("manual-page"));
        let mut open = None;
        let mut turn = None;
        egui::ScrollArea::vertical().id_salt(("page", self.manual_page)).auto_shrink([false, false]).show(&mut right, |ui| {
            egui::Frame::new().inner_margin(Margin::symmetric(32, 20)).show(ui, |ui| {
                if let Some((_, blocks)) = &self.manual_cache {
                    open = manual::show(ui, blocks, &self.known, th);
                }
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    if self.manual_page > 0 && ui.button(format!("Previous: {}", manual::chapter_title(manual::CHAPTERS[self.manual_page - 1].1))).clicked() {
                        turn = Some(self.manual_page - 1);
                    }
                    if self.manual_page + 1 < manual::CHAPTERS.len() && ui_util::primary_button(ui, th, &format!("Next: {}", manual::chapter_title(manual::CHAPTERS[self.manual_page + 1].1))).clicked() {
                        turn = Some(self.manual_page + 1);
                    }
                });
            });
        });
        if let Some(page) = turn {
            self.manual_page = page;
        }
        if let Some(code) = open {
            self.new_file();
            self.with_doc(|d| {
                d.buf.insert(&code, crate::buffer::EditKind::Other);
            });
        }
    }
}

fn welcome_row(ui: &mut Ui, th: &Theme, icon: Icon, label: &str, keys: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(380.0), 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, th.hover);
    }
    paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 14.0, r.center().y), Vec2::splat(14.0)), icon, th.accent);
    ui.painter().text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), th.text);
    if !keys.is_empty() {
        ui.painter().text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, keys, FontId::proportional(12.0), th.faint);
    }
    resp
}

fn sep(ui: &mut Ui, th: &Theme) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(11.0, 22.0), Sense::hover());
    ui.painter().vline(r.center().x, r.shrink2(Vec2::new(0.0, 2.0)).y_range(), Stroke::new(1.0, th.line));
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
}
