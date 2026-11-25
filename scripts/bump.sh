#!/usr/bin/env bash
# A new version, everywhere it is written: the four Cargo.toml, both Cargo.lock,
# tauri.conf.json, the AUR PKGBUILD (pkgrel back to 1) and its .SRCINFO (made in
# the archlinux Docker image when makepkg is missing), the example of aur.org, and
# an empty `* X.Y.Z` section in both changelogs (the notes are written by hand).
#
#   scripts/bump.sh 1.11.0
#
# Not orgamnesia-bin (packaging/arch-bin): its checksum is the one of the package
# the release workflow attaches to the GitHub release; see packaging/arch-release/aur.org.
set -euo pipefail
cd "$(dirname "$0")/.."

v="${1:-}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/bump.sh X.Y.Z" >&2; exit 1; }

# The first `version =` of each Cargo.toml is the one of its [package]
for f in Cargo.toml core/Cargo.toml backend/Cargo.toml src-tauri/Cargo.toml; do
    sed -i "0,/^version = \".*\"/s//version = \"$v\"/" "$f"
done
sed -i "0,/\"version\": \".*\"/s//\"version\": \"$v\"/" src-tauri/tauri.conf.json
sed -i "s/^pkgver=.*/pkgver=$v/; s/^pkgrel=.*/pkgrel=1/" packaging/arch-release/PKGBUILD
sed -i "s/git commit -m \"orgamnesia [0-9.]*\"/git commit -m \"orgamnesia $v\"/" packaging/arch-release/aur.org

# Only the workspace's own packages change in the lockfiles
cargo update --workspace --offline -q
(cd src-tauri && cargo update --workspace --offline -q)

# An empty section before the newest one, unless there already is one
for f in changelog.org changelog.fr.org; do
    grep -q "^\* $v\b" "$f" || sed -i "0,/^\* [0-9]/s//* $v\n\n&/" "$f"
done

scripts/srcinfo.sh packaging/arch-release

echo "Version $v written. Still to do:"
echo "  - the notes of $v in changelog.org and changelog.fr.org"
echo "  - python3 packaging/release/changelog.py check $v"
echo "  - scripts/check.sh, commit, git push origin main (the Release workflow tags v$v)"
echo "  - once the release is on GitHub: scripts/aur.sh (both AUR packages)"
