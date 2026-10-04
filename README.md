# encre

A fast, minimal note-taking app. Notes are plain Markdown files in a folder you choose, styled as you type. Written in Rust with [GPUI](https://www.gpui.rs), the GPU-accelerated UI framework behind Zed.

![encre, light and dark](docs/screenshots/encre.png)

[Features](#features) · [Install](#install) · [First run](#first-run) · [Using encre](#using-encre) · [Markdown](#markdown) · [Notes and vault](#notes-and-vault) · [Compatibility](#compatibility) · [Troubleshooting](#troubleshooting) · [How it works](#how-it-works) · [Development](#development)

## Features

- **Opens instantly**: the window is on screen in about 75 ms on the author's machine.
- **Plain files**: one `.md` file per note, in a folder (the *vault*) you can sync with git, Syncthing or anything else. An existing Obsidian vault works as is.
- **Styled as you type**: headings grow, bold and italic render, markers stay visible but dimmed. The file on disk is always plain Markdown.
- **Lists that continue themselves**: `- `, `1. ` and `[] ` start a list, Enter continues it, numbering stays in order.
- **Links between notes**: `[[` suggests your notes; a link to a note that does not exist creates it. `#tags` filter the note list.
- **Keyboard first**: one palette to find, create and switch notes. No sidebar, no toolbar.
- **No save button**: notes are written to disk as you type, and named after their first line.
- Light and dark, following your system. English and French, following the system language.

## Install

| Your system | Go to |
|---|---|
| Arch Linux | [Arch](#arch-linux) |
| Ubuntu, Linux Mint, Debian | [Ubuntu, Linux Mint, Debian](#ubuntu-linux-mint-debian) |
| Fedora | [Fedora](#fedora) |
| Another Linux distribution | [Other distributions](#other-distributions) |
| macOS | [macOS](#macos) |
| Windows | [Windows](#windows) |

On Arch there is a prebuilt package; everywhere else encre is compiled on your machine, which takes a few minutes the first time. Compiling needs a recent stable Rust: install it with [rustup](https://rustup.rs) if `rustc --version` shows less than 1.88.

### Arch Linux

Every release ships a ready-to-install package (no compiler needed), which pacman then tracks as `encre`. The package is not signed, so download it first: given a URL, pacman looks for a `.sig` file and warns when there is none.

```bash
curl -LO https://github.com/alarboulletmarin/encre/releases/latest/download/encre-x86_64.pkg.tar.zst
sudo pacman -U ./encre-x86_64.pkg.tar.zst
```

To follow `main` instead, build the development package from a checkout (needs `cargo` and `git`, it conflicts with `encre`):

```bash
git clone https://github.com/alarboulletmarin/encre.git
cd encre/aur/encre-git
makepkg -si
```

Or build the latest tagged release yourself with `makepkg -si` from the repository root. Not on the AUR yet.

### Ubuntu, Linux Mint, Debian

1. Install the build tools and libraries:

   ```bash
   sudo apt install build-essential pkg-config libfontconfig-dev libfreetype-dev \
     libwayland-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libvulkan1
   ```

2. Install Rust with [rustup](https://rustup.rs); the `cargo` from apt is usually too old.

3. Build and install:

   ```bash
   git clone https://github.com/alarboulletmarin/encre.git
   cd encre
   make
   sudo make install
   ```

### Fedora

Same steps as above, with these packages (names not tested):

```bash
sudo dnf install gcc pkgconf-pkg-config fontconfig-devel freetype-devel \
  wayland-devel libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel vulkan-loader
```

### Other distributions

You need the development files of fontconfig, freetype, Wayland, libxcb and libxkbcommon (with its X11 part), a Vulkan driver for your GPU, and Rust. Then:

```bash
make                    # cargo build --release --locked
sudo make install       # PREFIX=/usr/local by default, PREFIX=/usr for a system-wide install
```

`make install` adds the `encre` command, an entry in your application menu and its icon. `sudo make uninstall` removes them.

### macOS

Needs Xcode (the full app, for the Metal shader compiler) and Rust.

```bash
git clone https://github.com/alarboulletmarin/encre.git
cd encre
cargo install --path . --locked
```

This installs the `encre` command in `~/.cargo/bin`. There is no `.app` bundle yet: start it from a terminal.

### Windows

Needs the Visual Studio C++ build tools with the Windows SDK, and Rust (MSVC toolchain).

```powershell
git clone https://github.com/alarboulletmarin/encre.git
cd encre
cargo install --path . --locked
```

This installs `encre.exe` in `%USERPROFILE%\.cargo\bin`. There is no installer yet.

### Upgrading

| Installed with | Upgrade |
|---|---|
| Arch, prebuilt package | run the two commands above again |
| Arch, `encre-git` | `git pull`, then `makepkg -si` in `aur/encre-git` |
| `make install` | `git pull`, then `make` and `sudo make install` |
| `cargo install` | `git pull`, then `cargo install --path . --locked` |

## First run

encre asks for a **vault**: the folder that holds your notes. Pick an existing folder, or create a new one from the file dialog. That is the only setup.

The choice is remembered, along with the last note you had open. Change vault at any time with `Ctrl + O`.

On Linux, give it a keyboard shortcut if you want it one key away: bind the command `encre` in your desktop settings (GNOME: Settings → Keyboard → Custom Shortcuts).

## Using encre

Start typing: the first line is the title of the note, and the name of its file.

On macOS, read `Cmd` for `Ctrl`, and `Alt` for `Ctrl` when moving by word.

| Key | Action |
|---|---|
| `Ctrl + P` | palette: find a note, create one, filter by `#tag` |
| `Ctrl + N` | new note |
| `Ctrl + O` | change vault |
| `Ctrl + Shift + C` | copy the whole note (also the icon at the bottom right) |
| `Ctrl + click` | open a `[[link]]`, a `#tag` or a URL |
| `F1`, `Ctrl + /` | the list of shortcuts, inside the app |
| `Ctrl + Q` | quit |

In the palette, type to search; `Enter` opens the selected note. If no note has that name, the last row creates it. Typing `#` lists the notes carrying a tag.

| Key | While editing |
|---|---|
| `Enter` | continue the list; on an empty item, leave the list |
| `Tab`, `Shift + Tab` | indent, outdent (the selected lines, or the current list item) |
| `Ctrl + Enter` | check or uncheck a task; turn a bullet into a task |
| `Ctrl + B`, `Ctrl + I` | bold, italic |
| `Ctrl + Z`, `Ctrl + Shift + Z` | undo, redo |
| `Ctrl + ←` `→`, `Ctrl + Backspace` | move, delete by word |
| `Ctrl + Home` `End` | start, end of the note |
| `Ctrl + A` `C` `X` `V` | select all, copy, cut, paste |

![The shortcut panel](docs/screenshots/help.png)

## Markdown

What you type is what is saved. encre only changes how it looks.

| Type | You get |
|---|---|
| `# `, `## `, `### ` | headings |
| `- `, `* `, `+ ` | a bullet list |
| `1. ` | a numbered list, renumbered as you add, remove or indent items |
| `[] ` or `- [ ] ` | a task; click the box or press `Ctrl + Enter` to check it |
| `> ` | a quote |
| `` ``` `` then `Enter` | a code block, closed for you |
| `---` | a divider |
| `**bold**`, `*italic*`, `~~struck~~`, `` `code` `` | inline styles |
| `[[Note name]]` | a link to another note, with suggestions as you type |
| `#tag` | a tag, usable as a filter in the palette |
| `https://…` | a link, opened in your browser |

Tables, images and footnotes are kept as you typed them, without special rendering.

## Notes and vault

- A vault is an ordinary folder. Notes in subfolders are found too; hidden folders (`.git`, `.obsidian`…) are ignored. New notes are created at the top of the vault.
- A note is saved shortly after you stop typing, and when you switch note or quit. Saving is atomic: a crash never leaves a half-written file.
- A new note is named after its first line: `# Groceries` becomes `Groceries.md`, and the file is renamed when you change that line. A note whose file name did not already match its first line (typical of an existing vault) is never renamed.
- `[[Groceries]]` finds the note by file name, in any subfolder, ignoring case.
- The vault and the last open note are remembered in a small text file:

  | System | File |
  |---|---|
  | Linux | `~/.config/encre/config` |
  | macOS | `~/Library/Application Support/encre/config` |
  | Windows | `%APPDATA%\encre\config` |

## Compatibility

| System | Status |
|---|---|
| Linux, GNOME on Wayland | first-class: this is what encre is built and used on (Arch Linux) |
| Linux, other desktops, Wayland or X11 | should work; the desktop draws the title bar where it offers one. Not tested |
| macOS, Windows | built and tested by CI on every commit; not used day to day by the author |

- **Linux** needs a working Vulkan driver (Mesa or the vendor one). On GNOME under Wayland, which draws no title bar for applications, encre draws its own; elsewhere it uses the system one.
- **Language**: French if the system language is French, English otherwise. On Windows it is always English for now.
- **Fonts**: encre picks Inter, Noto Sans, Segoe UI or Helvetica Neue, whichever is installed, and JetBrains Mono, Menlo or Consolas for code.

## Troubleshooting

**The window takes two seconds to open (Linux).** Another Vulkan driver is slow to load. encre already skips the NVIDIA one when the machine has no NVIDIA card. To check what remains, run `VK_LOADER_DEBUG=driver encre` in a terminal.

**The window does not open, with an error about a surface or an adapter.** No usable Vulkan driver: install the one for your GPU (`vulkan-intel`, `vulkan-radeon`, `nvidia-utils`… on Arch; `mesa-vulkan-drivers` on Ubuntu).

**Bold text is not bold.** The font in use is a variable font, which the text engine cannot embolden. Install Inter or Noto Sans.

**A note was not renamed after I changed its title.** Either another note already has that name, or the file name did not match the first line to begin with, see [Notes and vault](#notes-and-vault).

**I edited a note in another program while it was open in encre.** encre does not watch files: it keeps its own version and writes it back on the next edit. Switch to another note and back to reload it from disk.

**Limits.** No way to delete a note from the app yet (delete the file). Renaming a note does not update the `[[links]]` pointing to it. No full-text search: the palette matches note names and tags.

## How it works

- **Editor**: a custom text element drawn directly with GPUI's text system. Each line is classified (heading, list item, quote, code…) and shaped with its own size and style runs; the text itself is never transformed.
- **Typing rules** (`src/markdown.rs`): pure functions decide what Enter and Tab do on a line and renumber the list around the cursor. They are unit-tested without any UI.
- **Vault** (`src/vault.rs`): the note list and its tags are indexed off the UI thread when the vault opens, then kept up to date on each save.
- **Palette**: substring matches first, ranked by position, then subsequence matches; ties keep the most recently edited note first.
- **Startup**: the last note is read before the first frame, so the window appears with its content. Icons are embedded in the binary.
- **Window**: encre asks the system for a title bar and draws its own, with shadow and resize edges, only when the compositor provides none.

## Development

```bash
cargo run                 # debug build
cargo run --release       # what gets installed
make test                 # cargo test --locked
```

The tests include an end-to-end run driven by simulated keystrokes (typing, lists, autosave, palette, links), using GPUI's test platform: no display needed.

`Cargo.lock` started as a copy of the one GPUI 0.2.2 was published with: newer versions of some of its dependencies no longer build together. Update dependencies one at a time.

Releasing (maintainer): `scripts/release.sh X.Y.Z` bumps the version, tags, pushes, creates the GitHub release (the Arch package is attached by `.github/workflows/arch-package.yml`) and pins the PKGBUILD checksum.

## License

MIT
