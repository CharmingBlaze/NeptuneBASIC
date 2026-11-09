//! The code editor widget. Only the rows on screen are laid out and painted,
//! so a 100,000-line file costs the same as a short one.

use std::collections::{BTreeSet, HashMap};

use eframe::egui::{
    self, text::LayoutJob, Align2, Color32, CornerRadius, Event, EventFilter, FontId, Id, Key, Mesh, Modifiers, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextFormat, Ui, Vec2,
};

use crate::actions::Action;
use crate::buffer::{self, char_len, col_at_visual, is_word, visual_col, Caret, EditKind, Pos};
use crate::document::Doc;
use crate::lang::{self, Known, Tk, Token};
use crate::settings::Settings;
use crate::theme::Theme;
use crate::ui_util;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reveal {
    Nearest,
    Center,
}

pub struct View {
    pub pane: usize,
    pub scroll: Vec2,
    pub reveal: Option<Reveal>,
    last_activity: f64,
    hover: Option<(Pos, f64)>,
    bar_grab: Option<f32>,
    mini_drag: bool,
}

impl View {
    pub fn new(pane: usize) -> Self {
        View { pane, scroll: Vec2::ZERO, reveal: None, last_activity: 0.0, hover: None, bar_grab: None, mini_drag: false }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompKind {
    Keyword,
    Command,
    Constant,
    Symbol,
    Snippet,
    Word,
}

#[derive(Clone, Debug)]
pub struct CompItem {
    pub label: String,
    pub lower: String,
    pub kind: CompKind,
    pub detail: String,
    pub doc: String,
    pub snippet: Option<&'static str>,
}

impl CompItem {
    pub fn new(label: &str, kind: CompKind, detail: &str, doc: &str) -> Self {
        CompItem { label: label.to_string(), lower: label.to_ascii_lowercase(), kind, detail: detail.to_string(), doc: doc.to_string(), snippet: None }
    }
}

pub struct Completion {
    pub items: Vec<CompItem>,
    pub selected: usize,
    pub first_shown: usize,
}

/// Things the editor asks the rest of the IDE to do.
pub enum Out {
    Do(Action),
    Focused,
    GotoDefinition(String),
    Eval(String),
    BreakpointsChanged,
    Zoom(f32),
}

pub struct Shared<'a> {
    pub st: &'a Settings,
    pub th: &'a Theme,
    pub known: &'a Known,
    pub comp_items: &'a [CompItem],
    pub completion: &'a mut Option<Completion>,
    pub exec_line: Option<usize>,
    pub matches: &'a [(Pos, Pos)],
    pub current_match: Option<usize>,
    pub eval: &'a HashMap<String, String>,
    pub debugging: bool,
    pub out: &'a mut Vec<Out>,
    pub want_focus: bool,
    pub read_only: bool,
}

struct Geo {
    gutter: Rect,
    text: Rect,
    mini: Option<Rect>,
    vbar: Rect,
    hbar: Option<Rect>,
    adv: f32,
    row_h: f32,
    font: FontId,
    tab: usize,
}

const MINI_ROW: f32 = 3.0;

impl Geo {
    fn visible_rows(&self) -> usize {
        (self.text.height() / self.row_h).floor().max(1.0) as usize
    }
}

fn indent_unit(st: &Settings) -> String {
    if st.use_spaces {
        " ".repeat(st.tab_size)
    } else {
        "\t".to_string()
    }
}

fn leading_ws(line: &str) -> String {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

// ---------------------------------------------------------------- edit commands

pub fn toggle_comment(doc: &mut Doc) {
    let ranges = doc.buf.caret_line_ranges();
    let lines: Vec<usize> = ranges.iter().flat_map(|&(a, b)| a..=b).filter(|&l| !doc.buf.line(l).trim().is_empty()).collect();
    if lines.is_empty() {
        return;
    }
    let all = lines.iter().all(|&l| doc.buf.line(l).trim_start().starts_with(';'));
    let min = lines.iter().map(|&l| leading_ws(doc.buf.line(l)).chars().count()).min().unwrap_or(0);
    doc.buf.rewrite_lines(&ranges, |_, s| {
        if s.trim().is_empty() {
            return s.to_string();
        }
        if all {
            let ws = leading_ws(s);
            let rest = &s[ws.len()..];
            let rest = rest.strip_prefix("; ").or_else(|| rest.strip_prefix(';')).unwrap_or(rest);
            format!("{ws}{rest}")
        } else {
            let head: String = s.chars().take(min).collect();
            let tail: String = s.chars().skip(min).collect();
            format!("{head}; {tail}")
        }
    });
}

pub fn indent_lines(doc: &mut Doc, st: &Settings) {
    let unit = indent_unit(st);
    let ranges = doc.buf.caret_line_ranges();
    doc.buf.rewrite_lines(&ranges, |_, s| if s.is_empty() { String::new() } else { format!("{unit}{s}") });
}

fn outdent(s: &str, tab: usize) -> String {
    if let Some(r) = s.strip_prefix('\t') {
        return r.to_string();
    }
    let n = s.chars().take(tab).take_while(|c| *c == ' ').count();
    s[n..].to_string()
}

pub fn outdent_lines(doc: &mut Doc, st: &Settings) {
    let ranges = doc.buf.caret_line_ranges();
    let tab = st.tab_size;
    doc.buf.rewrite_lines(&ranges, |_, s| outdent(s, tab));
}

/// Re-indents by block structure and gives names their proper capitals.
pub fn format_document(doc: &mut Doc, st: &Settings, known: &Known) {
    let unit = indent_unit(st);
    let last = doc.buf.line_count() - 1;
    let mut level = 0usize;
    let mut tokens = Vec::new();
    doc.buf.rewrite_lines(&[(0, last)], |_, s| {
        let body = s.trim();
        if body.is_empty() {
            return String::new();
        }
        if lang::dedents_self(body) {
            level = level.saturating_sub(1);
        }
        let mut chars: Vec<char> = body.chars().collect();
        lang::tokenize(&chars, known, &mut tokens);
        for t in &tokens {
            if matches!(t.kind, Tk::Keyword | Tk::Command | Tk::Constant) {
                let end = if matches!(chars[t.end - 1], '%' | '#' | '$') { t.end - 1 } else { t.end };
                let word: String = chars[t.start..end].iter().collect();
                if let Some(c) = known.canonical(&word) {
                    if c.len() == word.len() {
                        for (i, ch) in c.chars().enumerate() {
                            chars[t.start + i] = ch;
                        }
                    }
                }
            }
        }
        let out = format!("{}{}", unit.repeat(level), chars.iter().collect::<String>());
        if lang::indents_next(body) {
            level += 1;
        }
        out
    });
}

pub fn change_case(doc: &mut Doc, upper: bool) {
    let items: Vec<_> = doc
        .buf
        .carets
        .iter()
        .filter(|c| c.has_selection())
        .map(|c| {
            let t = doc.buf.text_in(c.start(), c.end());
            (c.start(), c.end(), if upper { t.to_uppercase() } else { t.to_lowercase() })
        })
        .collect();
    let carets = doc.buf.carets.clone();
    doc.buf.edit(items, EditKind::Other, |_| carets);
}

pub fn add_caret_vertical(doc: &mut Doc, st: &Settings, dir: isize) {
    let anchor = if dir < 0 { doc.buf.carets[0] } else { doc.buf.primary() };
    let line = anchor.head.line as isize + dir;
    if line < 0 || line as usize >= doc.buf.line_count() {
        return;
    }
    let goal = anchor.goal.unwrap_or_else(|| visual_col(doc.buf.line(anchor.head.line), anchor.head.col, st.tab_size));
    let col = col_at_visual(doc.buf.line(line as usize), goal, st.tab_size);
    let p = Pos::new(line as usize, col);
    doc.buf.carets.push(Caret { anchor: p, head: p, goal: Some(goal) });
    doc.buf.normalize();
}

fn autocap_before(doc: &mut Doc, known: &Known, line: usize, upto: usize, kind: EditKind, tokens: &mut Vec<Token>) {
    let chars: Vec<char> = doc.buf.line(line).chars().collect();
    lang::tokenize(&chars, known, tokens);
    let Some(t) = tokens.iter().find(|t| t.end == upto || (t.end == upto + 1 && t.kind != Tk::Comment && t.kind != Tk::Str && is_word(chars[upto.min(chars.len() - 1)]))) else { return };
    if !matches!(t.kind, Tk::Keyword | Tk::Command | Tk::Constant) {
        return;
    }
    let end = if matches!(chars[t.end - 1], '%' | '#' | '$') { t.end - 1 } else { t.end };
    let word: String = chars[t.start..end].iter().collect();
    let Some(c) = known.canonical(&word) else { return };
    if c != word && c.len() == word.len() {
        let c = c.to_string();
        let carets = doc.buf.carets.clone();
        doc.buf.edit(vec![(Pos::new(line, t.start), Pos::new(line, end), c)], kind, |_| carets);
    }
}

fn in_string_or_comment(line: &[char], col: usize, known: &Known, tokens: &mut Vec<Token>) -> bool {
    lang::tokenize(line, known, tokens);
    tokens.iter().any(|t| matches!(t.kind, Tk::Str | Tk::Comment) && t.start < col && (col < t.end || t.kind == Tk::Comment || !line.get(t.end - 1).is_some_and(|c| *c == '"') || t.end - t.start == 1))
}

/// Rebuilds the completion list for the word at the caret.
pub fn update_completion(doc: &Doc, sh: &mut Shared, force: bool) {
    let c = doc.buf.primary();
    if doc.buf.carets.len() > 1 || c.has_selection() {
        *sh.completion = None;
        return;
    }
    let chars: Vec<char> = doc.buf.line(c.head.line).chars().collect();
    let col = c.head.col.min(chars.len());
    let mut start = col;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut tokens = Vec::new();
    if in_string_or_comment(&chars, col, sh.known, &mut tokens) || (col - start < 2 && !force) {
        *sh.completion = None;
        return;
    }
    let prefix: String = chars[start..col].iter().collect::<String>().to_ascii_lowercase();
    let score = |lower: &str| -> Option<u32> {
        if prefix.is_empty() {
            return Some(50);
        }
        if lower == prefix {
            return Some(0);
        }
        if lower.starts_with(&prefix) {
            return Some(1);
        }
        if let Some(i) = lower.find(&prefix) {
            return Some(20 + i as u32);
        }
        // letters in order: "posent" finds PositionEntity
        let mut it = lower.chars();
        prefix.chars().all(|p| it.any(|c| c == p)).then_some(40)
    };
    let mut found: Vec<(u32, CompItem)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for d in &doc.decls {
        let lower = d.name.to_ascii_lowercase();
        if let Some(s) = score(&lower) {
            if seen.insert(lower) {
                found.push((s.saturating_sub(1), CompItem::new(&d.name, CompKind::Symbol, &d.text, &format!("Line {}", d.line + 1))));
            }
        }
    }
    for it in sh.comp_items {
        if let Some(s) = score(&it.lower) {
            if it.snippet.is_some() || seen.insert(it.lower.clone()) {
                found.push((s, it.clone()));
            }
        }
    }
    // names used nearby, for locals the outline does not list
    let from = c.head.line.saturating_sub(300);
    let to = (c.head.line + 300).min(doc.buf.line_count());
    for l in from..to {
        if l == c.head.line {
            continue;
        }
        let cs: Vec<char> = doc.buf.line(l).chars().collect();
        lang::tokenize(&cs, sh.known, &mut tokens);
        for t in tokens.iter().filter(|t| t.kind == Tk::Plain && t.end - t.start >= 3) {
            let w: String = cs[t.start..t.end].iter().collect::<String>();
            let lower = w.trim_end_matches(['%', '#', '$']).to_ascii_lowercase();
            if lower == prefix || !seen.insert(lower.clone()) {
                continue;
            }
            if let Some(s) = score(&lower) {
                found.push((s + 5, CompItem::new(w.trim_end_matches(['%', '#', '$']), CompKind::Word, "", "")));
            }
        }
    }
    if found.is_empty() {
        *sh.completion = None;
        return;
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.label.len().cmp(&b.1.label.len())).then(a.1.lower.cmp(&b.1.lower)));
    found.truncate(60);
    let keep = sh.completion.as_ref().and_then(|c| c.items.get(c.selected)).map(|i| i.label.clone());
    let items: Vec<CompItem> = found.into_iter().map(|f| f.1).collect();
    let selected = keep.and_then(|k| items.iter().position(|i| i.label == k)).unwrap_or(0);
    *sh.completion = Some(Completion { items, selected, first_shown: selected.saturating_sub(5) });
}

fn accept_completion(doc: &mut Doc, sh: &mut Shared) {
    let Some(comp) = sh.completion.take() else { return };
    let Some(item) = comp.items.get(comp.selected) else { return };
    let head = doc.buf.primary().head;
    let mut start = head.col;
    let chars: Vec<char> = doc.buf.line(head.line).chars().collect();
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let from = Pos::new(head.line, start);
    match item.snippet {
        Some(body) => {
            let ws = leading_ws(doc.buf.line(head.line));
            let unit = indent_unit(sh.st);
            let text = body.replace('\t', &unit).replace('\n', &format!("\n{ws}"));
            let at = text.find("$0").unwrap_or(text.len());
            let before = text[..at].to_string();
            let clean = text.replacen("$0", "", 1);
            let lines = before.matches('\n').count();
            let col = if lines == 0 { start + char_len(&before) } else { char_len(before.rsplit('\n').next().unwrap_or("")) };
            doc.buf.edit(vec![(from, head, clean)], EditKind::Other, |_| vec![Caret::at(Pos::new(head.line + lines, col))]);
        }
        None => {
            let label = item.label.clone();
            doc.buf.edit(vec![(from, head, label)], EditKind::Other, |e| e.iter().map(|&p| Caret::at(p)).collect());
        }
    }
    doc.buf.break_undo_group();
}

/// True when the highlighted suggestion is exactly the word already typed.
fn completion_is_typed_word(doc: &Doc, sh: &Shared) -> bool {
    let Some(item) = sh.completion.as_ref().and_then(|c| c.items.get(c.selected)) else { return false };
    let head = doc.buf.primary().head;
    lang::word_at(doc.buf.line(head.line), head.col).is_some_and(|(w, _, end)| end == head.col && w.eq_ignore_ascii_case(&item.label))
}

fn all_carets<F: Fn(Option<char>) -> bool>(doc: &Doc, f: F) -> bool {
    doc.buf.carets.iter().all(|c| !c.has_selection() && f(doc.buf.char_at(c.head)))
}

fn opens_pair_here(next: Option<char>) -> bool {
    next.is_none_or(|c| c.is_whitespace() || matches!(c, ')' | ']' | ',' | ':' | ';'))
}

fn type_char(doc: &mut Doc, sh: &mut Shared, c: char) {
    let mut tokens = Vec::new();
    let pairs = sh.st.auto_pairs;
    let any_selection = doc.buf.carets.iter().any(Caret::has_selection);
    let wrap = |doc: &mut Doc, open: char, close: char| {
        let items: Vec<_> = doc.buf.carets.iter().map(|k| (k.start(), k.end(), format!("{open}{}{close}", doc.buf.text_in(k.start(), k.end())))).collect();
        doc.buf.edit(items, EditKind::Other, |e| e.iter().map(|&p| Caret::at(p)).collect());
    };
    let pair = |doc: &mut Doc, text: &str| {
        let items: Vec<_> = doc.buf.carets.iter().map(|k| (k.start(), k.end(), text.to_string())).collect();
        doc.buf.edit(items, EditKind::Typing, |e| e.iter().map(|&p| Caret::at(Pos::new(p.line, p.col - 1))).collect());
    };
    let step_over = pairs && matches!(c, ')' | ']' | '"') && all_carets(doc, |n| n == Some(c));
    if step_over {
        doc.buf.move_carets(false, |b, k| Caret { head: b.right_of(k.head), ..k });
    } else if pairs && any_selection && matches!(c, '(' | '[' | '"') && doc.buf.primary().start().line == doc.buf.primary().end().line {
        let close = match c {
            '(' => ')',
            '[' => ']',
            _ => '"',
        };
        wrap(doc, c, close);
    } else if pairs && matches!(c, '(' | '[') && all_carets(doc, opens_pair_here) {
        pair(doc, if c == '(' { "()" } else { "[]" });
    } else if pairs && c == '"' && all_carets(doc, opens_pair_here) && {
        let k = doc.buf.primary().head;
        let before: Vec<char> = doc.buf.line(k.line).chars().take(k.col).collect();
        !in_string_or_comment(&before, before.len(), sh.known, &mut tokens) && before.iter().filter(|c| **c == '"').count() % 2 == 0
    } {
        pair(doc, "\"\"");
    } else {
        let mut b = [0; 4];
        doc.buf.insert(c.encode_utf8(&mut b), EditKind::Typing);
    }
    if !is_word(c) && sh.st.autocap {
        let places: Vec<Pos> = doc.buf.carets.iter().map(|k| k.head).collect();
        for p in places {
            if p.col > 0 {
                autocap_before(doc, sh.known, p.line, p.col - 1, EditKind::Typing, &mut tokens);
            }
        }
    }
    if sh.st.completion && (is_word(c) || sh.completion.is_some()) {
        if is_word(c) {
            update_completion(doc, sh, false);
        } else {
            *sh.completion = None;
        }
    }
}

fn press_enter(doc: &mut Doc, sh: &mut Shared) {
    if doc.buf.carets.len() > 1 || doc.buf.primary().has_selection() || !sh.st.auto_indent {
        let items: Vec<_> = doc
            .buf
            .carets
            .iter()
            .map(|c| {
                let ws = if sh.st.auto_indent { leading_ws(doc.buf.line(c.start().line)) } else { String::new() };
                (c.start(), c.end(), format!("\n{ws}"))
            })
            .collect();
        doc.buf.edit(items, EditKind::Other, |e| e.iter().map(|&p| Caret::at(p)).collect());
        return;
    }
    let mut tokens = Vec::new();
    let p = doc.buf.primary().head;
    if sh.st.autocap {
        autocap_before(doc, sh.known, p.line, p.col, EditKind::Other, &mut tokens);
    }
    let line = doc.buf.line(p.line).to_string();
    let before: String = line.chars().take(p.col).collect();
    let mut ws = leading_ws(&line);
    if ws.chars().count() > p.col {
        ws = ws.chars().take(p.col).collect();
    }
    let unit = indent_unit(sh.st);
    let mut items = Vec::new();
    // a closing word typed at body depth moves out to its opener's depth
    if lang::dedents_self(&before) && p.line > 0 {
        let prev = (0..p.line).rev().map(|l| doc.buf.line(l)).find(|l| !l.trim().is_empty()).unwrap_or("");
        let same = leading_ws(prev) == ws && !lang::indents_next(prev);
        let inside = leading_ws(prev).chars().count() > ws.chars().count();
        if same && !inside && !ws.is_empty() {
            let out = outdent(&ws, sh.st.tab_size);
            let n = ws.chars().count();
            items.push((Pos::new(p.line, 0), Pos::new(p.line, n), out.clone()));
            ws = out;
        }
    }
    let mut new_indent = ws.clone();
    if lang::indents_next(&before) {
        new_indent.push_str(&unit);
    }
    items.push((p, p, format!("\n{new_indent}")));
    doc.buf.edit(items, EditKind::Other, |e| vec![Caret::at(*e.last().unwrap())]);
}

fn backspace_range(b: &buffer::Buffer, p: Pos, st: &Settings) -> (Pos, Pos) {
    let chars: Vec<char> = b.line(p.line).chars().collect();
    if st.auto_pairs && p.col > 0 && p.col < chars.len() {
        if let ('(', ')') | ('[', ']') | ('"', '"') = (chars[p.col - 1], chars[p.col]) {
            return (Pos::new(p.line, p.col - 1), Pos::new(p.line, p.col + 1));
        }
    }
    if p.col > 0 && chars[..p.col].iter().all(|c| *c == ' ') {
        let n = ((p.col - 1) % st.tab_size) + 1;
        return (Pos::new(p.line, p.col - n), p);
    }
    (b.left_of(p), p)
}

fn move_vertical(doc: &Doc, c: Caret, rows: isize, tab: usize) -> Caret {
    let row = doc.row_of_line(c.head.line) as isize + rows;
    if row < 0 {
        return Caret { head: Pos::new(0, 0), goal: None, ..c };
    }
    if row as usize >= doc.row_count() {
        return Caret { head: doc.buf.end(), goal: None, ..c };
    }
    let goal = c.goal.unwrap_or_else(|| visual_col(doc.buf.line(c.head.line), c.head.col, tab));
    let line = doc.line_of_row(row as usize);
    Caret { head: Pos::new(line, col_at_visual(doc.buf.line(line), goal, tab)), goal: Some(goal), ..c }
}

/// Handles one key press; false when the key is not for the editor.
fn handle_key(doc: &mut Doc, view: &mut View, sh: &mut Shared, key: Key, m: Modifiers, page: isize) -> bool {
    let tab = sh.st.tab_size;
    let cmd = m.command;
    let shift = m.shift;
    if sh.completion.is_some() {
        match key {
            Key::ArrowDown | Key::ArrowUp | Key::PageDown | Key::PageUp if !cmd && !shift => {
                let c = sh.completion.as_mut().unwrap();
                let n = c.items.len() as isize;
                let d = match key {
                    Key::ArrowDown => 1,
                    Key::ArrowUp => -1,
                    Key::PageDown => 8,
                    _ => -8,
                };
                c.selected = (c.selected as isize + d).rem_euclid(n) as usize;
                return true;
            }
            Key::Enter if !cmd && !shift && completion_is_typed_word(doc, sh) => *sh.completion = None,
            Key::Enter | Key::Tab if !cmd && !shift => {
                accept_completion(doc, sh);
                return true;
            }
            Key::Escape => {
                *sh.completion = None;
                return true;
            }
            _ => {}
        }
    }
    if sh.read_only && !matches!(key, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End | Key::PageUp | Key::PageDown | Key::Escape) {
        return false;
    }
    match key {
        Key::ArrowLeft | Key::ArrowRight => {
            let left = key == Key::ArrowLeft;
            doc.buf.move_carets(shift, |b, c| {
                let head = if cmd {
                    if left {
                        b.word_left(c.head)
                    } else {
                        b.word_right(c.head)
                    }
                } else if c.has_selection() && !shift {
                    if left {
                        c.start()
                    } else {
                        c.end()
                    }
                } else if left {
                    b.left_of(c.head)
                } else {
                    b.right_of(c.head)
                };
                Caret { head, goal: None, ..c }
            });
            *sh.completion = None;
        }
        Key::ArrowUp | Key::ArrowDown => {
            let d = if key == Key::ArrowUp { -1 } else { 1 };
            if cmd {
                view.scroll.y += d as f32 * 3.0 * (sh.st.font_size * 1.5).round();
                return true;
            }
            let moved: Vec<Caret> = doc.buf.carets.iter().map(|&c| move_vertical(doc, c, d, tab)).collect();
            let mut it = moved.into_iter();
            doc.buf.move_carets(shift, |_, c| it.next().unwrap_or(c));
        }
        Key::PageUp | Key::PageDown => {
            let d = if key == Key::PageUp { -page } else { page };
            let moved: Vec<Caret> = doc.buf.carets.iter().map(|&c| move_vertical(doc, c, d, tab)).collect();
            let mut it = moved.into_iter();
            doc.buf.move_carets(shift, |_, c| it.next().unwrap_or(c));
            view.scroll.y += d as f32 * (sh.st.font_size * 1.5).round();
        }
        Key::Home => {
            doc.buf.move_carets(shift, |b, c| Caret { head: if cmd { Pos::default() } else { b.home_of(c.head) }, goal: None, ..c });
            *sh.completion = None;
        }
        Key::End => {
            doc.buf.move_carets(shift, |b, c| Caret { head: if cmd { b.end() } else { Pos::new(c.head.line, b.line_len(c.head.line)) }, goal: None, ..c });
            *sh.completion = None;
        }
        Key::Backspace => {
            let st = sh.st;
            if cmd {
                doc.buf.delete_with(EditKind::Deleting, |b, p| (b.word_left(p), p));
            } else {
                doc.buf.delete_with(EditKind::Deleting, |b, p| backspace_range(b, p, st));
            }
            if sh.completion.is_some() {
                update_completion(doc, sh, false);
            }
        }
        Key::Delete => {
            if shift {
                doc.buf.delete_lines();
            } else if cmd {
                doc.buf.delete_with(EditKind::Deleting, |b, p| (p, b.word_right(p)));
            } else {
                doc.buf.delete_with(EditKind::Deleting, |b, p| (p, b.right_of(p)));
            }
            *sh.completion = None;
        }
        Key::Enter if !cmd => {
            press_enter(doc, sh);
            *sh.completion = None;
        }
        Key::Tab if !cmd => {
            let multi = doc.buf.carets.iter().any(|c| c.start().line != c.end().line);
            if shift {
                outdent_lines(doc, sh.st);
            } else if multi {
                indent_lines(doc, sh.st);
            } else if sh.st.use_spaces {
                let items: Vec<_> = doc
                    .buf
                    .carets
                    .iter()
                    .map(|c| {
                        let v = visual_col(doc.buf.line(c.start().line), c.start().col, tab);
                        (c.start(), c.end(), " ".repeat(tab - v % tab))
                    })
                    .collect();
                doc.buf.edit(items, EditKind::Typing, |e| e.iter().map(|&p| Caret::at(p)).collect());
            } else {
                doc.buf.insert("\t", EditKind::Typing);
            }
        }
        Key::Escape
            if (doc.buf.carets.len() > 1 || doc.buf.primary().has_selection()) => {
                let p = doc.buf.primary().head;
                doc.buf.set_caret(p);
            }
        _ => return false,
    }
    true
}

fn handle_events(ui: &Ui, doc: &mut Doc, view: &mut View, sh: &mut Shared, page: isize) {
    let events = ui.input(|i| i.events.clone());
    let mut touched = false;
    for ev in events {
        match ev {
            Event::Text(t) if !sh.read_only => {
                let mut chars = t.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c != '\u{7f}' => type_char(doc, sh, c),
                    (Some(_), Some(_)) => doc.buf.insert(&t, EditKind::Other),
                    _ => {}
                }
                touched = true;
            }
            Event::Ime(egui::ImeEvent::Commit(t)) if !sh.read_only && !t.is_empty() => {
                doc.buf.insert(&t, EditKind::Other);
                touched = true;
            }
            Event::Copy => ui.ctx().copy_text(doc.buf.copy_text()),
            Event::Cut if !sh.read_only => {
                ui.ctx().copy_text(doc.buf.copy_text());
                if doc.buf.carets.iter().any(Caret::has_selection) {
                    doc.buf.delete_with(EditKind::Other, |_, p| (p, p));
                } else {
                    doc.buf.delete_lines();
                }
                touched = true;
            }
            Event::Paste(t) if !sh.read_only => {
                doc.buf.paste(&t);
                *sh.completion = None;
                touched = true;
            }
            Event::Key { key, pressed: true, modifiers, .. }
                if handle_key(doc, view, sh, key, modifiers, page) => {
                    touched = true;
                }
            _ => {}
        }
    }
    if touched {
        view.reveal.get_or_insert(Reveal::Nearest);
        view.last_activity = ui.input(|i| i.time);
    }
}

// ---------------------------------------------------------------- measuring

fn x_of(ui: &Ui, line: &str, col: usize, g: &Geo) -> f32 {
    if line.is_ascii() {
        return visual_col(line, col, g.tab) as f32 * g.adv;
    }
    ui.fonts_mut(|f| {
        let (mut x, mut v) = (0.0, 0usize);
        for c in line.chars().take(col) {
            if c == '\t' {
                let next = (v / g.tab + 1) * g.tab;
                x += (next - v) as f32 * g.adv;
                v = next;
            } else {
                x += if c.is_ascii() { g.adv } else { f.glyph_width(&g.font, c) };
                v += 1;
            }
        }
        x
    })
}

fn col_at_x(ui: &Ui, line: &str, x: f32, g: &Geo) -> usize {
    if line.is_ascii() {
        return col_at_visual(line, (x / g.adv).round().max(0.0) as usize, g.tab);
    }
    ui.fonts_mut(|f| {
        let (mut at, mut v) = (0.0, 0usize);
        for (i, c) in line.chars().enumerate() {
            let w = if c == '\t' {
                let next = (v / g.tab + 1) * g.tab;
                let w = (next - v) as f32 * g.adv;
                v = next;
                w
            } else {
                v += 1;
                if c.is_ascii() {
                    g.adv
                } else {
                    f.glyph_width(&g.font, c)
                }
            };
            if x < at + w * 0.5 {
                return i;
            }
            at += w;
        }
        char_len(line)
    })
}

fn pos_at(ui: &Ui, doc: &Doc, view: &View, g: &Geo, p: Pos2) -> Pos {
    let row = ((p.y - g.text.top() + view.scroll.y) / g.row_h).floor().max(0.0) as usize;
    if row >= doc.row_count() {
        return doc.buf.end();
    }
    let line = doc.line_of_row(row);
    let x = p.x - g.text.left() + view.scroll.x;
    Pos::new(line, col_at_x(ui, doc.buf.line(line), x, g))
}

// ---------------------------------------------------------------- the widget

pub fn show(ui: &mut Ui, doc: &mut Doc, view: &mut View, sh: &mut Shared) {
    let id = Id::unique(("editor", view.pane));
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    doc.sync();
    doc.prepare_rows();

    let font = FontId::monospace(sh.st.font_size);
    let adv = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let row_h = (sh.st.font_size * 1.5).round();
    let digits = (doc.buf.line_count().max(99) as f32).log10().floor() as usize + 1;
    let gutter_w = digits as f32 * adv + 44.0;
    let vbar_w = 12.0;
    let mini_w = if sh.st.minimap { 96.0 } else { 0.0 };
    let content_right = rect.right() - vbar_w - mini_w;
    let content_px = (doc.longest as f32 + 8.0) * adv;
    let text_width = content_right - rect.left() - gutter_w;
    let hbar_h = if content_px > text_width { 11.0 } else { 0.0 };
    let bottom = rect.bottom() - hbar_h;
    let g = Geo {
        gutter: Rect::from_min_max(rect.min, Pos2::new(rect.left() + gutter_w, bottom)),
        text: Rect::from_min_max(Pos2::new(rect.left() + gutter_w, rect.top()), Pos2::new(content_right, bottom)),
        mini: (mini_w > 0.0).then(|| Rect::from_min_max(Pos2::new(content_right, rect.top()), Pos2::new(content_right + mini_w, rect.bottom()))),
        vbar: Rect::from_min_max(Pos2::new(rect.right() - vbar_w, rect.top()), rect.max),
        hbar: (hbar_h > 0.0).then(|| Rect::from_min_max(Pos2::new(rect.left() + gutter_w, bottom), Pos2::new(content_right, rect.bottom()))),
        adv,
        row_h,
        font: font.clone(),
        tab: sh.st.tab_size,
    };

    // mouse on the text
    let resp = ui.interact(g.text, id, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Text);
    if sh.want_focus || resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        resp.request_focus();
    }
    let focused = resp.has_focus();
    if focused {
        sh.out.push(Out::Focused);
        ui.memory_mut(|m| m.set_focus_lock_filter(id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
    }
    let time = ui.input(|i| i.time);
    let mods = ui.input(|i| i.modifiers);
    if let Some(p) = resp.interact_pointer_pos() {
        let at = pos_at(ui, doc, view, &g, p);
        if resp.triple_clicked() {
            let next = if at.line + 1 < doc.buf.line_count() { Pos::new(at.line + 1, 0) } else { Pos::new(at.line, doc.buf.line_len(at.line)) };
            doc.buf.select(Pos::new(at.line, 0), next);
        } else if resp.double_clicked() {
            let (a, b) = doc.buf.word_range(at);
            doc.buf.select(a, b);
        } else if resp.clicked() || resp.drag_started() {
            *sh.completion = None;
            if resp.clicked() && mods.command {
                if let Some((w, _, _)) = lang::word_at(doc.buf.line(at.line), at.col) {
                    sh.out.push(Out::GotoDefinition(w));
                }
            }
            if mods.alt {
                doc.buf.carets.push(Caret::at(at));
                doc.buf.normalize();
            } else if mods.shift {
                let anchor = doc.buf.carets[0].anchor;
                doc.buf.select(anchor, at);
            } else {
                // the click on a folded row's [...] opens it
                let line = at.line;
                if resp.clicked() && doc.folded.contains(&line) && at.col >= doc.buf.line_len(line) {
                    let endx = x_of(ui, doc.buf.line(line), at.col, &g);
                    let x = p.x - g.text.left() + view.scroll.x;
                    if x > endx + adv && x < endx + adv * 6.0 {
                        doc.toggle_fold(line);
                        doc.prepare_rows();
                    }
                }
                doc.buf.set_caret(at);
            }
            view.last_activity = time;
        } else if resp.dragged() {
            let i = doc.buf.carets.len() - 1;
            doc.buf.carets[i].head = at;
            doc.buf.carets[i].goal = None;
            view.last_activity = time;
            // keep selecting past the edges
            let (y, x) = (p.y, p.x);
            let mut moved = false;
            if y < g.text.top() {
                view.scroll.y -= (g.text.top() - y).min(row_h * 3.0);
                moved = true;
            } else if y > g.text.bottom() {
                view.scroll.y += (y - g.text.bottom()).min(row_h * 3.0);
                moved = true;
            }
            if x < g.text.left() {
                view.scroll.x -= g.text.left() - x;
                moved = true;
            } else if x > g.text.right() {
                view.scroll.x += x - g.text.right();
                moved = true;
            }
            if moved {
                ui.ctx().request_repaint();
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let at = pos_at(ui, doc, view, &g, p);
            if !doc.buf.carets.iter().any(|c| c.has_selection() && c.start() <= at && at <= c.end()) {
                doc.buf.set_caret(at);
            }
        }
    }
    resp.context_menu(|ui| {
        ui.set_min_width(250.0);
        let mut act = |ui: &mut Ui, a: Action| {
            if ui_util::menu_item(ui, a, true) {
                sh.out.push(Out::Do(a));
            }
        };
        act(ui, Action::Cut);
        act(ui, Action::Copy);
        act(ui, Action::Paste);
        act(ui, Action::SelectAll);
        ui.separator();
        act(ui, Action::GotoDefinition);
        act(ui, Action::QuickHelp);
        ui.separator();
        act(ui, Action::ToggleComment);
        act(ui, Action::ToggleBreakpoint);
        act(ui, Action::ToggleBookmark);
        act(ui, Action::ToggleFold);
        ui.separator();
        act(ui, Action::FormatFile);
        act(ui, Action::Palette);
    });

    // mouse on the gutter: breakpoints, fold arrows, line selection
    let gutter_resp = ui.interact(g.gutter, id.with("gutter"), Sense::click_and_drag());
    if let Some(p) = gutter_resp.interact_pointer_pos() {
        let row = ((p.y - g.gutter.top() + view.scroll.y) / row_h).floor().max(0.0) as usize;
        let line = doc.line_of_row(row);
        let x = p.x - g.gutter.left();
        if gutter_resp.clicked() {
            resp.request_focus();
            if x < 20.0 {
                if !doc.breakpoints.remove(&line) {
                    doc.breakpoints.insert(line);
                }
                sh.out.push(Out::BreakpointsChanged);
            } else if x > gutter_w - 18.0 && doc.fold_at(line).is_some() {
                doc.toggle_fold(line);
                doc.prepare_rows();
            } else {
                let next = if line + 1 < doc.buf.line_count() { Pos::new(line + 1, 0) } else { Pos::new(line, doc.buf.line_len(line)) };
                doc.buf.select(Pos::new(line, 0), next);
            }
        } else if gutter_resp.dragged() && x >= 20.0 && x <= gutter_w - 18.0 {
            let anchor = doc.buf.carets[0].anchor;
            let to = if line + 1 < doc.buf.line_count() { Pos::new(line + 1, 0) } else { Pos::new(line, doc.buf.line_len(line)) };
            doc.buf.select(anchor, to);
        }
    }

    if focused {
        handle_events(ui, doc, view, sh, g.visible_rows() as isize);
        doc.sync();
        for l in doc.buf.carets.iter().map(|c| c.head.line).collect::<Vec<_>>() {
            doc.unfold_around(l);
        }
        doc.prepare_rows();
        let (r, c) = (g.text, doc.buf.primary().head);
        let cursor_y = r.top() + doc.row_of_line(c.line) as f32 * row_h - view.scroll.y;
        let cursor_x = r.left() + x_of(ui, doc.buf.line(c.line), c.col, &g) - view.scroll.x;
        let cursor_rect = Rect::from_min_size(Pos2::new(cursor_x, cursor_y), Vec2::new(2.0, row_h));
        ui.ctx().output_mut(|o| o.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect: r, cursor_rect, should_interrupt_composition: false }));
    }

    // scrolling
    let rows = doc.row_count();
    let max_y = ((rows.saturating_sub(1)) as f32 * row_h).max(0.0);
    let max_x = (content_px - g.text.width()).max(0.0);
    if ui.rect_contains_pointer(rect) {
        let (wheel, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if zoom != 1.0 {
            sh.out.push(Out::Zoom(zoom));
        } else if wheel != Vec2::ZERO {
            view.scroll -= wheel;
            *sh.completion = None;
        }
    }
    if let Some(mode) = view.reveal.take() {
        let c = doc.buf.primary().head;
        let y = doc.row_of_line(c.line) as f32 * row_h;
        let h = g.text.height();
        match mode {
            Reveal::Center => view.scroll.y = y - h * 0.4,
            Reveal::Nearest => {
                let margin = row_h * 2.0;
                if y < view.scroll.y + margin {
                    view.scroll.y = y - margin;
                } else if y + row_h > view.scroll.y + h - margin {
                    view.scroll.y = y + row_h + margin - h;
                }
            }
        }
        let x = x_of(ui, doc.buf.line(c.line), c.col, &g);
        let w = g.text.width();
        if x < view.scroll.x + adv * 4.0 {
            view.scroll.x = x - adv * 4.0;
        } else if x > view.scroll.x + w - adv * 8.0 {
            view.scroll.x = x + adv * 8.0 - w;
        }
    }
    bars(ui, doc, view, sh, &g, id, max_y, max_x, rows);
    view.scroll.y = view.scroll.y.clamp(0.0, max_y);
    view.scroll.x = view.scroll.x.clamp(0.0, max_x);

    paint(ui, doc, view, sh, &g, focused, time);
    popups(ui, doc, view, sh, &g, &resp, focused, time);
}

#[allow(clippy::too_many_arguments)]
fn bars(ui: &mut Ui, doc: &Doc, view: &mut View, sh: &mut Shared, g: &Geo, id: Id, max_y: f32, max_x: f32, rows: usize) {
    let th = sh.th;
    let p = ui.painter().clone();
    let h = g.vbar.height();
    let page_px = g.text.height();
    let total = max_y + page_px;
    let thumb_h = (page_px / total * h).clamp(24.0, h);
    let track = h - thumb_h;
    let resp = ui.interact(g.vbar, id.with("vbar"), Sense::click_and_drag());
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() || (resp.is_pointer_button_down_on() && view.bar_grab.is_none()) {
            let thumb_top = g.vbar.top() + if max_y > 0.0 { view.scroll.y / max_y * track } else { 0.0 };
            view.bar_grab = Some(if pos.y >= thumb_top && pos.y <= thumb_top + thumb_h { pos.y - thumb_top } else { thumb_h * 0.5 });
        }
        if let Some(grab) = view.bar_grab {
            view.scroll.y = ((pos.y - grab - g.vbar.top()) / track.max(1.0)) * max_y;
        }
    }
    if !resp.is_pointer_button_down_on() {
        view.bar_grab = None;
    }
    p.rect_filled(g.vbar, 0.0, th.bg);
    let y0 = g.vbar.top() + if max_y > 0.0 { view.scroll.y.clamp(0.0, max_y) / max_y * track } else { 0.0 };
    let thumb = Rect::from_min_size(Pos2::new(g.vbar.left() + 1.0, y0), Vec2::new(g.vbar.width() - 1.0, thumb_h));
    let colour = if resp.dragged() { th.faint } else if resp.hovered() { th.faint.linear_multiply(0.8) } else { th.line };
    p.rect_filled(thumb, 0.0, colour);
    // overview marks
    let total_rows = rows.max(1) as f32;
    let mark = |line: usize, c: Color32, x0: f32, x1: f32| {
        let y = g.vbar.top() + doc.row_of_line(line) as f32 / total_rows * h;
        p.rect_filled(Rect::from_min_max(Pos2::new(g.vbar.left() + x0, y), Pos2::new(g.vbar.left() + x1, y + 2.0)), 0.0, c);
    };
    for (a, _) in sh.matches.iter().take(4000) {
        mark(a.line, th.match_bg, 3.0, 9.0);
    }
    for d in &doc.diags {
        mark(d.start.line, if d.error { th.error } else { th.warning }, 7.0, 12.0);
    }
    for &l in &doc.breakpoints {
        mark(l, th.error, 0.0, 4.0);
    }
    if let Some(l) = sh.exec_line {
        mark(l, th.warning, 0.0, 12.0);
    }
    mark(doc.buf.primary().head.line, th.text, 3.0, 9.0);

    if let Some(bar) = g.hbar {
        let resp = ui.interact(bar, id.with("hbar"), Sense::click_and_drag());
        p.rect_filled(bar, 0.0, th.bg);
        let w = bar.width();
        let total = max_x + g.text.width();
        let thumb_w = (g.text.width() / total * w).clamp(24.0, w);
        if let Some(pos) = resp.interact_pointer_pos() {
            view.scroll.x = ((pos.x - bar.left() - thumb_w * 0.5) / (w - thumb_w).max(1.0)) * max_x;
        }
        let x0 = bar.left() + if max_x > 0.0 { view.scroll.x.clamp(0.0, max_x) / max_x * (w - thumb_w) } else { 0.0 };
        p.rect_filled(Rect::from_min_size(Pos2::new(x0, bar.top() + 2.0), Vec2::new(thumb_w, bar.height() - 3.0)), 0.0, if resp.hovered() || resp.dragged() { th.faint } else { th.line });
    }
    if let Some(mini) = g.mini {
        let resp = ui.interact(mini, id.with("mini"), Sense::click_and_drag());
        if let Some(pos) = resp.interact_pointer_pos() {
            view.mini_drag = true;
            let content = rows as f32 * MINI_ROW;
            let off = mini_offset(view, g, max_y, content);
            let row = (pos.y - mini.top() + off) / MINI_ROW;
            view.scroll.y = row * g.row_h - g.text.height() * 0.5;
        } else {
            view.mini_drag = false;
        }
    }
}

fn mini_offset(view: &View, g: &Geo, max_y: f32, content: f32) -> f32 {
    let h = g.mini.map_or(0.0, |m| m.height());
    if content <= h || max_y <= 0.0 {
        0.0
    } else {
        (view.scroll.y.clamp(0.0, max_y) / max_y) * (content - h)
    }
}

fn row_job(chars: &[char], tokens: &[Token], g: &Geo, th: &Theme, scratch: &mut String) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.break_on_newline = false;
    let mut v = 0usize;
    let mut at = 0usize;
    let mut push = |job: &mut LayoutJob, from: usize, to: usize, colour: Color32, v: &mut usize| {
        scratch.clear();
        for &c in &chars[from..to] {
            if c == '\t' {
                let next = (*v / g.tab + 1) * g.tab;
                for _ in *v..next {
                    scratch.push(' ');
                }
                *v = next;
            } else {
                scratch.push(c);
                *v += 1;
            }
        }
        job.append(scratch, 0.0, TextFormat::simple(g.font.clone(), colour));
    };
    for t in tokens {
        if t.start > at {
            push(&mut job, at, t.start, th.text, &mut v);
        }
        push(&mut job, t.start, t.end, th.token(t.kind), &mut v);
        at = t.end;
    }
    if at < chars.len() {
        push(&mut job, at, chars.len(), th.text, &mut v);
    }
    job
}

fn paint(ui: &mut Ui, doc: &mut Doc, view: &mut View, sh: &mut Shared, g: &Geo, focused: bool, time: f64) {
    let th = sh.th;
    let full = g.gutter.union(g.text);
    ui.painter().rect_filled(full, 0.0, th.bg);
    let painter = ui.painter_at(g.text);
    let gp = ui.painter_at(g.gutter);
    let first = (view.scroll.y / g.row_h).floor().max(0.0) as usize;
    let count = (g.text.height() / g.row_h).ceil() as usize + 1;
    let last = (first + count).min(doc.row_count());
    let primary = doc.buf.primary();
    let cur_line = primary.head.line;
    let row_rect = |row: usize| {
        let y = g.text.top() + row as f32 * g.row_h - view.scroll.y;
        Rect::from_min_size(Pos2::new(g.text.left(), y), Vec2::new(g.text.width(), g.row_h))
    };
    let text_x = |x: f32| g.text.left() + x - view.scroll.x;

    // first the word at the caret, so it can be marked as we go through the rows
    let mut word: Option<String> = None;
    if doc.buf.carets.len() == 1 {
        if primary.has_selection() {
            if primary.start().line == primary.end().line {
                let t = doc.buf.text_in(primary.start(), primary.end());
                if t.trim().len() >= 2 {
                    word = Some(t.to_ascii_lowercase());
                }
            }
        } else if let Some((w, _, _)) = lang::word_at(doc.buf.line(cur_line), primary.head.col) {
            if sh.known.words.get(&w.to_ascii_lowercase()).is_none_or(|(_, k)| *k != Tk::Keyword) {
                word = Some(w.to_ascii_lowercase());
            }
        }
    }

    let mut tokens = Vec::new();
    let mut scratch = String::new();
    // which lines belong to the block at the caret
    let block = doc.enclosing_fold(cur_line);
    let first_word_end = |s: &str| s.chars().take_while(|c| c.is_whitespace()).count()..s.chars().take_while(|c| c.is_whitespace()).count() + s.trim_start().chars().take_while(|c| is_word(*c)).count();

    for row in first..last {
        let line = doc.line_of_row(row);
        let r = row_rect(row);
        let text = doc.buf.line(line);
        let chars: Vec<char> = text.chars().collect();
        if sh.exec_line == Some(line) {
            painter.rect_filled(r, 0.0, th.debug_line);
        } else if doc.buf.carets.iter().any(|c| c.head.line == line) {
            painter.rect_filled(r, 0.0, th.current_line);
        }

        // indent guides
        if sh.st.indent_guides {
            let mut indent = visual_col(text, chars.iter().take_while(|c| c.is_whitespace()).count(), g.tab);
            if text.trim().is_empty() {
                // blank lines inherit the guides of the next line with text
                let mut l = line + 1;
                while l < doc.buf.line_count() && l < line + 40 {
                    let t = doc.buf.line(l);
                    if !t.trim().is_empty() {
                        indent = visual_col(t, t.chars().take_while(|c| c.is_whitespace()).count(), g.tab);
                        break;
                    }
                    l += 1;
                }
            }
            let mut v = g.tab;
            while v < indent + 1 && v <= indent {
                if v < indent || !text.trim().is_empty() {
                    let x = text_x(v as f32 * g.adv - g.tab as f32 * g.adv) + 0.5;
                    let in_block = block.is_some_and(|(a, b)| a <= line && line <= b && visual_col(doc.buf.line(a), leading_ws(doc.buf.line(a)).chars().count(), g.tab) == v - g.tab);
                    painter.vline(x, r.y_range(), Stroke::new(1.0, if in_block { th.faint } else { th.guide }));
                }
                v += g.tab;
            }
        }

        // find results
        let span = |a: usize, b: usize| {
            let x0 = text_x(x_of(ui, text, a, g));
            let x1 = text_x(x_of(ui, text, b, g));
            Rect::from_min_max(Pos2::new(x0, r.top()), Pos2::new(x1.max(x0 + 1.0), r.bottom()))
        };
        let from = sh.matches.partition_point(|(a, _)| a.line < line);
        for (i, (a, b)) in sh.matches.iter().enumerate().skip(from) {
            if a.line != line {
                break;
            }
            let area = span(a.col, b.col);
            painter.rect_filled(area, 0.0, th.match_bg);
            if sh.current_match == Some(i) {
                painter.rect_stroke(area, 0.0, Stroke::new(1.0, th.warning), StrokeKind::Inside);
            }
        }

        // other uses of the word at the caret
        lang::tokenize(&chars, sh.known, &mut tokens);
        if let Some(w) = &word {
            let wc: Vec<char> = w.chars().collect();
            if primary.has_selection() {
                let mut i = 0;
                while i + wc.len() <= chars.len() {
                    if chars[i..i + wc.len()].iter().zip(&wc).all(|(a, b)| a.to_ascii_lowercase() == *b) {
                        painter.rect_filled(span(i, i + wc.len()), 0.0, th.word_bg);
                        i += wc.len();
                    } else {
                        i += 1;
                    }
                }
            } else {
                for t in tokens.iter().filter(|t| !matches!(t.kind, Tk::Str | Tk::Comment | Tk::Op | Tk::Number)) {
                    let end = if matches!(chars[t.end - 1], '%' | '#' | '$') { t.end - 1 } else { t.end };
                    if end - t.start == wc.len() && chars[t.start..end].iter().zip(&wc).all(|(a, b)| a.to_ascii_lowercase() == *b) {
                        painter.rect_filled(span(t.start, end), 0.0, th.word_bg);
                    }
                }
            }
        }

        // selections
        for c in &doc.buf.carets {
            if !c.has_selection() || line < c.start().line || line > c.end().line {
                continue;
            }
            let a = if line == c.start().line { c.start().col } else { 0 };
            let b = if line == c.end().line { c.end().col } else { chars.len() };
            let mut area = span(a, b);
            if line < c.end().line {
                area.max.x += g.adv * 0.6;
            }
            painter.rect_filled(area, 0.0, th.selection);
        }

        // matching opener and closer of the block at the caret
        if let Some((a, b)) = doc.enclosing_fold(cur_line).filter(|(a, b)| (cur_line == *a || cur_line == *b) && !primary.has_selection()) {
            if line == a || line == b {
                let range = first_word_end(text);
                let caret_in = if cur_line == a || cur_line == b { first_word_end(doc.buf.line(cur_line)).contains(&primary.head.col) || first_word_end(doc.buf.line(cur_line)).end == primary.head.col } else { false };
                if caret_in && range.end > range.start {
                    painter.rect_stroke(span(range.start, range.end), 0.0, Stroke::new(1.0, th.faint), StrokeKind::Inside);
                }
            }
        }

        // the text itself
        if !chars.is_empty() {
            let job = row_job(&chars, &tokens, g, th, &mut scratch);
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let y = r.top() + (g.row_h - galley.size().y) * 0.5;
            painter.galley(Pos2::new(text_x(0.0), y), galley, th.text);
        }

        if sh.st.whitespace {
            let ws_colour = th.faint;
            for (i, c) in chars.iter().enumerate() {
                let x = text_x(x_of(ui, text, i, g));
                let mid = r.center().y;
                if *c == ' ' {
                    painter.rect_filled(Rect::from_center_size(Pos2::new(x + g.adv * 0.5, mid), Vec2::splat(2.0)), 0.0, ws_colour);
                } else if *c == '\t' {
                    painter.hline(x + 2.0..=x + g.adv * 2.0, mid, Stroke::new(1.0, ws_colour));
                }
            }
        }

        // the fold marker and inline error text sit after the line's end
        let mut end_x = text_x(x_of(ui, text, chars.len(), g)) + g.adv;
        if doc.folded.contains(&line) {
            let area = Rect::from_min_size(Pos2::new(end_x, r.top() + 3.0), Vec2::new(g.adv * 4.0, g.row_h - 6.0));
            painter.rect_filled(area, 0.0, th.active);
            painter.text(area.center(), Align2::CENTER_CENTER, "...", g.font.clone(), th.dim);
            end_x += g.adv * 5.0;
        }
        if sh.st.inline_errors {
            if let Some(d) = doc.diags.iter().filter(|d| d.start.line == line).min_by_key(|d| !d.error) {
                let c = if d.error { th.error } else { th.warning };
                let msg = d.message.lines().next().unwrap_or("");
                painter.text(Pos2::new(end_x + g.adv * 2.0, r.center().y), Align2::LEFT_CENTER, msg, g.font.clone(), c.gamma_multiply(0.75));
            }
        }
        for d in doc.diags.iter().filter(|d| d.start.line <= line && line <= d.end.line) {
            let a = if line == d.start.line { d.start.col } else { 0 };
            let b = if line == d.end.line { d.end.col.max(a + 1) } else { chars.len().max(a + 1) };
            let x0 = text_x(x_of(ui, text, a.min(chars.len()), g));
            let x1 = (text_x(x_of(ui, text, b.min(chars.len()), g))).max(x0 + g.adv);
            let y = r.center().y + g.row_h * 0.40;
            let c = if d.error { th.error } else { th.warning };
            let mut pts = Vec::new();
            let mut x = x0;
            let mut up = false;
            while x < x1 {
                pts.push(Pos2::new(x, if up { y - 2.0 } else { y }));
                x += 2.0;
                up = !up;
            }
            pts.push(Pos2::new(x1, y));
            painter.add(Shape::line(pts, Stroke::new(1.0, c)));
        }

        // gutter
        let gr = Rect::from_min_size(Pos2::new(g.gutter.left(), r.top()), Vec2::new(g.gutter.width(), g.row_h));
        let cy = gr.center().y;
        let has_err = doc.diags.iter().any(|d| d.start.line == line && d.error);
        let has_warn = doc.diags.iter().any(|d| d.start.line == line && !d.error);
        let num_colour = if has_err {
            th.error
        } else if has_warn {
            th.warning
        } else if line == cur_line {
            th.text
        } else {
            th.faint
        };
        gp.text(Pos2::new(g.gutter.right() - 20.0, cy), Align2::RIGHT_CENTER, (line + 1).to_string(), g.font.clone(), num_colour);
        if doc.breakpoints.contains(&line) {
            gp.rect_filled(Rect::from_center_size(Pos2::new(gr.left() + 10.0, cy), Vec2::splat(10.0)), CornerRadius::ZERO, th.error);
        }
        if doc.bookmarks.contains(&line) {
            gp.rect_filled(Rect::from_min_size(Pos2::new(gr.left() + 1.0, gr.top() + 2.0), Vec2::new(3.0, g.row_h - 4.0)), 0.0, th.accent);
        }
        if sh.exec_line == Some(line) {
            let tip = Pos2::new(gr.left() + 17.0, cy);
            let pts = vec![Pos2::new(gr.left() + 5.0, cy - 6.0), tip, Pos2::new(gr.left() + 5.0, cy + 6.0)];
            gp.add(Shape::convex_polygon(pts, th.warning, Stroke::NONE));
        }
        if doc.fold_at(line).is_some() {
            let c = Pos2::new(g.gutter.right() - 9.0, cy);
            let closed = doc.folded.contains(&line);
            let colour = if closed { th.accent } else { th.faint };
            let s = 3.5;
            let pts = if closed { vec![c + Vec2::new(-s * 0.7, -s), c + Vec2::new(s, 0.0), c + Vec2::new(-s * 0.7, s)] } else { vec![c + Vec2::new(-s, -s * 0.7), c + Vec2::new(s, -s * 0.7), c + Vec2::new(0.0, s)] };
            gp.add(Shape::convex_polygon(pts, colour, Stroke::NONE));
        }
    }

    // past the last line
    ui.painter().vline(g.gutter.right() - 0.5, g.gutter.y_range(), Stroke::new(1.0, th.line));

    // matching bracket
    if let Some((a, b)) = matching_bracket(doc, primary.head) {
        for at in [a, b] {
            let row = doc.row_of_line(at.line);
            if row >= first && row < last {
                let r = row_rect(row);
                let t = doc.buf.line(at.line);
                let x0 = text_x(x_of(ui, t, at.col, g));
                let x1 = text_x(x_of(ui, t, at.col + 1, g));
                painter.rect_stroke(Rect::from_min_max(Pos2::new(x0, r.top() + 1.0), Pos2::new(x1, r.bottom() - 1.0)), 0.0, Stroke::new(1.0, th.accent), StrokeKind::Inside);
            }
        }
    }

    // carets
    let cycle = 1.06;
    let t = (time - view.last_activity).rem_euclid(cycle);
    let on = !sh.st.caret_blink || t < cycle * 0.55;
    if focused {
        if sh.st.caret_blink {
            let wait = if on { cycle * 0.55 - t } else { cycle - t };
            ui.ctx().request_repaint_after_secs(wait as f32 + 0.01);
        }
        if on {
            for c in &doc.buf.carets {
                let row = doc.row_of_line(c.head.line);
                if row < first || row >= last {
                    continue;
                }
                let r = row_rect(row);
                let x = text_x(x_of(ui, doc.buf.line(c.head.line), c.head.col, g));
                painter.rect_filled(Rect::from_min_size(Pos2::new(x - 1.0, r.top() + 1.0), Vec2::new(2.0, g.row_h - 2.0)), 0.0, th.caret);
            }
        }
    } else {
        for c in &doc.buf.carets {
            let row = doc.row_of_line(c.head.line);
            if row >= first && row < last {
                let r = row_rect(row);
                let x = text_x(x_of(ui, doc.buf.line(c.head.line), c.head.col, g));
                painter.rect_stroke(Rect::from_min_size(Pos2::new(x - 1.0, r.top() + 1.0), Vec2::new(2.0, g.row_h - 2.0)), 0.0, Stroke::new(1.0, th.faint), StrokeKind::Inside);
            }
        }
    }

    // minimap
    if let Some(mini) = g.mini {
        let p = ui.painter_at(mini);
        p.rect_filled(mini, 0.0, th.bg);
        p.vline(mini.left() + 0.5, mini.y_range(), Stroke::new(1.0, th.line));
        let rows = doc.row_count();
        let content = rows as f32 * MINI_ROW;
        let max_y = ((rows.saturating_sub(1)) as f32 * g.row_h).max(0.0);
        let off = mini_offset(view, g, max_y, content);
        let first_row = (off / MINI_ROW).floor() as usize;
        let n = (mini.height() / MINI_ROW).ceil() as usize + 1;
        let cell = 1.1_f32;
        let left = mini.left() + 6.0;
        let right = mini.right() - 2.0;
        let mut mesh = Mesh::default();
        for row in first_row..(first_row + n).min(rows) {
            let line = doc.line_of_row(row);
            let y = mini.top() + row as f32 * MINI_ROW - off;
            let chars: Vec<char> = doc.buf.line(line).chars().collect();
            lang::tokenize(&chars, sh.known, &mut tokens);
            let vis = |col: usize| -> f32 {
                let mut v = 0usize;
                for c in chars.iter().take(col) {
                    v = if *c == '\t' { (v / g.tab + 1) * g.tab } else { v + 1 };
                }
                v as f32
            };
            for t in &tokens {
                let x0 = left + vis(t.start) * cell;
                let x1 = (left + vis(t.end) * cell).min(right);
                if x0 < x1 {
                    mesh.add_colored_rect(Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 2.0)), th.token(t.kind).gamma_multiply(0.7));
                }
            }
            if doc.diags.iter().any(|d| d.start.line == line && d.error) {
                mesh.add_colored_rect(Rect::from_min_max(Pos2::new(mini.left() + 1.0, y), Pos2::new(mini.left() + 4.0, y + 3.0)), th.error);
            }
        }
        p.add(Shape::mesh(mesh));
        let vp = Rect::from_min_size(
            Pos2::new(mini.left() + 1.0, mini.top() + (view.scroll.y / g.row_h) * MINI_ROW - off),
            Vec2::new(mini.width() - 1.0, (g.text.height() / g.row_h) * MINI_ROW),
        );
        p.rect_filled(vp, 0.0, th.word_bg);
    }
}

fn matching_bracket(doc: &Doc, p: Pos) -> Option<(Pos, Pos)> {
    let chars: Vec<char> = doc.buf.line(p.line).chars().collect();
    let try_at = |i: usize| -> Option<(Pos, Pos)> {
        let c = *chars.get(i)?;
        let (other, dir) = match c {
            '(' => (')', 1i32),
            '[' => (']', 1),
            ')' => ('(', -1),
            ']' => ('[', -1),
            _ => return None,
        };
        let mut depth = 0;
        let mut j = i as i32;
        while j >= 0 && (j as usize) < chars.len() {
            let k = chars[j as usize];
            if k == c {
                depth += 1;
            } else if k == other {
                depth -= 1;
                if depth == 0 {
                    return Some((Pos::new(p.line, i), Pos::new(p.line, j as usize)));
                }
            }
            j += dir;
        }
        None
    };
    try_at(p.col).or_else(|| p.col.checked_sub(1).and_then(try_at))
}

fn popup_frame(th: &Theme) -> egui::Frame {
    egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.line)).inner_margin(egui::Margin::same(6)).shadow(egui::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(90) })
}

#[allow(clippy::too_many_arguments)]
fn popups(ui: &mut Ui, doc: &mut Doc, view: &mut View, sh: &mut Shared, g: &Geo, resp: &egui::Response, focused: bool, time: f64) {
    let th = sh.th;
    let ctx = ui.ctx().clone();
    let c = doc.buf.primary().head;
    let row = doc.row_of_line(c.line);
    let caret = Pos2::new(g.text.left() + x_of(ui, doc.buf.line(c.line), c.col, g) - view.scroll.x, g.text.top() + row as f32 * g.row_h - view.scroll.y);
    let mono = FontId::monospace((sh.st.font_size - 1.0).max(10.0));
    let body = FontId::proportional(13.0);

    // signature help
    if focused && doc.buf.carets.len() == 1 {
        if let Some((name, arg)) = lang::call_at(doc.buf.line(c.line), c.col) {
            let sig = sh.known.help_for(&name).map(|h| (h.signature.clone(), h.text.clone())).filter(|(s, _)| !s.is_empty()).or_else(|| {
                doc.decls.iter().find(|d| d.kind == lang::DeclKind::Function && d.name.eq_ignore_ascii_case(&name)).map(|d| (d.text.trim_start_matches("Function ").to_string(), String::new()))
            });
            if let Some((sig, help)) = sig {
                let params = lang::signature_params(&sig);
                let mut job = LayoutJob::default();
                job.wrap.max_width = 620.0;
                let open = sig.find('(').unwrap_or(sig.len());
                job.append(&sig[..(open + 1).min(sig.len())], 0.0, TextFormat::simple(mono.clone(), th.text));
                for (i, p) in params.iter().enumerate() {
                    let active = i == arg.min(params.len() - 1);
                    if i > 0 {
                        job.append(", ", 0.0, TextFormat::simple(mono.clone(), th.dim));
                    }
                    let mut f = TextFormat::simple(mono.clone(), if active { th.accent } else { th.dim });
                    if active {
                        f.underline = Stroke::new(1.0, th.accent);
                    }
                    job.append(p, 0.0, f);
                }
                job.append(")", 0.0, TextFormat::simple(mono.clone(), th.text));
                let galley = ctx.fonts_mut(|f| f.layout_job(job));
                let help_galley = (!help.is_empty()).then(|| ctx.fonts_mut(|f| f.layout(help.clone(), body.clone(), th.dim, 620.0)));
                let h = galley.size().y + help_galley.as_ref().map_or(0.0, |g| g.size().y + 4.0) + 14.0;
                let pos = Pos2::new(caret.x.min(ctx.content_rect().right() - 650.0).max(g.text.left()), (caret.y - h - 2.0).max(ctx.content_rect().top()));
                egui::Area::new(Id::unique(("sig", view.pane))).order(Order::Foreground).fixed_pos(pos).interactable(false).show(&ctx, |ui| {
                    popup_frame(th).show(ui, |ui| {
                        ui.label(galley);
                        if let Some(hg) = help_galley {
                            ui.label(hg);
                        }
                    });
                });
            }
        }
    }

    // completion
    if focused {
        let screen = ctx.content_rect();
        if let Some(comp) = sh.completion.as_mut() {
            let shown = 10usize;
            if comp.selected < comp.first_shown {
                comp.first_shown = comp.selected;
            } else if comp.selected >= comp.first_shown + shown {
                comp.first_shown = comp.selected + 1 - shown;
            }
            let row_h = 22.0;
            let list_h = comp.items.len().min(shown) as f32 * row_h;
            let below = caret.y + g.row_h + 2.0;
            let y = if below + list_h + 16.0 > screen.bottom() { caret.y - list_h - 16.0 } else { below };
            let pos = Pos2::new((caret.x - 28.0).max(screen.left()), y);
            let mut clicked = None;
            egui::Area::new(Id::unique(("comp", view.pane))).order(Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
                popup_frame(th).inner_margin(egui::Margin::same(1)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            for i in comp.first_shown..(comp.first_shown + shown).min(comp.items.len()) {
                                let it = &comp.items[i];
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(300.0, row_h), Sense::click());
                                let sel = i == comp.selected;
                                if sel || resp.hovered() {
                                    ui.painter().rect_filled(r, 0.0, if sel { th.active } else { th.hover });
                                }
                                if resp.clicked() {
                                    clicked = Some(i);
                                }
                                let (tag, colour) = match it.kind {
                                    CompKind::Keyword => ("K", th.keyword),
                                    CompKind::Command => ("C", th.command),
                                    CompKind::Constant => ("#", th.constant),
                                    CompKind::Symbol => ("S", th.function),
                                    CompKind::Snippet => ("T", th.ty),
                                    CompKind::Word => ("W", th.dim),
                                };
                                let badge = Rect::from_center_size(Pos2::new(r.left() + 13.0, r.center().y), Vec2::splat(15.0));
                                ui.painter().rect_stroke(badge, 0.0, Stroke::new(1.0, colour.gamma_multiply(0.6)), StrokeKind::Inside);
                                ui.painter().text(badge.center(), Align2::CENTER_CENTER, tag, FontId::monospace(10.0), colour);
                                ui.painter().text(Pos2::new(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, &it.label, mono.clone(), th.text);
                                if it.kind == CompKind::Snippet {
                                    ui.painter().text(Pos2::new(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, "snippet", FontId::proportional(11.0), th.faint);
                                }
                            }
                        });
                        let it = &comp.items[comp.selected.min(comp.items.len() - 1)];
                        if !it.detail.is_empty() || !it.doc.is_empty() {
                            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(300.0);
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    if !it.detail.is_empty() {
                                        ui.add(egui::Label::new(egui::RichText::new(&it.detail).font(mono.clone()).color(th.accent)).wrap());
                                    }
                                    if !it.doc.is_empty() {
                                        ui.add(egui::Label::new(egui::RichText::new(&it.doc).font(body.clone()).color(th.dim)).wrap());
                                    }
                                });
                            });
                        }
                    });
                });
            });
            if let Some(i) = clicked {
                comp.selected = i;
                accept_completion(doc, sh);
            }
        }
    }

    // hover
    let hover_pos = if resp.hovered() { resp.hover_pos() } else { None };
    let mut show_hover = None;
    if let Some(p) = hover_pos {
        let at = pos_at(ui, doc, view, g, p);
        let on_text = {
            let row_top = g.text.top() + doc.row_of_line(at.line) as f32 * g.row_h - view.scroll.y;
            p.y >= row_top && p.y < row_top + g.row_h && at.col < doc.buf.line_len(at.line) + 1
        };
        match view.hover {
            Some((old, since)) if old == at => {
                if time - since > 0.4 && on_text {
                    show_hover = Some((at, p));
                }
            }
            _ => {
                view.hover = Some((at, time));
                ctx.request_repaint_after_secs(0.45);
            }
        }
    } else {
        view.hover = None;
    }
    if let (Some((at, p)), true) = (show_hover, sh.completion.is_none()) {
        let line = doc.buf.line(at.line);
        let word = lang::word_at(line, at.col);
        let diag = doc.diags.iter().find(|d| d.start.line <= at.line && at.line <= d.end.line && (at.line != d.start.line || at.col >= d.start.col) && (at.line != d.end.line || at.col <= d.end.col.max(d.start.col + 1)));
        let mut parts: Vec<(String, FontId, Color32)> = Vec::new();
        if let Some(d) = diag {
            parts.push((d.message.clone(), body.clone(), if d.error { th.error } else { th.warning }));
        }
        if let Some((w, ..)) = &word {
            let chars: Vec<char> = line.chars().collect();
            let mut toks = Vec::new();
            lang::tokenize(&chars, sh.known, &mut toks);
            let kind = toks.iter().find(|t| t.start <= at.col && at.col < t.end).map(|t| t.kind);
            if matches!(kind, Some(Tk::Str | Tk::Comment)) {
                // nothing to say about the inside of a string
            } else if let Some(h) = sh.known.help_for(w) {
                parts.push((if h.signature.is_empty() { h.name.clone() } else { h.signature.clone() }, mono.clone(), th.accent));
                parts.push((h.text.clone(), body.clone(), th.text));
            } else if let Some(d) = doc.decls.iter().find(|d| d.name.eq_ignore_ascii_case(w)) {
                parts.push((d.text.clone(), mono.clone(), th.accent));
                parts.push((format!("Line {}", d.line + 1), body.clone(), th.dim));
            }
            if sh.debugging && kind == Some(Tk::Plain) {
                match sh.eval.get(w) {
                    Some(v) => parts.push((format!("{w} = {v}"), mono.clone(), th.number)),
                    None => sh.out.push(Out::Eval(w.clone())),
                }
            }
        }
        if !parts.is_empty() {
            egui::Area::new(Id::unique(("hover", view.pane))).order(Order::Tooltip).fixed_pos(p + Vec2::new(8.0, 14.0)).interactable(false).constrain(true).show(&ctx, |ui| {
                popup_frame(th).show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for (t, f, c) in parts {
                        ui.add(egui::Label::new(egui::RichText::new(t).font(f).color(c)).wrap());
                    }
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Doc {
        let mut d = Doc::new(1, text);
        d.sync();
        d
    }

    #[test]
    fn comments_toggle() {
        let mut d = doc("a\n\tb");
        d.buf.select(Pos::new(0, 0), Pos::new(1, 2));
        toggle_comment(&mut d);
        assert_eq!(d.buf.text(), "; a\n; \tb");
        toggle_comment(&mut d);
        assert_eq!(d.buf.text(), "a\n\tb");
    }

    #[test]
    fn formatting_indents_blocks_and_fixes_capitals() {
        let known = Known::new();
        let st = Settings::default();
        let mut d = doc("function f()\nif a then\nprint \"x\"\nelse\nb=1\nendif\nend function");
        format_document(&mut d, &st, &known);
        assert_eq!(d.buf.text(), "Function f()\n\tIf a Then\n\t\tPrint \"x\"\n\tElse\n\t\tb=1\n\tEndIf\nEnd Function");
    }

    #[test]
    fn indenting_keeps_selection() {
        let st = Settings::default();
        let mut d = doc("ab\ncd");
        d.buf.select(Pos::new(0, 1), Pos::new(1, 1));
        indent_lines(&mut d, &st);
        assert_eq!(d.buf.text(), "\tab\n\tcd");
        assert_eq!(d.buf.primary().start(), Pos::new(0, 2));
        outdent_lines(&mut d, &st);
        assert_eq!(d.buf.text(), "ab\ncd");
    }

    #[test]
    fn brackets_match() {
        let d = doc("a(b[1], c)");
        assert_eq!(matching_bracket(&d, Pos::new(0, 1)), Some((Pos::new(0, 1), Pos::new(0, 9))));
        assert_eq!(matching_bracket(&d, Pos::new(0, 6)), Some((Pos::new(0, 5), Pos::new(0, 3))));
        assert_eq!(matching_bracket(&d, Pos::new(0, 8)), None);
    }

    #[test]
    fn autocap_fixes_the_word_just_finished() {
        let known = Known::new();
        let mut d = doc("positionentity");
        let mut t = Vec::new();
        autocap_before(&mut d, &known, 0, 14, EditKind::Typing, &mut t);
        assert_eq!(d.buf.text(), "PositionEntity");
        let mut d = doc("\"print\"");
        autocap_before(&mut d, &known, 0, 6, EditKind::Typing, &mut t);
        assert_eq!(d.buf.text(), "\"print\"");
    }
}
