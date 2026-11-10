//! The IDE's state and everything it can do. Drawing lives in `view.rs`,
//! `panels.rs` and `dialogs.rs`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, KeyboardShortcut, ViewportCommand};

use crate::actions::{self, Action};
use crate::buffer::{Caret, Pos};
use crate::dialogs::{Dialog, FileDialog, FileMode, PromptKind};
use crate::document::{Diag, Doc};
use crate::editor::{self, CompItem, CompKind, Completion, Reveal, View};
use crate::json::Json;
use crate::lang::{self, Known, Tk};
use crate::procs::{self, Dap, Lsp, Msg, Poster, RunKind, Running};
use crate::settings::{self, SavedDoc, Session, Settings};
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Doc(u64),
    Welcome,
    Manual,
}

pub struct Pane {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub views: HashMap<u64, View>,
    pub id: usize,
}

impl Pane {
    pub fn new(id: usize) -> Self {
        Pane { tabs: Vec::new(), active: 0, views: HashMap::new(), id }
    }
    pub fn view(&mut self, doc: u64) -> &mut View {
        let id = self.id;
        self.views.entry(doc).or_insert_with(|| View::new(id))
    }
    pub fn current(&self) -> Option<Tab> {
        self.tabs.get(self.active).copied()
    }
}

#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub replace: bool,
    pub query: String,
    pub replacement: String,
    pub case: bool,
    pub word: bool,
    pub matches: Vec<(Pos, Pos)>,
    pub current: Option<usize>,
    pub key: (u64, u64, String, bool, bool),
    pub focus: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SideTab {
    Explorer,
    Outline,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Output,
    Problems,
    Debug,
}

pub struct OutLine {
    pub text: String,
    pub error: bool,
    pub loc: Option<(String, usize, usize)>,
}

pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    pub len: usize,
    pub text: String,
}

pub struct Var {
    pub name: String,
    pub value: String,
    pub reference: u32,
}

pub struct Frame {
    pub name: String,
    pub path: String,
    pub line: usize,
    pub col: usize,
}

pub struct Watch {
    pub expr: String,
    pub value: String,
    pub reference: u32,
    pub failed: bool,
}

#[derive(Clone)]
pub enum Pending {
    Launch,
    Stack,
    Scopes,
    Vars(u32),
    Watch(usize),
    Hover(String),
    Other,
}

#[derive(Default)]
pub struct Debug {
    pub session: Option<Dap>,
    pub paused: bool,
    pub reason: String,
    pub exception: Option<String>,
    pub frames: Vec<Frame>,
    pub frame: usize,
    pub scopes: Vec<(String, u32)>,
    pub vars: HashMap<u32, Vec<Var>>,
    pub open: BTreeSet<String>,
    pub watches: Vec<Watch>,
    pub new_watch: String,
    pub pending: HashMap<u32, Pending>,
    pub hover: HashMap<String, String>,
}

pub struct Palette {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    pub focus: bool,
}

pub struct App {
    pub st: Settings,
    pub applied: Option<Settings>,
    pub th: &'static Theme,
    pub known: Known,
    pub comp_items: Vec<CompItem>,
    pub completion: Option<Completion>,
    pub docs: Vec<Doc>,
    pub next_id: u64,
    pub next_untitled: usize,
    pub panes: Vec<Pane>,
    pub focus_pane: usize,
    pub focus_editor: bool,
    pub editor_focused: bool,
    pub find: Find,
    pub palette: Palette,
    pub dialog: Option<Dialog>,
    pub queue: Vec<Action>,
    pub rx: Receiver<Msg>,
    pub post: Poster,
    pub lsp: Option<Lsp>,
    pub lsp_ready: bool,
    pub lsp_retry: Option<Instant>,
    pub running: Option<Running>,
    pub run_id: u64,
    pub stop_asked: bool,
    pub run_started: Instant,
    pub last_target: Option<PathBuf>,
    pub debug: Debug,
    pub output: Vec<OutLine>,
    pub out_open: bool,
    pub out_follow: bool,
    pub out_input: String,
    pub folder: Option<PathBuf>,
    pub file_index: Vec<PathBuf>,
    pub tree_open: HashSet<PathBuf>,
    pub tree_cache: HashMap<PathBuf, Vec<(PathBuf, bool)>>,
    pub tree_time: Instant,
    pub tree_selected: Option<PathBuf>,
    pub show_side: bool,
    pub show_dock: bool,
    pub show_ref: bool,
    pub side: SideTab,
    pub dock: DockTab,
    pub outline_filter: String,
    pub search_query: String,
    pub search_case: bool,
    pub search_word: bool,
    pub search_hits: Vec<SearchHit>,
    pub search_busy: bool,
    pub search_gen: Arc<AtomicU64>,
    pub search_focus: bool,
    pub ref_query: String,
    pub ref_focus: bool,
    pub ref_selected: Option<usize>,
    pub ref_scroll_to: bool,
    pub manual_page: usize,
    pub manual_cache: Option<(usize, Vec<crate::manual::Block>)>,
    pub manual_query: String,
    pub recent: Vec<String>,
    pub command_line: String,
    pub back: Vec<(u64, Pos)>,
    pub forward: Vec<(u64, Pos)>,
    pub status: Option<(String, Instant, bool)>,
    pub examples: Vec<PathBuf>,
    pub shortcuts: Vec<(Action, KeyboardShortcut)>,
    pub last_store: Instant,
    pub stored_versions: u64,
    pub last_disk_check: Instant,
    pub help_tip: Option<(String, String)>,
    pub capture: Option<crate::capture::Capture>,
    pub window_size: Option<(f32, f32)>,
    pub maximized: bool,
    pub title: String,
}

fn find_examples() -> Vec<PathBuf> {
    let Ok(exe) = std::env::current_exe() else { return Vec::new() };
    for dir in exe.ancestors().skip(1).take(6) {
        let e = dir.join("examples");
        let mut out: Vec<PathBuf> = std::fs::read_dir(&e).into_iter().flatten().flatten().map(|x| x.path()).filter(|p| procs::is_source(p)).collect();
        if let Ok(rd) = std::fs::read_dir(e.join("games")) {
            out.extend(rd.flatten().map(|x| x.path()).filter(|p| procs::is_source(p)));
        }
        // cargo makes an empty "examples" folder of its own next to the program
        if !out.is_empty() {
            out.sort();
            return out;
        }
    }
    Vec::new()
}

impl App {
    pub fn new(cc: &eframe::CreationContext, session: Session, st: Settings, files: Vec<PathBuf>, capture: Option<crate::capture::Capture>) -> Self {
        let (tx, rx) = channel();
        let post = Poster { tx, ctx: cc.egui_ctx.clone() };
        let mut shortcuts: Vec<(Action, KeyboardShortcut)> = actions::ALL.iter().filter_map(|i| actions::shortcut_of(i.action).map(|k| (i.action, k))).collect();
        // most specific first: Ctrl+Shift+S must be tried before Ctrl+S
        shortcuts.sort_by_key(|(_, k)| std::cmp::Reverse(k.modifiers.shift as u8 + k.modifiers.command as u8 + k.modifiers.alt as u8));
        let known = Known::new();
        let mut comp_items: Vec<CompItem> = lang::KEYWORDS.iter().map(|k| CompItem::new(k, CompKind::Keyword, "keyword", "")).collect();
        for h in &known.help {
            comp_items.push(CompItem::new(&h.name, CompKind::Command, "", &h.text));
        }
        for (trigger, title, body) in lang::SNIPPETS {
            let mut item = CompItem::new(trigger, CompKind::Snippet, title, body);
            item.snippet = Some(body);
            comp_items.push(item);
        }
        let th = Theme::by_name(&st.theme);
        let mut app = App {
            th,
            applied: None,
            known,
            comp_items,
            completion: None,
            docs: Vec::new(),
            next_id: 1,
            next_untitled: 1,
            panes: vec![Pane::new(0)],
            focus_pane: 0,
            focus_editor: true,
            editor_focused: false,
            find: Find::default(),
            palette: Palette { open: false, query: String::new(), selected: 0, focus: false },
            dialog: None,
            queue: Vec::new(),
            rx,
            post,
            lsp: None,
            lsp_ready: false,
            lsp_retry: Some(Instant::now()),
            running: None,
            run_id: 0,
            stop_asked: false,
            run_started: Instant::now(),
            last_target: None,
            debug: Debug::default(),
            output: Vec::new(),
            out_open: false,
            out_follow: true,
            out_input: String::new(),
            folder: None,
            file_index: Vec::new(),
            tree_open: HashSet::new(),
            tree_cache: HashMap::new(),
            tree_time: Instant::now(),
            tree_selected: None,
            show_side: session.sidebar.unwrap_or(true),
            show_dock: session.bottom.unwrap_or(true),
            show_ref: session.reference.unwrap_or(false),
            side: SideTab::Outline,
            dock: DockTab::Output,
            outline_filter: String::new(),
            search_query: String::new(),
            search_case: false,
            search_word: false,
            search_hits: Vec::new(),
            search_busy: false,
            search_gen: Arc::new(AtomicU64::new(0)),
            search_focus: false,
            ref_query: String::new(),
            ref_focus: false,
            ref_selected: None,
            ref_scroll_to: false,
            manual_page: 0,
            manual_cache: None,
            manual_query: String::new(),
            recent: session.recent.clone(),
            command_line: session.command_line.clone(),
            back: Vec::new(),
            forward: Vec::new(),
            status: None,
            examples: find_examples(),
            shortcuts,
            last_store: Instant::now(),
            stored_versions: 0,
            last_disk_check: Instant::now(),
            help_tip: None,
            capture,
            window_size: session.window,
            maximized: session.maximized,
            title: String::new(),
            st,
        };
        if !session.folder.is_empty() && Path::new(&session.folder).is_dir() {
            app.set_folder(PathBuf::from(&session.folder));
        }
        app.restore(&session);
        for f in files {
            if f.is_dir() {
                app.set_folder(f);
            } else {
                app.open_path(&f);
            }
        }
        if app.panes[0].tabs.is_empty() {
            app.panes[0].tabs.push(Tab::Welcome);
        }
        app
    }

    fn restore(&mut self, s: &Session) {
        for d in &s.docs {
            let id = self.fresh_id();
            let doc = if d.path.is_empty() {
                let mut doc = Doc::new(id, d.text.as_deref().unwrap_or(""));
                doc.untitled = d.untitled.max(1);
                self.next_untitled = self.next_untitled.max(doc.untitled + 1);
                if d.text.is_some() {
                    doc.buf.mark_unsaved();
                }
                doc
            } else {
                let path = PathBuf::from(&d.path);
                let opened = Doc::open(id, &path);
                match (opened, &d.text) {
                    (Ok(mut doc), Some(t)) => {
                        doc.buf = crate::buffer::Buffer::new(t);
                        doc.buf.mark_unsaved();
                        doc
                    }
                    (Ok(doc), None) => doc,
                    (Err(_), Some(t)) => {
                        let mut doc = Doc::new(id, t);
                        doc.path = Some(path);
                        doc.buf.mark_unsaved();
                        doc
                    }
                    (Err(_), None) => continue,
                }
            };
            let mut doc = doc;
            doc.sync();
            doc.breakpoints = d.breakpoints.iter().copied().collect();
            doc.break_rules = d.break_rules.iter().filter(|(l, _)| doc.breakpoints.contains(l)).cloned().collect();
            doc.bookmarks = d.bookmarks.iter().copied().collect();
            let p = doc.buf.clamp(Pos::new(d.line, d.col));
            doc.buf.set_caret(p);
            self.docs.push(doc);
            self.panes[0].tabs.push(Tab::Doc(id));
            self.panes[0].view(id).reveal = Some(Reveal::Center);
        }
        self.panes[0].active = s.active.min(self.panes[0].tabs.len().saturating_sub(1));
    }

    pub fn session(&self) -> Session {
        let docs = self
            .docs
            .iter()
            .map(|d| {
                let head = d.buf.primary().head;
                SavedDoc {
                    path: d.path.as_ref().map_or(String::new(), |p| p.display().to_string()),
                    text: d.buf.is_modified().then(|| d.buf.text()),
                    untitled: d.untitled,
                    line: head.line,
                    col: head.col,
                    breakpoints: d.breakpoints.iter().copied().collect(),
                    break_rules: d.break_rules.iter().map(|(l, r)| (*l, r.clone())).collect(),
                    bookmarks: d.bookmarks.iter().copied().collect(),
                }
            })
            .collect();
        let active = match self.panes[self.focus_pane].current() {
            Some(Tab::Doc(id)) => self.docs.iter().position(|d| d.id == id).unwrap_or(0),
            _ => 0,
        };
        Session {
            folder: self.folder.as_ref().map_or(String::new(), |p| p.display().to_string()),
            docs,
            active,
            recent: self.recent.clone(),
            command_line: self.command_line.clone(),
            window: self.window_size,
            maximized: self.maximized,
            sidebar: Some(self.show_side),
            bottom: Some(self.show_dock),
            reference: Some(self.show_ref),
        }
    }

    pub fn store_now(&mut self) {
        settings::store(&self.st, &self.session());
        self.last_store = Instant::now();
    }

    /// Saves settings and unsaved text a moment after anything changes.
    pub fn store_if_due(&mut self) {
        let sum: u64 = self.docs.iter().map(|d| d.buf.version).sum::<u64>() + self.docs.len() as u64;
        if sum != self.stored_versions && self.last_store.elapsed() > Duration::from_secs(3) {
            self.stored_versions = sum;
            self.store_now();
        }
    }

    pub fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn doc(&self, id: u64) -> Option<&Doc> {
        self.docs.iter().find(|d| d.id == id)
    }

    pub fn doc_mut(&mut self, id: u64) -> Option<&mut Doc> {
        self.docs.iter_mut().find(|d| d.id == id)
    }

    pub fn active_id(&self) -> Option<u64> {
        match self.panes[self.focus_pane].current() {
            Some(Tab::Doc(id)) => Some(id),
            _ => None,
        }
    }

    pub fn active(&self) -> Option<&Doc> {
        self.active_id().and_then(|id| self.doc(id))
    }

    pub fn active_mut(&mut self) -> Option<&mut Doc> {
        let id = self.active_id()?;
        self.doc_mut(id)
    }

    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some((text.into(), Instant::now(), error));
    }

    // ---------------------------------------------------------- files and tabs

    pub fn new_file(&mut self) {
        let id = self.fresh_id();
        let mut d = Doc::new(id, "");
        d.untitled = self.next_untitled;
        self.next_untitled += 1;
        self.docs.push(d);
        self.show_tab(Tab::Doc(id));
    }

    pub fn show_tab(&mut self, t: Tab) {
        let p = &mut self.panes[self.focus_pane];
        match p.tabs.iter().position(|x| *x == t) {
            Some(i) => p.active = i,
            None => {
                let at = (p.active + 1).min(p.tabs.len());
                p.tabs.insert(at, t);
                p.active = at;
            }
        }
        // the welcome page steps aside once there is real work
        if matches!(t, Tab::Doc(_)) {
            let p = &mut self.panes[self.focus_pane];
            if let Some(i) = p.tabs.iter().position(|x| *x == Tab::Welcome) {
                p.tabs.remove(i);
                if p.active > i {
                    p.active -= 1;
                }
            }
        }
        self.focus_editor = true;
    }

    pub fn add_recent(&mut self, path: &Path) {
        let s = path.display().to_string();
        self.recent.retain(|r| *r != s);
        self.recent.insert(0, s);
        self.recent.truncate(14);
    }

    pub fn open_path(&mut self, path: &Path) -> Option<u64> {
        let path = path.canonicalize().map(|p| clean_path(&p)).unwrap_or_else(|_| path.to_path_buf());
        if let Some(d) = self.docs.iter().find(|d| d.path.as_ref() == Some(&path)) {
            let id = d.id;
            self.show_tab(Tab::Doc(id));
            return Some(id);
        }
        let id = self.fresh_id();
        match Doc::open(id, &path) {
            Ok(mut d) => {
                d.sync();
                self.docs.push(d);
                self.add_recent(&path);
                self.show_tab(Tab::Doc(id));
                Some(id)
            }
            Err(e) => {
                self.say(e, true);
                None
            }
        }
    }

    pub fn set_folder(&mut self, path: PathBuf) {
        let path = path.canonicalize().map(|p| clean_path(&p)).unwrap_or(path);
        self.tree_open.clear();
        self.tree_cache.clear();
        self.tree_open.insert(path.clone());
        procs::index_files(path.clone(), self.post.clone());
        self.folder = Some(path);
        self.side = SideTab::Explorer;
        self.show_side = true;
    }

    pub fn save(&mut self, id: u64) -> bool {
        let trim = self.st.trim_on_save;
        let Some(d) = self.doc_mut(id) else { return false };
        if d.path.is_none() {
            self.save_as(id);
            return false;
        }
        match d.save(trim) {
            Ok(()) => {
                let name = d.title();
                self.say(format!("Saved {name}"), false);
                true
            }
            Err(e) => {
                self.say(e, true);
                false
            }
        }
    }

    pub fn save_as(&mut self, id: u64) {
        let name = self.doc(id).map_or("program.nb".to_string(), Doc::title);
        let start = self.doc(id).and_then(|d| d.path.as_ref()).and_then(|p| p.parent().map(Path::to_path_buf)).or_else(|| self.folder.clone());
        self.dialog = Some(Dialog::File(FileDialog::new(FileMode::SaveAs(id), start, &name)));
    }

    pub fn finish_save_as(&mut self, id: u64, path: PathBuf) {
        let trim = self.st.trim_on_save;
        let Some(d) = self.doc_mut(id) else { return };
        d.path = Some(path.clone());
        d.lsp_open = false;
        match d.save(trim) {
            Ok(()) => {
                self.add_recent(&path);
                self.say(format!("Saved {}", path.display()), false);
            }
            Err(e) => self.say(e, true),
        }
    }

    pub fn save_all(&mut self) {
        let ids: Vec<u64> = self.docs.iter().filter(|d| d.buf.is_modified() && d.path.is_some()).map(|d| d.id).collect();
        for id in ids {
            self.save(id);
        }
    }

    /// Asks first when this tab holds the last view of unsaved text.
    pub fn request_close(&mut self, pane: usize, index: usize) {
        if let Some(Tab::Doc(id)) = self.panes[pane].tabs.get(index).copied() {
            let views = self.panes.iter().flat_map(|p| &p.tabs).filter(|t| **t == Tab::Doc(id)).count();
            if views == 1 && self.doc(id).is_some_and(|d| d.buf.is_modified()) {
                self.dialog = Some(Dialog::CloseUnsaved { pane, index, id });
                return;
            }
        }
        self.close_tab(pane, index);
    }

    pub fn close_tab(&mut self, pane: usize, index: usize) {
        let Some(tab) = self.panes[pane].tabs.get(index).copied() else { return };
        self.panes[pane].tabs.remove(index);
        let p = &mut self.panes[pane];
        if p.active > index || p.active >= p.tabs.len() {
            p.active = p.active.saturating_sub(1);
        }
        if let Tab::Doc(id) = tab {
            p.views.remove(&id);
            if !self.panes.iter().any(|p| p.tabs.contains(&tab)) {
                if let Some(i) = self.docs.iter().position(|d| d.id == id) {
                    if let Some(lsp) = &self.lsp {
                        lsp.did_close(&doc_uri(&self.docs[i]));
                    }
                    self.docs.remove(i);
                }
            }
        }
        if self.panes.len() > 1 && self.panes[pane].tabs.is_empty() {
            self.panes.remove(pane);
            self.focus_pane = 0;
            self.panes[0].id = 0;
        }
        self.focus_editor = true;
        self.find.key.0 = 0;
    }

    pub fn close_all(&mut self) {
        for pane in (0..self.panes.len()).rev() {
            for i in (0..self.panes[pane].tabs.len()).rev() {
                let unsaved = matches!(self.panes[pane].tabs[i], Tab::Doc(id) if self.doc(id).is_some_and(|d| d.buf.is_modified()));
                if !unsaved && pane < self.panes.len() {
                    self.close_tab(pane, i);
                }
            }
        }
        let left = self.docs.iter().filter(|d| d.buf.is_modified()).count();
        if left > 0 {
            self.say(format!("{left} unsaved file(s) left open"), false);
        }
    }

    pub fn goto(&mut self, id: u64, p: Pos, select_to: Option<Pos>) {
        if let Some(cur) = self.active() {
            let at = (cur.id, cur.buf.primary().head);
            if self.back.last() != Some(&at) {
                self.back.push(at);
                if self.back.len() > 200 {
                    self.back.remove(0);
                }
            }
            self.forward.clear();
        }
        self.jump(id, p, select_to);
    }

    fn jump(&mut self, id: u64, p: Pos, select_to: Option<Pos>) {
        self.show_tab(Tab::Doc(id));
        let Some(d) = self.doc_mut(id) else { return };
        let p = d.buf.clamp(p);
        match select_to {
            Some(q) => d.buf.select(p, q),
            None => d.buf.set_caret(p),
        }
        d.unfold_around(p.line);
        let pane = self.focus_pane;
        self.panes[pane].view(id).reveal = Some(Reveal::Center);
        self.completion = None;
    }

    pub fn goto_file(&mut self, path: &Path, line: usize, col: usize) {
        if let Some(id) = self.open_path(path) {
            self.goto(id, Pos::new(line, col), None);
        }
    }

    pub fn resolve_output_path(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            return path.to_path_buf();
        }
        if let Some(t) = self.last_target.as_ref().and_then(|t| t.parent()) {
            let j = t.join(path);
            if j.exists() {
                return j;
            }
        }
        path.to_path_buf()
    }

    fn include_targets(&self, d: &Doc) -> Vec<PathBuf> {
        let Some(dir) = d.path.as_ref().and_then(|p| p.parent()) else { return Vec::new() };
        d.buf
            .lines()
            .iter()
            .filter_map(|l| {
                let t = l.trim_start();
                if !t.to_ascii_lowercase().starts_with("include") {
                    return None;
                }
                let a = t.find('"')?;
                let b = t[a + 1..].find('"')?;
                Some(dir.join(&t[a + 1..a + 1 + b]))
            })
            .collect()
    }

    pub fn goto_definition(&mut self, word: &str) {
        // this file, the other open files, then the files it includes
        let found = self.active().and_then(|d| d.decls.iter().find(|x| x.name.eq_ignore_ascii_case(word)).map(|x| (d.id, x.line))).or_else(|| {
            self.docs.iter().find_map(|d| d.decls.iter().find(|x| x.name.eq_ignore_ascii_case(word)).map(|x| (d.id, x.line)))
        });
        if let Some((id, line)) = found {
            self.goto(id, Pos::new(line, 0), None);
            return;
        }
        if let Some(d) = self.active() {
            for inc in self.include_targets(d) {
                let Ok(bytes) = std::fs::read(&inc) else { continue };
                let (text, _) = crate::document::decode(&bytes);
                let lines: Vec<String> = text.lines().map(str::to_string).collect();
                if let Some(x) = lang::declarations(&lines).into_iter().find(|x| x.name.eq_ignore_ascii_case(word)) {
                    self.goto_file(&inc, x.line, 0);
                    return;
                }
            }
        }
        if self.known.help_for(word).is_some() {
            self.show_reference(word);
        } else {
            self.say(format!("No definition found for {word}"), false);
        }
    }

    pub fn show_reference(&mut self, word: &str) {
        if let Some(&i) = self.known.help_index.get(&word.to_ascii_lowercase()) {
            self.ref_selected = Some(i);
            self.ref_query.clear();
            self.ref_scroll_to = true;
            self.show_ref = true;
        }
    }

    fn word_at_caret(&self) -> Option<String> {
        let d = self.active()?;
        let h = d.buf.primary().head;
        lang::word_at(d.buf.line(h.line), h.col).map(|x| x.0)
    }

    // ---------------------------------------------------------- find

    pub fn refresh_find(&mut self) {
        let Some(id) = self.active_id() else {
            self.find.matches.clear();
            return;
        };
        let version = self.doc(id).map_or(0, |d| d.buf.version);
        let key = (id, version, self.find.query.clone(), self.find.case, self.find.word);
        if self.find.key == key {
            return;
        }
        self.find.key = key;
        self.find.matches.clear();
        self.find.current = None;
        if !self.find.open || self.find.query.is_empty() {
            return;
        }
        let o = procs::SearchOptions { needle: self.find.query.clone(), case: self.find.case, word: self.find.word };
        let Some(d) = self.doc(id) else { return };
        let mut out = Vec::new();
        for (l, text) in d.buf.lines().iter().enumerate() {
            for (c, n) in procs::find_in_line(text, &o) {
                out.push((Pos::new(l, c), Pos::new(l, c + n)));
            }
            if out.len() >= 100_000 {
                break;
            }
        }
        self.find.matches = out;
    }

    /// Selects the next or previous match from the caret.
    pub fn find_step(&mut self, forward: bool) {
        self.find.open = true;
        self.find.key.0 = 0;
        self.refresh_find();
        if self.find.matches.is_empty() {
            return;
        }
        let Some(id) = self.active_id() else { return };
        let Some(d) = self.doc(id) else { return };
        let c = d.buf.primary();
        let n = self.find.matches.len();
        let i = if forward {
            self.find.matches.iter().position(|(a, _)| *a >= c.end()).unwrap_or(0)
        } else {
            self.find.matches.iter().rposition(|(_, b)| *b <= c.start()).unwrap_or(n - 1)
        };
        self.find.current = Some(i);
        let (a, b) = self.find.matches[i];
        let pane = self.focus_pane;
        if let Some(d) = self.doc_mut(id) {
            d.unfold_around(a.line);
            d.buf.select(a, b);
        }
        self.panes[pane].view(id).reveal = Some(Reveal::Center);
    }

    pub fn open_find(&mut self, replace: bool) {
        if let Some(d) = self.active() {
            let c = d.buf.primary();
            if c.has_selection() && c.start().line == c.end().line {
                self.find.query = d.buf.text_in(c.start(), c.end());
            } else if let Some(w) = self.word_at_caret().filter(|_| self.find.query.is_empty()) {
                self.find.query = w;
            }
        }
        self.find.open = true;
        self.find.replace = replace;
        self.find.focus = true;
        self.find.key.0 = 0;
    }

    pub fn replace_one(&mut self) {
        let Some(id) = self.active_id() else { return };
        let rep = self.find.replacement.clone();
        self.refresh_find();
        let sel = self.doc(id).map(|d| d.buf.primary());
        let hit = sel.is_some_and(|c| self.find.matches.iter().any(|(a, b)| *a == c.start() && *b == c.end()));
        if hit {
            if let Some(d) = self.doc_mut(id) {
                d.buf.insert(&rep, crate::buffer::EditKind::Other);
            }
        }
        self.find_step(true);
    }

    pub fn replace_all(&mut self) {
        let Some(id) = self.active_id() else { return };
        self.refresh_find();
        let rep = self.find.replacement.clone();
        let items: Vec<_> = self.find.matches.iter().map(|(a, b)| (*a, *b, rep.clone())).collect();
        let n = items.len();
        if let Some(d) = self.doc_mut(id) {
            let carets = d.buf.carets.clone();
            d.buf.edit(items, crate::buffer::EditKind::Other, |_| carets);
        }
        self.say(format!("Replaced {n} occurrence(s)"), false);
    }

    pub fn start_project_search(&mut self) {
        use std::sync::atomic::Ordering;
        self.search_hits.clear();
        let generation = self.search_gen.fetch_add(1, Ordering::Relaxed) + 1;
        self.search_busy = false;
        if self.search_query.is_empty() {
            return;
        }
        let o = procs::SearchOptions { needle: self.search_query.clone(), case: self.search_case, word: self.search_word };
        let mut files = self.file_index.clone();
        // open files count even outside the folder
        for d in &self.docs {
            if let Some(p) = &d.path {
                if !files.contains(p) {
                    files.push(p.clone());
                }
            }
        }
        self.search_busy = true;
        procs::search_files(files, o, generation, self.search_gen.clone(), self.post.clone());
    }

    // ---------------------------------------------------------- running

    pub fn neptune(&self) -> PathBuf {
        procs::find_neptune(&self.st.neptune_path)
    }

    /// The file to run, on disk and up to date.
    fn prepare_target(&mut self) -> Option<PathBuf> {
        if self.st.save_before_run {
            self.save_all();
        }
        let d = self.active()?;
        if let Some(p) = &d.path {
            return Some(p.clone());
        }
        // like Blitz, an unsaved program can simply be run
        let dir = settings::config_dir().join("run");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(d.title());
        let bytes = d.encoded();
        if let Err(e) = std::fs::write(&path, bytes) {
            self.say(format!("Cannot write {}: {e}", path.display()), true);
            return None;
        }
        Some(path)
    }

    pub fn is_busy(&self) -> bool {
        self.running.is_some() || self.debug.session.is_some()
    }

    fn begin_output(&mut self, title: String) {
        self.output.clear();
        self.out_open = false;
        self.out_follow = true;
        self.push_output(&format!("{title}\n"), false);
        self.dock = DockTab::Output;
        self.show_dock = true;
        self.run_started = Instant::now();
    }

    fn quality_args(&self) -> Vec<String> {
        if self.st.quality.is_empty() {
            Vec::new()
        } else {
            vec!["--quality".to_string(), self.st.quality.clone()]
        }
    }

    /// Ends whatever is running and starts numbering a new run.
    fn replace_run(&mut self) {
        self.stop_program();
        if let Some(mut r) = self.running.take() {
            let _ = r.child.wait();
        }
        self.debug.session = None;
        self.run_id += 1;
        self.stop_asked = false;
    }

    pub fn run_target(&mut self, path: PathBuf, debug: bool) {
        self.replace_run();
        self.last_target = Some(path.clone());
        let cwd = path.parent().map(Path::to_path_buf);
        let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        self.begin_output(format!("Running {name}{}", if debug { " (debugger on)" } else { "" }));
        let exe = self.neptune();
        if debug {
            match Dap::start(&exe, cwd.as_deref(), self.run_id, self.post.clone()) {
                Ok(d) => {
                    self.debug = Debug { session: Some(d), watches: std::mem::take(&mut self.debug.watches), open: std::mem::take(&mut self.debug.open), ..Debug::default() };
                    self.begin_debug(&path);
                }
                Err(e) => self.report_start_failure(e),
            }
        } else {
            let mut args = self.quality_args();
            args.push("run".into());
            args.push(path.display().to_string());
            args.extend(procs::split_args(&self.command_line));
            self.spawn(&exe, args, cwd, RunKind::Run);
        }
    }

    fn report_start_failure(&mut self, e: String) {
        self.push_output(&format!("{e}\nThe IDE needs the neptune program: build it with `cargo build`, or set its path in Settings.\n"), true);
        self.say("Could not start neptune", true);
    }

    fn spawn(&mut self, exe: &Path, args: Vec<String>, cwd: Option<PathBuf>, kind: RunKind) {
        match procs::start(exe, &args, cwd.as_deref(), kind, self.run_id, self.post.clone()) {
            Ok(r) => self.running = Some(r),
            Err(e) => self.report_start_failure(e),
        }
    }

    pub fn run(&mut self, debug: bool) {
        if let Some(p) = self.prepare_target() {
            self.run_target(p, debug);
        } else {
            self.say("Open a program to run", false);
        }
    }

    pub fn stop_program(&mut self) {
        self.stop_asked = self.is_busy();
        if let Some(r) = &mut self.running {
            r.kill();
        }
        if let Some(d) = &mut self.debug.session {
            d.kill();
        }
    }

    pub fn check(&mut self) {
        let Some(p) = self.prepare_target() else { return };
        self.replace_run();
        self.last_target = Some(p.clone());
        self.begin_output(format!("Checking {}", p.display()));
        let exe = self.neptune();
        self.spawn(&exe, vec!["check".into(), p.display().to_string()], p.parent().map(Path::to_path_buf), RunKind::Check);
    }

    pub fn build(&mut self, out: String, console: bool) {
        let Some(p) = self.prepare_target() else { return };
        self.replace_run();
        self.last_target = Some(p.clone());
        self.begin_output(format!("Building {}", p.display()));
        let mut args = vec!["build".to_string(), p.display().to_string()];
        if !out.trim().is_empty() {
            args.push("-o".into());
            args.push(out.trim().to_string());
        }
        if console {
            args.push("--console".into());
        }
        let exe = self.neptune();
        self.spawn(&exe, args, p.parent().map(Path::to_path_buf), RunKind::Build);
    }

    pub fn push_output(&mut self, text: &str, error: bool) {
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            let done = parts.peek().is_some();
            let part = part.strip_suffix('\r').unwrap_or(part);
            if self.out_open {
                if let Some(l) = self.output.last_mut() {
                    l.text.push_str(part);
                    l.loc = lang::output_location(&l.text);
                }
            } else {
                self.output.push(OutLine { text: part.to_string(), error, loc: lang::output_location(part) });
            }
            self.out_open = !done;
        }
        // the unterminated tail of a message that ended with a newline
        if self.output.len() > 60_000 {
            self.output.drain(..10_000);
        }
    }

    fn goto_first_error(&mut self) {
        if let Some((p, l, c)) = self.output.iter().find_map(|o| o.loc.clone()) {
            let path = self.resolve_output_path(&p);
            if path.exists() {
                self.goto_file(&path, l.saturating_sub(1), c.saturating_sub(1));
            }
        }
    }

    fn finished(&mut self, kind: RunKind) {
        let mut code = None;
        if let Some(r) = &mut self.running {
            for _ in 0..50 {
                match r.child.try_wait() {
                    Ok(Some(s)) => {
                        code = s.code();
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(1)),
                    Err(_) => break,
                }
            }
        }
        self.running = None;
        self.out_open = false;
        let secs = self.run_started.elapsed().as_secs_f32();
        let stopped = std::mem::take(&mut self.stop_asked);
        if stopped {
            code = None;
        }
        let ok = code == Some(0);
        let verb = match kind {
            RunKind::Check => "Check",
            RunKind::Build => "Build",
            _ => "Program",
        };
        let text = match code {
            Some(0) => format!("{verb} finished in {secs:.1}s"),
            Some(c) => format!("{verb} failed with exit code {c} after {secs:.1}s"),
            None => format!("{verb} stopped after {secs:.1}s"),
        };
        self.push_output(&format!("\n{text}\n"), !ok && code.is_some());
        self.say(text, !ok && code.is_some());
        if !ok && code.is_some() {
            self.goto_first_error();
        }
    }

    // ---------------------------------------------------------- messages

    pub fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Output { text, error } => self.push_output(&text, error),
                Msg::Finished { run, kind } => {
                    if run == self.run_id && self.running.is_some() {
                        self.finished(kind);
                    }
                }
                Msg::Lsp(j) => self.lsp_message(j),
                Msg::LspStopped => {
                    self.lsp = None;
                    self.lsp_ready = false;
                    self.lsp_retry = Some(Instant::now() + Duration::from_secs(3));
                    for d in &mut self.docs {
                        d.lsp_open = false;
                    }
                }
                Msg::DapConnected(run, s) => {
                    if let (true, Some(d)) = (run == self.run_id, &mut self.debug.session) {
                        d.connected(s);
                    }
                }
                Msg::Dap(run, j) => {
                    if run == self.run_id {
                        self.dap_message(j);
                    }
                }
                Msg::DapStopped(run) => {
                    if run == self.run_id {
                        self.debug_ended();
                    }
                }
                Msg::FoundFiles(f) => self.file_index = f,
                Msg::SearchHit { path, line, col, len, text } => self.search_hits.push(SearchHit { path, line, col, len, text }),
                Msg::SearchDone { generation } => {
                    if generation == self.search_gen.load(std::sync::atomic::Ordering::Relaxed) {
                        self.search_busy = false;
                    }
                }
            }
        }
    }

    fn lsp_message(&mut self, j: Json) {
        if j.get("method").str() == Some("textDocument/publishDiagnostics") {
            let params = j.get("params");
            let uri = params.get("uri").str().unwrap_or("");
            let Some(d) = self.docs.iter_mut().find(|d| doc_uri(d) == uri) else { return };
            d.diags = params
                .get("diagnostics")
                .arr()
                .iter()
                .map(|x| {
                    let p = |k: &str| {
                        let at = x.get("range").get(k);
                        Pos::new(at.get("line").num().unwrap_or(0.0) as usize, at.get("character").num().unwrap_or(0.0) as usize)
                    };
                    Diag { start: p("start"), end: p("end"), error: x.get("severity").num() == Some(1.0), message: x.get("message").str().unwrap_or("").to_string() }
                })
                .collect();
            return;
        }
        match j.get("id").num() {
            Some(1.0) => self.lsp_ready = true,
            Some(2.0) => {
                for it in j.get("result").arr() {
                    let label = it.get("label").str().unwrap_or("");
                    let detail = it.get("detail").str().unwrap_or("");
                    match it.get("kind").num().unwrap_or(0.0) as u32 {
                        3 => {
                            self.known.add(label, Tk::Command);
                            self.known.set_signature(label, detail);
                            let lower = label.to_ascii_lowercase();
                            match self.comp_items.iter_mut().find(|c| c.lower == lower && c.kind == CompKind::Command) {
                                Some(c) => c.detail = detail.to_string(),
                                None => self.comp_items.push(CompItem::new(label, CompKind::Command, detail, it.get("documentation").str().unwrap_or(""))),
                            }
                        }
                        21 => {
                            self.known.add(label, Tk::Constant);
                            self.comp_items.push(CompItem::new(label, CompKind::Constant, detail, ""));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Sends text to the language server once typing pauses.
    pub fn sync_lsp(&mut self, ctx: &egui::Context) {
        if let Some(when) = self.lsp_retry {
            if Instant::now() >= when {
                self.lsp_retry = None;
                match Lsp::start(&self.neptune(), self.post.clone()) {
                    Ok(l) => self.lsp = Some(l),
                    Err(_) => self.say("Language server not found: build `neptune` or set its path in Settings", true),
                }
            }
        }
        let Some(lsp) = &self.lsp else { return };
        if !self.lsp_ready {
            return;
        }
        for d in &mut self.docs {
            if !d.lsp_open {
                d.lsp_open = true;
                d.lsp_sent = d.buf.version;
                lsp.did_open(&doc_uri(d), d.buf.text());
            } else if d.lsp_sent != d.buf.version {
                let wait = 180u128.saturating_sub(d.last_edit.elapsed().as_millis());
                if wait == 0 {
                    d.lsp_sent = d.buf.version;
                    lsp.did_change(&doc_uri(d), d.buf.text());
                } else {
                    ctx.request_repaint_after(Duration::from_millis(wait as u64 + 1));
                }
            }
        }
    }

    /// Notices files changed by other programs.
    pub fn check_disk(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        if !focused || self.last_disk_check.elapsed() < Duration::from_secs(2) || self.dialog.is_some() {
            return;
        }
        self.last_disk_check = Instant::now();
        let mut ask = None;
        for d in &mut self.docs {
            let Some(p) = &d.path else { continue };
            let now = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if now.is_some() && now != d.mtime {
                if d.buf.is_modified() {
                    ask = Some(d.id);
                    d.mtime = now;
                    break;
                }
                let _ = d.reload();
            }
        }
        if let Some(id) = ask {
            self.dialog = Some(Dialog::Reload(id));
        }
    }

    // ---------------------------------------------------------- actions

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let editing = self.editor_focused;
        for i in 0..self.shortcuts.len() {
            let (a, k) = self.shortcuts[i];
            // text boxes keep these for themselves
            if !editing && matches!(a, Action::Undo | Action::Redo | Action::SelectAll) {
                continue;
            }
            if ctx.input_mut(|inp| inp.consume_shortcut(&k)) {
                self.queue.push(a);
            }
        }
    }

    pub fn run_queue(&mut self, ctx: &egui::Context) {
        for a in std::mem::take(&mut self.queue) {
            self.perform(a, ctx);
        }
    }

    pub fn perform(&mut self, a: Action, ctx: &egui::Context) {
        use Action as A;
        let st = self.st.clone();
        match a {
            A::New => self.new_file(),
            A::Open => self.dialog = Some(Dialog::File(FileDialog::new(FileMode::Open, self.folder.clone(), ""))),
            A::OpenFolder => self.dialog = Some(Dialog::File(FileDialog::new(FileMode::Folder, self.folder.clone(), ""))),
            A::Save => {
                if let Some(id) = self.active_id() {
                    self.save(id);
                }
            }
            A::SaveAs => {
                if let Some(id) = self.active_id() {
                    self.save_as(id);
                }
            }
            A::SaveAll => self.save_all(),
            A::Close => {
                let (p, i) = (self.focus_pane, self.panes[self.focus_pane].active);
                self.request_close(p, i);
            }
            A::CloseAll => self.close_all(),
            A::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            A::Undo => self.with_doc(|d| {
                d.buf.undo();
            }),
            A::Redo => self.with_doc(|d| {
                d.buf.redo();
            }),
            A::Cut => {
                if let Some(d) = self.active_mut() {
                    let text = d.buf.copy_text();
                    if d.buf.carets.iter().any(Caret::has_selection) {
                        d.buf.delete_with(crate::buffer::EditKind::Other, |_, p| (p, p));
                    } else {
                        d.buf.delete_lines();
                    }
                    ctx.copy_text(text);
                }
                self.focus_editor = true;
            }
            A::Copy => {
                if let Some(d) = self.active() {
                    ctx.copy_text(d.buf.copy_text());
                }
                self.focus_editor = true;
            }
            A::Paste => {
                self.focus_editor = true;
                ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
            }
            A::SelectAll => self.with_doc(|d| d.buf.select_all()),
            A::Find => self.open_find(false),
            A::Replace => self.open_find(true),
            A::FindNext => self.find_step(true),
            A::FindPrev => self.find_step(false),
            A::FindInFiles => {
                if let Some(w) = self.active().map(|d| d.buf.primary()).filter(Caret::has_selection).and_then(|c| self.active().map(|d| d.buf.text_in(c.start(), c.end()))) {
                    self.search_query = w;
                }
                self.side = SideTab::Search;
                self.show_side = true;
                self.search_focus = true;
            }
            A::ToggleComment => self.with_doc(editor::toggle_comment),
            A::Indent => self.with_doc(|d| editor::indent_lines(d, &st)),
            A::Outdent => self.with_doc(|d| editor::outdent_lines(d, &st)),
            A::DuplicateLine => self.with_doc(|d| d.buf.duplicate_lines()),
            A::DeleteLine => self.with_doc(|d| d.buf.delete_lines()),
            A::MoveLineUp => self.with_doc(|d| d.buf.move_lines(-1)),
            A::MoveLineDown => self.with_doc(|d| d.buf.move_lines(1)),
            A::AddNextOccurrence => self.with_doc(|d| d.buf.add_next_occurrence()),
            A::CaretAbove => self.with_doc(|d| editor::add_caret_vertical(d, &st, -1)),
            A::CaretBelow => self.with_doc(|d| editor::add_caret_vertical(d, &st, 1)),
            A::UpperCase => self.with_doc(|d| editor::change_case(d, true)),
            A::LowerCase => self.with_doc(|d| editor::change_case(d, false)),
            A::TrimTrailing => self.with_doc(Doc::trim_trailing),
            A::FormatFile => {
                if let Some(id) = self.active_id() {
                    let i = self.docs.iter().position(|d| d.id == id).unwrap();
                    editor::format_document(&mut self.docs[i], &st, &self.known);
                    self.say("Formatted", false);
                }
            }
            A::TriggerCompletion => {
                if let Some(id) = self.active_id() {
                    let i = self.docs.iter().position(|d| d.id == id).unwrap();
                    self.force_completion(i);
                }
            }
            A::Palette => {
                self.palette = Palette { open: true, query: ">".into(), selected: 0, focus: true };
            }
            A::QuickOpen => {
                self.palette = Palette { open: true, query: String::new(), selected: 0, focus: true };
            }
            A::GotoLine => {
                self.palette = Palette { open: true, query: ":".into(), selected: 0, focus: true };
            }
            A::GotoSymbol => {
                self.palette = Palette { open: true, query: "@".into(), selected: 0, focus: true };
            }
            A::ToggleSidebar => self.show_side = !self.show_side,
            A::ToggleBottom => self.show_dock = !self.show_dock,
            A::ToggleReference => {
                self.show_ref = !self.show_ref;
                self.ref_focus = self.show_ref;
            }
            A::ZoomIn => self.zoom(1.0),
            A::ZoomOut => self.zoom(-1.0),
            A::ZoomReset => self.st.font_size = Settings::default().font_size,
            A::ToggleMinimap => self.st.minimap = !self.st.minimap,
            A::ToggleWhitespace => self.st.whitespace = !self.st.whitespace,
            A::ToggleFold => self.with_doc(|d| {
                let l = d.buf.primary().head.line;
                if d.fold_at(l).is_some() {
                    d.toggle_fold(l);
                } else if let Some((a, _)) = d.enclosing_fold(l) {
                    d.toggle_fold(a);
                    d.buf.set_caret(Pos::new(a, 0));
                }
            }),
            A::FoldAll => self.with_doc(Doc::fold_all),
            A::UnfoldAll => self.with_doc(Doc::unfold_all),
            A::SplitEditor => self.toggle_split(),
            A::Settings => self.dialog = Some(Dialog::Settings),
            A::NextTab | A::PrevTab => {
                let p = &mut self.panes[self.focus_pane];
                if !p.tabs.is_empty() {
                    let n = p.tabs.len();
                    p.active = if a == A::NextTab { (p.active + 1) % n } else { (p.active + n - 1) % n };
                    self.focus_editor = true;
                }
            }
            A::GotoDefinition => {
                if let Some(w) = self.word_at_caret() {
                    self.goto_definition(&w);
                }
            }
            A::Back => {
                if let (Some(to), Some(d)) = (self.back.pop(), self.active()) {
                    let here = (d.id, d.buf.primary().head);
                    self.forward.push(here);
                    self.jump(to.0, to.1, None);
                }
            }
            A::Forward => {
                if let (Some(to), Some(d)) = (self.forward.pop(), self.active()) {
                    let here = (d.id, d.buf.primary().head);
                    self.back.push(here);
                    self.jump(to.0, to.1, None);
                }
            }
            A::NextProblem | A::PrevProblem => self.step_problem(a == A::NextProblem),
            A::ToggleBookmark => self.with_doc(|d| {
                let l = d.buf.primary().head.line;
                if !d.bookmarks.remove(&l) {
                    d.bookmarks.insert(l);
                }
            }),
            A::NextBookmark | A::PrevBookmark => self.step_bookmark(a == A::NextBookmark),
            A::Run => {
                if self.debug.paused {
                    self.debug_continue();
                } else {
                    self.run(st.debug_enabled);
                }
            }
            A::RunAgain => match self.last_target.clone() {
                Some(p) => {
                    if st.save_before_run {
                        self.save_all();
                    }
                    self.run_target(p, st.debug_enabled)
                }
                None => self.run(st.debug_enabled),
            },
            A::RunNoDebug => self.run(false),
            A::Stop => self.stop_program(),
            A::Check => self.check(),
            A::Build => self.dialog = Some(Dialog::Build { out: String::new(), console: false }),
            A::CommandLine => self.dialog = Some(Dialog::Prompt { title: "Program command line".into(), value: self.command_line.clone(), kind: PromptKind::CommandLine, focus: true }),
            A::ToggleDebug => self.st.debug_enabled = !self.st.debug_enabled,
            A::Continue => self.debug_continue(),
            A::Pause => self.debug_request_simple("pause"),
            A::StepOver => self.debug_step("next"),
            A::StepIn => self.debug_step("stepIn"),
            A::StepOut => self.debug_step("stepOut"),
            A::ToggleBreakpoint => {
                self.with_doc(|d| {
                    let lines: BTreeSet<usize> = d.buf.carets.iter().map(|c| c.head.line).collect();
                    for l in lines {
                        d.toggle_breakpoint(l);
                    }
                });
                self.send_breakpoints();
            }
            A::EditBreakpoint => {
                if let Some(Tab::Doc(id)) = self.panes[self.focus_pane].current() {
                    if let Some(line) = self.docs.iter().find(|d| d.id == id).map(|d| d.buf.primary().head.line) {
                        self.edit_breakpoint(id, line);
                    }
                }
            }
            A::ClearBreakpoints => {
                for d in &mut self.docs {
                    d.clear_breakpoints();
                }
                self.send_breakpoints();
            }
            A::QuickHelp => self.quick_help(),
            A::Reference => {
                self.show_ref = true;
                self.ref_focus = true;
            }
            A::Manual => self.show_tab(Tab::Manual),
            A::Welcome => self.show_tab(Tab::Welcome),
            A::Shortcuts => self.dialog = Some(Dialog::Shortcuts { filter: String::new() }),
            A::About => self.dialog = Some(Dialog::About),
        }
        if let Some(id) = self.active_id() {
            if matches!(a, A::Undo | A::Redo | A::Cut | A::Paste | A::ToggleComment | A::Indent | A::Outdent | A::DuplicateLine | A::DeleteLine | A::MoveLineUp | A::MoveLineDown | A::AddNextOccurrence | A::CaretAbove | A::CaretBelow) {
                let pane = self.focus_pane;
                self.panes[pane].view(id).reveal = Some(Reveal::Nearest);
            }
        }
    }

    fn force_completion(&mut self, i: usize) {
        let mut out = Vec::new();
        let hover = HashMap::new();
        let mut sh = editor::Shared {
            st: &self.st,
            th: self.th,
            known: &self.known,
            comp_items: &self.comp_items,
            completion: &mut self.completion,
            exec_line: None,
            matches: &[],
            current_match: None,
            eval: &hover,
            debugging: false,
            out: &mut out,
            want_focus: false,
            read_only: false,
        };
        editor::update_completion(&self.docs[i], &mut sh, true);
    }

    /// Runs an edit on the active document.
    pub fn with_doc(&mut self, f: impl FnOnce(&mut Doc)) {
        if let Some(d) = self.active_mut() {
            f(d);
        }
    }

    pub fn zoom(&mut self, delta: f32) {
        self.st.font_size = (self.st.font_size + delta).clamp(8.0, 40.0);
    }

    fn toggle_split(&mut self) {
        if self.panes.len() > 1 {
            let moved = self.panes.remove(1);
            for t in moved.tabs {
                if !self.panes[0].tabs.contains(&t) {
                    self.panes[0].tabs.push(t);
                }
            }
            self.focus_pane = 0;
        } else {
            let mut p = Pane::new(1);
            if let Some(t) = self.panes[0].current() {
                p.tabs.push(t);
            }
            self.panes.push(p);
            self.focus_pane = 1;
            self.focus_editor = true;
        }
    }

    fn step_problem(&mut self, forward: bool) {
        let Some(d) = self.active() else { return };
        let h = d.buf.primary().head;
        let id = d.id;
        let mut all: Vec<Pos> = d.diags.iter().map(|x| x.start).collect();
        all.sort();
        let target = if forward { all.iter().find(|p| **p > h).or(all.first()) } else { all.iter().rev().find(|p| **p < h).or(all.last()) }.copied();
        if let Some(p) = target {
            self.goto(id, p, None);
        } else {
            self.say("No problems in this file", false);
        }
    }

    fn step_bookmark(&mut self, forward: bool) {
        let Some(d) = self.active() else { return };
        let (l, id) = (d.buf.primary().head.line, d.id);
        let target = if forward { d.bookmarks.range(l + 1..).next().or(d.bookmarks.iter().next()) } else { d.bookmarks.range(..l).next_back().or(d.bookmarks.iter().next_back()) }.copied();
        if let Some(t) = target {
            self.goto(id, Pos::new(t, 0), None);
        }
    }

    /// F1: the signature in the status bar; F1 again opens its reference page.
    fn quick_help(&mut self) {
        let Some(w) = self.word_at_caret() else { return };
        let Some(h) = self.known.help_for(&w) else {
            self.say(format!("No help for {w}"), false);
            return;
        };
        let name = h.name.clone();
        let sig = if h.signature.is_empty() { h.name.clone() } else { h.signature.clone() };
        if self.help_tip.as_ref().is_some_and(|(n, _)| *n == name) {
            self.show_reference(&name);
            self.help_tip = None;
        } else {
            self.help_tip = Some((name, sig));
        }
    }

    pub fn apply_outs(&mut self, outs: Vec<editor::Out>, ctx: &egui::Context) {
        for o in outs {
            match o {
                editor::Out::Do(a) => self.perform(a, ctx),
                editor::Out::Focused => {}
                editor::Out::GotoDefinition(w) => self.goto_definition(&w),
                editor::Out::Eval(w) => self.debug_hover(w),
                editor::Out::BreakpointsChanged => self.send_breakpoints(),
                editor::Out::EditBreakpoint(id, line) => self.edit_breakpoint(id, line),
                editor::Out::Zoom(z) => {
                    let d = if z > 1.0 { 1.0 } else { -1.0 };
                    self.zoom(d);
                }
            }
        }
    }
}

/// The name the language server knows a document by.
pub fn doc_uri(d: &Doc) -> String {
    match &d.path {
        Some(p) => procs::file_uri(&p.display().to_string()),
        None => format!("file:///{}", d.title()),
    }
}

/// `canonicalize` on Windows gives `\\?\C:\...`; people do not write that.
pub fn clean_path(p: &Path) -> PathBuf {
    let t = p.to_string_lossy();
    match t.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}
