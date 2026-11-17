use crate::config::{parse_vid_pid, Config};
use crate::{inject, usb};
use mm_proto::Msg;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const HELP: &str = "Upotreba:
  multi-mouse.exe                 pokreni (prvi put pokreće setup)
  multi-mouse.exe setup           ponovno odaberi USB uređaj i Mac
  multi-mouse.exe list            ispiši USB HID uređaje i imaju li WinUSB driver
  multi-mouse.exe monitor [vid:pid]  samo ispisuj događaje s uređaja (test, bez mreže)";

pub fn main() -> Result<(), String> {
    println!("multi-mouse {}", env!("CARGO_PKG_VERSION"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("setup") => setup(),
        Some("list") => {
            list();
            Ok(())
        }
        Some("monitor") => monitor(args.get(1).map(|s| s.as_str())),
        Some("help" | "-h" | "--help") => {
            println!("{HELP}");
            Ok(())
        }
        Some(other) => Err(format!("Nepoznata naredba '{other}'.\n{HELP}")),
        None => match Config::load()? {
            Some(cfg) => run(cfg),
            None => {
                println!("Konfiguracija ne postoji, pokrećem setup.\n");
                setup()
            }
        },
    }
}

fn prompt(question: &str) -> String {
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    s.trim().to_string()
}

fn list() -> Vec<usb::Candidate> {
    let cands = usb::list_candidates();
    if cands.is_empty() {
        println!("Nema USB HID uređaja.");
    }
    for c in &cands {
        let services = usb::interface_services(c.vid, c.pid);
        let ifaces: Vec<String> = c
            .interfaces
            .iter()
            .map(|i| format!("{}:{}", i.number, if usb::has_winusb(&services, i.number) { "WinUSB" } else { "Windows" }))
            .collect();
        println!(
            "  {:04x}:{:04x}  {:<28} sučelja [{}]  {}",
            c.vid,
            c.pid,
            if c.name.is_empty() { "?" } else { &c.name },
            ifaces.join(","),
            if c.openable { "-> spreman za multi-mouse" } else { "-> koristi ga Windows" }
        );
    }
    cands
}

const ZADIG_HELP: &str = "Vanjski uređaj još ima Windows HID driver. Instaliraj WinUSB pomoću Zadiga:
  1. Pokreni zadig.exe (traži administratorska prava)
  2. Options -> List All Devices
  3. Odaberi \"USB Receiver (Interface 0)\" -> desno WinUSB -> Replace Driver
  4. Ponovi za \"USB Receiver (Interface 1)\"
Nakon toga vanjska tipkovnica i miš NEĆE raditi na Windowsu dok multi-mouse ne radi
(to je i cilj). Povratak: README -> \"Vraćanje na normalno\".";

fn setup() -> Result<(), String> {
    let old = Config::load().ok().flatten();
    println!("USB HID uređaji:");
    let ready: Vec<usb::Candidate> = list().into_iter().filter(|c| c.openable).collect();
    let chosen = match ready.len() {
        0 => return Err(ZADIG_HELP.into()),
        1 => ready[0].clone(),
        _ => {
            for (i, c) in ready.iter().enumerate() {
                println!("  {}) {:04x}:{:04x} {}", i + 1, c.vid, c.pid, c.name);
            }
            let n: usize = prompt("Koji uređaj ide na Mac? Broj: ").parse().map_err(|_| "Neispravan broj.")?;
            ready.get(n.wrapping_sub(1)).cloned().ok_or("Neispravan broj.")?
        }
    };
    let usb_device = format!("{:04x}:{:04x}", chosen.vid, chosen.pid);
    println!("Odabran uređaj: {usb_device} {}\n", chosen.name);

    let default_addr = old.as_ref().map(|c| c.mac_address.clone()).unwrap_or_default();
    let mut mac_address = prompt(&format!(
        "Tailscale ime ili IP Maca (npr. moj-macbook ili 100.x.y.z){}: ",
        if default_addr.is_empty() { String::new() } else { format!(" [{default_addr}]") }
    ));
    if mac_address.is_empty() {
        mac_address = default_addr;
    }
    if mac_address.is_empty() {
        return Err("Adresa Maca je obavezna.".into());
    }

    let default_token = old.as_ref().map(|c| c.token.clone()).unwrap_or_default();
    let mut token = prompt(&format!(
        "Token koji je ispisao multi-mouse-agent na Macu{}: ",
        if default_token.is_empty() { "" } else { " [Enter = zadrži stari]" }
    ));
    if token.is_empty() {
        token = default_token;
    }
    if token.is_empty() {
        return Err("Token je obavezan.".into());
    }

    let cfg = Config { mac_address, token, usb_device };
    let path = cfg.save()?;
    println!("\nSpremljeno u {}\n", path.display());
    run(cfg)
}

/// Keeps reading the USB device, reconnecting when it is unplugged/replugged.
fn spawn_usb(vid: u16, pid: u16, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let stop = AtomicBool::new(false);
        let mut last = String::new();
        loop {
            let reason = usb::run_device(vid, pid, &tx, &stop);
            if reason != last {
                println!("[usb] {reason}. Pokušavam ponovno...");
                last = reason;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn monitor(arg: Option<&str>) -> Result<(), String> {
    let (vid, pid) = match arg {
        Some(a) => parse_vid_pid(a).ok_or("Očekujem vid:pid, npr. 046d:c534")?,
        None => Config::load()?.ok_or("Nema konfiguracije; navedi vid:pid")?.vid_pid()?,
    };
    let (tx, rx) = mpsc::channel();
    spawn_usb(vid, pid, tx);
    println!("Ispisujem događaje s {vid:04x}:{pid:04x} (Ctrl+C za kraj)...");
    let mut moved = (0i64, 0i64, 0u32);
    let mut last_print = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Msg::Move { dx, dy }) => {
                moved.0 += dx as i64;
                moved.1 += dy as i64;
                moved.2 += 1;
            }
            Ok(m) => println!("  {m:?}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if moved.2 > 0 && last_print.elapsed() > Duration::from_millis(500) {
            println!("  Move: {} događaja, ukupno dx={} dy={}", moved.2, moved.0, moved.1);
            moved = (0, 0, 0);
            last_print = Instant::now();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dest {
    Mac,
    Windows,
    Swallow,
}

const SC_CTRL: u16 = 0x1D;
const SC_SHIFT_L: u16 = 0x2A;
const SC_SHIFT_R: u16 = 0x36;
const SC_ALT: u16 = 0x38;
const SC_CAPS: u16 = 0x3A;
const SC_F12: u16 = 0x58;
const SC_WIN_L: u16 = 0x5B;
const SC_WIN_R: u16 = 0x5C;

const REPEAT_DELAY: Duration = Duration::from_millis(500);
const REPEAT_RATE: Duration = Duration::from_millis(33);

struct Router {
    net: Sender<Msg>,
    mode: Dest, // Mac or Windows
    key_dest: HashMap<(u16, bool), Dest>,
    button_dest: [Dest; 5],
    ctrl_down: [bool; 2],
    alt_down: [bool; 2],
    /// USB keyboards do not auto-repeat; we do: (scancode, e0, dest, next repeat time).
    repeat: Option<(u16, bool, Dest, Instant)>,
}

fn is_modifier(scancode: u16, e0: bool) -> bool {
    matches!(scancode, SC_CTRL | SC_ALT | SC_SHIFT_L | SC_SHIFT_R | SC_CAPS) || (e0 && matches!(scancode, SC_WIN_L | SC_WIN_R))
}

impl Router {
    fn dispatch(&self, dest: Dest, m: Msg) {
        match dest {
            Dest::Mac => {
                let _ = self.net.send(m);
            }
            Dest::Windows => inject::to_windows(m),
            Dest::Swallow => {}
        }
    }

    fn toggle(&mut self) {
        self.mode = if self.mode == Dest::Mac { Dest::Windows } else { Dest::Mac };
        if self.mode == Dest::Windows {
            let _ = self.net.send(Msg::ReleaseAll);
            println!("[način] Vanjska tipkovnica i miš sada upravljaju WINDOWSOM (Ctrl+Alt+F12 za povratak na Mac).");
        } else {
            println!("[način] Vanjska tipkovnica i miš sada upravljaju MACOM.");
        }
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Key { scancode, e0, down } => {
                let side = e0 as usize;
                match scancode {
                    SC_CTRL => self.ctrl_down[side] = down,
                    SC_ALT => self.alt_down[side] = down,
                    _ => {}
                }
                let id = (scancode, e0);
                let dest = if down {
                    let hotkey = scancode == SC_F12
                        && !e0
                        && self.ctrl_down.iter().any(|&b| b)
                        && self.alt_down.iter().any(|&b| b);
                    let dest = if hotkey { Dest::Swallow } else { self.mode };
                    if hotkey {
                        self.toggle();
                    }
                    self.key_dest.insert(id, dest);
                    if !is_modifier(scancode, e0) && dest != Dest::Swallow {
                        self.repeat = Some((scancode, e0, dest, Instant::now() + REPEAT_DELAY));
                    }
                    dest
                } else {
                    if self.repeat.is_some_and(|r| (r.0, r.1) == id) {
                        self.repeat = None;
                    }
                    self.key_dest.remove(&id).unwrap_or(self.mode)
                };
                self.dispatch(dest, m);
            }
            Msg::Button { button, down } => {
                let i = button as usize;
                let dest = if down { self.mode } else { self.button_dest[i] };
                self.button_dest[i] = self.mode;
                self.dispatch(dest, m);
            }
            Msg::Move { .. } | Msg::Wheel { .. } => self.dispatch(self.mode, m),
            Msg::ReleaseAll | Msg::Ping => {}
        }
    }

    /// Time until the next auto-repeat, if any.
    fn next_timeout(&self) -> Duration {
        self.repeat
            .map(|r| r.3.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(1))
    }

    fn tick(&mut self) {
        if let Some((scancode, e0, dest, at)) = self.repeat {
            if Instant::now() >= at {
                self.dispatch(dest, Msg::Key { scancode, e0, down: true });
                self.repeat = Some((scancode, e0, dest, Instant::now() + REPEAT_RATE));
            }
        }
    }
}

fn run(cfg: Config) -> Result<(), String> {
    let (vid, pid) = cfg.vid_pid()?;

    let (net_tx, net_rx) = mpsc::channel();
    {
        let (addr, token) = (cfg.socket_address(), cfg.token.clone());
        std::thread::spawn(move || crate::net::run(addr, token, net_rx));
    }
    let (in_tx, in_rx) = mpsc::channel();
    spawn_usb(vid, pid, in_tx);

    println!("Uređaj: {}   Mac: {}", cfg.usb_device, cfg.socket_address());
    println!("Prečac na vanjskoj tipkovnici: Ctrl+Alt+F12 = prebaci vanjske uređaje Mac <-> Windows.");
    println!("Dok ovaj prozor nije otvoren, vanjska tipkovnica i miš ne rade nigdje.\n");

    let mut router = Router {
        net: net_tx,
        mode: Dest::Mac,
        key_dest: HashMap::new(),
        button_dest: [Dest::Mac; 5],
        ctrl_down: [false; 2],
        alt_down: [false; 2],
        repeat: None,
    };
    loop {
        match in_rx.recv_timeout(router.next_timeout()) {
            Ok(m) => router.handle(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("USB nit je završila".into()),
        }
        router.tick();
    }
}
