//! `neptune bench`: runs the benchmark programs and writes a report.
//!
//! ```text
//! neptune bench [folder] [-o report.md] [--only name]
//! ```
//!
//! Every `*.nb` file in the folder (default `benchmarks`) except the shared
//! `bench.nb` is run in its own process with `run --uncapped` (no vsync, no
//! 60 Hz limit). Programs report results by appending `group|name|value|unit`
//! lines to the file named by `NEPTUNE_BENCH_OUT` (see `benchmarks/bench.nb`).
//! The demos `examples/island.nb` and `examples/beach.nb` are measured too:
//! with `NEPTUNE_BENCH_FRAMES` set they run that many frames and report
//! their frame rate. The report (default `docs/BENCHMARKS.md`) lists the
//! machine and every result, followed by `notes.md` from the folder.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

struct Result_ {
    group: String,
    name: String,
    value: f64,
    unit: String,
}

/// What each group measures, for the report.
const GROUPS: &[(&str, &str)] = &[
    ("language", "The bytecode VM on its own, no window: wall-clock time of each program (lower is better)."),
    ("entities", "Copies of a cube: still, all turned every frame by the program, and merged with `EntityBatch` (frames per second, higher is better)."),
    ("models", "Loading glTF files and textures (milliseconds) and animated glTF characters, each playing its animation (frames per second)."),
    ("physics", "Rigid bodies falling and stacking (Rapier physics), and spheres sliding over a level with Blitz3D-style collisions (frames per second)."),
    ("queries", "Picking, navigation meshes, pathfinding, agents and terrain erosion (milliseconds per call, or frames per second)."),
    ("rendering", "Terrain with and without level of detail, particles, sprites and 2D drawing over the 3D view (frames per second)."),
    ("demos", "The example demos as they are, run for a fixed number of frames (frames per second)."),
];

const DEMOS: &[&str] = &["examples/island.nb", "examples/beach.nb"];

pub fn run(args: &[String]) -> ExitCode {
    let mut folder = PathBuf::from("benchmarks");
    let mut out = PathBuf::from("docs/BENCHMARKS.md");
    let mut only: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                out = args.get(i + 1).map(PathBuf::from).unwrap_or(out);
                i += 1;
            }
            "--only" => {
                only = args.get(i + 1).cloned();
                i += 1;
            }
            f => folder = PathBuf::from(f),
        }
        i += 1;
    }
    let mut programs: Vec<(PathBuf, bool)> = match std::fs::read_dir(&folder) {
        Ok(dir) => dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "nb") && p.file_name().is_some_and(|n| n != "bench.nb"))
            .map(|p| (p, false))
            .collect(),
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", folder.display());
            return ExitCode::from(1);
        }
    };
    programs.sort();
    programs.extend(DEMOS.iter().map(|d| (PathBuf::from(d), true)).filter(|(p, _)| p.is_file()));
    if let Some(o) = &only {
        programs.retain(|(p, _)| p.file_stem().is_some_and(|s| s.to_string_lossy() == *o));
    }
    if cfg!(debug_assertions) {
        eprintln!("warning: this is a debug build of neptune; build with `cargo build --release` for real numbers");
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot find the neptune executable: {e}");
            return ExitCode::from(1);
        }
    };
    let mut results: Vec<Result_> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let started = Instant::now();
    for (program, demo) in &programs {
        println!("== {}", program.display());
        let file = std::env::temp_dir().join(format!("neptune-bench-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut cmd = Command::new(&exe);
        cmd.args(["run", "--uncapped"]).arg(program).env("NEPTUNE_BENCH_OUT", &file);
        if *demo {
            cmd.env("NEPTUNE_BENCH_FRAMES", "600");
        }
        match cmd.status() {
            Ok(s) if s.success() => {}
            Ok(s) => failures.push(format!("{} exited with {s}", program.display())),
            Err(e) => failures.push(format!("{}: {e}", program.display())),
        }
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        for line in text.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if let [group, name, value, unit] = parts.as_slice() {
                if let Ok(v) = value.trim().parse::<f64>() {
                    results.push(Result_ { group: group.to_string(), name: name.to_string(), value: v, unit: unit.to_string() });
                }
            }
        }
    }
    let mut report = report(&results, &failures, started.elapsed().as_secs());
    // hand-written notes (what the numbers show, what was improved) go after the results
    if let Ok(notes) = std::fs::read_to_string(folder.join("notes.md")) {
        report += "\n";
        report += &notes;
    }
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(&out, report) {
        Ok(()) => println!("wrote {}", out.display()),
        Err(e) => {
            eprintln!("error: cannot write '{}': {e}", out.display());
            return ExitCode::from(1);
        }
    }
    for f in &failures {
        eprintln!("warning: {f}");
    }
    if results.is_empty() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn report(results: &[Result_], failures: &[String], seconds: u64) -> String {
    let m = machine();
    let mut s = String::new();
    s += "# Benchmarks\n\n";
    s += "Measured with `neptune bench` (programs in [`benchmarks/`](../benchmarks)), \
          run with `neptune run --uncapped`: no vsync and no 60 updates-a-second \
          limit, so frame rates show what the machine can do. Re-run it on your \
          own machine to compare: build with `cargo build --release`, then run \
          `target/release/neptune bench` from the repository root.\n\n";
    s += "## Machine\n\n| | |\n|---|---|\n";
    for (k, v) in &m {
        s += &format!("| {k} | {v} |\n");
    }
    s += &format!("| NeptuneBASIC | {} ({} build) |\n", env!("CARGO_PKG_VERSION"), if cfg!(debug_assertions) { "debug" } else { "release" });
    s += &format!("| Run time | {} s |\n\n", seconds);
    let mut groups: BTreeMap<usize, (&str, Vec<&Result_>)> = BTreeMap::new();
    for r in results {
        let order = GROUPS.iter().position(|(g, _)| *g == r.group).unwrap_or(GROUPS.len());
        groups.entry(order).or_insert((r.group.as_str(), Vec::new())).1.push(r);
    }
    for (order, (group, rs)) in groups {
        let title = group[..1].to_uppercase() + &group[1..];
        s += &format!("## {title}\n\n");
        if let Some((_, about)) = GROUPS.get(order) {
            s += &format!("{about}\n\n");
        }
        s += "| Benchmark | Result |\n|---|---:|\n";
        for r in rs {
            let v = if r.value >= 100.0 { format!("{:.0}", r.value) } else if r.value >= 10.0 { format!("{:.1}", r.value) } else { format!("{:.3}", r.value) };
            s += &format!("| {} | {v} {} |\n", r.name, r.unit);
        }
        s += "\n";
    }
    if !failures.is_empty() {
        s += "## Problems\n\n";
        for f in failures {
            s += &format!("* {f}\n");
        }
        s += "\n";
    }
    s
}

/// The first non-empty line a command prints, trimmed.
fn first_line(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

/// Every non-empty line a command prints.
fn lines(program: &str, args: &[&str]) -> Vec<String> {
    Command::new(program)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// What the benchmarks ran on: OS, CPU, cores, memory, GPU (best effort on
/// Windows, Linux and macOS).
fn machine() -> Vec<(&'static str, String)> {
    let cores = std::thread::available_parallelism().map(|n| n.get().to_string()).unwrap_or_default();
    let unknown = || "unknown".to_string();
    let (os, cpu, ram, gpu): (Option<String>, Option<String>, Option<f64>, Vec<String>);
    #[cfg(windows)]
    {
        let ps = |q: &str| first_line("powershell", &["-NoProfile", "-Command", q]);
        os = ps("(Get-CimInstance Win32_OperatingSystem).Caption + ' ' + (Get-CimInstance Win32_OperatingSystem).Version");
        cpu = ps("(Get-CimInstance Win32_Processor | Select-Object -First 1).Name");
        ram = ps("(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory").and_then(|v| v.parse::<f64>().ok());
        gpu = lines("powershell", &["-NoProfile", "-Command", "(Get-CimInstance Win32_VideoController).Name"]);
    }
    #[cfg(target_os = "macos")]
    {
        os = first_line("sw_vers", &["-productVersion"]).map(|v| format!("macOS {v}"));
        cpu = first_line("sysctl", &["-n", "machdep.cpu.brand_string"]);
        ram = first_line("sysctl", &["-n", "hw.memsize"]).and_then(|v| v.parse::<f64>().ok());
        gpu = lines("system_profiler", &["SPDisplaysDataType"])
            .into_iter()
            .filter_map(|l| l.strip_prefix("Chipset Model:").map(|g| g.trim().to_string()))
            .collect();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        os = read("/etc/os-release")
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()));
        cpu = read("/proc/cpuinfo").lines().find_map(|l| l.strip_prefix("model name").map(|v| v.trim_start_matches([' ', '\t', ':']).to_string()));
        ram = read("/proc/meminfo")
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:").and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok()))
            .map(|kb| kb * 1024.0);
        gpu = lines("lspci", &[])
            .into_iter()
            .filter(|l| l.contains("VGA") || l.contains("3D controller") || l.contains("Display controller"))
            .map(|l| l.split_once(": ").map_or(l.clone(), |(_, g)| g.to_string()))
            .collect();
    }
    vec![
        ("OS", os.unwrap_or_else(|| std::env::consts::OS.to_string())),
        ("CPU", cpu.unwrap_or_else(unknown)),
        ("Cores (threads)", cores),
        ("Memory", ram.map(|b| format!("{:.0} GB", b / 1024.0 / 1024.0 / 1024.0)).unwrap_or_else(unknown)),
        ("Graphics", if gpu.is_empty() { unknown() } else { gpu.join(", ") }),
    ]
}
