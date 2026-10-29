#!/usr/bin/env python3
"""The changelogs (changelog.org, changelog.fr.org) for the release workflow.

    changelog.py check VERSION
        Fails unless both changelogs have notes for VERSION.
    changelog.py body VERSION
        The text of the GitHub release of VERSION (English, then French).
    changelog.py latest VERSION ASSETS_DIR BASE_URL
        The `latest.json` the app's updater reads: for each package signature
        (`<package>.sig`) in ASSETS_DIR, the package's address under BASE_URL and its
        signature; and the notes of every version, in both languages, so that the
        app shows all that changed since the version it runs.

A changelog is an org file: one `* X.Y.Z (YYYY-MM-DD)` headline per version,
newest first, then `- ` items, written for the users, in plain text.
"""

import datetime
import json
import os
import re
import sys
import urllib.parse

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
FILES = {"en": "changelog.org", "fr": "changelog.fr.org"}
HEADLINE = re.compile(r"^\* (\d+\.\d+\.\d+)(?: \((\d{4}-\d{2}-\d{2})\))?\s*$")

# Package file ending → the updater targets it serves (`{os}-{arch}-{installer}`,
# then `{os}-{arch}` for the package installed most)
PACKAGES = [
    (".AppImage", ["linux-x86_64-appimage", "linux-x86_64"]),
    (".deb", ["linux-x86_64-deb"]),
    ("-setup.exe", ["windows-x86_64-nsis", "windows-x86_64"]),
    # One app for Apple Silicon and Intel Macs
    (".app.tar.gz", ["darwin-aarch64-app", "darwin-x86_64-app", "darwin-aarch64", "darwin-x86_64"]),
]


def parse(text):
    """[(version, date, [items])] of a changelog, in its order."""
    versions = []
    for line in text.splitlines():
        m = HEADLINE.match(line)
        if m:
            versions.append((m.group(1), m.group(2), []))
        elif versions and line.startswith("- "):
            versions[-1][2].append(line[2:].strip())
        elif versions and versions[-1][2] and line.startswith("  ") and line.strip():
            # An item going on over several lines
            versions[-1][2][-1] += " " + line.strip()
    return versions


def read(lang):
    with open(os.path.join(ROOT, FILES[lang]), encoding="utf-8") as f:
        return parse(f.read())


def notes(version):
    """{lang: (date, items)} of `version`, for the languages that have it."""
    found = {}
    for lang in FILES:
        for v, date, items in read(lang):
            if v == version and items:
                found[lang] = (date, items)
    return found


def check(version):
    found = notes(version)
    missing = [FILES[lang] for lang in FILES if lang not in found]
    if missing:
        sys.exit(f"No notes for {version} in {', '.join(missing)}: write them before releasing")


def body(version):
    found = notes(version)
    parts = []
    for lang, title in (("en", "What's new"), ("fr", "Nouveautés")):
        if lang in found:
            parts.append(f"## {title}\n\n" + "\n".join(f"- {i}" for i in found[lang][1]))
    return "\n\n".join(parts) + "\n"


def history():
    """Every version, newest first, with its date and its notes in each language."""
    by_version = {}
    order = []
    for lang in FILES:
        for v, date, items in read(lang):
            if v not in by_version:
                by_version[v] = {"version": v, "date": date}
                order.append(v)
            by_version[v][lang] = items
    order.sort(key=lambda v: tuple(int(n) for n in v.split(".")), reverse=True)
    return [by_version[v] for v in order]


def latest(version, assets, base_url):
    platforms = {}
    # The signatures are enough: `<package>.sig` names its package
    for sig_name in sorted(os.listdir(assets)):
        if not sig_name.endswith(".sig"):
            continue
        name = sig_name[: -len(".sig")]
        sig = os.path.join(assets, sig_name)
        for ending, targets in PACKAGES:
            if name.endswith(ending):
                with open(sig, encoding="utf-8") as f:
                    signature = f.read().strip()
                url = base_url.rstrip("/") + "/" + urllib.parse.quote(name)
                for target in targets:
                    platforms.setdefault(target, {"signature": signature, "url": url})
    if not platforms:
        sys.exit(f"No signed package in {assets}")
    found = notes(version)
    return {
        "version": version,
        "notes": "\n".join(f"- {i}" for i in found.get("en", (None, []))[1]),
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
        # Read by the app (Update::raw_json), in the user's language
        "changelog": history(),
    }


def main(argv):
    if len(argv) == 3 and argv[1] == "check":
        check(argv[2])
    elif len(argv) == 3 and argv[1] == "body":
        sys.stdout.write(body(argv[2]))
    elif len(argv) == 5 and argv[1] == "latest":
        json.dump(latest(argv[2], argv[3], argv[4]), sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
