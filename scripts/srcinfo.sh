#!/usr/bin/env bash
# The .SRCINFO of the PKGBUILD in the folder DIR, written next to it: with
# makepkg when there is one, else in the archlinux Docker image (makepkg
# refuses to run as root, hence the user made there).
#
#   scripts/srcinfo.sh packaging/arch-release
set -euo pipefail
dir="${1:?usage: scripts/srcinfo.sh DIR}"
out="$dir/.SRCINFO"
if command -v makepkg >/dev/null; then
    (cd "$dir" && makepkg --printsrcinfo) > "$out.new"
else
    docker run --rm -v "$(realpath "$dir"):/pkg:ro" archlinux:latest bash -c \
        'useradd -m b && install -o b -m 644 /pkg/PKGBUILD /home/b/ && cd /home/b && su b -c "makepkg --printsrcinfo"' > "$out.new"
fi
[[ -s "$out.new" ]] && mv "$out.new" "$out" || { rm -f "$out.new"; echo "the .SRCINFO of $dir could not be made" >&2; exit 1; }
