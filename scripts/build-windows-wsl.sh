#!/usr/bin/env bash
# Prerequisites: Node/npm, Rust MSVC target, cargo-xwin, clang, lld, llvm, NSIS.
set -euo pipefail
cd "$(dirname "$0")/.."
case "${RUSTFLAGS:-}" in
  *+crt-static*) ;;
  *) export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static" ;;
esac
# cargo-xwin links the full CRT statically with +crt-static. Disable Tauri's
# separate static-VC/dynamic-UCRT helper, which would conflict with that mode.
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis --config "$(pwd)/src-tauri/tauri.windows-cross.conf.json"
