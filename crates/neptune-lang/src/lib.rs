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
pub mod fuse;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod rename;
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

/// Stack for the compiler's recursive passes. Only the part used is ever
/// committed. It has to hold the deepest code the parser accepts in a debug build.
const COMPILER_STACK: usize = 128 * 1024 * 1024;

/// Moves a value to the compiler thread and back. [`compile`] waits for that
/// thread while it runs, so the two never touch the value at the same time.
struct HandOver<T>(T);

// SAFETY: see above. The value is only used by one thread at a time, and the
// join in `compile` orders the hand-over. No `SourceLoader` depends on the
// thread it runs on.
unsafe impl<T> Send for HandOver<T> {}

impl<T> HandOver<T> {
    fn take(self) -> T {
        self.0
    }
}

/// Compiles source text. `name` is used in diagnostics and for resolving includes.
///
/// Runs on its own thread: the passes recurse over the syntax tree, and the
/// main thread's stack is only 1 MB on Windows.
pub fn compile(name: &str, source: &str, env: &Env, loader: &mut dyn SourceLoader) -> Compiled {
    let job = HandOver(move || compile_here(name, source, env, loader));
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("compiler".into())
            .stack_size(COMPILER_STACK)
            .spawn_scoped(scope, move || HandOver(job.take()()));
        match worker {
            Ok(w) => match w.join() {
                Ok(done) => done.take(),
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(e) => {
                let mut diagnostics = Diagnostics::default();
                diagnostics.error(diag::Span::default(), format!("unable to start the compiler: {e}"));
                Compiled { program: None, diagnostics, sources: Some(SourceMap::default()) }
            }
        }
    })
}

fn compile_here(name: &str, source: &str, env: &Env, loader: &mut dyn SourceLoader) -> Compiled {
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
