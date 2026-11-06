//! Source files, spans and compiler diagnostics.

use std::fmt;

/// Identifies a source file inside a [`SourceMap`].
pub type FileId = u32;

/// A byte range inside one source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: usize, end: usize) -> Self {
        Span { file, start: start as u32, end: end as u32 }
    }
    /// Smallest span covering both `self` and `other` (same file assumed).
    pub fn to(self, other: Span) -> Span {
        Span { file: self.file, start: self.start.min(other.start), end: self.end.max(other.end) }
    }
}

/// A loaded source file.
#[derive(Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        SourceFile { name: name.into(), text, line_starts }
    }

    /// 1-based (line, column) of a byte offset.
    pub fn line_col(&self, offset: u32) -> (usize, usize) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let start = self.line_starts[line] as usize;
        let col = self.text[start..(offset as usize).min(self.text.len())].chars().count();
        (line + 1, col + 1)
    }

    /// Text of a 1-based line without its terminator.
    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line - 1] as usize;
        let end = self.line_starts.get(line).map(|&e| e as usize).unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }
}

/// All source files of one compilation.
#[derive(Debug, Default)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        self.files.push(SourceFile::new(name, text));
        (self.files.len() - 1) as FileId
    }
    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id as usize]
    }
    /// `file:line:col` for a span.
    pub fn location(&self, span: Span) -> String {
        match self.files.get(span.file as usize) {
            Some(f) => {
                let (l, c) = f.line_col(span.start);
                format!("{}:{}:{}", f.name, l, c)
            }
            None => "<unknown>".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A compiler message attached to a source location.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Diagnostic { severity: Severity::Error, message: message.into(), span }
    }
    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Diagnostic { severity: Severity::Warning, message: message.into(), span }
    }

    /// Renders the diagnostic with the offending source line and a caret marker.
    pub fn render(&self, sources: &SourceMap) -> String {
        let kind = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let Some(file) = sources.files.get(self.span.file as usize) else {
            return format!("{kind}: {}", self.message);
        };
        let (line, col) = file.line_col(self.span.start);
        let text = file.line_text(line);
        let (_, end_col) = file.line_col(self.span.end.max(self.span.start + 1));
        let width = if end_col > col { end_col - col } else { 1 };
        let gutter = line.to_string().len();
        format!(
            "{kind}: {msg}\n{pad} --> {name}:{line}:{col}\n{pad} |\n{line} | {text}\n{pad} | {sp}{carets}\n",
            msg = self.message,
            pad = " ".repeat(gutter),
            name = file.name,
            sp = " ".repeat(col - 1),
            carets = "^".repeat(width.min(text.len().saturating_sub(col - 1).max(1))),
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Collection of diagnostics produced by a compiler stage.
#[derive(Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.items.push(Diagnostic::error(span, msg));
    }
    pub fn warning(&mut self, span: Span, msg: impl Into<String>) {
        self.items.push(Diagnostic::warning(span, msg));
    }
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn render(&self, sources: &SourceMap) -> String {
        self.items.iter().map(|d| d.render(sources)).collect::<Vec<_>>().join("\n")
    }
}
