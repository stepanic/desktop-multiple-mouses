# multi-mouse

Dvoje ljudi, jedan Windows laptop:

- **Osoba A** radi na Windowsu s ugrađenom tipkovnicom i trackpadom laptopa. Ništa se ne mijenja.
- **Osoba B** na vanjskom monitoru gleda udaljeni Mac (RustDesk) i upravlja njime **vanjskom USB tipkovnicom i mišem**. Ti uređaji više ne diraju Windows.

```
Ugrađena tipkovnica + trackpad ──► Windows (normalno)

Vanjski USB prijemnik ──► multi-mouse.exe ──Tailscale──► multi-mouse-agent (Mac) ──► macOS
(WinUSB driver)            (libusb)

RustDesk na vanjskom monitoru = samo prikaz Mac ekrana (View mode)
```

📖 **Kako radi, s dijagramima:** [docs/KAKO-RADI.md](docs/KAKO-RADI.md)
🧭 **Stanje projekta i sljedeći koraci:** [HANDOFF.md](HANDOFF.md)

## Instalacija

### Mac (jednom)

```sh
git clone https://github.com/stepanic/desktop-multiple-mouses.git
cd desktop-multiple-mouses
cargo build --release -p multi-mouse-agent     # treba Rust + Xcode Command Line Tools
./target/release/multi-mouse-agent
```

Gotov binary možeš skinuti i s GitHub Actions (artifact `multi-mouse-agent-macos`). Nakon skidanja:
`xattr -d com.apple.quarantine multi-mouse-agent && chmod +x multi-mouse-agent`.

1. macOS traži **Accessibility** dozvolu: *System Settings → Privacy & Security → Accessibility* → uključi Terminal (ili aplikaciju iz koje pokrećeš agenta). Zatim ponovno pokreni agenta.
2. Agent ispiše **token**. Treba ti za Windows setup.

| opcija | značenje |
|---|---|
| `--speed 1.5` | brži miš |
| `--scroll 5` | više redaka po kliku kotačića |
| `--invert-scroll` | obrnut smjer scrolla |
| `--ctrl-is-cmd` | PC Ctrl radi kao ⌘ (Ctrl+C / Ctrl+V kopiraju na Macu) |
| `--swap-iso` | ako su tipka lijevo od `1` i tipka lijevo od `Z` zamijenjene |
| `--allow-any` | prihvati veze izvan Tailscalea (nije preporučeno) |

### Windows (jednom)

1. Skini [Zadig](https://zadig.akeo.ie/) i pokreni ga (traži administratorska prava).
2. **Options → List All Devices**.
3. Odaberi **USB Receiver (Interface 0)**, desno **WinUSB** → **Replace Driver**.
4. Ponovi za **USB Receiver (Interface 1)**.
5. `multi-mouse.exe list` mora pokazati `-> spreman za multi-mouse`.
6. `multi-mouse.exe monitor`: kad tipkaš ili mičeš vanjskim mišem, ispisuju se događaji.
7. `multi-mouse.exe` (prvi put se pokrene setup) → upiši Tailscale ime Maca i token.

Od trenutka kad je WinUSB instaliran, vanjska tipkovnica i miš rade **samo kroz multi-mouse**. Laptop tipkovnica i trackpad rade normalno.

### RustDesk

U sesiji prema Macu uključi **View mode**. Tako trackpad laptopa ne upravlja Macom preko RustDeska.

## Korištenje

| naredba | što radi |
|---|---|
| `multi-mouse.exe` | pokreni (vanjski uređaji idu na Mac) |
| **Ctrl+Alt+F12** na vanjskoj tipkovnici | prebaci vanjske uređaje Mac ↔ Windows |
| `multi-mouse.exe setup` | ponovno odaberi uređaj ili Mac |
| `multi-mouse.exe list` | USB HID uređaji i njihovi driveri |
| `multi-mouse.exe monitor [vid:pid]` | samo ispisuj događaje (test, bez mreže) |

## Vraćanje na normalno

Device Manager → *Universal Serial Bus devices* → **USB Receiver** → desni klik → **Uninstall device** (uključi *Attempt to remove the driver for this device*). Ponovi za oba sučelja, pa izvuci i vrati prijemnik. Windows opet koristi standardni HID driver.

## Build

| što | kako |
|---|---|
| Windows exe iz WSL/Linuxa | `scripts/build-windows-from-wsl.sh` (treba [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) u `~/.local/toolchains`) |
| Mac agent | `cargo build --release -p multi-mouse-agent` na Macu |
| oboje | GitHub Actions (`.github/workflows/build.yml`), a tag `v*` napravi release |
| test Windows strane bez Maca | `python3 scripts/fake-agent.py <token>` pa u `multi-mouse.toml` `mac_address = "127.0.0.1"` |

Struktura:

- `crates/proto`: mrežni protokol (zajednički)
- `crates/win-capture`: `multi-mouse.exe` (libusb, HID parser, router, mreža)
- `crates/mac-agent`: `multi-mouse-agent` (CGEvent)
