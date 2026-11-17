//! multi-mouse: reads an external USB keyboard/mouse (WinUSB driver) directly and sends
//! its input to a Mac agent, while the laptop's own keyboard/touchpad keep controlling Windows.

#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg_attr(not(windows), allow(dead_code))]
mod hid;
#[cfg_attr(not(windows), allow(dead_code))]
mod net;
#[cfg_attr(not(windows), allow(dead_code))]
mod translate;
#[cfg_attr(not(windows), allow(dead_code))]
mod usbkeys;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod inject;
#[cfg(windows)]
mod usb;

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
