//! The built-in manual: the chapters in `docs/manual`, drawn from Markdown.

use eframe::egui::{self, text::LayoutJob, FontFamily, FontId, RichText, Stroke, TextFormat, Ui};

use crate::lang::{self, Known};
use crate::theme::Theme;

include!(concat!(env!("OUT_DIR"), "/manual.rs"));

pub fn chapter_title(text: &str) -> &str {
    text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("Untitled")
}

pub enum Block {
    Heading(u8, String),
    Para(String),
    Item(String),
    /// The code, and whether it is a BASIC program.
    Code(String, bool),
    Table(Vec<Vec<String>>),
    Rule,
}

pub fn parse(md: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = md.lines().peekable();
    let mut para = String::new();
    let flush = |para: &mut String, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(std::mem::take(para)));
        }
    };
    while let Some(l) = lines.next() {
        let t = l.trim_end();
        if t.starts_with("<!--") {
            continue;
        }
        if let Some(lang) = t.strip_prefix("```") {
            flush(&mut para, &mut out);
            let basic = lang.is_empty() || lang == "basic";
            let mut code = String::new();
            for c in lines.by_ref() {
                if c.trim_end().starts_with("```") {
                    break;
                }
                code.push_str(c);
                code.push('\n');
            }
            out.push(Block::Code(code.trim_end().to_string(), basic && lang == "basic"));
        } else if t.is_empty() {
            flush(&mut para, &mut out);
        } else if let Some(h) = t.strip_prefix("### ") {
            flush(&mut para, &mut out);
            out.push(Block::Heading(3, h.to_string()));
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut out);
            out.push(Block::Heading(2, h.to_string()));
        } else if let Some(h) = t.strip_prefix("# ") {
            flush(&mut para, &mut out);
            out.push(Block::Heading(1, h.to_string()));
        } else if t == "---" {
            flush(&mut para, &mut out);
            out.push(Block::Rule);
        } else if let Some(i) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut out);
            out.push(Block::Item(i.to_string()));
        } else if t.starts_with('|') {
            flush(&mut para, &mut out);
            let mut rows = vec![t.to_string()];
            while lines.peek().is_some_and(|n| n.trim_start().starts_with('|')) {
                rows.push(lines.next().unwrap().trim_end().to_string());
            }
            let cells = rows
                .iter()
                .filter(|r| !r.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')))
                .map(|r| r.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect())
                .collect();
            out.push(Block::Table(cells));
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t.trim_start());
        }
    }
    flush(&mut para, &mut out);
    out
}

/// Inline Markdown: `code`, **bold** and [links](x) as their text.
fn inline(text: &str, size: f32, th: &Theme, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let plain = TextFormat::simple(FontId::proportional(size), th.text);
    let code = TextFormat { font_id: FontId::monospace(size - 1.0), color: th.command, background: th.hover, ..Default::default() };
    let bold = TextFormat { font_id: FontId::new(size, FontFamily::Proportional), color: th.accent, ..Default::default() };
    let mut rest = text;
    while !rest.is_empty() {
        let next = rest.find(['`', '*', '[']).unwrap_or(rest.len());
        if next > 0 {
            job.append(&rest[..next], 0.0, plain.clone());
            rest = &rest[next..];
            continue;
        }
        if let Some(r) = rest.strip_prefix('`') {
            if let Some(e) = r.find('`') {
                job.append(&r[..e], 0.0, code.clone());
                rest = &r[e + 1..];
                continue;
            }
        } else if let Some(r) = rest.strip_prefix("**") {
            if let Some(e) = r.find("**") {
                job.append(&r[..e], 0.0, bold.clone());
                rest = &r[e + 2..];
                continue;
            }
        } else if let Some(r) = rest.strip_prefix('[') {
            if let Some((label, after)) = r.split_once("](") {
                if let Some(e) = after.find(')') {
                    let mut f = plain.clone();
                    f.color = th.accent;
                    f.underline = Stroke::new(1.0, th.accent.gamma_multiply(0.5));
                    job.append(label, 0.0, f);
                    rest = &after[e + 1..];
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        job.append(&rest[..ch.len_utf8()], 0.0, plain.clone());
        rest = &rest[ch.len_utf8()..];
    }
    job
}

pub fn code_job(code: &str, known: &Known, th: &Theme, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::monospace(size);
    let mut toks = Vec::new();
    for (n, line) in code.split('\n').enumerate() {
        if n > 0 {
            job.append("\n", 0.0, TextFormat::simple(font.clone(), th.text));
        }
        let chars: Vec<char> = line.chars().collect();
        lang::tokenize(&chars, known, &mut toks);
        let mut at = 0;
        for t in &toks {
            if t.start > at {
                job.append(&chars[at..t.start].iter().collect::<String>(), 0.0, TextFormat::simple(font.clone(), th.text));
            }
            job.append(&chars[t.start..t.end].iter().collect::<String>(), 0.0, TextFormat::simple(font.clone(), th.token(t.kind)));
            at = t.end;
        }
        if at < chars.len() {
            job.append(&chars[at..].iter().collect::<String>(), 0.0, TextFormat::simple(font.clone(), th.text));
        }
    }
    job
}

/// Draws a chapter; returns source to open as a new program, if asked.
pub fn show(ui: &mut Ui, blocks: &[Block], known: &Known, th: &Theme) -> Option<String> {
    let mut open = None;
    let width = ui.available_width().min(860.0);
    ui.set_max_width(width);
    for (i, b) in blocks.iter().enumerate() {
        match b {
            Block::Heading(level, t) => {
                let size = match level {
                    1 => 28.0,
                    2 => 20.0,
                    _ => 16.0,
                };
                ui.add_space(if *level == 1 { 4.0 } else { 16.0 });
                ui.label(RichText::new(t).size(size).color(if *level == 3 { th.accent } else { th.text }).strong());
                ui.add_space(4.0);
            }
            Block::Para(t) => {
                ui.label(inline(t, 14.5, th, width));
                ui.add_space(6.0);
            }
            Block::Item(t) => {
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::hover());
                    ui.painter().rect_filled(egui::Rect::from_center_size(egui::pos2(r.center().x, r.top() + 9.0), egui::vec2(4.0, 4.0)), 0.0, th.accent);
                    ui.label(inline(t, 14.5, th, width - 24.0));
                });
            }
            Block::Code(c, basic) => {
                egui::Frame::new().fill(th.bg).stroke(Stroke::new(1.0, th.line)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(width - 24.0);
                    ui.horizontal(|ui| {
                        if *basic {
                            ui.add(egui::Label::new(code_job(c, known, th, 13.0)).selectable(true));
                        } else {
                            ui.add(egui::Label::new(RichText::new(c).font(FontId::monospace(13.0)).color(th.dim)).selectable(true));
                        }
                    });
                    if *basic {
                        ui.horizontal(|ui| {
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(c.clone());
                            }
                            if ui.small_button("Open as new program").clicked() {
                                open = Some(c.clone());
                            }
                        });
                    }
                });
                ui.add_space(8.0);
            }
            Block::Table(rows) => {
                egui::Grid::new(("mdtable", i)).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
                    for (y, row) in rows.iter().enumerate() {
                        for cell in row {
                            if y == 0 {
                                ui.label(RichText::new(cell).color(th.accent).strong());
                            } else {
                                ui.label(inline(cell, 13.5, th, 380.0));
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chapter_has_a_title() {
        assert!(CHAPTERS.len() > 10);
        for (_, text) in CHAPTERS {
            assert_ne!(chapter_title(text), "Untitled");
        }
    }

    #[test]
    fn parses_markdown_blocks() {
        let b = parse("# T\n\nsome\ntext\n\n```\ncode\n```\n- item\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(matches!(&b[0], Block::Heading(1, t) if t == "T"));
        assert!(matches!(&b[1], Block::Para(t) if t == "some text"));
        assert!(matches!(&b[2], Block::Code(t, false) if t == "code"));
        assert!(matches!(&b[3], Block::Item(t) if t == "item"));
        assert!(matches!(&b[4], Block::Table(r) if r.len() == 2 && r[1][1] == "2"));
    }
}
