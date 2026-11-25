# Orgamnesia — notes for working on the code

A notes app (org-mode and Markdown pages, links, agenda, git) in Rust: a window
(Tauri + Leptos compiled to WebAssembly) and a terminal interface (ratatui,
`orgamnesia -nw`). The user speaks French: answer in French.

## Where things go

- `core/`: shared by everything, pure functions, no I/O. Parsing, editing
  commands, links, agenda, highlighting spans, keybindings registry, settings.
  **New logic goes here first**, with its unit tests.
- `backend/`: the open `Project` (pages in memory, backlinks, rename, search,
  git, spell, tags) without Tauri. `project.rs` holds the struct and the page I/O;
  `project/` the rest by theme (`queries`, `rename`, `trash`, `search`, `tests`…). Tested with `cargo test --workspace` from `src-tauri`.
- `src-tauri/src/commands/`: thin Tauri commands over `backend` (register new
  ones in `src-tauri/src/main.rs`, `generate_handler!`).
- `src-tauri/src/tui/`: the terminal interface. Actions in `input.rs`
  (`run_action`) and `commands.rs` (`command`, the prompt, page operations);
  `quick.rs` (quick open), `frames.rs` (popups, frames), `agenda.rs`, `git.rs`; editor in
  `editor.rs`, colours of the highlight classes in `highlight.rs`, tests in `tests.rs`.
- `src/`: the window (Leptos). `app.rs` (`run_app_action`) dispatches app
  actions, `components/editor/keys.rs` (`run_action`) the editor ones,
  `invoke.rs` wraps the Tauri commands, `actions/` the operations on pages and tabs,
  `state.rs` the shared state.
- `public/css/`: styles of the window, loaded in the order of `index.html`
  (`app.css` base and columns, `editor.css`, `panels.css` with the highlight
  classes, `dialogs.css`, `lists.css`).

**Orient by the module docs**: every file starts with `//!` saying what it holds.
`grep -m1 '^//!' -r core/src` (or another folder) maps a folder without reading it.

## File size

**No code file over 500 lines** (`scripts/check.sh` fails beyond). A file
nearing it is split by theme before anything is added: a child module
(`foo.rs` + `foo/bar.rs`, `pub use bar::*;` to keep the paths), tests in
`foo/tests.rs`.

- `src/i18n/*.rs`: the texts, one `("key", Lang::En|Fr) => "…"` line per
  language, by theme. Grep the key; add new keys as a pair of lines next to a related one.
- Do not read `target/`, `dist/`, the `Cargo.lock` files.

## A new feature

Parity: everything works in the window **and** the TUI. The usual path:

1. Logic and tests in `core/` (or `backend/` when it needs the project).
2. A keyboard action: `ActionDef` + presets (classic, emacs) in `core/src/keybindings.rs`,
   label key in `src/i18n.rs`.
3. Window: dispatch in `src/app.rs` or `components/editor/keys.rs`, UI in `src/components/`.
4. TUI: `tui/input.rs` / `tui/commands.rs`, a test in `tui/tests.rs`.
5. Docs: `docs/features.org` and `docs/fonctionnalites.org`; the todo list of
   `readme.org` / `readme.fr.org`.

## Checks

- `scripts/check.sh`: the CI (clippy `-D warnings` + tests for core, interface,
  backend). Run it before saying something is done. One crate while working:
  `(cd core && cargo test -q nom_du_test)`.

## Release

1. `scripts/bump.sh X.Y.Z`: the version everywhere, lockfiles, AUR PKGBUILD and
   `.SRCINFO` (in the `archlinux` Docker image), empty changelog sections.
2. Write the notes in `changelog.org` and `changelog.fr.org` (for users, plain text),
   then `python3 packaging/release/changelog.py check X.Y.Z`.
3. `scripts/check.sh`.
4. AUR publication: `packaging/arch-release/aur.org`. `orgamnesia-bin`
   (`packaging/arch-bin`) waits for the release's `.pkg.tar.zst`.

## Rules of the user

- Never commit unless asked; never push (the user does it). Single branch: `main`.
- Do not redirect `rtk`-wrapped output to a file (`head > f`): use Python or Write.
- A setting missing from a JSON/YAML file means its default, never an error.
