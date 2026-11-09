//! An open file: its text plus everything attached to its lines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::buffer::{Buffer, LineShift, Pos};
use crate::lang::{self, Decl};

#[derive(Clone, Debug)]
pub struct Diag {
    pub start: Pos,
    pub end: Pos,
    pub error: bool,
    pub message: String,
}

pub struct Doc {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub untitled: usize,
    pub buf: Buffer,
    pub crlf: bool,
    /// Old Blitz sources are often not UTF-8; they are saved back as they came.
    pub latin1: bool,
    pub breakpoints: BTreeSet<usize>,
    pub bookmarks: BTreeSet<usize>,
    pub folded: BTreeSet<usize>,
    pub folds: Vec<(usize, usize)>,
    pub decls: Vec<Decl>,
    pub longest: usize,
    pub diags: Vec<Diag>,
    pub mtime: Option<SystemTime>,
    pub lsp_sent: u64,
    pub lsp_open: bool,
    analysed: u64,
    rows: Vec<usize>,
    rows_valid: bool,
    pub last_edit: Instant,
    seen_version: u64,
}

pub fn decode(bytes: &[u8]) -> (String, bool) {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), false),
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), true),
    }
}

impl Doc {
    pub fn new(id: u64, text: &str) -> Self {
        let mut d = Doc {
            id,
            path: None,
            untitled: 0,
            buf: Buffer::new(text),
            crlf: cfg!(windows),
            latin1: false,
            breakpoints: BTreeSet::new(),
            bookmarks: BTreeSet::new(),
            folded: BTreeSet::new(),
            folds: Vec::new(),
            decls: Vec::new(),
            longest: 0,
            diags: Vec::new(),
            mtime: None,
            lsp_sent: 0,
            lsp_open: false,
            analysed: 0,
            rows: Vec::new(),
            rows_valid: false,
            last_edit: Instant::now(),
            seen_version: 1,
        };
        d.analyse();
        d
    }

    pub fn open(id: u64, path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let (text, latin1) = decode(&bytes);
        let mut d = Doc::new(id, &text);
        d.crlf = text.contains("\r\n") || (cfg!(windows) && !text.contains('\n'));
        d.latin1 = latin1;
        d.path = Some(path.to_path_buf());
        d.mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        Ok(d)
    }

    /// The text as it goes to disk.
    pub fn encoded(&self) -> Vec<u8> {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let text = self.buf.lines().join(eol);
        if self.latin1 {
            text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
        } else {
            text.into_bytes()
        }
    }

    pub fn save(&mut self, trim: bool) -> Result<(), String> {
        let Some(path) = self.path.clone() else { return Err("No file name".into()) };
        if trim {
            self.trim_trailing();
        }
        std::fs::write(&path, self.encoded()).map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
        self.buf.mark_saved();
        self.mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Ok(())
    }

    pub fn trim_trailing(&mut self) {
        let last = self.buf.line_count().saturating_sub(1);
        let carets: Vec<usize> = self.buf.carets.iter().map(|c| c.head.line).collect();
        self.buf.rewrite_lines(&[(0, last)], |l, s| if carets.contains(&l) { s.to_string() } else { s.trim_end().to_string() });
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else { return Ok(()) };
        let bytes = std::fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        let (text, latin1) = decode(&bytes);
        let old = self.buf.primary().head;
        self.latin1 = latin1;
        self.buf = Buffer::new(&text);
        self.buf.set_caret(old);
        self.seen_version = 0;
        self.mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.rows_valid = false;
        self.lsp_sent = 0;
        Ok(())
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
            None => format!("untitled-{}.nb", self.untitled),
        }
    }

    /// Keeps line marks in step with the text; call once per frame.
    pub fn sync(&mut self) {
        if self.buf.version != self.seen_version {
            self.seen_version = self.buf.version;
            self.last_edit = Instant::now();
            let shifts: Vec<LineShift> = std::mem::take(&mut self.buf.shifts);
            for s in &shifts {
                let shift_set = |set: &mut BTreeSet<usize>| *set = set.iter().filter_map(|&l| s.apply(l)).collect();
                shift_set(&mut self.breakpoints);
                shift_set(&mut self.bookmarks);
                shift_set(&mut self.folded);
                self.diags.retain_mut(|d| match s.apply(d.start.line) {
                    Some(l) => {
                        let span = d.end.line - d.start.line;
                        d.start.line = l;
                        d.end.line = l + span;
                        true
                    }
                    None => false,
                });
            }
            self.rows_valid = false;
        }
        // very long files are analysed when typing pauses
        let settled = self.buf.line_count() < 8000 || self.last_edit.elapsed().as_millis() > 150;
        if self.analysed != self.buf.version && settled {
            self.analyse();
        }
    }

    fn analyse(&mut self) {
        self.analysed = self.buf.version;
        let lines = self.buf.lines();
        self.folds = lang::fold_regions(lines);
        self.decls = lang::declarations(lines);
        self.longest = lines.iter().map(|l| l.len() + l.bytes().filter(|b| *b == 9).count() * 3).max().unwrap_or(0);
        let folds = &self.folds;
        self.folded.retain(|l| folds.binary_search_by_key(l, |f| f.0).is_ok());
        self.rows_valid = false;
    }

    pub fn fold_at(&self, line: usize) -> Option<(usize, usize)> {
        self.folds.binary_search_by_key(&line, |f| f.0).ok().map(|i| self.folds[i])
    }

    /// The innermost block around a line.
    pub fn enclosing_fold(&self, line: usize) -> Option<(usize, usize)> {
        self.folds.iter().rfind(|f| f.0 <= line && line <= f.1).copied()
    }

    pub fn toggle_fold(&mut self, line: usize) {
        if !self.folded.remove(&line) && self.fold_at(line).is_some() {
            self.folded.insert(line);
        }
        self.rows_valid = false;
    }

    pub fn fold_all(&mut self) {
        // top-level blocks only: nested ones are hidden with them
        let mut end = None;
        for &(a, b) in &self.folds {
            if end.is_none_or(|e| a > e) {
                self.folded.insert(a);
                end = Some(b);
            }
        }
        self.rows_valid = false;
    }

    pub fn unfold_all(&mut self) {
        self.folded.clear();
        self.rows_valid = false;
    }

    /// Opens folds hiding a line, so carets are never out of sight.
    pub fn unfold_around(&mut self, line: usize) {
        let hiding: Vec<usize> = self.folded.iter().copied().filter(|&h| self.fold_at(h).is_some_and(|(a, b)| a < line && line <= b)).collect();
        if !hiding.is_empty() {
            for h in hiding {
                self.folded.remove(&h);
            }
            self.rows_valid = false;
        }
    }

    pub fn prepare_rows(&mut self) {
        if self.rows_valid {
            return;
        }
        self.rows_valid = true;
        self.rows.clear();
        if self.folded.is_empty() {
            return;
        }
        let mut l = 0;
        while l < self.buf.line_count() {
            self.rows.push(l);
            l = match self.folded.contains(&l).then(|| self.fold_at(l)).flatten() {
                Some((_, end)) => end + 1,
                None => l + 1,
            };
        }
    }

    pub fn row_count(&self) -> usize {
        if self.rows.is_empty() {
            self.buf.line_count()
        } else {
            self.rows.len()
        }
    }

    pub fn line_of_row(&self, row: usize) -> usize {
        if self.rows.is_empty() {
            row.min(self.buf.line_count().saturating_sub(1))
        } else {
            self.rows[row.min(self.rows.len() - 1)]
        }
    }

    pub fn row_of_line(&self, line: usize) -> usize {
        if self.rows.is_empty() {
            line
        } else {
            match self.rows.binary_search(&line) {
                Ok(r) => r,
                Err(r) => r.saturating_sub(1),
            }
        }
    }

    pub fn errors(&self) -> usize {
        self.diags.iter().filter(|d| d.error).count()
    }

    pub fn warnings(&self) -> usize {
        self.diags.len() - self.errors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::EditKind;

    #[test]
    fn breakpoints_follow_inserted_lines() {
        let mut d = Doc::new(1, "a\nb\nc");
        d.breakpoints.insert(2);
        d.buf.set_caret(Pos::new(0, 1));
        d.buf.insert("\nnew", EditKind::Other);
        d.sync();
        assert!(d.breakpoints.contains(&3));
    }

    #[test]
    fn folded_rows_skip_hidden_lines() {
        let mut d = Doc::new(1, "If a Then\nb\nEndIf\nc");
        d.sync();
        d.toggle_fold(0);
        d.prepare_rows();
        assert_eq!(d.row_count(), 2);
        assert_eq!(d.line_of_row(1), 3);
        assert_eq!(d.row_of_line(3), 1);
        d.unfold_around(1);
        d.prepare_rows();
        assert_eq!(d.row_count(), 4);
    }

    #[test]
    fn decodes_old_files() {
        assert_eq!(decode(&[0x41, 0xE9]), ("A\u{e9}".to_string(), true));
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, 0x41]), ("A".to_string(), false));
    }
}
