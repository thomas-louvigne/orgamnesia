# Notes project (Orgamnesia)

<!-- Copy this file as CLAUDE.md (or AGENTS.md) at the root of your notes folder,
     then adapt it: the folders, the TODO states, your own conventions. -->

This folder is a personal wiki edited with Orgamnesia. Every note is a plain text
page; there is no database. Treat it as the user's second brain: read before
writing, keep their wording, never delete a page or a paragraph unless asked.

## Layout

- `pages/`: the pages, `.org` (org-mode) or `.md` (Markdown); both mix. Without a
  `pages/` folder, the pages sit at the root.
- `journals/`: one page per day, named by its date (`2026-11-15.org`).
- `assets/`: images and files the pages show (`[[file:../assets/a.png]]`,
  `![text](../assets/a.png)`).
- `.trash/`: deleted pages. Ignore it.

A page is named by its file name without the extension: `Garden.org` is the page
`Garden`. New pages take the format of the others (ask when unsure).

## Links

- `[[Page]]` or `[[Page][text shown]]` link to a page, in both formats. Matching
  ignores case and accents: `[[élody]]` leads to `Elody`.
- `[[Page::*Title]]` leads to a headline, `[[Page::target]]` to a `<<target>>`.
- `#keyword` links to the page of that name.
- A link to a page that does not exist yet is fine: it is a page to write.
- Renaming a page: rewrite every link to it in the other pages.
- Backlinks (the pages linking to a page) are found by searching `[[Name` and
  `#Name`.

## Tags

- Org-mode: `#+FILETAGS: :a:b:` tags the whole page; `* Title :a:b:` tags a
  headline and its sub-headlines.
- Markdown: `tags: [a, b]` in the YAML front matter (between two `---` lines).
- A tag also links to the page of the same name.

## To do

- Headlines carry a state: `* TODO Title` (org-mode), `## TODO Title` (Markdown).
  States, in order: TODO → DOING → HANGUP → DONE. A page may define its own with a
  `#+TODO:` line.
- Checkboxes: `- [ ] task`, `- [x] done`.
- `SCHEDULED: <2026-11-15>` and `DEADLINE: <2026-11-15>` under a headline date it.

## How to help

- Answer from the pages: search them, quote the page names as `[[links]]`.
- Add to the page of the day for quick notes, unless told where.
- Link new notes to the existing pages they talk about.
- Keep the format of each page (org-mode stays org-mode, Markdown stays Markdown).
- Git may sync this folder: do not commit or push unless asked.
