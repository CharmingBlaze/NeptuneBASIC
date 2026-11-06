//! # neptune-lang
//!
//! The NeptuneBASIC compiler front end and bytecode back end. Independent of
//! any engine: hosts describe their commands with [`commands::parse_sig`]
//! signatures and pass them in an [`Env`].
//!
//! Pipeline: [`lexer`] → [`parser`] (→ [`ast`]) → [`sema`] (type checking,
//! inference → [`ir`]) → [`codegen`] (→ [`bytecode`]).

pub mod ast;
pub mod bytecode;
pub mod codegen;
pub mod commands;
pub mod diag;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod sema;

pub use bytecode::Program;
pub use diag::{Diagnostic, Diagnostics, SourceMap};
pub use parser::{FsLoader, NoLoader, SourceLoader};
pub use sema::Env;

/// Result of a compilation: the program (when there were no errors) and all diagnostics.
pub struct Compiled {
    pub program: Option<Program>,
    pub diagnostics: Diagnostics,
    /// Present when compilation failed, so diagnostics can be rendered.
    pub sources: Option<SourceMap>,
}

impl Compiled {
    /// Renders all diagnostics (errors and warnings) as text.
    pub fn render_diagnostics(&self) -> String {
        let sources = self.sources.as_ref().or(self.program.as_ref().map(|p| &p.sources));
        match sources {
            Some(s) => self.diagnostics.render(s),
            None => String::new(),
        }
    }
}

/// Compiles source text. `name` is used in diagnostics and for resolving includes.
pub fn compile(name: &str, source: &str, env: &Env, loader: &mut dyn SourceLoader) -> Compiled {
    let mut sources = SourceMap::default();
    let file = sources.add(name, source.replace("\r\n", "\n"));
    let mut diags = Diagnostics::default();
    let ast = parser::parse_program(&mut sources, file, loader, &mut diags);
    if diags.has_errors() {
        return Compiled { program: None, diagnostics: diags, sources: Some(sources) };
    }
    let hir = sema::analyze(&ast, env, &mut diags);
    if diags.has_errors() {
        return Compiled { program: None, diagnostics: diags, sources: Some(sources) };
    }
    let program = codegen::generate(&hir, &env.commands, sources);
    Compiled { program: Some(program), diagnostics: diags, sources: None }
}
