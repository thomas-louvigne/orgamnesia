#!/usr/bin/env bash
# The checks of the CI (.github/workflows/ci.yml), run here before a push: same
# lints as errors (-D warnings), with the latest stable Rust as the CI takes it
# (a newer clippy brings new lints: the usual reason the CI fails).
#
#   scripts/check.sh            Linux: core, interface (WebAssembly), backend
#   scripts/check.sh --windows  also the backend built for Windows (needs Docker)
#
# Installed as the pre-push hook by `git config core.hooksPath .githooks`.
set -euo pipefail
cd "$(dirname "$0")/.."
export RUSTFLAGS="-D warnings" CARGO_TERM_COLOR=always

step() { printf '\n\033[1;35m==> %s\033[0m\n' "$*"; }

step "Rust stable up to date (the CI's version)"
rustup update stable --no-self-update >/dev/null
rustup target add wasm32-unknown-unknown >/dev/null
rustc --version

step "Shared core"
(cd core && cargo clippy --all-targets && cargo test -q)

step "Interface (WebAssembly)"
cargo clippy --target wasm32-unknown-unknown
cargo test -q --lib

step "Backend (Tauri)"
mkdir -p dist  # Tauri only needs the interface's folder to exist
(cd src-tauri && cargo clippy --workspace --all-targets && cargo test -q --workspace)

if [[ "${1:-}" == "--windows" ]]; then
    step "Backend for Windows (cargo-xwin in Docker)"
    scripts/check-windows.sh
fi

step "All checks passed"
