//! The text of one document: lines, carets (multi-caret from the ground up)
//! and undo. Knows nothing about the screen.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash)]
pub struct Pos {
    pub line: usize,
    /// In characters.
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Pos { line, col }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Caret {
    pub anchor: Pos,
    pub head: Pos,
    /// Screen column kept while moving up and down.
    pub goal: Option<usize>,
}

impl Caret {
    pub fn at(p: Pos) -> Self {
        Caret { anchor: p, head: p, goal: None }
    }
    pub fn start(&self) -> Pos {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> Pos {
        self.anchor.max(self.head)
    }
    pub fn has_selection(&self) -> bool {
        self.anchor != self.head
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Edit {
    at: Pos,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Step {
    edits: Vec<Edit>,
    before: Vec<Caret>,
    after: Vec<Caret>,
    kind: EditKind,
    when: Instant,
    /// Identifies the text after this step (see `Buffer::state`).
    state: u64,
}

/// Lets line-based marks (breakpoints, folds, bookmarks) follow the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineShift {
    pub first: usize,
    pub last: usize,
    pub delta: isize,
    /// The edit began at column 0, so a mark on `first` moves with the text.
    pub at_line_start: bool,
}

impl LineShift {
    /// Where a mark on `line` ends up, or `None` when its line was removed.
    pub fn apply(&self, line: usize) -> Option<usize> {
        if self.delta == 0 || line < self.first {
            return Some(line);
        }
        if line == self.first && !(self.at_line_start && self.delta > 0) {
            return Some(line);
        }
        if self.delta < 0 && line <= self.last {
            return None;
        }
        Some((line as isize + self.delta).max(0) as usize)
    }
}

pub struct Buffer {
    lines: Vec<String>,
    pub carets: Vec<Caret>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Goes up on every change to the text.
    pub version: u64,
    /// Identifies the text: undoing back to the saved text gets its id again.
    state: u64,
    next_state: u64,
    saved_state: u64,
    joinable: bool,
    pub shifts: Vec<LineShift>,
}

fn byte_of(s: &str, col: usize) -> usize {
    if s.is_ascii() {
        return col.min(s.len());
    }
    s.char_indices().nth(col).map_or(s.len(), |(b, _)| b)
}

pub fn char_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The position just after `text` when it is written at `at`.
fn advance(at: Pos, text: &str) -> Pos {
    match text.rfind('\n') {
        None => Pos::new(at.line, at.col + char_len(text)),
        Some(i) => Pos::new(at.line + text.matches('\n').count(), char_len(&text[i + 1..])),
    }
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Buffer { lines, carets: vec![Caret::default()], undo: Vec::new(), redo: Vec::new(), version: 1, state: 0, next_state: 1, saved_state: 0, joinable: false, shifts: Vec::new() }
    }

    pub fn is_modified(&self) -> bool {
        self.state != self.saved_state
    }
    pub fn mark_saved(&mut self) {
        self.saved_state = self.state;
    }
    pub fn mark_unsaved(&mut self) {
        self.saved_state = u64::MAX;
    }
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", String::as_str)
    }
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn line_len(&self, i: usize) -> usize {
        char_len(self.line(i))
    }
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
    pub fn end(&self) -> Pos {
        let last = self.lines.len() - 1;
        Pos::new(last, self.line_len(last))
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len() - 1);
        Pos::new(line, p.col.min(self.line_len(line)))
    }

    pub fn text_in(&self, a: Pos, b: Pos) -> String {
        let (a, b) = (self.clamp(a), self.clamp(b));
        if a.line == b.line {
            let l = self.line(a.line);
            return l[byte_of(l, a.col)..byte_of(l, b.col)].to_string();
        }
        let first = self.line(a.line);
        let mut out = first[byte_of(first, a.col)..].to_string();
        for l in &self.lines[a.line + 1..b.line] {
            out.push('\n');
            out.push_str(l);
        }
        out.push('\n');
        let last = self.line(b.line);
        out.push_str(&last[..byte_of(last, b.col)]);
        out
    }

    pub fn char_at(&self, p: Pos) -> Option<char> {
        self.line(p.line).chars().nth(p.col)
    }

    /// Replaces a range and returns where the new text ends.
    fn raw_replace(&mut self, s: Pos, e: Pos, text: &str) -> Pos {
        let end = advance(s, text);
        if s.line == e.line && !text.contains('\n') {
            let l = &mut self.lines[s.line];
            let (a, b) = (byte_of(l, s.col), byte_of(l, e.col));
            l.replace_range(a..b, text);
        } else {
            let tail_at = byte_of(&self.lines[e.line], e.col);
            let tail = self.lines[e.line][tail_at..].to_string();
            let head_at = byte_of(&self.lines[s.line], s.col);
            let mut parts = text.split('\n');
            let mut first = self.lines[s.line][..head_at].to_string();
            first.push_str(parts.next().unwrap_or(""));
            let mut new: Vec<String> = vec![first];
            new.extend(parts.map(str::to_string));
            new.last_mut().unwrap().push_str(&tail);
            self.lines.splice(s.line..=e.line, new);
        }
        let delta = end.line as isize - e.line as isize;
        if delta != 0 {
            self.shifts.push(LineShift { first: s.line, last: e.line, delta, at_line_start: s.col == 0 });
        }
        end
    }

    fn bump(&mut self) {
        self.version += 1;
    }

    /// Applies sorted, non-overlapping replacements; `carets` gets where each
    /// one ended and chooses the carets after the edit.
    pub fn edit(&mut self, items: Vec<(Pos, Pos, String)>, kind: EditKind, carets: impl FnOnce(&[Pos]) -> Vec<Caret>) {
        let before = self.carets.clone();
        let mut items: Vec<(Pos, Pos, String)> = items.into_iter().map(|(a, b, t)| (self.clamp(a), self.clamp(b), t)).collect();
        items.retain(|(a, b, t)| a != b || !t.is_empty());
        let mut edits = Vec::with_capacity(items.len());
        let mut ends = Vec::with_capacity(items.len());
        for i in 0..items.len() {
            let (s, e, text) = items[i].clone();
            let removed = self.text_in(s, e);
            let p = self.raw_replace(s, e, &text);
            edits.push(Edit { at: s, removed, inserted: text });
            ends.push(p);
            // later replacements move with this one
            let shift = |q: Pos| {
                if q.line == e.line {
                    Pos::new(p.line, q.col - e.col + p.col)
                } else {
                    Pos::new((q.line as isize + p.line as isize - e.line as isize) as usize, q.col)
                }
            };
            for later in &mut items[i + 1..] {
                later.0 = shift(later.0);
                later.1 = shift(later.1);
            }
        }
        self.carets = carets(&ends);
        self.normalize();
        if edits.is_empty() {
            return;
        }
        self.bump();
        self.redo.clear();
        self.state = self.next_state;
        self.next_state += 1;
        let state = self.state;
        let now = Instant::now();
        let breaks = edits.iter().any(|e| e.inserted.contains('\n') || e.removed.contains('\n'));
        if self.joinable && kind != EditKind::Other && !breaks {
            if let Some(last) = self.undo.last_mut() {
                if last.kind == kind && now.duration_since(last.when) < Duration::from_millis(900) {
                    last.edits.extend(edits);
                    last.after = self.carets.clone();
                    last.when = now;
                    last.state = state;
                    return;
                }
            }
        }
        self.undo.push(Step { edits, before, after: self.carets.clone(), kind, when: now, state });
        if self.undo.len() > 4000 {
            self.undo.drain(..500);
        }
        self.joinable = kind != EditKind::Other && !breaks;
    }

    /// The next typed character starts a new undo step.
    pub fn break_undo_group(&mut self) {
        self.joinable = false;
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else { return false };
        for e in step.edits.iter().rev() {
            let end = advance(e.at, &e.inserted);
            self.raw_replace(e.at, end, &e.removed);
        }
        self.carets = step.before.clone();
        self.redo.push(step);
        self.joinable = false;
        self.bump();
        self.state = self.undo_state();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else { return false };
        for e in &step.edits {
            let end = advance(e.at, &e.removed);
            self.raw_replace(e.at, end, &e.inserted);
        }
        self.carets = step.after.clone();
        self.undo.push(step);
        self.joinable = false;
        self.bump();
        self.state = self.undo_state();
        true
    }

    fn undo_state(&self) -> u64 {
        self.undo.last().map_or(0, |s| s.state)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Sorts carets and merges the ones that touch.
    pub fn normalize(&mut self) {
        for c in &mut self.carets {
            let line = c.head.line.min(self.lines.len() - 1);
            c.head = Pos::new(line, c.head.col.min(char_len(&self.lines[line])));
            let line = c.anchor.line.min(self.lines.len() - 1);
            c.anchor = Pos::new(line, c.anchor.col.min(char_len(&self.lines[line])));
        }
        self.carets.sort_by_key(|c| c.start());
        let mut out: Vec<Caret> = Vec::with_capacity(self.carets.len());
        for c in self.carets.drain(..) {
            match out.last_mut() {
                Some(p) if c.start() < p.end() || (c.start() == p.end() && (!c.has_selection() || !p.has_selection())) => {
                    let (s, e) = (p.start().min(c.start()), p.end().max(c.end()));
                    let flipped = p.head < p.anchor;
                    *p = if flipped { Caret { anchor: e, head: s, goal: None } } else { Caret { anchor: s, head: e, goal: None } };
                }
                _ => out.push(c),
            }
        }
        self.carets = out;
        if self.carets.is_empty() {
            self.carets.push(Caret::default());
        }
    }

    pub fn primary(&self) -> Caret {
        *self.carets.last().unwrap()
    }

    pub fn set_caret(&mut self, p: Pos) {
        let p = self.clamp(p);
        self.carets = vec![Caret::at(p)];
        self.joinable = false;
    }

    pub fn select(&mut self, a: Pos, b: Pos) {
        self.carets = vec![Caret { anchor: self.clamp(a), head: self.clamp(b), goal: None }];
        self.joinable = false;
    }

    pub fn select_all(&mut self) {
        self.select(Pos::default(), self.end());
    }

    /// Moves every caret; with `extend` the selections grow.
    pub fn move_carets(&mut self, extend: bool, mut f: impl FnMut(&Buffer, Caret) -> Caret) {
        let carets: Vec<Caret> = self.carets.iter().map(|&c| f(self, c)).collect();
        self.carets = carets
            .into_iter()
            .zip(self.carets.iter())
            .map(|(n, o)| if extend { Caret { anchor: o.anchor, head: n.head, goal: n.goal } } else { Caret { anchor: n.head, head: n.head, goal: n.goal } })
            .collect();
        self.joinable = false;
        self.normalize();
    }

    pub fn left_of(&self, p: Pos) -> Pos {
        if p.col > 0 {
            Pos::new(p.line, p.col - 1)
        } else if p.line > 0 {
            Pos::new(p.line - 1, self.line_len(p.line - 1))
        } else {
            p
        }
    }

    pub fn right_of(&self, p: Pos) -> Pos {
        if p.col < self.line_len(p.line) {
            Pos::new(p.line, p.col + 1)
        } else if p.line + 1 < self.lines.len() {
            Pos::new(p.line + 1, 0)
        } else {
            p
        }
    }

    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return self.left_of(p);
        }
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let mut i = p.col.min(chars.len());
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let word = is_word(chars[i - 1]);
            while i > 0 && !chars[i - 1].is_whitespace() && is_word(chars[i - 1]) == word {
                i -= 1;
            }
        }
        Pos::new(p.line, i)
    }

    pub fn word_right(&self, p: Pos) -> Pos {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        if p.col >= chars.len() {
            return self.right_of(p);
        }
        let mut i = p.col;
        let word = is_word(chars[i]);
        if !chars[i].is_whitespace() {
            while i < chars.len() && !chars[i].is_whitespace() && is_word(chars[i]) == word {
                i += 1;
            }
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        Pos::new(p.line, i)
    }

    /// The word around a position as a range.
    pub fn word_range(&self, p: Pos) -> (Pos, Pos) {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let col = p.col.min(chars.len());
        let class = |c: char| if is_word(c) { 0 } else if c.is_whitespace() { 1 } else { 2 };
        let probe = if col < chars.len() && (is_word(chars[col]) || col == 0 || !is_word(chars[col - 1])) { col } else { col.saturating_sub(1) };
        let Some(&c) = chars.get(probe) else { return (p, p) };
        let k = class(c);
        let (mut a, mut b) = (probe, probe + 1);
        while a > 0 && class(chars[a - 1]) == k {
            a -= 1;
        }
        while b < chars.len() && class(chars[b]) == k {
            b += 1;
        }
        (Pos::new(p.line, a), Pos::new(p.line, b))
    }

    /// Smart Home: first the start of the text, then column 0.
    pub fn home_of(&self, p: Pos) -> Pos {
        let first = self.line(p.line).chars().take_while(|c| c.is_whitespace()).count();
        Pos::new(p.line, if p.col == first { 0 } else { first })
    }

    /// Writes `text` at each caret, replacing selections.
    pub fn insert(&mut self, text: &str, kind: EditKind) {
        let items = self.carets.iter().map(|c| (c.start(), c.end(), text.to_string())).collect();
        self.edit(items, kind, |ends| ends.iter().map(|&p| Caret::at(p)).collect());
    }

    /// Pastes text; one line per caret when the counts match.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let parts: Vec<&str> = text.split('\n').collect();
        if self.carets.len() > 1 && parts.len() == self.carets.len() {
            let items = self.carets.iter().zip(parts).map(|(c, p)| (c.start(), c.end(), p.to_string())).collect();
            self.edit(items, EditKind::Other, |ends| ends.iter().map(|&p| Caret::at(p)).collect());
        } else {
            self.insert(&text, EditKind::Other);
        }
    }

    /// The selected text, or the caret's whole line with its line ending.
    pub fn copy_text(&self) -> String {
        if self.carets.iter().any(Caret::has_selection) {
            let parts: Vec<String> = self.carets.iter().filter(|c| c.has_selection()).map(|c| self.text_in(c.start(), c.end())).collect();
            return parts.join("\n");
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut out = String::new();
        for c in &self.carets {
            if seen.insert(c.head.line) {
                out.push_str(self.line(c.head.line));
                out.push('\n');
            }
        }
        out
    }

    /// Deletes the selections, or what `range` picks for a caret without one.
    pub fn delete_with(&mut self, kind: EditKind, mut range: impl FnMut(&Buffer, Pos) -> (Pos, Pos)) {
        let items = self
            .carets
            .iter()
            .map(|c| if c.has_selection() { (c.start(), c.end(), String::new()) } else { let (a, b) = range(self, c.head); (a, b, String::new()) })
            .collect::<Vec<_>>();
        // two carets can reach for the same character
        let mut merged: Vec<(Pos, Pos, String)> = Vec::with_capacity(items.len());
        let mut items = items;
        items.sort_by_key(|i| i.0);
        for it in items {
            match merged.last_mut() {
                Some(p) if it.0 < p.1 => p.1 = p.1.max(it.1),
                _ => merged.push(it),
            }
        }
        self.edit(merged, kind, |ends| ends.iter().map(|&p| Caret::at(p)).collect());
    }

    /// Lines covered by carets, merged into ranges of whole lines.
    pub fn caret_line_ranges(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for c in &self.carets {
            let (s, mut e) = (c.start(), c.end());
            if e.col == 0 && e.line > s.line {
                e.line -= 1;
            }
            match out.last_mut() {
                Some(r) if s.line <= r.1 + 1 => r.1 = r.1.max(e.line),
                _ => out.push((s.line, e.line)),
            }
        }
        out
    }

    /// Replaces a whole-line range with new lines, keeping carets in place
    /// where the transform keeps columns the same size (`col_delta` by line).
    pub fn rewrite_lines(&mut self, ranges: &[(usize, usize)], mut f: impl FnMut(usize, &str) -> String) {
        let mut items = Vec::new();
        let mut deltas: Vec<(usize, isize)> = Vec::new();
        for &(a, b) in ranges {
            for l in a..=b {
                let old = self.line(l).to_string();
                let new = f(l, &old);
                if new != old {
                    let (mut p, oc, nc) = (0, old.chars().collect::<Vec<_>>(), new.chars().collect::<Vec<_>>());
                    while p < oc.len() && p < nc.len() && oc[p] == nc[p] {
                        p += 1;
                    }
                    let (mut oe, mut ne) = (oc.len(), nc.len());
                    while oe > p && ne > p && oc[oe - 1] == nc[ne - 1] {
                        oe -= 1;
                        ne -= 1;
                    }
                    items.push((Pos::new(l, p), Pos::new(l, oe), nc[p..ne].iter().collect::<String>()));
                    deltas.push((l, nc.len() as isize - oc.len() as isize));
                }
            }
        }
        let old = self.carets.clone();
        self.edit(items, EditKind::Other, |_| {
            let adj = |p: Pos| {
                let d = deltas.iter().find(|(l, _)| *l == p.line).map_or(0, |x| x.1);
                Pos::new(p.line, if p.col == 0 && d > 0 { 0 } else { (p.col as isize + d).max(0) as usize })
            };
            old.iter().map(|c| Caret { anchor: adj(c.anchor), head: adj(c.head), goal: None }).collect()
        });
    }

    pub fn delete_lines(&mut self) {
        let ranges = self.caret_line_ranges();
        let last = self.lines.len() - 1;
        let items = ranges
            .iter()
            .map(|&(a, b)| {
                if b < last {
                    (Pos::new(a, 0), Pos::new(b + 1, 0), String::new())
                } else if a > 0 {
                    (Pos::new(a - 1, self.line_len(a - 1)), Pos::new(b, self.line_len(b)), String::new())
                } else {
                    (Pos::new(a, 0), Pos::new(b, self.line_len(b)), String::new())
                }
            })
            .collect();
        self.edit(items, EditKind::Other, |e| e.iter().map(|&p| Caret::at(p)).collect());
    }

    /// Copies the selected lines below themselves.
    pub fn duplicate_lines(&mut self) {
        let ranges = self.caret_line_ranges();
        let carets = self.carets.clone();
        let items: Vec<_> = ranges
            .iter()
            .map(|&(a, b)| {
                let end = Pos::new(b, self.line_len(b));
                (end, end, format!("\n{}", self.text_in(Pos::new(a, 0), end)))
            })
            .collect();
        let shift = move |p: Pos| {
            let mut add = 0;
            for &(a, b) in &ranges {
                if p.line >= a {
                    add += b - a + 1;
                }
            }
            Pos::new(p.line + add, p.col)
        };
        self.edit(items, EditKind::Other, |_| carets.iter().map(|c| Caret { anchor: shift(c.anchor), head: shift(c.head), goal: None }).collect());
    }

    /// Moves the selected lines up (`-1`) or down (`1`).
    pub fn move_lines(&mut self, dir: isize) {
        let ranges = self.caret_line_ranges();
        let last = self.lines.len() - 1;
        if (dir < 0 && ranges.first().is_none_or(|r| r.0 == 0)) || (dir > 0 && ranges.last().is_none_or(|r| r.1 == last)) {
            return;
        }
        let carets = self.carets.clone();
        let mut items = Vec::new();
        for &(a, b) in &ranges {
            let (from, to) = if dir < 0 { (a - 1, b) } else { (a, b + 1) };
            let block = self.text_in(Pos::new(a, 0), Pos::new(b, self.line_len(b)));
            let other = if dir < 0 { self.line(a - 1).to_string() } else { self.line(b + 1).to_string() };
            let text = if dir < 0 { format!("{block}\n{other}") } else { format!("{other}\n{block}") };
            items.push((Pos::new(from, 0), Pos::new(to, self.line_len(to)), text));
        }
        let mv = |p: Pos| Pos::new((p.line as isize + dir) as usize, p.col);
        self.edit(items, EditKind::Other, |_| carets.iter().map(|c| Caret { anchor: mv(c.anchor), head: mv(c.head), goal: None }).collect());
    }

    /// Adds the next occurrence of the selection (or word) as another caret.
    pub fn add_next_occurrence(&mut self) {
        let p = self.primary();
        if !p.has_selection() {
            let (a, b) = self.word_range(p.head);
            let idx = self.carets.len() - 1;
            self.carets[idx] = Caret { anchor: a, head: b, goal: None };
            return;
        }
        let needle = self.text_in(p.start(), p.end());
        if needle.contains('\n') {
            return;
        }
        let from = p.end();
        let total = self.lines.len();
        for i in 0..=total {
            let line_no = (from.line + i) % total;
            let line = self.line(line_no);
            let start_col = if i == 0 { from.col } else { 0 };
            let start_byte = byte_of(line, start_col);
            if let Some(found) = line[start_byte..].find(&needle) {
                let col = char_len(&line[..start_byte + found]);
                let c = Caret { anchor: Pos::new(line_no, col), head: Pos::new(line_no, col + char_len(&needle)), goal: None };
                if !self.carets.iter().any(|o| o.start() == c.start()) {
                    self.carets.push(c);
                    // the newest caret is the primary one
                    let keep = c;
                    self.normalize();
                    if let Some(i) = self.carets.iter().position(|o| *o == keep) {
                        let c = self.carets.remove(i);
                        self.carets.push(c);
                    }
                }
                return;
            }
        }
    }
}

/// Screen column of character `col`.
pub fn visual_col(line: &str, col: usize, tab: usize) -> usize {
    let mut v = 0;
    for c in line.chars().take(col) {
        v = if c == '\t' { (v / tab + 1) * tab } else { v + 1 };
    }
    v
}

/// Character index at a screen column.
pub fn col_at_visual(line: &str, target: usize, tab: usize) -> usize {
    let mut v = 0;
    let mut i = 0;
    for c in line.chars() {
        let next = if c == '\t' { (v / tab + 1) * tab } else { v + 1 };
        if target < next && target * 2 < v + next {
            return i;
        }
        if target < next {
            return i + 1;
        }
        v = next;
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_undo() {
        let mut b = Buffer::new("hello\nworld");
        b.set_caret(Pos::new(0, 5));
        b.insert(" there\nnew", EditKind::Other);
        assert_eq!(b.text(), "hello there\nnew\nworld");
        assert_eq!(b.primary().head, Pos::new(1, 3));
        assert!(b.is_modified());
        b.undo();
        assert_eq!(b.text(), "hello\nworld");
        assert!(!b.is_modified());
        b.redo();
        assert_eq!(b.text(), "hello there\nnew\nworld");
    }

    #[test]
    fn multi_caret_typing() {
        let mut b = Buffer::new("aa\nbb\ncc");
        b.carets = vec![Caret::at(Pos::new(0, 1)), Caret::at(Pos::new(1, 1)), Caret::at(Pos::new(2, 1))];
        b.insert("X\nY", EditKind::Other);
        assert_eq!(b.text(), "aX\nYa\nbX\nYb\ncX\nYc");
        assert_eq!(b.carets.iter().map(|c| c.head).collect::<Vec<_>>(), vec![Pos::new(1, 1), Pos::new(3, 1), Pos::new(5, 1)]);
        b.undo();
        assert_eq!(b.text(), "aa\nbb\ncc");
    }

    #[test]
    fn same_line_carets() {
        let mut b = Buffer::new("abc");
        b.carets = vec![Caret::at(Pos::new(0, 1)), Caret::at(Pos::new(0, 2))];
        b.insert("--", EditKind::Other);
        assert_eq!(b.text(), "a--b--c");
        assert_eq!(b.carets[1].head, Pos::new(0, 6));
    }

    #[test]
    fn typing_groups_into_one_undo() {
        let mut b = Buffer::new("");
        for ch in ["a", "b", "c"] {
            b.insert(ch, EditKind::Typing);
        }
        b.undo();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn line_operations() {
        let mut b = Buffer::new("1\n2\n3");
        b.set_caret(Pos::new(1, 0));
        b.move_lines(-1);
        assert_eq!(b.text(), "2\n1\n3");
        assert_eq!(b.primary().head.line, 0);
        b.duplicate_lines();
        assert_eq!(b.text(), "2\n2\n1\n3");
        assert_eq!(b.primary().head.line, 1);
        b.delete_lines();
        assert_eq!(b.text(), "2\n1\n3");
    }

    #[test]
    fn unicode_columns() {
        let mut b = Buffer::new("héllo");
        b.set_caret(Pos::new(0, 2));
        b.insert("X", EditKind::Other);
        assert_eq!(b.text(), "héXllo");
        b.delete_with(EditKind::Deleting, |b, p| (b.left_of(p), p));
        assert_eq!(b.text(), "héllo");
    }

    #[test]
    fn line_marks_follow_edits() {
        let s = LineShift { first: 2, last: 2, delta: 3, at_line_start: false };
        assert_eq!((s.apply(1), s.apply(2), s.apply(3)), (Some(1), Some(2), Some(6)));
        let s = LineShift { first: 2, last: 2, delta: 1, at_line_start: true };
        assert_eq!(s.apply(2), Some(3));
        let s = LineShift { first: 2, last: 5, delta: -3, at_line_start: false };
        assert_eq!((s.apply(2), s.apply(4), s.apply(6)), (Some(2), None, Some(3)));
    }

    #[test]
    fn next_occurrence_adds_carets() {
        let mut b = Buffer::new("foo bar foo\nfoo");
        b.set_caret(Pos::new(0, 1));
        b.add_next_occurrence();
        assert!(b.primary().has_selection());
        b.add_next_occurrence();
        b.add_next_occurrence();
        assert_eq!(b.carets.len(), 3);
        b.insert("x", EditKind::Other);
        assert_eq!(b.text(), "x bar x\nx");
    }

    #[test]
    fn tabs_and_screen_columns() {
        assert_eq!(visual_col("\tab", 1, 4), 4);
        assert_eq!(visual_col("a\tb", 2, 4), 4);
        assert_eq!(col_at_visual("\tab", 4, 4), 1);
        assert_eq!(col_at_visual("\tab", 1, 4), 0);
        assert_eq!(col_at_visual("\tab", 3, 4), 1);
    }
}
