## What and why

<!-- What changes for the user, and why. Link the issue: "Closes #12". -->

## How it was checked

<!-- Tests added, platforms tried, what could not be verified. -->

## Checklist

- [ ] `cargo test --locked` passes
- [ ] A new feature has its path in `end_to_end` (`src/main.rs`); pure functions are tested in their module
- [ ] Interface texts use `tr("English", "Français")`
- [ ] For a visible change: help panel and `make docs`, the page of the guide (`book/src/`), and the README or `docs/index.html` if worth a line there
- [ ] No new dependency, or it was agreed in the issue
