# Plugins

Plugins add new commands and constants to NeptuneBASIC. They are shared
libraries – `.dll` on Windows, `.so` on Linux, `.dylib` on macOS – written in
Rust, C, C++, Zig or anything else that can export a C function.

Plugin commands are real BASIC commands: the compiler checks their
arguments, editors complete them (`neptune lsp`), errors they report can be
caught with `Try`, and `neptune build` copies them next to a packaged game.
That makes them a better way to extend the language than Blitz3D's
`CallDLL` (which still works, on every platform, for old programs).

## Using a plugin

Put the library in a `plugins` folder, either

* next to your program (the folder you run `neptune` from), or
* next to the `neptune` executable, or
* in any folder listed in the `NEPTUNE_PLUGIN_PATH` environment variable.

`neptune plugins` lists what was loaded and the commands each plugin added.
Plugins are native code with the same access to your computer as any
program, so only use plugins you trust.

## Writing a plugin in Rust

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
neptune-plugin = { path = "path/to/NeptuneBASIC/sdk/rust" }
```

```rust
use neptune_plugin::{neptune_plugin, Plugin, Ret};

fn init(p: &mut Plugin) {
    p.command("$Greet$name", |a| Ok(format!("Hello, {}!", a[0].str()).into()));
    p.command("%AddInts%a%b", |a| Ok((a[0].int() + a[1].int()).into()));
    p.command("#Average#a#b", |a| Ok(((a[0].float() + a[1].float()) / 2.0).into()));
    p.command("Log$text", |a| {
        eprintln!("{}", a[0].str());
        Ok(Ret::None)
    });
    p.constant_int("MY_PLUGIN_VERSION", 1);
}

neptune_plugin!(init);
```

Return `Err(message)` to raise a runtime error. A panic is caught and turned
into an error too. `sdk/examples/rust-hello` is a complete example; build it
with `cargo build -p neptune-hello-plugin --release`.

## Writing a plugin in C or C++

Include `sdk/neptune_plugin.h` and export `neptune_plugin_init`:

```c
#include "neptune_plugin.h"

static int32_t times_table(void *user, const NbValue *args, int32_t argc, NbValue *result) {
    result->kind = NB_INT;
    result->i = args[0].i * args[1].i;
    return 0;
}

NEPTUNE_EXPORT int32_t neptune_plugin_init(const NbHost *host) {
    host->add_command(host->host, "%TimesTable%a%b", times_table, NULL);
    return 0;
}
```

`sdk/examples/c/hello_plugin.c` shows strings and errors and has build lines
for each platform.

## Command signatures

Signatures use Blitz3D's notation:

| Signature | Means |
|---|---|
| `"%AddInts%a%b"` | `Function AddInts%(a%, b%)` |
| `"$Greet$name"` | `Function Greet$(name$)` |
| `"#Lerp#a#b#t=0.5"` | `Function Lerp#(a#, b#, t# = 0.5)` |
| `"Beep%times=1"` | a command with no result |

Tags: `%` int, `#` float, `$` string, `?` any value. Arguments arrive
converted to the declared types. A plugin cannot replace a built-in command
or another plugin's command; such registrations are refused with a warning.

## Rules for the C interface

* Strings are UTF-8 with a byte length (`len`); the host's strings are valid
  only during the call. A string you return must stay valid until your
  plugin's next command call (use a static or plugin-owned buffer).
* Return 0 for success. A non-zero return is a runtime error whose message is
  the string you put in `result`.
* `NEPTUNE_PLUGIN_API_VERSION` is 1. Check `host->api_version` and return
  non-zero from `neptune_plugin_init` if you need a newer one.
* Commands are called on the game's main thread, one at a time.
