//! `--capture out.png`: draws a few frames, saves what is on screen and
//! exits. Used to check the interface without someone at the keyboard.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::{self, ColorImage, ViewportCommand};

use crate::actions::{self, Action};
use crate::app::App;

pub struct Capture {
    path: PathBuf,
    /// Frames still to draw, and what to do on each script step.
    frame: u32,
    script: Vec<String>,
    asked: bool,
    done: Arc<AtomicBool>,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    times: Vec<std::time::Duration>,
}

impl Capture {
    pub fn new(path: PathBuf, script: Vec<String>) -> Self {
        Capture { path, frame: 0, script, asked: false, done: Arc::new(AtomicBool::new(false)), events: Vec::new(), modifiers: egui::Modifiers::NONE, times: Vec::new() }
    }

    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        if !self.events.is_empty() {
            raw.events.push(egui::Event::ModifiersChanged(self.modifiers));
        }
        raw.events.append(&mut self.events);
    }

    pub fn timed(&mut self, t: std::time::Duration) {
        self.times.push(t);
    }

    fn report_times(&self) {
        // the first frames build the font atlas and do not reflect typing
        let mut ms: Vec<f32> = self.times.iter().skip(10).map(|d| d.as_secs_f32() * 1000.0).collect();
        ms.sort_by(f32::total_cmp);
        if let (Some(mid), Some(top)) = (ms.get(ms.len() / 2), ms.last()) {
            eprintln!("frame cost: median {mid:.2} ms, worst {top:.2} ms over {} frames", ms.len());
        }
    }

    /// Presses keys like real typing: plain text, with chords such as `<ctrl+b>`.
    fn queue_keys(&mut self, text: &str) {
        let mut rest = text;
        while !rest.is_empty() {
            if let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
                let chord = &rest[1..end + 1];
                let mut m = egui::Modifiers::NONE;
                let mut key = None;
                for part in chord.split('+') {
                    match part.to_ascii_lowercase().as_str() {
                        "ctrl" => {
                            m.ctrl = true;
                            m.command = true;
                        }
                        "shift" => m.shift = true,
                        "alt" => m.alt = true,
                        k => key = egui::Key::from_name(part).or_else(|| egui::Key::from_name(&title(k))),
                    }
                }
                if let Some(key) = key {
                    for pressed in [true, false] {
                        self.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: m });
                    }
                    self.modifiers = m;
                }
                rest = &rest[end + 2..];
            } else {
                let c = rest.chars().next().unwrap();
                self.events.push(egui::Event::Text(c.to_string()));
                rest = &rest[c.len_utf8()..];
            }
        }
    }

    /// Returns true once the picture has been written.
    pub fn step(&mut self, ctx: &egui::Context, app: &mut App) -> bool {
        self.frame += 1;
        ctx.request_repaint();
        // a script step every few frames, with time for slow things to arrive
        if self.frame.is_multiple_of(12) && !self.script.is_empty() {
            self.modifiers = egui::Modifiers::NONE;
            let cmd = self.script.remove(0);
            if let Some(keys) = cmd.strip_prefix("keys:") {
                self.queue_keys(keys);
            } else {
                run_step(&cmd, app, ctx);
            }
            return false;
        }
        if !self.script.is_empty() || self.frame < 24 {
            return false;
        }
        if !self.asked {
            self.asked = true;
            let (path, done, repaint) = (self.path.clone(), self.done.clone(), ctx.clone());
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image: Arc<ColorImage>| {
                if let Err(e) = write_png(&path, &image) {
                    eprintln!("capture failed: {e}");
                }
                done.store(true, Ordering::Relaxed);
                repaint.request_repaint();
            })));
            return false;
        }
        if self.done.load(Ordering::Relaxed) {
            self.report_times();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return true;
        }
        false
    }
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
}

fn run_step(cmd: &str, app: &mut App, ctx: &egui::Context) {
    if let Some(text) = cmd.strip_prefix("type:") {
        let text = text.replace("\\n", "\n").replace("\\t", "\t");
        app.with_doc(|d| d.buf.insert(&text, crate::buffer::EditKind::Other));
    } else if let Some(file) = cmd.strip_prefix("open:") {
        app.open_path(std::path::Path::new(file));
    } else if let Some(line) = cmd.strip_prefix("line:") {
        app.goto_pos_in_active(line.parse::<usize>().unwrap_or(1).saturating_sub(1), 0);
    } else if let Some(text) = cmd.strip_prefix("palette:") {
        app.palette.open = true;
        app.palette.query = text.to_string();
    } else if let Some(text) = cmd.strip_prefix("help:") {
        app.show_reference(text);
    } else if let Some(text) = cmd.strip_prefix("complete:") {
        app.with_doc(|d| d.buf.insert(text, crate::buffer::EditKind::Other));
        app.queue.push(Action::TriggerCompletion);
    } else if let Some(text) = cmd.strip_prefix("rename:") {
        if let Some((id, at)) = app.active().map(|d| (d.id, d.buf.primary().head)) {
            app.request_rename(id, at, text.to_string());
        }
    } else if let Some(text) = cmd.strip_prefix("replace-in-files:") {
        app.search_replace = text.to_string();
        app.replace_in_files();
    } else if cmd == "wait" {
        // gives slow things time to arrive
    } else if cmd == "breakpoint" {
        app.queue.push(Action::ToggleBreakpoint);
    } else if let Some(i) = actions::ALL.iter().find(|i| i.title.trim_end_matches("...").eq_ignore_ascii_case(cmd)) {
        app.perform(i.action, ctx);
    } else {
        eprintln!("capture: unknown step {cmd}");
    }
}

fn crc32(data: &[u8], mut crc: u32) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = !crc32(data, crc32(kind, !0));
    out.extend_from_slice(&crc.to_be_bytes());
}

/// An uncompressed PNG: bigger than it could be, but needs no library.
fn write_png(path: &std::path::Path, image: &ColorImage) -> std::io::Result<()> {
    let (w, h) = (image.width(), image.height());
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in image.pixels.chunks(w) {
        raw.push(0);
        for p in row {
            let [r, g, b, _] = p.to_srgba_unmultiplied();
            raw.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(blocks.peek().is_none() as u8);
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &raw {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    z.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn crc_matches_the_png_standard() {
        assert_eq!(!crc32(b"IEND", !0), 0xAE42_6082);
    }
}
