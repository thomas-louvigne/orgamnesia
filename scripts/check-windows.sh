#!/usr/bin/env bash
# Clippy and the build of the tests for the Windows backend (x86_64-pc-windows-msvc),
# from Linux: the code under cfg(windows) is only compiled there. cargo-xwin, in
# Docker, brings the MSVC libraries and clang-cl. The repository is mounted read-only
# and copied in the container; the compiled crates are kept in Docker volumes.
# The tests themselves only run on Windows (the CI's backend-windows job).
set -euo pipefail
cd "$(dirname "$0")/.."
IMAGE=messense/cargo-xwin  # https://github.com/rust-cross/cargo-xwin
docker run --rm --network host \
    -v "$PWD":/repo:ro \
    -v orgamnesia-xwin-target:/target \
    -v orgamnesia-xwin-registry:/usr/local/cargo/registry \
    -v orgamnesia-xwin-cache:/root/.cache \
    -e CARGO_TARGET_DIR=/target -e RUSTFLAGS="-D warnings" -e CARGO_TERM_COLOR=always \
    "$IMAGE" bash -euo pipefail -c '
        rustup update stable --no-self-update >/dev/null
        rustup default stable >/dev/null
        rustup component add clippy >/dev/null
        rustup target add x86_64-pc-windows-msvc >/dev/null
        rustc --version
        mkdir /work
        tar -C /repo --exclude=./target --exclude=./src-tauri/target --exclude=./dist --exclude=./.git -cf - . | tar -C /work -xf -
        mkdir -p /work/dist /work/src-tauri/windows-tools/mingit /work/src-tauri/windows-tools/dictionaries  # bundled by the installer; clippy only needs them to exist
        cd /work/src-tauri
        cargo xwin clippy --workspace --all-targets --target x86_64-pc-windows-msvc
    '
