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
```

The system packages needed on each platform are listed in the README (*Install*). The tests need no display: they run on GPUI's test platform.

## Rules of the code base

- **GPUI is pinned to 0.2.2** and its API moves. Check a signature in the source of that version, not in the documentation of `main`.
- **`Cargo.lock` is fragile**: it started as the one GPUI 0.2.2 was published with. No global `cargo update`; one dependency at a time. No new dependency without an agreement in the issue.
- **Comments are in French**; commit messages, the README and issues are in English.
- **Interface texts** always go through `tr("English", "Français")`. The tests run in French.
- **Nothing is destroyed or overwritten on disk**: atomic writes (`vault::write`), the `.trash` folder, and a refusal when a name is taken.
- **A deliberate shortcut** carries a `// ponytail:` comment that names its limit and what would come next.

## Tests

- Pure functions are tested in their own module.
- Every user path goes through `end_to_end` in `src/main.rs`, with simulated keystrokes. A new feature adds its path there.

## Documentation

A visible feature updates the README, the help panel (`help_sections` in `src/main.rs`) and the feature list of `docs/index.html`. Do not add screenshots.

## Commits and pull requests

- One branch per change, from `main`: `feat/…` or `fix/…`.
- Subjects start with `feat:`, `fix:`, `docs:`, `test:`, `build:` or `chore:`, in the imperative; the body explains why.
- One commit per coherent step, tests passing at each commit.
- The history stays linear: rebase on `main`, no merge commits.
- CI builds and tests on Linux, macOS and Windows; it has to pass.
