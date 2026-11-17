# Handoff: stanje projekta i sljedeći koraci

Ovaj dokument je za nastavak rada, i za čovjeka i za Claude Code sesiju na drugom računalu.
Kako sustav radi (s dijagramima): [docs/KAKO-RADI.md](docs/KAKO-RADI.md). Upute za korisnika: [README.md](README.md).

## Cilj

Windows laptop s vanjskim monitorom i vanjskom tipkovnicom i mišem (Logitech USB prijemnik `046d:c534`). Osoba A radi na Windowsu s ugrađenom tipkovnicom i trackpadom. Osoba B istovremeno upravlja **udaljenim Macom** (vidi ga kroz RustDesk na vanjskom monitoru) vanjskom tipkovnicom i mišem. Oba računala su u istom Tailscale tailnetu.

## Odluke koje su već donesene

- **Ne koristimo Interception kernel driver.** Na laptopu je uključen Memory Integrity (HVCI) i vulnerable driver blocklist. Interception je UpperFilter za *cijelu* klasu tipkovnica i miševa. Kad bi ga HVCI blokirao, laptop bi ostao bez ikakvog inputa. Prva verzija koda ga je koristila (vidi git povijest), ali je zamijenjena.
- **Umjesto toga: WinUSB + libusb.** Vanjski prijemnik dobije WinUSB driver (Zadig), a `multi-mouse.exe` ga čita direktno i parsira HID izvještaje. Laptop input se ne dira, nema restarta, a vraćanje je jednostavno.
- **Input ide mimo RustDeska.** RustDesk je samo prikaz (View mode). Input ide našim protokolom preko Tailscalea.
- Repo je javan: https://github.com/stepanic/desktop-multiple-mouses

## Stanje (2026-11-17)

| dio | stanje |
|---|---|
| `crates/proto` | gotovo, testovi prolaze |
| `crates/win-capture` | WinUSB/libusb backend napisan i cross-compilan iz WSL-a. `list` na pravom laptopu ispravno vidi `046d:c534` i javlja da sučelja 0 i 1 još koriste Windows HID driver. Unit testovi za HID parser i prevođenje prolaze |
| `crates/mac-agent` | napisano, CI ga gradi i testira na macOS-u. **Nikad pokrenuto na pravom Macu** |
| CI | zeleno za prvi commit (Windows + macOS). Tag `v*` napravi release s oba binarija |

### Što NIJE testirano na stvarnom hardveru

1. **Zadig → WinUSB još nije primijenjen** na prijemnik. Zadig 2.9 je skinut u `C:\Users\User\multi-mouse\zadig-2.9.exe` i jednom pokrenut, ali driver nije zamijenjen.
2. Čitanje USB izvještaja preko libusb (`multi-mouse.exe monitor`).
3. Stvarni HID descriptor Logitech prijemnika (sučelje 1 ima report ID-jeve; parser je testiran na sintetičkom descriptoru u istom stilu).
4. `SendInput` Windows način (Ctrl+Alt+F12).
5. Mac agent: CGEvent injekcija, Accessibility, dvoklik, drag, modifikatori, scroll smjer, hrvatski ISO raspored.

## Sljedeći koraci

### Windows (laptop)

1. Zadig: Options → List All Devices → `USB Receiver (Interface 0)` → WinUSB → Replace Driver, pa isto za `Interface 1`.
2. `multi-mouse.exe list` → mora pisati `0:WinUSB,1:WinUSB -> spreman za multi-mouse`.
3. `multi-mouse.exe monitor` → tipkanje, klikovi, pomaci i scroll moraju se ispisivati.
   - Ako ništa ne stiže: provjeriti `read_interrupt` endpoint i je li `claim_interface` uspio (`crates/win-capture/src/usb.rs`).
   - Ako tipke stižu krivo: ispisati sirove izvještaje i descriptor (dodati debug ispis u `usb.rs`).
4. Test cijelog lanca bez Maca: u WSL-u `python3 scripts/fake-agent.py test`, u `multi-mouse.toml` postaviti `mac_address = "127.0.0.1"` i `token = "test"`, pa pokrenuti `multi-mouse.exe`.
5. Probati Ctrl+Alt+F12 (Windows način preko `SendInput`).

### Mac

```sh
git clone https://github.com/stepanic/desktop-multiple-mouses.git
cd desktop-multiple-mouses
xcode-select --install          # ako već nije
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # ako nema Rusta
cargo test --workspace
cargo build --release -p multi-mouse-agent
./target/release/multi-mouse-agent
```

1. Dati Accessibility dozvolu Terminalu i ponovno pokrenuti agenta.
2. Zapisati token i Tailscale ime Maca (`tailscale status`).
3. Na Windowsu: `multi-mouse.exe setup` → ime Maca + token.
4. Doraditi nakon prvog testa: `--speed`, smjer scrolla, `--swap-iso`, `--ctrl-is-cmd`. Provjeriti Cmd+klik, dvoklik, drag i AltGr znakove (`@ { } [ ]`) na hrvatskom rasporedu.
5. LaunchAgent (`~/Library/LaunchAgents/…plist`) da se agent sam pokreće pri loginu. Još nije napravljeno.

### Ideje za kasnije

- Windows: tray ikona umjesto konzole i autostart.
- Mac: provjeriti rade li tipke za glasnoću (CGEvent keycode 72–74 možda ne mijenja glasnoću; za to trebaju NX system-defined eventi).
- Podrška za više vanjskih uređaja (sada je jedan `usb_device`).

## Build okruženje (WSL na laptopu)

- Rust instaliran u `~/.cargo` (rustup, targeti `x86_64-pc-windows-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`).
- llvm-mingw u `~/.local/toolchains/llvm-mingw-*`. Shim `~/.local/toolchains/gcc-shim` sadrži `libgcc_eh.a`, kopiju `libunwind.a`, jer Rustov windows-gnu target traži libgcc.
- `scripts/build-windows-from-wsl.sh` → `target/x86_64-pc-windows-gnu/release/multi-mouse.exe`. Kopirati u `C:\Users\User\multi-mouse\`.
- `cargo check --target aarch64-apple-darwin` radi u WSL-u (bez linkanja), pa se Mac kod može provjeravati i bez Maca.

## Arhitektura ukratko

- `crates/win-capture`: `usb.rs` (libusb + provjera WinUSB-a u registryju, jer libusb-ov HID backend "otvori" i uređaje s Windows driverom) → `hid.rs` (parser report descriptora) → `translate.rs` (snimka stanja → pritisak/otpuštanje) → `app.rs` Router (Mac/Windows način, Ctrl+Alt+F12, auto-repeat 500/33 ms) → `net.rs` (TCP, spajanje pomaka, ping, reconnect) ili `inject.rs` (`SendInput`).
- `crates/mac-agent`: prihvaća samo Tailscale IP-ove i ispravan token, prevodi scan code u Mac keycode po fizičkom položaju i šalje CGEvente na `kCGHIDEventTap`. Sam prati modifikatore, dvoklik i drag. Na disconnect otpušta sve.
- `crates/proto`: `MMUS` hello + token, zatim poruke s tagovima 1–6.
