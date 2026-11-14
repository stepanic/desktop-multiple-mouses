//! multi-mouse: sends input from chosen (external) keyboards/mice to a Mac agent,
//! while every other keyboard/mouse keeps controlling Windows normally.

#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg_attr(not(windows), allow(dead_code))]
mod net;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod interception;

#[cfg(windows)]
fn main() {
    if let Err(e) = app::main() {
        eprintln!("\nGREŠKA: {e}");
        eprintln!("\nPritisni Enter za izlaz...");
        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("multi-mouse radi samo na Windowsima.");
    std::process::exit(1);
}
