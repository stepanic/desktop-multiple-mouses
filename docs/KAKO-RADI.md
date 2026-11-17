# Kako radi multi-mouse

Ovaj dokument objašnjava kako multi-mouse radi, korak po korak, uz dijagrame.
Upute za instalaciju su u [README.md](../README.md), a stanje projekta i sljedeći koraci u [HANDOFF.md](../HANDOFF.md).

## 1. Problem

Windows ima **jedan kursor i jedan fokus tipkovnice** za cijeli sustav. Svaka priključena tipkovnica i svaki miš upravljaju istim kursorom. Zato dvoje ljudi ne može raditi istovremeno.

```mermaid
flowchart LR
    LK[Tipkovnica laptopa] --> W
    LT[Trackpad laptopa] --> W
    VK[Vanjska tipkovnica] --> W
    VM[Vanjski miš] --> W
    W((Windows:<br/>JEDAN kursor,<br/>JEDAN fokus))
```

## 2. Rješenje u jednoj slici

Vanjsku periferiju **oduzmemo Windowsu** i njezine događaje šaljemo preko Tailscalea **direktno na Mac**. RustDesk služi samo za **gledanje** Mac ekrana.

```mermaid
flowchart LR
    subgraph LAPTOP["Windows laptop"]
        LK[Tipkovnica laptopa] --> WIN((Windows))
        LT[Trackpad laptopa] --> WIN
        RX["Logitech USB prijemnik<br/>vanjska tipkovnica + miš<br/>(WinUSB driver)"] --> MM[multi-mouse.exe]
        WIN --> MON1[Ekran laptopa]
        RD["RustDesk<br/>(View mode)"] --> MON2[Vanjski monitor]
    end

    subgraph MAC["Udaljeni Mac"]
        AG[multi-mouse-agent] --> MACOS((macOS))
        MACOS --> SCR[Mac ekran]
    end

    MM -- "TCP preko Tailscale VPN-a" --> AG
    SCR -. "slika ekrana (RustDesk)" .-> RD

    style RX fill:#ffe9b3,stroke:#c90
    style MM fill:#cfe8ff,stroke:#06c
    style AG fill:#cfe8ff,stroke:#06c
```

- **Osoba A** radi na Windowsu s tipkovnicom i trackpadom laptopa i ništa ne primijeti.
- **Osoba B** gleda Mac na vanjskom monitoru i upravlja njime vanjskom tipkovnicom i mišem.

## 3. Kako vanjsku periferiju "oduzmemo" Windowsu

Windows inače USB tipkovnicu i miš drži za sebe preko **HID drivera** (`HidUsb` → `kbdhid`/`mouhid`). Obične aplikacije ne smiju čitati tipkovnicu i miš direktno.

Trik: pomoću **Zadiga** na Logitech prijemnik stavimo Microsoftov **WinUSB** driver. Windows ga onda više ne vidi kao tipkovnicu i miš, a multi-mouse ga čita kao običan USB uređaj (preko libusb).

```mermaid
flowchart TB
    subgraph PRIJE["PRIJE (normalno stanje)"]
        direction TB
        U1[USB prijemnik 046D:C534] --> H1["HidUsb driver"]
        H1 --> K1["kbdhid / mouhid"]
        K1 --> C1(("Windows kursor<br/>i tipkovnica"))
    end

    subgraph POSLIJE["POSLIJE (WinUSB preko Zadiga)"]
        direction TB
        U2[USB prijemnik 046D:C534] --> W2["WinUSB driver<br/>(potpisan od Microsofta)"]
        W2 --> L2["libusb u multi-mouse.exe"]
        L2 --> N2(("mreža → Mac"))
    end

    PRIJE ~~~ POSLIJE
```

**Zašto ne Interception kernel driver?** Na ovom laptopu je uključen *Memory Integrity (HVCI)*. Interception je stari filter driver za **sve** tipkovnice i miševe. Kad bi ga HVCI blokirao, nakon restarta ne bi radila ni tipkovnica ni trackpad laptopa. WinUSB pristup dira **samo** vanjski prijemnik, ne treba restart i lako se vraća.

Logitech prijemnik ima dva USB sučelja, a WinUSB ide na oba:

| sučelje | što nosi |
|---|---|
| Interface 0 (`MI_00`) | tipkovnica |
| Interface 1 (`MI_01`) | miš, tipke za glasnoću, Logitech HID++ |

## 4. Što se događa unutar `multi-mouse.exe`

Aplikacija ima tri vrste niti (threadova) koje komuniciraju preko kanala (`mpsc`):

```mermaid
flowchart LR
    subgraph USB["USB niti (jedna po sučelju)"]
        R0["Interface 0<br/>read_interrupt()"] --> T0[HID dekoder]
        R1["Interface 1<br/>read_interrupt()"] --> T1[HID dekoder]
    end

    T0 -- Msg --> RT
    T1 -- Msg --> RT

    subgraph MAIN["Glavna nit: Router"]
        RT{"Način rada?<br/>Ctrl+Alt+F12"}
        REP["Auto-repeat<br/>tipki"]
    end

    RT -- "Mac način" --> NET["Mrežna nit<br/>TCP + reconnect + ping"]
    RT -- "Windows način" --> SI["SendInput<br/>(Windows dobije tipke)"]
    REP --> RT
    NET --> AG[(Mac agent)]
```

| datoteka | uloga |
|---|---|
| `usb.rs` | pronađe prijemnik, provjeri ima li WinUSB (registry), čita USB izvještaje |
| `hid.rs` | parsira **HID report descriptor**, tj. "kartu" izvještaja: gdje su tipke, X/Y, kotačić |
| `translate.rs` | usporedi novi izvještaj s prethodnim i iz razlike napravi događaje "pritisnuto"/"pušteno" |
| `usbkeys.rs` | HID kod tipke → PC scan code |
| `app.rs` | Router: Mac/Windows način, prečac, auto-repeat |
| `net.rs` | TCP veza prema Macu, spajanje pomaka miša, ping, ponovno spajanje |
| `inject.rs` | `SendInput` za Windows način |

## 5. Od pritiska tipke do slova na Macu

```mermaid
sequenceDiagram
    autonumber
    participant P as Osoba B
    participant K as Vanjska tipkovnica
    participant U as USB nit (multi-mouse.exe)
    participant R as Router
    participant N as Mrežna nit
    participant A as multi-mouse-agent (Mac)
    participant M as macOS

    P->>K: pritisne Shift + A
    K->>U: HID izvještaj [02 00 04 00 00 00 00 00]
    U->>U: dekoder: Shift (0xE1) i A (0x04) su novi
    U->>R: Key 0x2A down, Key 0x1E down
    R->>N: (Mac način) proslijedi
    N->>A: TCP: [04 2A 00 02] [04 1E 00 02]
    A->>A: scan code → Mac keycode (Shift=56, A=0)
    A->>M: CGEvent FlagsChanged(Shift)
    A->>M: CGEvent KeyDown(A) + flag Shift
    M-->>P: na ekranu (kroz RustDesk) se pojavi "A"
```

### Kako dekoder pretvara izvještaje u događaje

USB tipkovnica ne šalje "pritisnuto A", nego **snimku stanja**: popis svih trenutno pritisnutih tipki. Dekoder pamti prethodnu snimku i šalje samo razliku:

```mermaid
flowchart LR
    S1["prethodno:<br/>{ }"] --> D1{razlika}
    S2["sada:<br/>{Shift, A}"] --> D1
    D1 --> E1["Shift down<br/>A down"]

    S3["prethodno:<br/>{Shift, A}"] --> D2{razlika}
    S4["sada:<br/>{Shift}"] --> D2
    D2 --> E2["A up"]
```

Isto vrijedi za tipke miša. Pomaci (X/Y) i kotačić su **relativni**, pa se šalju direktno.

USB tipkovnice same ne ponavljaju tipku kad je držiš. Zato Router radi **auto-repeat**: nakon 500 ms ponavlja tipku svakih 33 ms.

## 6. Prebacivanje Mac ↔ Windows (Ctrl+Alt+F12)

```mermaid
stateDiagram-v2
    [*] --> Mac: pokretanje
    Mac --> Windows: Ctrl+Alt+F12<br/>(Macu se šalje ReleaseAll)
    Windows --> Mac: Ctrl+Alt+F12

    Mac: Mac način
    Mac: vanjski uređaji → mreža → Mac
    Windows: Windows način
    Windows: vanjski uređaji → SendInput → Windows
```

Svaki pritisak pamti **kamo je otišao**. Zato otpuštanje uvijek ide istom odredištu, čak i ako se način promijenio dok je tipka bila pritisnuta. Bez toga bi npr. Ctrl ostao "zaglavljen" na jednoj strani.

## 7. Mrežni protokol

Jednostavan binarni protokol preko TCP-a (port **47800**), opisan u `crates/proto/src/lib.rs`.

```mermaid
sequenceDiagram
    participant W as multi-mouse.exe
    participant A as multi-mouse-agent

    W->>A: TCP connect (Tailscale IP / MagicDNS ime)
    A->>A: je li IP iz Tailscale mreže?<br/>100.64.0.0/10 ili fd7a:115c:a1e0::/48
    W->>A: "MMUS" | verzija=1 | duljina tokena | token
    A->>A: usporedba tokena (constant-time)
    A-->>W: 0 = OK (1 = loš token, 2 = loša verzija)
    loop dok veza traje
        W->>A: događaji (Move / Button / Wheel / Key)
        W->>A: Ping (svake 2 s ako nema događaja)
    end
    Note over A: 8 s tišine → prekid veze<br/>i otpuštanje svih tipki
    Note over W: veza pukla → novi pokušaj svake 2 s
```

| tag | poruka | sadržaj |
|---|---|---|
| 1 | `Move` | `i32 dx`, `i32 dy` (sirovi pomak miša) |
| 2 | `Button` | gumb (lijevi/desni/srednji/natrag/naprijed), dolje/gore |
| 3 | `Wheel` | `i32 vertikalno`, `i32 horizontalno` (120 = jedan "klik") |
| 4 | `Key` | `u16 scan code`, zastavice (E0, dolje) |
| 5 | `ReleaseAll` | otpusti sve |
| 6 | `Ping` | održavanje veze |

Mrežna nit **spaja uzastopne pomake miša** u jedan paket. Miš koji šalje 1000 izvještaja u sekundi zato ne zaguši vezu.

## 8. Mac strana: `multi-mouse-agent`

```mermaid
flowchart TB
    IN[TCP poruka] --> T{tip poruke}
    T -- Move --> MV["trenutna pozicija kursora + pomak × speed<br/>ograniči na ekran"]
    MV --> DR{"drži li se gumb?"}
    DR -- da --> E1[CGEvent *MouseDragged]
    DR -- ne --> E2[CGEvent MouseMoved]
    T -- Button --> CL["broji klikove<br/>(dvoklik: < 500 ms, < 5 px)"] --> E3[CGEvent MouseDown/Up + ClickState]
    T -- Wheel --> E4["CGEvent ScrollWheel<br/>(× --scroll, --invert-scroll)"]
    T -- Key --> KM["scan code → Mac keycode<br/>(--ctrl-is-cmd, --swap-iso)"]
    KM --> MOD{modifikator?}
    MOD -- da --> E5["CGEvent FlagsChanged<br/>+ ažuriraj Shift/Ctrl/⌥/⌘"]
    MOD -- ne --> E6["CGEvent KeyDown/KeyUp<br/>+ trenutni modifikatori"]
    E1 & E2 & E3 & E4 & E5 & E6 --> POST["CGEventPost(kCGHIDEventTap)<br/>(treba Accessibility dozvolu)"]
```

Mapiranje tipki ide po **fizičkom položaju**. Raspored (npr. hrvatski) primjenjuje Mac, isto kao kad bi tipkovnica bila spojena direktno na Mac.

| PC tipka | Mac (zadano) | s `--ctrl-is-cmd` |
|---|---|---|
| Ctrl | Control ⌃ | Command ⌘ |
| Windows | Command ⌘ | Control ⌃ |
| Alt | Option ⌥ | Option ⌥ |
| AltGr | desni Option ⌥ | desni Option ⌥ |

## 9. Sigurnost

```mermaid
flowchart LR
    X[Netko izvana] -- "nije Tailscale IP" --> B1[❌ odbijeno]
    Y[Uređaj u tailnetu,<br/>bez tokena] -- "pogrešan token" --> B2[❌ odbijeno]
    Z[multi-mouse.exe<br/>s tokenom] --> OK[✅ upravlja Macom]
```

- Agent prihvaća veze **samo s Tailscale adresa**, osim ako se pokrene s `--allow-any`.
- Token (128 bita, nasumičan) je spremljen u `~/.config/multi-mouse/token` na Macu, s dozvolama `600`.
- Promet je kriptiran jer ide kroz Tailscale (WireGuard).

## 10. Otpornost na greške

| situacija | što se dogodi |
|---|---|
| vanjski prijemnik izvučen | USB nit otpusti sve tipke i svake 2 s pokušava ponovno |
| Mac nedostupan | Windows strana odbacuje događaje i svake 2 s se pokušava spojiti |
| veza pukne usred držanja tipke | Mac nakon najviše 8 s otpusti sve tipke i gumbe |
| `multi-mouse.exe` zatvoren | vanjski uređaji ne rade nigdje (WinUSB), a laptop radi normalno |
| želim prijemnik natrag na Windows | Device Manager → USB Receiver → Uninstall device (+ izbriši driver), za oba sučelja, pa izvuci i vrati prijemnik |

## 11. Postavljanje, redom

```mermaid
flowchart TB
    A1["Mac: git clone + cargo build<br/>(ili binary iz GitHub Actions)"] --> A2[Mac: pokreni multi-mouse-agent]
    A2 --> A3[Mac: daj Accessibility dozvolu<br/>i ponovno pokreni agenta]
    A3 --> A4[Mac: zapiši token]
    W1[Windows: Zadig → WinUSB<br/>na Interface 0 i 1] --> W2["Windows: multi-mouse.exe list<br/>→ 'spreman za multi-mouse'"]
    W2 --> W3["Windows: multi-mouse.exe monitor<br/>→ vidim tipke i pomake?"]
    A4 --> W4["Windows: multi-mouse.exe setup<br/>→ Tailscale ime Maca + token"]
    W3 --> W4
    W4 --> R1[RustDesk prema Macu: View mode<br/>na vanjskom monitoru]
    R1 --> DONE([✅ dvoje ljudi radi istovremeno])
```
