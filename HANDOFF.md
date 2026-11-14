# Handoff: stanje projekta i sljedeći koraci

Ovaj dokument je za nastavak rada, i za čovjeka i za Claude Code sesiju na drugom računalu.
Opis projekta i upute za korisnika su u [README.md](README.md).

## Cilj

Windows laptop s vanjskim monitorom, tipkovnicom i mišem. Osoba A radi na Windowsu s ugrađenom tipkovnicom i trackpadom. Osoba B istovremeno upravlja **udaljenim Macom** (vidi ga kroz RustDesk na vanjskom monitoru) vanjskom tipkovnicom i mišem. Oba računala su u istom Tailscale tailnetu.

## Stanje (2026-11-14)

| dio | stanje |
|---|---|
| `crates/proto` | gotovo, unit testovi prolaze |
| `crates/win-capture` (`multi-mouse.exe`) | napisano i cross-compilano iz WSL-a; na Windowsu se pokreće i učitava `interception.dll`. **Nije testirano s instaliranim driverom** (vidi blokator) |
| `crates/mac-agent` (`multi-mouse-agent`) | napisano, `cargo check --target aarch64-apple-darwin` prolazi. **Nikad pokrenuto na Macu** |
| CI (`.github/workflows/build.yml`) | gradi Windows exe i universal Mac binary; tag `v*` napravi GitHub release |

### Blokator: Interception driver i Memory Integrity (HVCI)

Na ciljnom Windows laptopu (Windows 11, build 26200) je **Memory Integrity / HVCI uključen** (`SecurityServicesRunning: 1,2`), a vulnerable driver blocklist je aktivan.
Interception je stari kernel filter driver (v1.0.1) koji se registrira kao UpperFilter za **cijelu** klasu tipkovnica i miševa.
Ako ga HVCI odbije učitati, nakon restarta mogu prestati raditi **sve** tipkovnice i miševi, uključujući ugrađene.
Zato driver još **nije instaliran**. Opcije:

1. Instalirati Interception i riskirati. Prije toga treba osigurati način oporavka, npr. RustDesk pristup *na* Windows ili Safe Mode.
2. Isključiti Memory Integrity, pa instalirati Interception. To smanjuje sigurnost sustava.
3. **Bez kernel drivera (preporučeno za ovaj laptop):** vanjski Logitech USB prijemnik prebaciti na WinUSB driver (Zadig) i čitati HID izvještaje direktno iz `multi-mouse.exe` preko libusb/`rusb`. Ugrađena tipkovnica i trackpad se uopće ne diraju, nema rizika od lockouta, ne treba restart, a vraća se jednim "Uninstall device".

### Uređaji na ciljnom laptopu

| uređaj | hardverski ID |
|---|---|
| ugrađena tipkovnica | `ACPI\IDEA0100` (PS/2) |
| touchpad | `HID\SYNA2BA6&COL01` |
| **vanjska tipkovnica + miš** (jedan Logitech prijemnik) | `HID\VID_046D&PID_C534&MI_00` (tipkovnica), `HID\VID_046D&PID_C534&MI_01&COL01` (miš) |

## Sljedeći koraci na Macu

```sh
git clone https://github.com/stepanic/desktop-multiple-mouses.git
cd desktop-multiple-mouses
xcode-select --install          # ako već nije instalirano
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # ako nema Rusta
cargo test --workspace
cargo build --release -p multi-mouse-agent
./target/release/multi-mouse-agent
```

1. Daj **Accessibility** dozvolu aplikaciji iz koje se agent pokreće (Terminal / iTerm), pa ga ponovno pokreni.
2. Zapiši **token** koji agent ispiše. Treba ga Windows setup.
3. Provjeri Tailscale ime Maca: `tailscale status` ili Tailscale aplikacija.
4. Opcionalno napravi LaunchAgent (`~/Library/LaunchAgents/…plist`) da se agent pokreće pri loginu. Još nije napravljeno.

Kako testirati Mac agenta bez Windowsa: mali Python klijent koji pošalje hello (`MMUS`, verzija `1`, duljina tokena, token), pročita bajt `0`, pa šalje npr. `Move` (tag `1`, `i32 dx`, `i32 dy` little-endian). Format je u `crates/proto/src/lib.rs`.

Moguće stvari za doraditi na Macu nakon prvog testa:
- osjetljivost miša (`--speed`) i smjer scrolla (`--invert-scroll`), jer prirodni scroll možda ne vrijedi za sintetičke evente
- ISO tipke na hrvatskom rasporedu (`--swap-iso`)
- Ctrl vs ⌘ (`--ctrl-is-cmd`)
- radi li modifier + klik (Cmd+klik) i dvoklik (`kCGMouseEventClickState`)

## Sljedeći koraci na Windowsu

Ovisno o odabranoj opciji za blokator:

- **Opcija 1 ili 2:** pokreni `interception-installer\install-interception.exe /install` kao administrator, restartaj, pa `multi-mouse.exe` (setup se pokrene sam).
- **Opcija 3:** treba implementirati novi backend u `crates/win-capture`:
  - `rusb` (libusb) otvori `VID_046D:PID_C534`, interface 0 (boot keyboard, 8-bajtni izvještaji) i interface 1 (miš, report protocol; parsirati HID report descriptor zbog wheela i 12-bitnih X/Y)
  - pretvori izvještaje u `mm_proto::Msg` (tipkovnica: HID usage → PC scan code, ili proširiti protokol na HID usage)
  - Ctrl+Alt+F12 "Windows mode" bi tada morao ići preko `SendInput`

Build Windows exe-a iz WSL-a: `scripts/build-windows-from-wsl.sh` (llvm-mingw u `~/.local/toolchains`). Gotov exe ide u `C:\Users\<user>\multi-mouse\`, zajedno s `interception.dll` iz [Interception v1.0.1](https://github.com/oblitum/Interception/releases/tag/v1.0.1).

## Arhitektura ukratko

- Windows (`crates/win-capture/src/app.rs`): Interception filter **samo** na uređajima čiji hardverski ID je u `multi-mouse.toml`. Ostali uređaji ne prolaze kroz aplikaciju, pa ako se aplikacija zaglavi, laptop i dalje radi. Svake 3 sekunde se uređaji ponovno skeniraju, zbog ponovnog spajanja. Svaki pritisak tipke ili gumba pamti odredište (Mac ili Windows), tako da otpuštanje uvijek ide istom odredištu i nakon Ctrl+Alt+F12.
- Mreža (`net.rs`): TCP s `nodelay`, uzastopni pomaci miša se spajaju, ping svake 2 s, automatski reconnect.
- Mac (`crates/mac-agent/src/agent.rs`): prihvaća samo Tailscale IP-ove i ispravan token. Ubacuje CGEvente na `kCGHIDEventTap`. Sam prati stanje modifikatora (flagovi na svakom eventu), dvoklik i drag. Na disconnect otpušta sve.
