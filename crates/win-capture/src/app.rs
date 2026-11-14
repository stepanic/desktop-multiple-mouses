use crate::config::Config;
use crate::interception::{self as ic, Interception, Stroke};
use mm_proto::{Button, Msg};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Which devices are "external" (routed to the Mac). Read by the C filter predicates.
static EXTERNAL: [AtomicBool; (ic::MAX_DEVICE + 1) as usize] = [const { AtomicBool::new(false) }; (ic::MAX_DEVICE + 1) as usize];

unsafe extern "C" fn pred_external_keyboard(d: i32) -> i32 {
    (ic::is_keyboard(d) && EXTERNAL[d as usize].load(Ordering::Relaxed)) as i32
}
unsafe extern "C" fn pred_external_mouse(d: i32) -> i32 {
    (ic::is_mouse(d) && EXTERNAL[d as usize].load(Ordering::Relaxed)) as i32
}
unsafe extern "C" fn pred_not_external(d: i32) -> i32 {
    (d >= 1 && d <= ic::MAX_DEVICE && !EXTERNAL[d as usize].load(Ordering::Relaxed)) as i32
}
unsafe extern "C" fn pred_keyboard(d: i32) -> i32 {
    ic::is_keyboard(d) as i32
}
unsafe extern "C" fn pred_mouse(d: i32) -> i32 {
    ic::is_mouse(d) as i32
}

pub fn main() -> Result<(), String> {
    println!("multi-mouse {}", env!("CARGO_PKG_VERSION"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("setup") => setup(),
        Some("list") => list(),
        Some("help" | "-h" | "--help") => {
            println!("Upotreba:\n  multi-mouse.exe          pokreni (prvi put pokreće setup)\n  multi-mouse.exe setup    ponovno odaberi vanjsku tipkovnicu/miš i Mac\n  multi-mouse.exe list     ispiši sve tipkovnice i miševe");
            Ok(())
        }
        Some(other) => Err(format!("Nepoznata naredba '{other}'. Probaj: multi-mouse.exe help")),
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

fn list() -> Result<(), String> {
    let ic = Interception::load()?;
    for d in 1..=ic::MAX_DEVICE {
        if let Some(id) = ic.hardware_id(d) {
            let kind = if ic::is_keyboard(d) { "tipkovnica" } else { "miš" };
            println!("  #{d:<2} {kind:<10} {id}");
        }
    }
    Ok(())
}

/// Waits (passing everything through to Windows) until a matching event arrives.
fn capture_device(ic: &Interception, want_keyboard: bool) -> (i32, String) {
    loop {
        let Some(d) = ic.wait(500) else { continue };
        let Some(s) = ic.receive(d) else { continue };
        ic.send(d, &s); // never block the user's input during setup
        let hit = if want_keyboard {
            ic::is_keyboard(d) && unsafe { s.key.state } & ic::KEY_UP == 0
        } else {
            ic::is_mouse(d) && unsafe { s.mouse.state } & ic::MOUSE_LEFT_DOWN != 0
        };
        if hit {
            if let Some(id) = ic.hardware_id(d) {
                return (d, id);
            }
        }
    }
}

fn warn_if_shared(ic: &Interception, device: i32, id: &str) {
    let range = if ic::is_keyboard(device) { 1..=ic::MAX_KEYBOARD } else { ic::MAX_KEYBOARD + 1..=ic::MAX_DEVICE };
    let others: Vec<i32> = range.filter(|&d| d != device && ic.hardware_id(d).as_deref() == Some(id)).collect();
    if !others.is_empty() {
        println!("  UPOZORENJE: još {} uređaj(a) ima isti hardverski ID, i oni će ići na Mac.", others.len());
    }
}

fn setup() -> Result<(), String> {
    let old = Config::load().ok().flatten();
    let (kb_id, mouse_id) = {
        let ic = Interception::load()?;
        ic.set_filter(pred_keyboard, ic::FILTER_ALL);
        ic.set_filter(pred_mouse, ic::FILTER_ALL);

        println!("1) Pritisni bilo koju tipku na VANJSKOJ tipkovnici (onoj za Mac)...");
        let (kd, kb_id) = capture_device(&ic, true);
        println!("   Tipkovnica: #{kd} {kb_id}");
        warn_if_shared(&ic, kd, &kb_id);

        println!("\n2) Klikni LIJEVIM klikom na VANJSKOM mišu...");
        let (md, mouse_id) = capture_device(&ic, false);
        println!("   Miš: #{md} {mouse_id}");
        warn_if_shared(&ic, md, &mouse_id);
        (kb_id, mouse_id)
        // context dropped here: Windows gets all devices back while we read stdin
    };

    println!();
    let default_addr = old.as_ref().map(|c| c.mac_address.clone()).unwrap_or_default();
    let mut mac_address = prompt(&format!(
        "3) Tailscale ime ili IP Maca (npr. moj-macbook ili 100.x.y.z){}: ",
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
        "4) Token koji je ispisao multi-mouse-agent na Macu{}: ",
        if default_token.is_empty() { "" } else { " [Enter = zadrži stari]" }
    ));
    if token.is_empty() {
        token = default_token;
    }
    if token.is_empty() {
        return Err("Token je obavezan.".into());
    }

    let cfg = Config { mac_address, token, keyboard_ids: vec![kb_id], mouse_ids: vec![mouse_id] };
    let path = cfg.save()?;
    println!("\nSpremljeno u {}\n", path.display());
    run(cfg)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dest {
    Mac,
    Windows,
    Swallow,
}

struct Router<'a> {
    ic: &'a Interception,
    tx: mpsc::Sender<Msg>,
    mode: Dest, // Mac or Windows
    key_dest: HashMap<(u16, bool), Dest>,
    button_dest: [Dest; 5],
    ctrl_down: [bool; 2],
    alt_down: [bool; 2],
}

const SC_CTRL: u16 = 0x1D;
const SC_ALT: u16 = 0x38;
const SC_F12: u16 = 0x58;

impl Router<'_> {
    fn to_mac(&self, m: Msg) {
        let _ = self.tx.send(m);
    }

    fn toggle(&mut self) {
        self.mode = if self.mode == Dest::Mac { Dest::Windows } else { Dest::Mac };
        if self.mode == Dest::Windows {
            self.to_mac(Msg::ReleaseAll);
            println!("[način] Vanjska tipkovnica i miš sada upravljaju WINDOWSOM (Ctrl+Alt+F12 za povratak na Mac).");
        } else {
            println!("[način] Vanjska tipkovnica i miš sada upravljaju MACOM.");
        }
    }

    fn key(&mut self, device: i32, stroke: &Stroke) {
        let k = unsafe { stroke.key };
        if k.state & ic::KEY_E1 != 0 {
            // Pause/Break: no Mac equivalent; give it to whoever owns the keyboard now.
            if self.mode == Dest::Windows {
                self.ic.send(device, stroke);
            }
            return;
        }
        let down = k.state & ic::KEY_UP == 0;
        let e0 = k.state & ic::KEY_E0 != 0;
        let side = e0 as usize;
        match k.code {
            SC_CTRL => self.ctrl_down[side] = down,
            SC_ALT => self.alt_down[side] = down,
            _ => {}
        }

        let id = (k.code, e0);
        let dest = if down {
            let is_hotkey = k.code == SC_F12 && !e0 && self.ctrl_down.iter().any(|&b| b) && self.alt_down.iter().any(|&b| b);
            let dest = if is_hotkey {
                if !self.key_dest.contains_key(&id) {
                    self.toggle();
                }
                Dest::Swallow
            } else {
                self.mode
            };
            // Typematic repeats keep the destination of the original press.
            *self.key_dest.entry(id).or_insert(dest)
        } else {
            self.key_dest.remove(&id).unwrap_or(self.mode)
        };

        match dest {
            Dest::Mac => self.to_mac(Msg::Key { scancode: k.code, e0, down }),
            Dest::Windows => self.ic.send(device, stroke),
            Dest::Swallow => {}
        }
    }

    fn mouse(&mut self, device: i32, stroke: &Stroke) {
        let m = unsafe { stroke.mouse };
        let mut win = m;
        win.state = 0;
        win.rolling = 0;
        let mut send_win = false;

        // Movement follows the current mode.
        let has_move = m.flags & ic::MOUSE_FLAG_ABSOLUTE != 0 || m.x != 0 || m.y != 0;
        if self.mode == Dest::Windows {
            send_win |= has_move;
        } else {
            win.x = 0;
            win.y = 0;
            win.flags &= !ic::MOUSE_FLAG_ABSOLUTE; // a button-only stroke must not warp the cursor
            if has_move && m.flags & ic::MOUSE_FLAG_ABSOLUTE == 0 {
                self.to_mac(Msg::Move { dx: m.x, dy: m.y });
            }
        }

        const BUTTONS: [(Button, u16, u16); 5] = [
            (Button::Left, ic::MOUSE_LEFT_DOWN, ic::MOUSE_LEFT_UP),
            (Button::Right, ic::MOUSE_RIGHT_DOWN, ic::MOUSE_RIGHT_UP),
            (Button::Middle, ic::MOUSE_MIDDLE_DOWN, ic::MOUSE_MIDDLE_UP),
            (Button::Back, ic::MOUSE_B4_DOWN, ic::MOUSE_B4_UP),
            (Button::Forward, ic::MOUSE_B5_DOWN, ic::MOUSE_B5_UP),
        ];
        for (button, down_bit, up_bit) in BUTTONS {
            let i = button as usize;
            if m.state & down_bit != 0 {
                self.button_dest[i] = self.mode;
                self.route_button(button, true, down_bit, &mut win, &mut send_win);
            }
            if m.state & up_bit != 0 {
                let saved = self.button_dest[i];
                self.route_button_to(saved, button, false, up_bit, &mut win, &mut send_win);
                self.button_dest[i] = self.mode;
            }
        }

        for (bit, vertical) in [(ic::MOUSE_WHEEL, true), (ic::MOUSE_HWHEEL, false)] {
            if m.state & bit != 0 {
                if self.mode == Dest::Mac {
                    let r = m.rolling as i32;
                    self.to_mac(if vertical { Msg::Wheel { vertical: r, horizontal: 0 } } else { Msg::Wheel { vertical: 0, horizontal: r } });
                } else {
                    win.state |= bit;
                    win.rolling = m.rolling;
                    send_win = true;
                }
            }
        }

        if send_win {
            self.ic.send(device, &Stroke { mouse: win });
        }
    }

    fn route_button(&self, b: Button, down: bool, bit: u16, win: &mut ic::MouseStroke, send_win: &mut bool) {
        self.route_button_to(self.mode, b, down, bit, win, send_win)
    }

    fn route_button_to(&self, dest: Dest, b: Button, down: bool, bit: u16, win: &mut ic::MouseStroke, send_win: &mut bool) {
        match dest {
            Dest::Mac => self.to_mac(Msg::Button { button: b, down }),
            Dest::Windows => {
                win.state |= bit;
                *send_win = true;
            }
            Dest::Swallow => {}
        }
    }
}

/// Re-evaluates which device slots hold external devices; updates filters if anything changed.
fn rescan(ic: &Interception, cfg: &Config) {
    let mut changed = false;
    for d in 1..=ic::MAX_DEVICE {
        let ids = if ic::is_keyboard(d) { &cfg.keyboard_ids } else { &cfg.mouse_ids };
        let ext = ic.hardware_id(d).is_some_and(|id| ids.iter().any(|w| w.eq_ignore_ascii_case(&id)));
        if EXTERNAL[d as usize].swap(ext, Ordering::Relaxed) != ext {
            changed = true;
        }
    }
    if changed {
        ic.set_filter(pred_not_external, ic::FILTER_NONE);
        ic.set_filter(pred_external_keyboard, ic::FILTER_ALL);
        ic.set_filter(pred_external_mouse, ic::FILTER_ALL);
        let kb = (1..=ic::MAX_KEYBOARD).filter(|&d| EXTERNAL[d as usize].load(Ordering::Relaxed)).count();
        let ms = (ic::MAX_KEYBOARD + 1..=ic::MAX_DEVICE).filter(|&d| EXTERNAL[d as usize].load(Ordering::Relaxed)).count();
        println!("[uređaji] Preusmjeravam na Mac: {kb} tipkovnica, {ms} miš(eva).");
        if kb == 0 || ms == 0 {
            println!("[uređaji] Neki vanjski uređaj nije pronađen - je li spojen? (multi-mouse.exe setup za ponovni odabir)");
        }
    }
}

fn run(cfg: Config) -> Result<(), String> {
    let ic = Interception::load()?;

    let (tx, rx) = mpsc::channel();
    {
        let (addr, token) = (cfg.socket_address(), cfg.token.clone());
        std::thread::spawn(move || crate::net::run(addr, token, rx));
    }

    println!("Mac: {}", cfg.socket_address());
    println!("Prečac na vanjskoj tipkovnici: Ctrl+Alt+F12 = prebaci vanjske uređaje Mac <-> Windows.");
    println!("Zatvori ovaj prozor (ili Ctrl+C) da sve vratiš na normalno.\n");

    rescan(&ic, &cfg);
    let mut router = Router {
        ic: &ic,
        tx,
        mode: Dest::Mac,
        key_dest: HashMap::new(),
        button_dest: [Dest::Mac; 5],
        ctrl_down: [false; 2],
        alt_down: [false; 2],
    };

    let mut last_scan = Instant::now();
    loop {
        if last_scan.elapsed() > Duration::from_secs(3) {
            rescan(&ic, &cfg);
            last_scan = Instant::now();
        }
        let Some(d) = ic.wait(1000) else { continue };
        let Some(stroke) = ic.receive(d) else { continue };
        if !EXTERNAL[d as usize].load(Ordering::Relaxed) {
            ic.send(d, &stroke); // filter race after a rescan: hand it back to Windows
        } else if ic::is_keyboard(d) {
            router.key(d, &stroke);
        } else {
            router.mouse(d, &stroke);
        }
    }
}
