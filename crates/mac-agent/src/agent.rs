use crate::ffi::*;
use crate::keymap::{self, *};
use mm_proto::{Button, Msg};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Options {
    port: u16,
    speed: f64,
    scroll_lines: i32,
    invert_scroll: bool,
    ctrl_is_cmd: bool,
    swap_iso: bool,
    allow_any: bool,
    new_token: bool,
}

const HELP: &str = "multi-mouse-agent [opcije]
  --port N          TCP port (zadano 47800)
  --speed X         brzina miša, npr. 1.5 (zadano 1.0)
  --scroll N        redaka po 'kliku' kotačića (zadano 3)
  --invert-scroll   obrni smjer scrolla
  --ctrl-is-cmd     Ctrl na PC tipkovnici radi kao Cmd (i obrnuto: Win tipka -> Ctrl)
  --swap-iso        zamijeni tipku lijevo od 1 i tipku lijevo od Z (ako su obrnute)
  --allow-any       prihvati veze i izvan Tailscale mreže (nije preporučeno)
  --new-token       generiraj novi token";

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        port: mm_proto::DEFAULT_PORT,
        speed: 1.0,
        scroll_lines: 3,
        invert_scroll: false,
        ctrl_is_cmd: false,
        swap_iso: false,
        allow_any: false,
        new_token: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} traži vrijednost"));
        match a.as_str() {
            "--port" => o.port = val("--port")?.parse().map_err(|_| "neispravan --port")?,
            "--speed" => o.speed = val("--speed")?.parse().map_err(|_| "neispravan --speed")?,
            "--scroll" => o.scroll_lines = val("--scroll")?.parse().map_err(|_| "neispravan --scroll")?,
            "--invert-scroll" => o.invert_scroll = true,
            "--ctrl-is-cmd" => o.ctrl_is_cmd = true,
            "--swap-iso" => o.swap_iso = true,
            "--allow-any" => o.allow_any = true,
            "--new-token" => o.new_token = true,
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => return Err(format!("nepoznata opcija {other}\n{HELP}")),
        }
    }
    Ok(o)
}

fn token_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| ".".into());
    home.join(".config/multi-mouse/token")
}

fn load_or_create_token(regenerate: bool) -> Result<String, String> {
    let path = token_path();
    if !regenerate {
        if let Ok(t) = std::fs::read_to_string(&path) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Ok(t);
            }
        }
    }
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("ne mogu generirati token: {e}"))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, &token).map_err(|e| format!("{}: {e}", path.display()))?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Ok(token)
}

/// Tailscale addresses: 100.64.0.0/10 and fd7a:115c:a1e0::/48 (plus loopback for testing).
fn is_tailscale_or_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || (o[0] == 100 && (o[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_tailscale_or_local(IpAddr::V4(v4));
            }
            let s = v6.segments();
            v6.is_loopback() || (s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0)
        }
    }
}

pub fn main() -> Result<(), String> {
    let opts = parse_args()?;
    let token = load_or_create_token(opts.new_token)?;
    println!("multi-mouse-agent {}", env!("CARGO_PKG_VERSION"));
    println!("Token (upiši ga u Windows aplikaciju): {token}");
    println!("  (spremljen u {})", token_path().display());

    if request_accessibility() {
        println!("Accessibility dozvola: OK");
    } else {
        println!(
            "\nACCESSIBILITY DOZVOLA NIJE DANA - bez nje macOS ignorira miš i tipkovnicu.\n\
             System Settings -> Privacy & Security -> Accessibility -> uključi aplikaciju iz koje si pokrenuo agenta\n\
             (npr. Terminal), pa ponovno pokreni agenta.\n"
        );
    }

    let listener = TcpListener::bind(("::", opts.port))
        .or_else(|_| TcpListener::bind(("0.0.0.0", opts.port)))
        .map_err(|e| format!("ne mogu slušati na portu {}: {e}", opts.port))?;
    println!("Čekam Windows aplikaciju na portu {}...", opts.port);

    let injector = Arc::new(Mutex::new(Injector::new(&opts)));
    let opts = Arc::new(opts);
    let token = Arc::new(token);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let peer = match stream.peer_addr() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !opts.allow_any && !is_tailscale_or_local(peer.ip()) {
            println!("Odbijena veza s {peer} (nije Tailscale adresa; --allow-any to dopušta).");
            continue;
        }
        let (injector, token) = (injector.clone(), token.clone());
        std::thread::spawn(move || {
            match handle_client(stream, &token, &injector) {
                Ok(()) => println!("[{peer}] Odspojen."),
                Err(e) => println!("[{peer}] Odspojen: {e}"),
            }
            injector.lock().unwrap().release_all();
        });
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, token: &str, injector: &Mutex<Injector>) -> Result<(), String> {
    let peer = stream.peer_addr().map(|p| p.to_string()).unwrap_or_default();
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(Duration::from_secs(mm_proto::SERVER_TIMEOUT_SECS)))
        .map_err(|e| e.to_string())?;

    let (version, got) = mm_proto::read_hello(&mut stream).map_err(|e| format!("loš pozdrav: {e}"))?;
    let status = if version != mm_proto::VERSION {
        mm_proto::HELLO_BAD_VERSION
    } else if !mm_proto::constant_time_eq(&got, token.as_bytes()) {
        mm_proto::HELLO_BAD_TOKEN
    } else {
        mm_proto::HELLO_OK
    };
    stream.write_all(&[status]).map_err(|e| e.to_string())?;
    match status {
        mm_proto::HELLO_OK => {}
        mm_proto::HELLO_BAD_TOKEN => return Err("pogrešan token".into()),
        _ => return Err(format!("nepodržana verzija protokola {version}")),
    }
    println!("[{peer}] Spojen - vanjski miš i tipkovnica sada upravljaju ovim Macom.");

    let mut reader = std::io::BufReader::new(stream);
    loop {
        let msg = mm_proto::Msg::read_from(&mut reader).map_err(|e| e.to_string())?;
        injector.lock().unwrap().handle(msg);
    }
}

struct Source(CGEventSourceRef);
unsafe impl Send for Source {}

struct Injector {
    src: Source,
    speed: f64,
    scroll_lines: i32,
    invert_scroll: bool,
    ctrl_is_cmd: bool,
    swap_iso: bool,
    frac: (f64, f64),
    wheel_acc: (i32, i32),
    keys_down: HashSet<u16>,
    caps_lock: bool,
    buttons_down: [bool; 5],
    last_click: [(Option<Instant>, CGPoint, i64); 5],
    displays: Vec<CGRect>,
    displays_at: Option<Instant>,
}

/// Posts and releases a CGEvent; silently ignores null (creation failed).
unsafe fn post(ev: CGEventRef) {
    if !ev.is_null() {
        CGEventPost(kCGHIDEventTap, ev);
        CFRelease(ev as CFTypeRef);
    }
}

impl Injector {
    fn new(o: &Options) -> Injector {
        Injector {
            src: Source(unsafe { CGEventSourceCreate(kCGEventSourceStateHIDSystemState) }),
            speed: o.speed,
            scroll_lines: o.scroll_lines,
            invert_scroll: o.invert_scroll,
            ctrl_is_cmd: o.ctrl_is_cmd,
            swap_iso: o.swap_iso,
            frac: (0.0, 0.0),
            wheel_acc: (0, 0),
            keys_down: HashSet::new(),
            caps_lock: false,
            buttons_down: [false; 5],
            last_click: [(None, CGPoint::default(), 0); 5],
            displays: Vec::new(),
            displays_at: None,
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Move { dx, dy } => self.mouse_move(dx, dy),
            Msg::Button { button, down } => self.button(button, down),
            Msg::Wheel { vertical, horizontal } => self.wheel(vertical, horizontal),
            Msg::Key { scancode, e0, down } => {
                if let Some(kc) = keymap::scancode_to_mac(scancode, e0) {
                    self.key(self.remap(kc), down);
                }
            }
            Msg::ReleaseAll => self.release_all(),
            Msg::Ping => {}
        }
    }

    fn remap(&self, kc: u16) -> u16 {
        let kc = if self.ctrl_is_cmd {
            match kc {
                KC_LCTRL => KC_LCMD,
                KC_LCMD => KC_LCTRL,
                KC_RCTRL => KC_RCMD,
                KC_RCMD => KC_RCTRL,
                k => k,
            }
        } else {
            kc
        };
        if self.swap_iso {
            match kc {
                KC_GRAVE => KC_ISO_SECTION,
                KC_ISO_SECTION => KC_GRAVE,
                k => k,
            }
        } else {
            kc
        }
    }

    fn flags(&self) -> u64 {
        let mut f = self.keys_down.iter().filter_map(|&k| modifier_flags(k)).fold(0, |a, b| a | b);
        if self.caps_lock {
            f |= FLAG_ALPHA_SHIFT;
        }
        f
    }

    fn cursor(&self) -> CGPoint {
        unsafe {
            let ev = CGEventCreate(self.src.0);
            if ev.is_null() {
                return CGPoint::default();
            }
            let p = CGEventGetLocation(ev);
            CFRelease(ev as CFTypeRef);
            p
        }
    }

    fn displays(&mut self) -> &[CGRect] {
        let stale = self.displays_at.map_or(true, |t| t.elapsed() > Duration::from_secs(2));
        if stale || self.displays.is_empty() {
            let mut ids = [0u32; 16];
            let mut n = 0u32;
            unsafe {
                if CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut n) == 0 {
                    self.displays = ids[..n as usize].iter().map(|&d| CGDisplayBounds(d)).collect();
                }
            }
            self.displays_at = Some(Instant::now());
        }
        &self.displays
    }

    fn mouse_move(&mut self, dx: i32, dy: i32) {
        let fx = dx as f64 * self.speed + self.frac.0;
        let fy = dy as f64 * self.speed + self.frac.1;
        let (mx, my) = (fx.trunc(), fy.trunc());
        self.frac = (fx - mx, fy - my);
        if mx == 0.0 && my == 0.0 {
            return;
        }
        let cur = self.cursor();
        let mut next = CGPoint { x: cur.x + mx, y: cur.y + my };
        let displays = self.displays().to_vec();
        if !displays.iter().any(|r| r.contains(next)) {
            // Stay on the display we were on instead of jumping into a gap between screens.
            if let Some(r) = displays.iter().find(|r| r.contains(cur)).or(displays.first()) {
                next = r.clamp(next);
            }
        }

        let (ty, button) = if self.buttons_down[0] {
            (kCGEventLeftMouseDragged, 0)
        } else if self.buttons_down[1] {
            (kCGEventRightMouseDragged, 1)
        } else if let Some(i) = (2..5).find(|&i| self.buttons_down[i]) {
            (kCGEventOtherMouseDragged, i as u32)
        } else {
            (kCGEventMouseMoved, 0)
        };
        unsafe {
            let ev = CGEventCreateMouseEvent(self.src.0, ty, next, button);
            if !ev.is_null() {
                CGEventSetIntegerValueField(ev, kCGMouseEventDeltaX, mx as i64);
                CGEventSetIntegerValueField(ev, kCGMouseEventDeltaY, my as i64);
                CGEventSetFlags(ev, self.flags());
            }
            post(ev);
        }
    }

    fn button(&mut self, b: Button, down: bool) {
        let i = b as usize;
        if self.buttons_down[i] == down {
            return; // ignore duplicates (e.g. up after ReleaseAll)
        }
        self.buttons_down[i] = down;
        let pos = self.cursor();

        if down {
            let (last_t, last_p, count) = self.last_click[i];
            let near = (pos.x - last_p.x).abs() < 5.0 && (pos.y - last_p.y).abs() < 5.0;
            let quick = last_t.is_some_and(|t| t.elapsed() < Duration::from_millis(500));
            let count = if near && quick { count + 1 } else { 1 };
            self.last_click[i] = (Some(Instant::now()), pos, count);
        }
        let click_state = self.last_click[i].2.max(1);

        let ty = match (b, down) {
            (Button::Left, true) => kCGEventLeftMouseDown,
            (Button::Left, false) => kCGEventLeftMouseUp,
            (Button::Right, true) => kCGEventRightMouseDown,
            (Button::Right, false) => kCGEventRightMouseUp,
            (_, true) => kCGEventOtherMouseDown,
            (_, false) => kCGEventOtherMouseUp,
        };
        unsafe {
            let ev = CGEventCreateMouseEvent(self.src.0, ty, pos, i as u32);
            if !ev.is_null() {
                CGEventSetIntegerValueField(ev, kCGMouseEventButtonNumber, i as i64);
                CGEventSetIntegerValueField(ev, kCGMouseEventClickState, click_state);
                CGEventSetFlags(ev, self.flags());
            }
            post(ev);
        }
    }

    fn wheel(&mut self, vertical: i32, horizontal: i32) {
        // Windows: +vertical = scroll up, +horizontal = right. CG: +wheel1 = up, +wheel2 = left.
        self.wheel_acc.0 += vertical * self.scroll_lines;
        self.wheel_acc.1 -= horizontal * self.scroll_lines;
        let v = self.wheel_acc.0 / 120;
        let h = self.wheel_acc.1 / 120;
        self.wheel_acc.0 -= v * 120;
        self.wheel_acc.1 -= h * 120;
        if v == 0 && h == 0 {
            return;
        }
        let sign = if self.invert_scroll { -1 } else { 1 };
        unsafe {
            let ev = CGEventCreateScrollWheelEvent2(self.src.0, kCGScrollEventUnitLine, 2, v * sign, h * sign, 0);
            if !ev.is_null() {
                CGEventSetFlags(ev, self.flags());
            }
            post(ev);
        }
    }

    fn key(&mut self, kc: u16, down: bool) {
        if kc == KC_CAPS {
            if down {
                self.caps_lock = !self.caps_lock;
                self.post_flags_changed(kc);
            }
            return;
        }
        if modifier_flags(kc).is_some() {
            let changed = if down { self.keys_down.insert(kc) } else { self.keys_down.remove(&kc) };
            if changed {
                self.post_flags_changed(kc);
            }
            return;
        }
        let repeat = down && !self.keys_down.insert(kc);
        if !down && !self.keys_down.remove(&kc) {
            return; // up for a key we never pressed (e.g. after ReleaseAll)
        }
        unsafe {
            let ev = CGEventCreateKeyboardEvent(self.src.0, kc, down);
            if !ev.is_null() {
                CGEventSetFlags(ev, self.flags());
                if repeat {
                    CGEventSetIntegerValueField(ev, kCGKeyboardEventAutorepeat, 1);
                }
            }
            post(ev);
        }
    }

    fn post_flags_changed(&self, kc: u16) {
        unsafe {
            let ev = CGEventCreateKeyboardEvent(self.src.0, kc, true);
            if !ev.is_null() {
                CGEventSetType(ev, kCGEventFlagsChanged);
                CGEventSetFlags(ev, self.flags());
            }
            post(ev);
        }
    }

    fn release_all(&mut self) {
        let keys: Vec<u16> = self.keys_down.iter().copied().filter(|&k| modifier_flags(k).is_none()).collect();
        for k in keys {
            self.key(k, false);
        }
        let mods: Vec<u16> = self.keys_down.iter().copied().collect();
        for k in mods {
            self.key(k, false);
        }
        for b in [Button::Left, Button::Right, Button::Middle, Button::Back, Button::Forward] {
            if self.buttons_down[b as usize] {
                self.button(b, false);
            }
        }
        self.wheel_acc = (0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_ranges() {
        assert!(is_tailscale_or_local("100.101.102.103".parse().unwrap()));
        assert!(is_tailscale_or_local("127.0.0.1".parse().unwrap()));
        assert!(is_tailscale_or_local("fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(!is_tailscale_or_local("100.128.0.1".parse().unwrap()));
        assert!(!is_tailscale_or_local("192.168.1.10".parse().unwrap()));
    }
}
