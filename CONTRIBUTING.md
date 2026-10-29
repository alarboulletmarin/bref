# Contributing to Bref

Thank you for taking the time. Bref is a small app with a narrow goal: fast, minimal Markdown notes in a single binary. Contributions that keep it that way are welcome.

By taking part you agree to the [code of conduct](CODE_OF_CONDUCT.md). For a security problem, follow the [security policy](SECURITY.md) instead of opening an issue.

## Before you write code

- **A bug**: open an issue with the version, the system and the steps to reproduce.
- **A feature**: open an issue first and wait for an answer. Many ideas are declined to keep the app small; it is better to know before the work is done.
- **A typo, a small fix**: a pull request is enough.

## Building and testing

```bash
cargo run                 # debug build
cargo run --release       # what gets installed
cargo test --locked       # or: make test
make docs                 # rewrite the shortcuts page, build the user guide (needs mdbook)
```

The system packages needed on each platform are listed in the [install guide](book/src/install.md). The tests need no display: they run on GPUI's test platform. They include an end-to-end run driven by simulated keystrokes.

Ignored benchmarks measure vaults of thousands of notes, CSV files of 300,000 rows and notes with a table of 300,000 rows. Run them with `cargo test --release --locked -- --ignored --nocapture`; each reads its parameters from environment variables named in its doc comment. Their results are in the guide's [Performance](book/src/performance.md) page.

## Rules of the code base

- **Read [ARCHITECTURE.md](ARCHITECTURE.md) first**: the layers (`logic`, `platform`, `ui`, `views`, `shell`), which way they depend, and where each feature lives. A file that passes 1,500 lines gets split by subject.
- **GPUI is pinned to 0.2.2** and its API moves. Check a signature in the source of that version, not in the documentation of `main`.
- **`Cargo.lock` is fragile**: it started as the one GPUI 0.2.2 was published with. No global `cargo update`; one dependency at a time. No new dependency without an agreement in the issue.
- **Code, comments**, commit messages, the README and issues are in English.
- **Interface texts** always go through `tr("English", "Français")`. The tests run in French.
- **Nothing is destroyed or overwritten on disk**: atomic writes (`vault::write`), the `.trash` folder, and a refusal when a name is taken.
- **A deliberate shortcut** carries a `// ponytail:` comment that names its limit and what would come next.

## Tests

- Pure functions are tested in their own module.
- Every user path is tested end to end in `src/shell/tests/`, with simulated keystrokes: a chapter of `end_to_end/`, or a journey of its own. A new feature adds its path there.

## Documentation

Each kind of reader has one place:

| Reader | Place |
|---|---|
| someone deciding whether to try Bref | `README.md` and the website, `docs/index.html`: what it is, why, how to install. Short. |
| a user | the user guide, `book/src/` (an [mdBook](https://rust-lang.github.io/mdBook/)), one page per subject |
| a contributor | `CONTRIBUTING.md`, `ARCHITECTURE.md` |

A visible feature updates:

- the help panel (`help_sections` in `src/shell/keymap.rs`), then `make docs`, which rewrites `book/src/shortcuts.md` from it. The test `shortcuts_page_is_current` fails until the page matches;
- the page of the guide on its subject, or a new page listed in `book/src/SUMMARY.md`;
- the README and the feature list of `docs/index.html`, only for a feature worth a line there.

Write the guide in short sentences, one idea per bullet, keys in backticks, button and command names in bold. The README stays under about 120 lines: details go to the guide. Do not add screenshots; the GIF of the README is recorded by `scripts/record-demo.sh`.

CI builds the guide and checks every link (`.github/workflows/docs.yml`); pushes to `main` publish it with the website.

## Commits and pull requests

- One branch per change, from `main`: `feat/…` or `fix/…`.
- Subjects start with `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `build:` or `chore:`, in the imperative; the body explains why.
- One commit per coherent step, tests passing at each commit.
- The history stays linear: rebase on `main`, no merge commits.
- CI builds and tests on Linux, macOS and Windows; it has to pass.

## Releasing (maintainer)

`scripts/release.sh X.Y.Z` bumps the version, tags, pushes, creates the GitHub release (the Arch package is attached by `.github/workflows/arch-package.yml`) and pins the PKGBUILD checksum.
