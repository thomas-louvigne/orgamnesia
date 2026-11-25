#!/usr/bin/env bash
# Publish the version of packaging/arch-release/PKGBUILD on the AUR, once its
# release is on GitHub (pushing main with a new version makes it):
#
#   scripts/aur.sh        both packages
#   scripts/aur.sh src    orgamnesia      (built from the tag vX.Y.Z)
#   scripts/aur.sh bin    orgamnesia-bin  (the .pkg.tar.zst attached to the release)
#
# The AUR clones are in ~/aur (AUR_DIR to change it). For orgamnesia-bin, the
# checksum of the release's package is written in packaging/arch-bin and
# committed here: push main afterwards to keep GitHub in step.
set -euo pipefail
cd "$(dirname "$0")/.."
what="${1:-all}"
aur="${AUR_DIR:-$HOME/aur}"
v=$(sed -n 's/^pkgver=//p' packaging/arch-release/PKGBUILD)
url=$(sed -n 's/^url="\(.*\)"/\1/p' packaging/arch-release/PKGBUILD)

git ls-remote --exit-code --tags origin "refs/tags/v$v" >/dev/null \
    || { echo "No tag v$v on GitHub yet: push main, wait for the Release workflow." >&2; exit 1; }

# Copy PKGBUILD and .SRCINFO from $1 to the AUR clone $2, commit, push
publish() {
    git -C "$2" pull -q --rebase
    cp "$1/PKGBUILD" "$1/.SRCINFO" "$2/"
    git -C "$2" add PKGBUILD .SRCINFO
    if git -C "$2" diff --cached --quiet; then echo "$2: already up to date"; return; fi
    git -C "$2" commit -q -m "$(basename "$2") $v"
    git -C "$2" push origin HEAD:master   # the AUR only takes master
    echo "$2: $v published"
}

if [[ "$what" == all || "$what" == src ]]; then
    publish packaging/arch-release "$aur/orgamnesia"
fi

if [[ "$what" == all || "$what" == bin ]]; then
    pkg="$url/releases/download/v$v/orgamnesia-$v-1-x86_64.pkg.tar.zst"
    sum=$(curl -fsSL "$pkg" | sha256sum | cut -d' ' -f1) && [[ "$sum" != e3b0c442* ]] \
        || { echo "The release has no $pkg yet (job Arch Linux package)." >&2; exit 1; }
    sed -i "s/^pkgver=.*/pkgver=$v/; s/^pkgrel=.*/pkgrel=1/; s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$sum')/" packaging/arch-bin/PKGBUILD
    scripts/srcinfo.sh packaging/arch-bin
    files=(packaging/arch-bin/PKGBUILD packaging/arch-bin/.SRCINFO)
    git diff --quiet HEAD -- "${files[@]}" || git commit -q -m "orgamnesia-bin $v: checksum of the release's package" -- "${files[@]}"
    publish packaging/arch-bin "$aur/orgamnesia-bin"
fi
