//! Runs a console program on the VM alone, without building the engine:
//!
//! ```text
//! cargo run --release -p neptune-vm --example langbench -- benchmarks/language.nb
//! ```
//!
//! Used to measure the interpreter while working on it.

use neptune_lang::{FsLoader, SourceLoader};
use neptune_vm::{base_env, NoHost, Status, Vm};

/// `benchmarks/bench.nb` also holds helpers that need the 3D engine; here it
/// is replaced by a `Report` that only prints.
const BENCH_STUB: &str = "Function Report(group$, name$, value#, unit$)\n    Print group + \" / \" + name + \": \" + FormatFloat(value, 2) + \" \" + unit\nEnd Function\n";

struct Loader;

impl SourceLoader for Loader {
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String> {
        if path.eq_ignore_ascii_case("bench.nb") {
            return Ok(("bench.nb".into(), BENCH_STUB.into()));
        }
        FsLoader.load(path, from)
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "benchmarks/language.nb".into());
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };
    let env = base_env();
    let c = neptune_lang::compile(&path, &src, &env, &mut Loader);
    let Some(prog) = c.program else {
        eprintln!("{}", c.render_diagnostics());
        std::process::exit(1);
    };
    let mut vm = Vm::new(prog);
    loop {
        match vm.run(&mut NoHost) {
            Ok(Status::Finished) => break,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
}
