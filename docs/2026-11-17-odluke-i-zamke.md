# Odluke, odbačene alternative i zamke (2026-11-14 – 2026-11-17)

Što bi sljedeći prolaz inače morao ponovno otkriti. Kako sustav radi: [KAKO-RADI.md](KAKO-RADI.md). Stanje i sljedeći koraci: [../HANDOFF.md](../HANDOFF.md).

## Odbačene alternative

| alternativa | zašto odbačena |
|---|---|
| Pravi multiseat na Windowsu (ASTER, SoftXpand) | komercijalno; traži kernel-level izmjene sesija; nema open-source zamjene |
| VMware VM s USB passthroughom | radi, ali traži drugu licencu za OS i puno RAM-a; nije potrebno jer druga osoba ionako radi na udaljenom Macu |
| "Lažni drugi kursor" + `PostMessage` u prozore | radi samo za jednostavne Win32 aplikacije; Chrome, Office i igre ga ignoriraju |
| Input preko RustDeska | RustDesk uzima input iz Windows fokusa, pa ne razlikuje uređaje; zato RustDesk služi samo za prikaz (View mode) |
| Deskflow / Lan Mouse / Barrier | prebacuju **sve** uređaje odjednom (rub ekrana), a ne po uređaju |
| **Interception kernel driver** (prva implementacija, commit `55a6631`) | laptop ima **HVCI uključen** (`SecurityServicesRunning: 1,2`) i vulnerable driver blocklist; Interception je UpperFilter za *cijelu* klasu tipkovnica i miševa → ako ga HVCI blokira, nakon restarta nema **nikakvog** inputa. Korisnik je odabrao WinUSB |
| Raw Input + low-level hookovi | LL hook dobije event **prije** Raw Inputa, pa u trenutku blokiranja ne zna koji je uređaj; re-injection preko `SendInput` ne radi u elevated prozorima (UIPI) |
| WinUSB preko SetupAPI-ja bez Zadiga (forsirati `winusb.inf`) | moguće, ali krhko (klasa uređaja HIDClass vs USBDevice); Zadig (libwdi) je provjeren put |

## Zamke koje su koštale vremena

### libusb na Windowsu "otvori" i uređaje koje drži Windows
libusb ima i **HID backend**. Na prijemniku s običnim `HidUsb` driverom `open()` i `claim_interface()` **uspiju**, pa je `list` lažno javljao "WinUSB: DA". Zato se driver provjerava u registryju:
`HKLM\SYSTEM\CurrentControlSet\Enum\USB\VID_046D&PID_C534&MI_0x\<instanca>\Service` (`HidUsb` ili `WinUSB`).
Ograničenje: stare (odspojene) instance na drugim USB portovima mogu imati drukčiji servis. Uzima se "bilo koja instanca ima WinUSB".

### Cross-compile Windows exe-a iz WSL-a (llvm-mingw)
- Rustov `x86_64-pc-windows-gnu` traži `-lgcc_eh` i `-lgcc`, kojih llvm-mingw nema. Rješenje: shim direktorij s kopijom `libunwind.a` kao `libgcc_eh.a` i praznim `libgcc.a` (`llvm-ar rcs`), uz `RUSTFLAGS="-L <shim>"`.
- `libloading` i `windows-sys` raw-dylib trebaju `x86_64-w64-mingw32-dlltool` na `PATH`-u. Nije dovoljno postaviti samo linker.
- `rusb` s `vendored` gradi libusb iz C izvora, pa treba `CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-clang` i `AR_x86_64_pc_windows_gnu=llvm-ar`.
- Sve je u `scripts/build-windows-from-wsl.sh`.

### Ostalo
- `ls` je u ovom shellu alias (eza): `ls -d dir/` vraća put **bez** završne kosе crte. U skriptama koristi `command ls` ili `echo ~/put/*`.
- Mac kod se može provjeriti u WSL-u s `cargo check --target aarch64-apple-darwin` (nema linkanja, pa ne treba macOS SDK). Testovi Mac agenta se vrte samo u CI-ju na macOS-u.
- Za Mac FFI koristi se `CGEventCreateScrollWheelEvent2` (nije variadic), jer se variadic funkcije na Apple Silicon pozivaju drukčije.
- USB tipkovnice **ne ponavljaju** tipku same; auto-repeat radi `kbdhid` u Windowsu. Kad čitamo preko WinUSB-a, repeat radimo sami (500 ms / 33 ms).

## Otvoreno

- Zadig (WinUSB na Interface 0 i 1) još nije primijenjen. Nijedan USB, mrežni ni Mac dio nije testiran na stvarnom hardveru (detalji u [HANDOFF.md](../HANDOFF.md)).
- Tipke za glasnoću na Macu vjerojatno ne rade kao CGEvent keycode 72–74 (trebaju NX system-defined eventi).
- Prvi commitovi imaju poruke na engleskom, a ne po konvenciji `tip(opseg)` na hrvatskom. Već su pushani, pa nisu prepisivani.
