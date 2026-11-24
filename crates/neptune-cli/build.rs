//! On Windows laptops with two graphics cards, the drivers give a program the
//! slow power-saving one unless its executable exports two well-known
//! symbols (`NvOptimusEnablement` for NVIDIA, `AmdPowerXpressRequestHighPerformance`
//! for AMD). They are defined in `main.rs`; an executable only exports what
//! the linker is told to, which is done here.
//!
//! To measure the power-saving card on such a laptop (it stands in well for
//! an older PC), build without the exports:
//! `NEPTUNE_GPU_EXPORTS=0 cargo build --release`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NEPTUNE_GPU_EXPORTS");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    let wanted = std::env::var("NEPTUNE_GPU_EXPORTS").map_or(true, |v| v != "0");
    if windows && msvc && wanted {
        println!("cargo:rustc-link-arg-bins=/EXPORT:NvOptimusEnablement,DATA");
        println!("cargo:rustc-link-arg-bins=/EXPORT:AmdPowerXpressRequestHighPerformance,DATA");
    }
}
