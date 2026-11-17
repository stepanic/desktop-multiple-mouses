#!/usr/bin/env bash
# Cross-compiles multi-mouse.exe from Linux/WSL using llvm-mingw
# (https://github.com/mstorsjo/llvm-mingw) unpacked into ~/.local/toolchains.
set -euo pipefail
cd "$(dirname "$0")/.."
MINGW=$(echo ~/.local/toolchains/llvm-mingw-*)
SHIM=~/.local/toolchains/gcc-shim
export PATH="$MINGW/bin:$PATH"
# llvm-mingw has no libgcc; Rust's windows-gnu target asks for it. libunwind replaces it.
if [ ! -f "$SHIM/libgcc_eh.a" ]; then
  mkdir -p "$SHIM"
  cp "$MINGW/x86_64-w64-mingw32/lib/libunwind.a" "$SHIM/libgcc_eh.a"
  llvm-ar rcs "$SHIM/libgcc.a"
fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-clang
export RUSTFLAGS="-L $SHIM"
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-clang AR_x86_64_pc_windows_gnu=llvm-ar
cargo build --release --target x86_64-pc-windows-gnu -p multi-mouse
echo "-> target/x86_64-pc-windows-gnu/release/multi-mouse.exe"
