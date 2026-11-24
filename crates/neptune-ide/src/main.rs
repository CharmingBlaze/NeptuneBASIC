//! Neptune IDE: the editor for NeptuneBASIC.
//!
//! ```text
//! neptune-ide [file.nb | folder]...
//! ```
//!
//! It follows the Blitz3D IDE's way of working (F5 runs, tabs for files, a
//! list of functions, types and labels, F1 for help on the word at the caret,
//! a stop-and-step debugger) with what a modern editor adds. Compiling,
//! running and debugging are done by the `neptune` program.

// no console window behind the IDE in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod buffer;
mod capture;
mod debug;
mod dialogs;
mod document;
mod editor;
#[allow(dead_code)]
#[path = "../../neptune-runtime/src/json.rs"]
mod json;
mod lang;
mod manual;
mod panels;
mod procs;
mod settings;
mod theme;
mod ui_util;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Sharper, familiar system fonts where the machine has them; the fonts built
/// into egui otherwise.
fn load_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let mono: Vec<String> = if cfg!(windows) {
        vec![format!(r"{windows}\Fonts\CascadiaMono.ttf"), format!(r"{windows}\Fonts\consola.ttf")]
    } else if cfg!(target_os = "macos") {
        vec!["/System/Library/Fonts/SFNSMono.ttf".into(), "/System/Library/Fonts/Menlo.ttc".into()]
    } else {
        vec!["/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf".into(), "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into()]
    };
    let ui: Vec<String> = if cfg!(windows) {
        vec![format!(r"{windows}\Fonts\segoeui.ttf")]
    } else if cfg!(target_os = "macos") {
        vec!["/System/Library/Fonts/SFNS.ttf".into(), "/System/Library/Fonts/Helvetica.ttc".into()]
    } else {
        vec!["/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf".into(), "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()]
    };
    let mut install = |list: &[String], key: &str, family: FontFamily| {
        if let Some(bytes) = list.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts.font_data.insert(key.to_string(), Arc::new(FontData::from_owned(bytes)));
            fonts.families.entry(family).or_default().insert(0, key.to_string());
        }
    };
    install(&mono, "editor-mono", FontFamily::Monospace);
    install(&ui, "editor-ui", FontFamily::Proportional);
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut capture = None;
    let mut script = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--capture" => capture = args.next().map(PathBuf::from),
            "--step" => script.extend(args.next()),
            _ => files.push(PathBuf::from(a)),
        }
    }
    let (st, session) = settings::load();
    let size = session.window.unwrap_or((1360.0, 860.0));
    let capture = capture.map(|p| capture::Capture::new(p, script));
    let icon = {
        // a simple square mark: the planet and its ring
        let n = 32usize;
        let mut rgba = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
                let planet = dx * dx + dy * dy < 90.0;
                let ring = (dy + dx * 0.35).abs() < 1.8 && dx.abs() < 15.0;
                let c = if ring && !(planet && dy < 0.0) {
                    [0xE8, 0xB2, 0x3A, 255]
                } else if planet {
                    [0x2B, 0xC4, 0xE0, 255]
                } else {
                    [0x0E, 0x14, 0x20, 255]
                };
                rgba[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
            }
        }
        egui::IconData { rgba, width: n as u32, height: n as u32 }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Neptune IDE").with_inner_size([size.0, size.1]).with_min_inner_size([640.0, 400.0]).with_maximized(session.maximized).with_drag_and_drop(true).with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Neptune IDE",
        options,
        Box::new(move |cc| {
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, session, st, files, capture)))
        }),
    )
}
