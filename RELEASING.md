# Releasing Bref

A version is published only after the Arch package, universal macOS DMG, Windows installer and Windows ZIP have been built and checked. Preparation creates a **draft**; the final workflow attaches the complete set of downloads, verifies their remote names and sizes, and publishes it. A failed or cancelled build leaves the draft unpublished. The pushed Git tag and its source remain publicly visible.

## Prerequisites

Run preparation on Arch Linux (Bash, Cargo, Git, curl, sha256sum, makepkg and GitHub CLI). Python 3.11 or newer runs the publication gate on the GitHub runner; the preparation script itself uses Bash. Authenticate `gh` with repository write access and permission to dispatch workflows. CI uses its ordinary `GITHUB_TOKEN`, with `contents: write` only in the draft-validation and final publication jobs. Draft visibility requires push access; packaging jobs keep read permission.

Merge the new release scripts and workflows into `main` before making the first release with this process. `release-native.yml` must be available on the default branch and on `main`; the tag being built must contain `scripts/publish-release.py` and the packaging files. Older tags without these files cannot use this pipeline unchanged.

Start with a clean tracked working tree on `main`, synchronized with `origin/main`, and green application CI. The generated `aur/bref-git/PKGBUILD` may differ; the script preserves it. Untracked files are not staged. The preparation script runs local application tests; it does not independently query remote CI verdicts. Check those before preparation.

## Prepare and publish

```bash
# Safe local rehearsal, on any clean branch. Never tags, pushes or contacts GitHub.
# Cargo runs offline: dependencies must already be cached.
DRY_RUN=1 scripts/release.sh X.Y.Z

# From synchronized main: bump, test, tag/push, pin checksums, create draft,
# then explicitly dispatch the Native release workflow.
scripts/release.sh X.Y.Z

# Optional: supply the exact release notes in a local file.
scripts/release.sh X.Y.Z /path/to/notes.md
```

Notes come from the matching changelog section when present, otherwise from commit subjects since the previous tag. A first release without previous tags is supported. A supplied notes file takes precedence.

Simulation restores the managed version and package files on success, test failure or interruption. Normal preparation also restores an uncommitted phase if it fails. Once a version or checksum commit has succeeded, it is retained for recovery; the script does not rewrite committed history.

The local script returns after the workflow has been dispatched, **before publication finishes**. Follow **Actions → Native release**, or use:

```bash
gh run list --workflow release-native.yml
gh run watch RUN_ID --exit-status
```

The orchestrator validates the numeric `vX.Y.Z` tag, its manifest version and the draft state. Reusable packaging jobs produce artifacts in that workflow run without changing a release. The final job runs only after every required job succeeds, rejects missing, empty or unexpected files, generates `SHA256SUMS`, uploads all downloads to the draft and checks their exact names and sizes remotely. Only then does it publish. Remote byte content is not independently downloaded and rehashed by this gate.

## Recover without a new version

If local preparation stopped after committing or tagging, inspect its error and use:

```bash
RESUME=1 scripts/release.sh X.Y.Z
```

This requires the existing local tag, a checkout still carrying that version and a non-divergent `main`. It preserves the tag and existing draft notes, finishes any missing checksum/push/draft step and dispatches the workflow again. A public release is refused before local mutation. Authentication or API failure is an error, not evidence that a release is absent.

If preparation finished and only a build or upload failed, fix the environment or transient issue and **Re-run failed jobs** in the same Actions run. Successfully uploaded workflow artifacts are retained for seven days. Once they expire, rerun all jobs, or dispatch a fresh complete build:

```bash
gh workflow run release-native.yml --ref main -f tag=vX.Y.Z
```

Runs for the same tag are serialized. Each fresh run downloads only its own packaging artifacts. A failed upload can be retried against the draft; all expected assets are replaced and checked before publication. Unexpected stale release assets must be reviewed and removed explicitly before retrying. The workflow refuses to overwrite a release that is already public.

Do not move a released tag to fix application code. Use a new version. Old public releases can still be inspected and downloaded normally; this workflow intentionally does not rebuild or overwrite them.

## Build packages without publishing

```bash
gh workflow run release-binaries.yml --ref main -f tag=develop
gh workflow run arch-package.yml --ref main -f tag=vX.Y.Z
```

Standalone packaging workflows leave their output in Actions artifacts. They neither attach files to a release nor publish one. The previous `attach` input has been removed.

The Arch job retains `--skipinteg`: the tagged PKGBUILD predates the post-tag source checksum pinned on `main`. It builds the GitHub source archive for the selected tag and inspects the package metadata and installed executable path. AUR publication remains a separate maintainer operation. This change does not claim reproducible binary output or a signed distribution.

## Test the release machinery

```bash
bash -n scripts/release.sh
python3 scripts/test-release.py
python3 scripts/test-publish-release.py
```

The first Python suite uses temporary repositories and mocked Cargo/GitHub/download tools. The second checks the publication gate, including incomplete builds, public releases, failed uploads and retry. Neither suite publishes or modifies the real repository history. **Release checks** runs both on relevant pushes and pull requests.

These tests exercise orchestration, not a real Windows/macOS build or a live GitHub workflow. The first hosted run remains necessary to validate runner dependencies, permissions and packaging end to end.

The explicit dispatch avoids depending on [draft release events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release) or a recursive event generated by `GITHUB_TOKEN`. Publication uses [GitHub CLI draft editing](https://cli.github.com/manual/gh_release_edit).
