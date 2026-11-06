//! Example NeptuneBASIC plugin.
//!
//! Build it with `cargo build -p neptune-hello-plugin --release`, then copy
//! `target/release/neptune_hello_plugin.dll` (`libneptune_hello_plugin.so`
//! on Linux, `.dylib` on macOS) into a `plugins` folder. Then:
//!
//! ```basic
//! Print Greet("world")              ; Hello, world!
//! Print AddInts(2, 40)              ; 42
//! Print Average(3, 4)               ; 3.5
//! Print Fib(20)                     ; 6765
//! Print ReverseText("NeptuneBASIC") ; CISABenutpeN
//! Print HELLO_VERSION
//! ```

use neptune_plugin::{neptune_plugin, Arg, Plugin, Ret};

fn init(p: &mut Plugin) {
    p.command("$Greet$name", |a| Ok(format!("Hello, {}!", a[0].str()).into()));
    p.command("%AddInts%a%b", |a| Ok((a[0].int().wrapping_add(a[1].int())).into()));
    p.command("#Average#a#b", |a| Ok(((a[0].float() + a[1].float()) / 2.0).into()));
    p.command("$ReverseText$text", |a| Ok(a[0].str().chars().rev().collect::<String>().into()));
    p.command("%Fib%n", |a: &[Arg]| {
        let n = a[0].int();
        if !(0..=46).contains(&n) {
            return Err(format!("Fib: n must be 0-46, got {n}"));
        }
        let (mut x, mut y) = (0i32, 1i32);
        for _ in 0..n {
            (x, y) = (y, x + y);
        }
        Ok(Ret::Int(x))
    });
    p.constant_int("HELLO_VERSION", 1);
}

neptune_plugin!(init);
