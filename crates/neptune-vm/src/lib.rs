//! # neptune-vm
//!
//! Resumable bytecode virtual machine and engine-independent standard
//! library for NeptuneBASIC.
//!
//! ```
//! use neptune_vm::{run_source_captured};
//! let out = run_source_captured("x = 1.5\nPrint x * 2\n").unwrap();
//! assert_eq!(out, "3.0\n");
//! ```

pub mod dynlib;
pub mod net;
pub mod plugin;
pub mod stdlib;
pub mod value;
pub mod vm;

pub use stdlib::{base_env, Core, Output};
pub use value::Value;
pub use vm::{Host, NativeCtx, NoHost, RuntimeError, Status, Vm};

/// Compiles and runs a console program, capturing its output.
/// Returns compiler diagnostics or the runtime error as `Err`.
pub fn run_source_captured(src: &str) -> Result<String, String> {
    let env = base_env();
    let c = neptune_lang::compile("main.nb", src, &env, &mut neptune_lang::NoLoader);
    let Some(prog) = c.program else {
        return Err(c.render_diagnostics());
    };
    let mut vm = Vm::new(prog);
    vm.core.output = Output::Buffer(String::new());
    loop {
        match vm.run(&mut NoHost) {
            Ok(Status::Finished) => return Ok(vm.core.take_output()),
            Ok(Status::Yielded) => continue,
            Err(e) => return Err(format!("{}{e}", vm.core.take_output())),
        }
    }
}
