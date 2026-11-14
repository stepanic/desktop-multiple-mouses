# multi-mouse

Dvoje ljudi, jedan Windows laptop:

- **Osoba A** radi na Windowsu s ugrađenom tipkovnicom i trackpadom laptopa. Ništa se ne mijenja.
- **Osoba B** na vanjskom monitoru gleda udaljeni Mac (RustDesk) i upravlja njime **vanjskom tipkovnicom i mišem**. Ti uređaji više uopće ne diraju Windows.

```
Ugrađena tipkovnica + trackpad ──► Windows (normalno)

Vanjska tipkovnica + miš ──► multi-mouse.exe ──Tailscale──► multi-mouse-agent (Mac) ──► macOS
                              (Interception driver
                               ih "pojede" za Windows)

RustDesk na vanjskom monitoru = samo prikaz Mac ekrana (View mode)
```

## Instalacija

### Mac (jednom)

1. Kopiraj `multi-mouse-agent` na Mac (npr. u `~/bin/`), pa u Terminalu:
   ```sh
   xattr -d com.apple.quarantine ~/bin/multi-mouse-agent 2>/dev/null
   chmod +x ~/bin/multi-mouse-agent
   ~/bin/multi-mouse-agent
   ```
2. macOS će tražiti **Accessibility** dozvolu: *System Settings → Privacy & Security → Accessibility* → uključi **Terminal** (ili aplikaciju iz koje pokrećeš agenta). Zatim ponovno pokreni agenta.
3. Agent ispiše **token**. Treba ti za Windows setup.

Agent prihvaća samo veze s Tailscale adresa (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`), i to samo s ispravnim tokenom.

Opcije (`multi-mouse-agent --help`):

| opcija | značenje |
|---|---|
| `--speed 1.5` | brži miš |
| `--scroll 5` | više redaka po kliku kotačića |
| `--invert-scroll` | obrnut smjer scrolla |
| `--ctrl-is-cmd` | PC Ctrl radi kao ⌘ (Ctrl+C / Ctrl+V kopiraju na Macu) |
| `--swap-iso` | ako su tipka lijevo od `1` i tipka lijevo od `Z` zamijenjene |

### Windows (jednom)

1. Instaliraj **Interception driver**: otvori *Command Prompt kao administrator*:
   ```bat
   cd %USERPROFILE%\multi-mouse\interception-installer
   install-interception.exe /install
   ```
   i **restartaj računalo**.
2. U mapi `multi-mouse` moraju biti `multi-mouse.exe` i `interception.dll` (x64).
3. Pokreni `multi-mouse.exe`. Prvi put pokreće setup:
   1. pritisneš tipku na **vanjskoj** tipkovnici,
   2. klikneš **vanjskim** mišem,
   3. upišeš Tailscale ime Maca (npr. `moj-macbook`) ili njegov `100.x.y.z` IP,
   4. upišeš token s Maca.

   Postavke se spremaju u `multi-mouse.toml` pored exe-a.

### RustDesk

U RustDesk sesiji prema Macu uključi **View mode** (u alatnoj traci sesije). Tako ni trackpad laptopa ni RustDesk prozor u fokusu ne šalju ništa na Mac. Unos na Mac ide isključivo preko multi-mouse.

## Korištenje

- `multi-mouse.exe`: pokreni. Dok je prozor otvoren, vanjski uređaji idu na Mac.
- **Ctrl+Alt+F12** na vanjskoj tipkovnici prebacuje vanjske uređaje Mac ↔ Windows.
- Zatvoriš prozor i sve se vraća na normalno.
- `multi-mouse.exe setup`: ponovno odabereš uređaje ili Mac.
- `multi-mouse.exe list`: ispiše sve tipkovnice i miševe s hardverskim ID-jevima.

Ako veza s Macom pukne, Mac otpusti sve držane tipke i tipke miša, a Windows aplikacija se sama ponovno spaja.

## Poznata ograničenja

- Uređaji se prepoznaju po hardverskom ID-u. Ako vanjski miš i trackpad imaju isti ID (rijetko), setup upozori.
- Interception driver vidi najviše 10 tipkovnica i 10 miševa. Poznat je bug u kojem nakon jako puno spajanja i odspajanja USB uređaja novi uređaji ne rade do restarta.
- Pomak miša šalje se kao sirovi pomak uređaja bez Windows akceleracije. Brzinu podesi s `--speed` na Macu.

## Build

- **Windows exe iz WSL/Linuxa:** `scripts/build-windows-from-wsl.sh` (treba [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) u `~/.local/toolchains`)
- **Mac agent:** `cargo build --release -p multi-mouse-agent` na Macu, ili GitHub Actions (`.github/workflows/build.yml` gradi oba, a na tagu `v*` napravi release)

Struktura:

- `crates/proto`: mrežni protokol (zajednički)
- `crates/win-capture`: `multi-mouse.exe` (Interception + slanje)
- `crates/mac-agent`: `multi-mouse-agent` (CGEvent injekcija)
