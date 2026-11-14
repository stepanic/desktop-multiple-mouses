//! multi-mouse-agent: receives keyboard/mouse events from the Windows app and injects
//! them into macOS as if they came from a local keyboard and mouse.

#[cfg(target_os = "macos")]
mod agent;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod keymap;

#[cfg(target_os = "macos")]
fn main() {
    if let Err(e) = agent::main() {
        eprintln!("GREŠKA: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("multi-mouse-agent radi samo na macOS-u.");
    std::process::exit(1);
}
