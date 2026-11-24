//! Small square widgets and the icon set, drawn as vector shapes so they stay
//! sharp at any display scale.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::actions::{self, Action};
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAll,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    Find,
    Run,
    RunAgain,
    Stop,
    Pause,
    StepOver,
    StepIn,
    StepOut,
    Check,
    Build,
    Bug,
    Explorer,
    Outline,
    Search,
    Book,
    Gear,
    Close,
    Right,
    Down,
    Up,
    File,
    Folder,
    Warning,
    Error,
    Split,
    Palette,
    Plus,
    Refresh,
}

/// Draws an icon inside `r` on a 16 by 16 grid.
pub fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, c: Color32) {
    let k = r.width().min(r.height()) / 16.0;
    let o = r.center() - Vec2::splat(8.0 * k);
    let at = |x: f32, y: f32| o + Vec2::new(x * k, y * k);
    let s = Stroke::new(1.4, c);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), s));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(Shape::closed_line(pts.iter().map(|&(x, y)| at(x, y)).collect(), s));
    };
    let fill = |pts: &[(f32, f32)]| {
        p.add(Shape::convex_polygon(pts.iter().map(|&(x, y)| at(x, y)).collect(), c, Stroke::NONE));
    };
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32| {
        p.rect_filled(Rect::from_min_max(at(x0, y0), at(x1, y1)), 0.0, c);
    };
    let page = || closed(&[(3.0, 1.5), (9.5, 1.5), (13.0, 5.0), (13.0, 14.5), (3.0, 14.5)]);
    match icon {
        Icon::New | Icon::File => {
            page();
            line(&[(9.5, 1.5), (9.5, 5.0), (13.0, 5.0)]);
            if icon == Icon::New {
                bar(7.3, 7.0, 8.7, 13.0);
                bar(5.0, 9.3, 11.0, 10.7);
            }
        }
        Icon::Open | Icon::Folder => {
            closed(&[(1.5, 3.0), (6.0, 3.0), (7.5, 5.0), (14.5, 5.0), (14.5, 13.5), (1.5, 13.5)]);
            if icon == Icon::Open {
                bar(5.0, 8.5, 11.0, 10.0);
            }
        }
        Icon::Save | Icon::SaveAll => {
            closed(&[(2.0, 2.0), (12.0, 2.0), (14.0, 4.0), (14.0, 14.0), (2.0, 14.0)]);
            bar(5.0, 2.0, 11.0, 6.0);
            closed(&[(4.5, 9.0), (11.5, 9.0), (11.5, 14.0), (4.5, 14.0)]);
            if icon == Icon::SaveAll {
                bar(0.0, 0.0, 3.0, 1.2);
                bar(0.0, 0.0, 1.2, 3.0);
            }
        }
        Icon::Cut => {
            line(&[(4.0, 2.0), (11.0, 10.0)]);
            line(&[(12.0, 2.0), (5.0, 10.0)]);
            closed(&[(2.0, 10.0), (5.5, 10.0), (5.5, 14.0), (2.0, 14.0)]);
            closed(&[(10.5, 10.0), (14.0, 10.0), (14.0, 14.0), (10.5, 14.0)]);
        }
        Icon::Copy => {
            closed(&[(5.5, 5.0), (14.0, 5.0), (14.0, 14.5), (5.5, 14.5)]);
            line(&[(2.0, 11.0), (2.0, 1.5), (10.5, 1.5)]);
        }
        Icon::Paste => {
            closed(&[(2.5, 3.5), (13.5, 3.5), (13.5, 14.5), (2.5, 14.5)]);
            bar(5.5, 1.5, 10.5, 5.5);
        }
        Icon::Undo | Icon::Redo => {
            let m = |x: f32| if icon == Icon::Undo { x } else { 16.0 - x };
            line(&[(m(3.5), 6.5), (m(11.0), 6.5), (m(13.5), 9.0), (m(13.5), 11.0), (m(11.0), 13.5), (m(6.0), 13.5)]);
            line(&[(m(6.5), 3.0), (m(3.0), 6.5), (m(6.5), 10.0)]);
        }
        Icon::Find | Icon::Search => {
            closed(&[(6.0, 1.8), (10.0, 1.8), (12.0, 4.0), (12.0, 8.0), (10.0, 10.2), (6.0, 10.2), (4.0, 8.0), (4.0, 4.0)]);
            line(&[(5.0, 9.5), (1.5, 14.5)]);
        }
        Icon::Run => fill(&[(4.0, 2.0), (14.0, 8.0), (4.0, 14.0)]),
        Icon::RunAgain => {
            fill(&[(7.0, 3.0), (14.5, 8.0), (7.0, 13.0)]);
            bar(2.5, 3.0, 4.5, 13.0);
        }
        Icon::Stop => bar(3.0, 3.0, 13.0, 13.0),
        Icon::Pause => {
            bar(3.5, 2.5, 6.5, 13.5);
            bar(9.5, 2.5, 12.5, 13.5);
        }
        Icon::StepOver => {
            line(&[(2.0, 9.0), (2.0, 5.0), (5.0, 2.5), (11.0, 2.5), (14.0, 5.0), (14.0, 9.0)]);
            line(&[(11.0, 6.5), (14.0, 9.5), (16.0, 6.5)]);
            bar(6.5, 11.0, 9.5, 14.0);
        }
        Icon::StepIn | Icon::StepOut => {
            bar(6.5, 12.0, 9.5, 15.0);
            bar(7.3, 1.0, 8.7, 9.0);
            if icon == Icon::StepIn {
                line(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)]);
            } else {
                line(&[(4.0, 5.0), (8.0, 1.0), (12.0, 5.0)]);
            }
        }
        Icon::Check => line(&[(2.0, 8.5), (6.0, 13.0), (14.0, 3.0)]),
        Icon::Build => {
            closed(&[(2.0, 5.0), (8.0, 2.0), (14.0, 5.0), (14.0, 12.0), (8.0, 15.0), (2.0, 12.0)]);
            line(&[(2.0, 5.0), (8.0, 8.0), (14.0, 5.0)]);
            line(&[(8.0, 8.0), (8.0, 15.0)]);
        }
        Icon::Bug => {
            closed(&[(5.0, 5.0), (11.0, 5.0), (12.0, 7.0), (12.0, 13.0), (10.0, 15.0), (6.0, 15.0), (4.0, 13.0), (4.0, 7.0)]);
            line(&[(6.0, 5.0), (6.0, 2.5), (10.0, 2.5), (10.0, 5.0)]);
            line(&[(1.0, 7.0), (4.0, 9.0)]);
            line(&[(15.0, 7.0), (12.0, 9.0)]);
            line(&[(1.0, 14.0), (4.0, 12.0)]);
            line(&[(15.0, 14.0), (12.0, 12.0)]);
        }
        Icon::Explorer => {
            closed(&[(5.5, 1.5), (11.0, 1.5), (14.0, 4.5), (14.0, 11.5), (5.5, 11.5)]);
            line(&[(2.0, 5.0), (2.0, 14.5), (10.5, 14.5)]);
        }
        Icon::Outline => {
            bar(2.0, 3.0, 4.0, 5.0);
            bar(6.0, 3.3, 14.0, 4.7);
            bar(4.0, 7.0, 6.0, 9.0);
            bar(8.0, 7.3, 14.0, 8.7);
            bar(2.0, 11.0, 4.0, 13.0);
            bar(6.0, 11.3, 14.0, 12.7);
        }
        Icon::Book => {
            closed(&[(2.0, 2.5), (8.0, 4.0), (14.0, 2.5), (14.0, 13.0), (8.0, 14.5), (2.0, 13.0)]);
            line(&[(8.0, 4.0), (8.0, 14.5)]);
        }
        Icon::Gear => {
            closed(&[(5.0, 3.0), (11.0, 3.0), (13.0, 5.0), (13.0, 11.0), (11.0, 13.0), (5.0, 13.0), (3.0, 11.0), (3.0, 5.0)]);
            bar(6.5, 6.5, 9.5, 9.5);
            bar(7.0, 0.8, 9.0, 3.0);
            bar(7.0, 13.0, 9.0, 15.2);
            bar(0.8, 7.0, 3.0, 9.0);
            bar(13.0, 7.0, 15.2, 9.0);
        }
        Icon::Close => {
            line(&[(3.5, 3.5), (12.5, 12.5)]);
            line(&[(12.5, 3.5), (3.5, 12.5)]);
        }
        Icon::Right => fill(&[(5.5, 3.5), (11.5, 8.0), (5.5, 12.5)]),
        Icon::Down => fill(&[(3.5, 5.5), (12.5, 5.5), (8.0, 11.5)]),
        Icon::Up => fill(&[(3.5, 11.0), (12.5, 11.0), (8.0, 5.0)]),
        Icon::Warning => {
            closed(&[(8.0, 1.5), (15.0, 14.0), (1.0, 14.0)]);
            bar(7.3, 6.0, 8.7, 10.5);
            bar(7.3, 11.5, 8.7, 12.9);
        }
        Icon::Error => {
            closed(&[(5.0, 1.5), (11.0, 1.5), (14.5, 5.0), (14.5, 11.0), (11.0, 14.5), (5.0, 14.5), (1.5, 11.0), (1.5, 5.0)]);
            line(&[(5.5, 5.5), (10.5, 10.5)]);
            line(&[(10.5, 5.5), (5.5, 10.5)]);
        }
        Icon::Split => {
            closed(&[(2.0, 2.5), (14.0, 2.5), (14.0, 13.5), (2.0, 13.5)]);
            bar(7.4, 2.5, 8.6, 13.5);
        }
        Icon::Palette => {
            closed(&[(1.5, 2.5), (14.5, 2.5), (14.5, 13.5), (1.5, 13.5)]);
            line(&[(4.0, 5.5), (7.0, 8.0), (4.0, 10.5)]);
            bar(8.0, 10.0, 12.0, 11.3);
        }
        Icon::Plus => {
            bar(7.3, 3.0, 8.7, 13.0);
            bar(3.0, 7.3, 13.0, 8.7);
        }
        Icon::Refresh => {
            line(&[(13.5, 8.0), (13.5, 11.0), (11.0, 13.5), (5.0, 13.5), (2.5, 11.0), (2.5, 5.0), (5.0, 2.5), (11.0, 2.5), (13.5, 5.0)]);
            line(&[(13.5, 1.5), (13.5, 5.0), (10.0, 5.0)]);
        }
    }
}

/// A square icon button: flat until hovered.
pub fn icon_button(ui: &mut Ui, th: &Theme, icon: Icon, tip: &str, enabled: bool, on: bool) -> Response {
    icon_button_coloured(ui, th, icon, tip, enabled, on, th.text)
}

pub fn icon_button_coloured(ui: &mut Ui, th: &Theme, icon: Icon, tip: &str, enabled: bool, on: bool, colour: Color32) -> Response {
    let size = Vec2::splat(26.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(r) {
        let fill = if on || (resp.is_pointer_button_down_on() && enabled) {
            th.active
        } else if resp.hovered() && enabled {
            th.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(r, 0.0, fill);
        if on {
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, th.accent), StrokeKind::Inside);
        }
        let c = if enabled { colour } else { th.faint };
        paint_icon(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(16.0)), icon, c);
    }
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

/// A menu row with the action's name and keyboard shortcut.
pub fn menu_item(ui: &mut Ui, a: Action, enabled: bool) -> bool {
    let info = actions::info(a);
    check_item(ui, info.title, &actions::shortcut_text(a), None, enabled)
}

pub fn check_item(ui: &mut Ui, title: &str, keys: &str, checked: Option<bool>, enabled: bool) -> bool {
    let width = ui.available_width().max(240.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(width, 23.0), if enabled { Sense::click() } else { Sense::hover() });
    let v = ui.visuals();
    let (text, dim, sel) = (v.text_color(), v.widgets.noninteractive.fg_stroke.color, v.selection.bg_fill);
    if resp.hovered() && enabled {
        ui.painter().rect_filled(r, 0.0, sel);
    }
    let colour = if enabled { text } else { dim.gamma_multiply(0.6) };
    let left = Pos2::new(r.left() + 24.0, r.center().y);
    ui.painter().text(left, Align2::LEFT_CENTER, title, FontId::proportional(13.0), colour);
    if !keys.is_empty() {
        ui.painter().text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, keys, FontId::proportional(12.0), dim);
    }
    if checked == Some(true) {
        paint_icon(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 12.0, r.center().y), Vec2::splat(12.0)), Icon::Check, text);
    }
    let clicked = resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A flat row that fills the width: list items in the side panels.
pub fn row(ui: &mut Ui, th: &Theme, height: f32, selected: bool) -> (Rect, Response) {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    if selected {
        ui.painter().rect_filled(r, 0.0, th.active);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, th.hover);
    }
    (r, resp)
}

/// A square push button in the accent colour.
pub fn primary_button(ui: &mut Ui, th: &Theme, text: &str) -> Response {
    ui.add(egui::Button::new(egui::RichText::new(text).color(th.on_accent).strong()).fill(th.accent).corner_radius(0.0))
}

pub fn panel_title(ui: &mut Ui, th: &Theme, text: &str) {
    ui.label(egui::RichText::new(text.to_uppercase()).size(11.0).color(th.dim).strong());
}

/// Score for fuzzy matching, lower is better; `None` when it does not match.
pub fn fuzzy(query: &str, text: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let q = query.to_lowercase();
    let t = text.to_lowercase();
    if t.starts_with(&q) {
        return Some(1);
    }
    if let Some(i) = t.find(&q) {
        return Some(10 + i as u32);
    }
    let mut pos = 0u32;
    let mut spread = 0u32;
    let mut it = t.chars();
    for c in q.chars() {
        let mut gap = 0;
        loop {
            let ch = it.next()?;
            pos += 1;
            if ch == c {
                break;
            }
            gap += 1;
        }
        spread += gap;
    }
    let _ = pos;
    Some(100 + spread)
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_prefers_close_matches() {
        assert!(fuzzy("pos", "PositionEntity").unwrap() < fuzzy("pos", "RePosition").unwrap());
        assert!(fuzzy("pe", "PositionEntity").is_some());
        assert!(fuzzy("xyz", "PositionEntity").is_none());
    }
}
