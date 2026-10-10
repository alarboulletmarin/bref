# Bref

A fast, minimal note-taking app. Notes are plain Markdown files in a folder you choose, styled as you type. Written in Rust with [GPUI](https://www.gpui.rs), the GPU-accelerated UI framework behind Zed.

![Bref, light and dark](docs/screenshots/bref.png)

[Features](#features) · [Install](#install) · [First run](#first-run) · [Using Bref](#using-bref) · [Navigation](#navigation) · [Appearance](#appearance) · [Markdown](#markdown) · [Diagrams](#diagrams) · [CSV tables](#csv-and-tsv-tables) · [Syncing](#syncing-a-vault) · [Notes and vault](#notes-and-vault) · [Updates](#updates) · [Compatibility](#compatibility) · [Troubleshooting](#troubleshooting) · [How it works](#how-it-works) · [Development](#development)

## Features

- **Opens instantly**: the window is on screen in about 75 ms on the author's machine.
- **Plain files**: one `.md` file per note, in a folder (the *vault*) you can sync with git, Syncthing or anything else. An existing Obsidian vault works as is, YAML front matter included.
- **Styled as you type**: headings grow, bold and italic render, markers stay visible but dimmed. The file on disk is always plain Markdown.
- **Lists that continue themselves**: `- `, `1. ` and `[] ` start a list, Enter continues it, numbering stays in order.
- **Slash commands**: type `/` for a heading, a list, a colored panel, a code block, a formula or a table, whose size you pick on a grid. What lands in the note is plain Markdown.
- **Tables like on the web**: cells wrap inside their column, `Tab` moves from cell to cell, `Enter` adds a row, two `+` buttons add a row or a column, and pasting a table from a web page, a spreadsheet or Markdown just works.
- **Kanban boards**: type `kanban` in the palette and choose **New kanban board**: a note opens as a board with three columns. Everything is done on the board: **+ Add a card** opens a card you type in (`Enter` adds it and opens the next one, `Esc` stops), a double click rewrites a card or a column title in place (emptied, a card is removed), the `+` at the right adds a column. Pick a card up and it follows the pointer, while a dashed slot opens where it would land: between two cards, or at the bottom of a column; click its box to tick it. Each gesture is undone with `Ctrl+Z`, right on the board. A card being written is a real line of text: arrows, word by word with `Ctrl`, selection with `Shift`, `Ctrl+Backspace`, copy and paste, a click to place the cursor; so are the palette and a cell of a CSV table. Columns narrow to fit the window. Under the board the note stays plain Markdown, one `## heading` per column and one task per card, in the layout of the Obsidian Kanban plugin (the key `kanban: true` of its front matter is what makes it open as a board; typing that key into a note leaves you in the text); the button at the bottom right, left of the copy button, shows that text and comes back.
- **List pages**: right click a folder in the tree, or a tag in the tags panel, then **Show as a list**: its notes become the rows of a table, and the keys of their front matter (`author:`, `rating:`…) its columns. Type to filter the rows (`author:austen` looks in one column only, `Esc` clears); click a column title to sort by it, again to reverse; click the name of a note to open it. Click any other cell to rewrite it (`Enter` confirms, `Esc` gives up): only that key changes in the front matter of the note, added if it was missing, removed if you empty it, and nothing else in the file.
- **Icons**: right click a note, a file or a folder in the tree, then **Icon…**: pick one of forty in the grid, or the cross to remove it. The icon shows before the name in the panel, follows a rename or a move, and is kept in `.bref-icons` at the root of the vault, so it travels with it.
- **Comments**: `Ctrl + Shift + M` annotates a word, a line or the line of an image without changing its text. The comment is kept in the note itself as [CriticMarkup](https://criticmarkup.com/), `{==the text==}{>>the comment<<}`, so the file stays plain Markdown: the text is highlighted, the comment follows it, faded. A panel to the right of the note lists its comments: click one to go to it, click its check mark to resolve it. The speech bubble at the bottom right of the note, next to the copy button, closes the panel and opens it again (so does its cross); `comments` in the palette lists the comments to jump to one from the keyboard.
- **Links between notes**: `[[` suggests your notes; a link to a note that does not exist creates it. `@` suggests days (`@today`, `@tomorrow`, `@monday`, `@2026-10-09`) and links to the daily note of that day. `#tags` filter the note list.
- **Backlinks**: the notes that link to the open one, each with the line that holds the link.
- **Sync in one action**: `Ctrl + Shift + S` commits, receives, merges and sends a vault kept in git, in the background; a real conflict keeps both versions as two notes, never as markers in your text. A vault can also simply live in a cloud folder.
- **Two panes side by side**: `Ctrl + \` opens a second pane, to write a note while reading another, or to keep a table next to a note. Each pane shows anything Bref can show; a file dragged from the tree opens in the pane you drop it on.
- **Back and forward**: `Alt + Left` and `Alt + Right` (`Cmd + [` and `Cmd + ]` on macOS), the side buttons of the mouse or the two arrows at the top of the rail walk through what you opened, notes, pictures, diagrams, tables and list pages alike, and land where you were: same line, same scroll.
- **Four ways around your notes**: the vault as a tree, the notes you opened last, a graph of the links between them, and your tags, in a panel that folds down to a thin rail of icons.
- **Diagrams drawn by hand**: shapes, arrows that hold on to them, UML boxes, on a canvas like Excalidraw's. Each diagram is an SVG file in your vault, shown in your notes.
- **CSV and TSV files, edited in place**: a `.csv` or `.tsv` in the vault opens in a grid. The encoding (UTF-8, UTF-16, ISO-8859-1, Windows-1252) and the delimiter (comma, semicolon, tab, pipe…) are detected and can be changed; select cells with the keyboard or the mouse, copy, paste, export, and what you change is written straight into the file. A million rows are parsed in under a tenth of a second on the author's machine.
- **Keyboard first**: one palette to find, create and switch notes, and a shortcut for every view. No toolbar.
- **Shortcuts you can change**: the help panel (`F1`) lists every shortcut, searches them as you type, and lets you give any of them other keys. The menus show, beside each action, the shortcut that does the same. A combination already in use says by what, and is only taken when you ask.
- **No save button**: notes are written to disk as you type, and named after their first line.
- **Daily note**: one key opens the note of the day, or creates it.
- **Capture in one line**: one shortcut of your desktop (set up from the palette, `Super + Shift + N` on GNOME) opens a single line anywhere, anytime: type, `Enter`, it is at the bottom of today's note and the window is gone. See [Capture](#capture).
- **A blank page that remembers**: an empty note shows, under the line you write on, today's date (a click opens the note of the day), the keys that matter, and what to read again: the daily notes of a week, a month and a year ago, and one note you have not touched for a month or more, a different one each day. A click opens any of them; the first key you type clears the page.
- **Word count**: the bottom of the note shows its words and characters, or those of the selection.
- **Yours to dress**: a dozen themes (Dracula, Nord, Gruvbox, Catppuccin, Tokyo Night, Solarized…) previewed as you browse them, IBM Plex Sans and Plex Mono built in (nothing to install), any installed font if you prefer, and the text size you like.
- Light and dark, following your system, until you pick a theme. English and French, following the system language.

## Install

| Your system | Go to |
|---|---|
| Arch Linux | [Arch](#arch-linux) |
| Ubuntu, Linux Mint, Debian | [Ubuntu, Linux Mint, Debian](#ubuntu-linux-mint-debian) |
| Fedora | [Fedora](#fedora) |
| Another Linux distribution | [Other distributions](#other-distributions) |
| macOS | [macOS](#macos) |
| Windows | [Windows](#windows) |

On Arch, macOS and Windows there is a ready-made download; everywhere else Bref is compiled on your machine, which takes a few minutes the first time. Compiling needs a recent stable Rust: install it with [rustup](https://rustup.rs) if `rustc --version` shows less than 1.88.

### Arch Linux

Every release ships a ready-to-install package (no compiler needed), which pacman then tracks as `bref`. The package is not signed, so download it first: given a URL, pacman looks for a `.sig` file and warns when there is none.

```bash
curl -LO https://github.com/alarboulletmarin/bref/releases/latest/download/bref-x86_64.pkg.tar.zst
sudo pacman -U ./bref-x86_64.pkg.tar.zst
```

To follow `main` instead, build the development package from a checkout (needs `cargo` and `git`, it conflicts with `bref`):

```bash
git clone https://github.com/alarboulletmarin/bref.git
cd bref/aur/bref-git
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
   git clone https://github.com/alarboulletmarin/bref.git
   cd bref
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

`make install` adds the `bref` command, an entry in your application menu and its icon. `sudo make uninstall` removes them.

### macOS

Download [`bref-macos.dmg`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-macos.dmg) (Apple Silicon and Intel, macOS 11 or later), open it and drag Bref to Applications.

The app is not notarized (that needs a paid Apple Developer account), so macOS blocks the first launch. Open **System Settings**, **Privacy & Security**, scroll down and click **Open Anyway**. Or run this once in a terminal:

```bash
xattr -dr com.apple.quarantine /Applications/Bref.app
```

To compile it yourself instead you need Xcode (the full app, for the Metal shader compiler) and Rust:

```bash
git clone https://github.com/alarboulletmarin/bref.git
cd bref
cargo install --path . --locked
```

This installs the `bref` command in `~/.cargo/bin`, without the `.app` bundle: start it from a terminal.

### Windows

Download [`bref-windows-x86_64-setup.exe`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-windows-x86_64-setup.exe) and run it (Windows 10 or 11, 64-bit). It installs for your user only, with no administrator rights, adds a Start menu entry and an uninstaller. If you would rather install nothing, take [`bref-windows-x86_64.zip`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-windows-x86_64.zip), unzip it anywhere and run `bref.exe`.

The files are not signed (a code-signing certificate costs a few hundred euros a year), so SmartScreen may show "Windows protected your PC": click **More info**, then **Run anyway**.

To compile it yourself instead you need the Visual Studio C++ build tools with the Windows SDK, and Rust (MSVC toolchain):

```powershell
git clone https://github.com/alarboulletmarin/bref.git
cd bref
cargo install --path . --locked
```

This installs `bref.exe` in `%USERPROFILE%\.cargo\bin`.

### Upgrading

| Installed with | Upgrade |
|---|---|
| Arch, prebuilt package | the **Update** button of the banner, or run the two commands above again |
| macOS, `.dmg` | the **Update** button of the banner, or download the new `.dmg` and drop Bref on Applications again |
| Windows, installer | the **Update** button of the banner, or run the new installer: it upgrades in place |
| Windows, `.zip` | unzip the new version over the old one |
| Arch, `bref-git` | `git pull`, then `makepkg -si` in `aur/bref-git` |
| `make install` | `git pull`, then `make` and `sudo make install` |
| `cargo install` | `git pull`, then `cargo install --path . --locked` |

Bref was called encre until 0.1.4. Your notes are untouched, and the settings saved under the old name are still read: they are written under the new one the next time they change.

| Installed encre with | Before upgrading |
|---|---|
| Arch, either package | nothing: the `bref` packages replace `encre` and `encre-git` |
| `make install` | `sudo make uninstall` in the old checkout, before `git pull` |
| `cargo install` | `cargo uninstall encre` |

## First run

Bref asks for a **vault**: the folder that holds your notes. One click on **Start in ~/Documents/Notes** creates it there (in your home folder if you have no `Documents`; a folder already there is opened as it is), or **Open another folder…** picks an existing one, an Obsidian vault included. That is the only setup.

The choice is remembered, along with the last note you had open. Change vault at any time with `Ctrl + O`.

On Linux, give it a keyboard shortcut if you want it one key away: bind the command `bref` in your desktop settings (GNOME: Settings → Keyboard → Custom Shortcuts).

## Using Bref

Start typing: the first line is the title of the note, and the name of its file.

On macOS, read `Cmd` for `Ctrl`, and `Alt` for `Ctrl` when moving by word.

The bottom right of the note shows its number of words and characters; with a selection, the words and characters selected. Characters are counted as written, Markdown markers included, line ends excluded.

| Key | Action |
|---|---|
| `Ctrl + P` | palette: find a note by name or text, create one, filter by `#tag` |
| `Ctrl + F`, `Ctrl + H` | [find in the note](#find-and-replace), find and replace |
| `Ctrl + Shift + O` | outline: the headings of the note, to jump to one (also `Outline of the note` in the palette) |
| `Ctrl + Shift + F` | search the text of the whole vault: every line that holds what you type, whatever its case; `Enter` opens the note with the match selected |
| `Ctrl + N` | new note |
| `Ctrl + J` | today's note: `2026-10-09.md`, opened wherever it is in the vault, or created with the date as its title (also `Today's note` in the palette) |
| `Ctrl + E`, `Ctrl + R`, `Ctrl + G`, `Ctrl + T` | [navigation panel](#navigation): vault tree, recent notes, graph, tags |
| `Ctrl + M` | navigation panel on the whole window, and back |
| `Ctrl + Shift + D` | new [diagram](#diagrams) |
| `Ctrl + O` | change vault |
| `Ctrl + D` | select the word under the cursor; again, add a cursor on its next occurrence. What you type, delete or move then applies to every cursor, and one undo undoes it for all. `Esc` goes back to one cursor |
| `Alt + click` | add a cursor where you click (not in a table) |
| `Ctrl + K` | make the selection a link, `[text](address)`; on a link, select its address to change it |
| `Ctrl + +`, `Ctrl + -`, `Ctrl + 0` | bigger, smaller, default text size |
| `Ctrl + Shift + C` | copy the code block the cursor is in, otherwise the whole note (also the icon at the bottom right, for the note) |
| `Ctrl + click` | open a `[[link]]`, a `#tag`, a URL or a `[text](address)` link |
| `Ctrl + V` | over a selection, a pasted address makes it a link |
| Right click | in a note, a menu: open the link under the pointer or copy its address, cut, copy, paste, bold, italic, make or edit a link, comment |
| `Ctrl + Shift + M` | comment the selection (without one, the line); on a comment, resolve it: the markup goes, the text stays |
| `F1`, `Ctrl + /` | the list of shortcuts, inside the app: type to search it, by key or by effect (`ctrl+p`, `graph`), `Esc` empties the search, then closes; click the keys of a row to change them (see [Changing a shortcut](#changing-a-shortcut)); at its foot, the version of Bref and links to its source and to its Ko-fi page |
| `Alt + Left`, `Alt + Right` | back to what was shown before, and forward again (`Cmd + [`, `Cmd + ]` on macOS); also the back and forward buttons of the mouse, the two arrows at the top of the rail, and **Back** / **Forward** in the palette. Like a browser: opening something after going back drops what was ahead. A note only previewed (arrows in the tree, a click in the graph) does not count, a renamed file is followed, a deleted one is skipped. The history is not kept between two launches: the recent notes are. |
| `Ctrl + \` | a second pane, to the right: it opens on the palette, to choose what goes there. `Ctrl + 1` and `Ctrl + 2` move to the left and to the right pane, `Ctrl + W` closes the one that has the focus. In the palette, `Ctrl + Enter` opens the chosen note in the other pane; so do **Open to the side** in the menu of the tree and `Ctrl + Shift + click` on a `[[link]]`. Drag a file from the tree onto a pane to open it there, or onto the right half of a single pane to open a second one; a folder opens as its list page. The border between the two can be dragged, a double click puts it back in the middle. The palette, the tree and the shortcuts act on the pane that has the focus, marked by a line at its top. A note already shown in one pane is not opened twice: the focus moves to it. The layout comes back at the next launch. |
| `Ctrl + Shift + S` | sync the vault with git (see [Syncing a vault](#syncing-a-vault)); also **Sync the vault** in the palette |
| `Ctrl + Q` | quit |

In the palette, type to search; `Enter` opens the selected note. Notes whose name matches come first (CSV tables, pictures and diagrams are found by their file name too, `contacts.csv`), then the notes whose text contains every word you typed (from two letters), with the line where the first word appears. If no note has that name, the last row creates it. Typing `#` lists the notes carrying a tag. Commands are found by any of their words, accents ignored: typing `folder` offers *New folder*, and `table` offers the settings of the CSV table on display (delimiter, encoding, header row, copy as, export); `diagram`, `theme`, `font` and `update` offer theirs too.

| Key | While editing |
|---|---|
| `Enter` | continue the list; on an empty item, leave the list |
| `Tab`, `Shift + Tab` | indent, outdent (the selected lines, or the current list item) |
| `Ctrl + Enter` | check or uncheck a task or a `[ ]` box; turn a bullet into a task; in a table cell, add a box |
| `Ctrl + B`, `Ctrl + I` | bold, italic |
| `Ctrl + Z`, `Ctrl + Shift + Z` | undo, redo |
| `Ctrl + ←` `→`, `Ctrl + Backspace` | move, delete by word |
| `Ctrl + Home` `End` | start, end of the note |
| `Ctrl + A` `C` `X` `V` | select all, copy, cut, paste |
| Double click, triple click | select a word, a line; keep dragging to extend by words, by lines |

### Changing a shortcut

The shortcuts in this README are the default ones; the help panel (`F1`) always shows the ones in effect.

- Click the keys of a row, or move to it with `Down` and press `Enter`: the row waits, and the next combination you press becomes its shortcut, at once. `Esc` cancels, `Backspace` leaves the action with no shortcut.
- A combination already used where the action applies is not taken: the row says by which action, and `Enter` (**Replace**) gives it to the new one, the other being left without a shortcut. The same keys in two views that never meet (a table and a diagram) are not a conflict.
- A key alone that types text, and the keys the text and the views need (`Enter`, `Tab`, the arrows…), are refused, with the reason. A combination your system usually keeps for itself is accepted with a warning.
- A changed row is highlighted and has a **default** button; **Reset all**, at the top of the panel, puts every shortcut back.
- The right-click menus and the palette show the keys in effect next to each action that has a shortcut, so a change made here shows there too.
- Rows without a button are not key bindings (a click, a palette command, something you type): the search finds them, but they cannot be changed.

Only what you changed is kept, one `action=keys` line each, in the `keys` file of the configuration folder (`~/.config/bref/` on Linux), next to `settings`. `secondary` stands for `Cmd` on macOS and `Ctrl` elsewhere. A line that cannot be used (unknown action, unreadable keys, keys already taken) is ignored and reported at launch; the default stays.

### Capture

An idea rarely comes while Bref is open. `bref --capture` opens a window that is one line and nothing else, as fast as the app itself: type, press `Enter`, and the line is added to the note of the day as a list item (`- call Léa`; type `- [ ] ` first to make it a task). The note is created if the day has none yet, and found wherever you keep your daily notes in the vault. `Esc`, or a click beside the line, closes the window without writing anything.

To have it under a key, open the palette (`Ctrl + P`), type `capture` and choose **Set up the quick capture shortcut**:

- **GNOME**: done. Bref adds the shortcut `Super + Shift + N` to your keyboard settings and tells you so. If those keys are already used, it takes none and says so: give it the keys you like in Settings › Keyboard › Custom Shortcuts, under *Bref: capture*, where you can change them at any time. Asking again never adds a second one.
- **Elsewhere** (KDE, macOS, Windows…): an app cannot take a system-wide shortcut by itself, so Bref copies the command to the clipboard, ready to paste into a new shortcut of your system settings.

Without any setup, on Linux, right click the icon of Bref in the dock or the app grid: **Quick capture** is there.

If Bref is open on today's note, the line appears there a moment later. Without a vault chosen yet, the command opens Bref as usual.

### Outline

`Ctrl + Shift + O` lists the headings of the note, indented by level, with the section the cursor is in selected. Type to filter; `↑` `↓` show each heading in place as you move, `Enter` leaves the cursor there, `Esc` puts it back where it was. Headings inside code blocks are not listed.

### Find and replace

`Ctrl + F` opens a search bar at the top right of the note. Matches are highlighted as you type, the current one more strongly, with its rank (`3 / 12`). The search runs on the text of the file, so Markdown markers (`**`, `[[`) can be searched too, and it works inside tables and code blocks. A selection held on one line becomes the search; the first key typed replaces it.

| Key | In the search bar |
|---|---|
| `Enter`, `Shift + Enter` (or `F3`, `Shift + F3`) | next, previous match, wrapping around |
| `Alt + C`, `Alt + W` | match case, whole words (also the `Aa` and `“ab”` buttons) |
| `Ctrl + Shift + Enter` | replace across the whole vault (also the `Vault…` button), with the same options: Bref first shows how many matches in how many notes, and writes nothing until you accept. `Undo the replacement in the vault` in the palette puts every note back, except those changed since |
| `Alt + R` | regular expression (also the `.*` button): `^` and `$` are the ends of each line, and the replacement can reuse the groups, `$1` |
| `Ctrl + H` | show the *Replace with* field; `Tab` moves between the two fields |
| `Enter` in *Replace with* | replace the current match and go to the next |
| `Ctrl + Enter` | replace every match; a single `Ctrl + Z` in the note brings them all back |
| `Esc` | close; the cursor stays on the current match |

Click in the note to edit it while the bar stays open: the matches follow the text, `F3`, `Alt + C` and `Alt + W` still work, and `Esc` closes the bar. With no bar, `F3` opens it.

## Navigation

A rail of icons stays on the left of the window. It opens a panel beside the note, in one of six views:

| View | Key | Shows |
|---|---|---|
| Tree | `Ctrl + E` | the vault and its folders; the open note is highlighted |
| Recent | `Ctrl + R` | every note, from the most to the least recently opened |
| Graph | `Ctrl + G` | one dot per note, one line per `[[link]]` between two notes |
| Tags | `Ctrl + T` | every `#tag` of the vault with its number of notes; unfold a tag to see them |
| Backlinks | `Ctrl + L` | the notes that link to the open note, each with the line that holds the link |
| Calendar | `Ctrl + Shift + J` | one month; the days that have their daily note stand out. Arrows change the day, `Page Up` and `Page Down` the month, `Enter` or a click opens the note of the day, or creates it. The week starts on Monday in French, on Sunday in English |

Backlinks follow the note you open, not the one you preview while moving through the list, so you can look at each linking note in turn. Links written with an alias (`[[Note|alias]]`) and links inside tables count.

The same keys work in every view (the calendar has its own arrows, see above):

| Key | Action |
|---|---|
| `↑` `↓`, or a click | select a note and preview it beside the panel |
| `Enter`, or a double click | open the note and start typing |
| `←` `→` | tree and tags: fold or unfold a folder or a tag; graph: move to the nearest note on that side |
| `Tab` | graph: walk through the notes linked to the selected one |
| `Esc` | back to the note |
| `F2`, `Delete` | tree, recent and tags: rename the selected note or folder, move it to the trash |
| `Ctrl + Shift + N` | new folder, next to the selected line of the tree |

A previewed note only counts as opened, and moves to the top of the recent list, once you click or type in it.

- **Sizes**: drag the line between the panel and the note. Dragged all the way left, the panel folds back to its rail; all the way right, it takes the whole window. Double-click the line for the default width. `Ctrl + M`, or the arrows button at the top of the panel, also switches to the whole window and back.
- **Folding**: pressing the key of the view on display, or clicking its icon, folds the panel. The rail stays: back and forward, the six views, search and new note at the top; the trash, backup, theme and help at the bottom.
- **Graph**: drag the background to move around and scroll to zoom. Pointing at a note lights up the notes it is linked to. Drag a note to move it: the notes linked to it follow, the closest ones the most. The target button glides back to the whole graph. With the graph on the whole window, a click only selects: `Enter` or a double click brings the note back.
- **Folders**: the double-chevron button at the top of the tree folds every folder; when they are all folded, it unfolds them all.
- **New note from the tree**: with the tree on display, `Ctrl + N` creates the note in the folder of the selected line. The button to the left of the folder one, at the top of the tree, creates it at the root of the vault instead, whatever is selected.
- **New folder**: the folder button, `Ctrl + Shift + N`, or `new folder` in the palette. The field says where the folder goes.
- **Right click** on a line for its menu: open, new note here, new folder, copy the `[[link]]`, the path or the path relative to the vault, show in the file manager (Finder, Explorer), duplicate (`Ctrl + D`), rename, move to the trash. A right click below the last line acts on the vault itself.
- **Several lines at once**: `Ctrl + click` adds a line to the selection or takes it out, `Shift + click` selects everything from the last line chosen. Dragging, duplicating, copying links or paths and moving to the trash then apply to all of them.
- **Drag and drop**: in the tree, drag a note or a folder onto a folder to move it there, or below the last line to move it to the top of the vault. Nothing is ever overwritten: if the name is taken, the move is refused.
- **Renaming** a note whose first line is its name rewrites that line too, so both stay in step. The `[[links]]` to it in the other notes follow, see [Notes and vault](#notes-and-vault).
- **The trash** is the hidden folder `.trash` at the top of the vault: deleting moves the note or the folder there, and never destroys anything. To take something back, click the bin at the bottom of the rail (or type `trash` in the palette): it lists the trash, and `Enter` returns the chosen item to the top of the vault (the trash does not remember where it came from), under another name if its own is taken. Empty the trash with your file manager.

Names are sorted as in a file manager: `Note 2` before `Note 10`. Pointing at an icon of the rail or of a bar names it, with its shortcut.

Bref reopens with the panel as you left it.

## Appearance

- **Theme**: the half-filled circle at the bottom of the rail, or `theme` in the palette. Each theme is applied as you move through the list, so you see it before choosing: `Enter` keeps it, `Esc` goes back to the one you had. `Default` follows the light or dark setting of your system. In every theme, links, secondary text and the titles of colored panels are adjusted to read at a contrast of 4.5:1 at least (WCAG AA).
- **Fonts**: Bref carries IBM Plex Sans (text) and IBM Plex Mono (code), under the SIL Open Font License (`assets/fonts/`), so it looks the same everywhere without installing anything. Type `font` in the palette (`Ctrl + P`) to choose another font, for the app or for the code, among those installed on your system; type in the list to filter it. Ideograms and emoji, which Plex does not draw, come from a font of your system.
- **Text size**: `Ctrl + +` and `Ctrl + -`, `Ctrl + 0` for the default. Headings and code scale with it.

These choices are kept in a file named `settings`, next to the [config file](#notes-and-vault).

## Markdown

What you type is what is saved. Bref only changes how it looks.

| Type | You get |
|---|---|
| `# `, `## `, `### ` | headings |
| `- `, `* `, `+ ` | a bullet list |
| `1. ` | a numbered list, renumbered as you add, remove or indent items |
| `[] ` or `- [ ] ` | a task; click the box or press `Ctrl + Enter` to check it |
| `[ ]` or `[x]` | a box on its own, in a sentence or in a table cell; click it or press `Ctrl + Enter` next to it |
| `> ` | a quote |
| `> [!NOTE]` | a colored panel, made of that line and the quote lines after it. `NOTE`, `TIP`, `IMPORTANT`, `WARNING` and `CAUTION` each have their color, as on GitHub and in Obsidian |
| `\| a \| b \|` | a table, see [Tables](#tables) |
| `` ``` `` then `Enter` | a code block, closed for you. Name the language (`` ```rust ``) to get it colored; the copy icon, on its first line, copies its content |
| `---` | a divider |
| `**bold**`, `*italic*`, `~~struck~~`, `` `code` `` | inline styles |
| `[[Note name]]` | a link to another note, with suggestions as you type |
| `#tag` | a tag, usable as a filter in the palette |
| `https://…` | a link, opened in your browser |
| `![](picture.png)` or `![[picture.png]]` | the picture, under its line. It is looked for next to the note, at the top of the vault, then by its file name anywhere in the vault |
| `Ctrl + V` with a picture in the clipboard | the picture is saved next to the note, and its `![](…)` line inserted |
| `` ```mermaid `` | a [Mermaid](https://mermaid.js.org) diagram, drawn under its block once the cursor has left it |
| `$x^2$`, `$$…$$`, or lines between two `$$` lines | a LaTeX formula, drawn under its line |
| `->`, `<-`, `<->`, `=>`, `<=>`, `!=`, `<=`, `>=` | shown as →, ←, ↔, ⇒, ⇔, ≠, ≤, ≥, except on the line you are editing and inside code |

Footnotes are kept as you typed them, without special rendering. So are pictures given as a web address. A formula is drawn under its line, not within the text, and only the first one of a line.

### Slash commands

Type `/` at the start of a line or of a word: a list offers what can be inserted there. Keep typing to filter it by name (`/h2`, `/todo`, `/table`) or by label (`/panel` lists the five panels), `Up` and `Down` to choose, `Enter` or `Tab` to insert, `Esc` to dismiss.

| Command | Inserts |
|---|---|
| `/h1`, `/h2`, `/h3` | a heading |
| `/list`, `/num`, `/todo` | a bullet list, a numbered list, a task |
| `/note`, `/tip`, `/important`, `/warning`, `/caution` | a colored panel |
| `/quote`, `/rule` | a quote, a divider |
| `/code`, `/mermaid`, `/math` | a code block, a Mermaid diagram, a formula, with the cursor inside |
| `/table` | a table, after you chose its size |
| `/link`, `/image` | `[[` or `![[`, and their suggestions |
| `/date` | today's date (`2026-10-09`), in place |
| `/meta` (also `/tags`, `/alias`) | a YAML front matter at the top of the note, wherever you are, with the cursor between the brackets of `tags: []`; if the note has one, the cursor goes there |

### Tables

`/table` opens a grid under the cursor. Move the pointer over it, or use the arrows, to choose how many columns and rows you want, then click or press `Enter`: the empty table is written, and the cursor waits in its first cell. For a table larger than the grid, type its size while the grid is open, columns first: `12x5`.

- A table is drawn as a grid, like on the web: columns share the width of the page, a long cell wraps inside its column and its row grows, and a short column keeps its natural width. Header row in bold, `:--` `:-:` `--:` under it align a column. The text underneath is plain Markdown: `|`, the line of dashes and the padding that keeps the columns aligned in other editors are not drawn.
- `Tab` and `Shift + Tab` go to the next and the previous cell, whose content is selected: typing replaces it. After the last cell, `Tab` adds a row.
- `Enter` adds a row under the one you are in. On a last row left empty, it removes that row and leaves the table.
- Arrows move from cell to cell without stopping on the `|`; `Up` and `Down` follow the lines of a wrapped cell, then the cell below. `Backspace` and `Delete` stop at the edge of a cell. Selecting several cells and erasing, cutting or typing empties them without merging the columns.
- `Ctrl + Shift + L`, `E`, `R` (`Cmd` on macOS) align the column of the cursor left, centered or right.
- Two `+` buttons appear when the pointer or the cursor is on a table: the one below adds a row, the one on the right adds a column.
- **Very large tables**: past 1,500 rows (a whole CSV turned into Markdown, for instance) a table is no longer laid out as one block. Its columns are sized from its first 300 rows, cells stop wrapping (what overflows is cut) so every row has the same height, and only the rows near the screen are drawn. A table of 300,000 rows opens in about 40 ms and typing in it takes under 10 ms. `Tab` and `Shift + Tab` still move between cells and `Enter` adds a row under the current one, but the columns of the text are not realigned as you type.
- A table you type by hand works the same: `| a | b |` then `Tab` is enough, the line of dashes is written for you. A literal `|` in a cell is written `\|` and shown as `|`.
- Columns of the text are aligned as you type, counting wide characters (ideograms, emoji) as two; a table too wide to stay aligned is written compactly. Only with more columns than the page can show, even at their narrowest, does the table scroll sideways (`Shift + wheel` or a horizontal swipe), following the cursor.
- To remove a column, delete its cells by hand.

Pasting keeps tables tidy. In a table, text goes into one cell on one line, its `|` escaped; cells copied from a spreadsheet or a web page (tab-separated) or from another Markdown table fill the grid from the current cell, adding rows and columns as needed. Anywhere else, a pasted Markdown table, or tab-separated cells, becomes a table on its own lines, aligned, with the line of dashes added.

## Diagrams

Mermaid turns text into a diagram. When you would rather place things yourself, `Ctrl + Shift + D` opens a canvas (also `New diagram` in the palette, and in the menu of the tree).

| Key | Tool |
|---|---|
| `R`, `U`, `O`, `D` | rectangle, rounded rectangle, ellipse, diamond |
| `C`, `P`, `N`, `T` | cylinder (a database), person (an actor), note, text |
| `A`, `L` | arrow, line |
| `V` | select |

Pick a tool, from its key or from the bar above the canvas, then drag; a click drops the shape at its usual size. A shape can also be dragged from the bar and dropped on the canvas.

- **Arrows** started or ended on a shape hold on to it: move the shape, they follow. Dropped in the middle of a shape, an end leaves by the side that faces the other end; dropped near an edge, it stays at that spot (the dots shown while you drag). Select an arrow and drag one of its ends to hook it elsewhere.
- **Routes**: an arrow is elbowed (right angles, leaving the shape by its side), straight or curved; with an arrow selected, a button of the bar goes from one to the next, and new arrows take the last one chosen. An elbowed arrow does not find its way around the shapes in its path: hook its ends on the sides that let it pass.
- **Text**: `Enter` or a double click writes in the selected shape, or on the arrow; `Esc` when done. A double click on the empty canvas starts a text there. A shape grows to hold what you write.
- **UML**: in a rectangle, a line made of `---` starts a new compartment: the name of the class on top, then its fields, then its methods. With an arrow selected, two buttons of the bar change the head at each end: none, arrow, hollow triangle (inheritance), hollow or full diamond (aggregation, composition). Another makes it dashed.
- **Selection**: click, `Shift` + click or `Ctrl` + click to add a shape or take it out, or drag on the empty canvas. Drag to move, on a grid; drag a corner to resize. The bar gives six colors, a tinted background and a dashed outline. `Delete` removes, `Ctrl + D` duplicates, `Ctrl + Z` undoes, the arrow keys move by one step.
- **View**: the wheel moves it and the middle button drags it. `+` and `-` zoom, as do `Ctrl` + wheel and the buttons at the bottom right; `0`, or a click on the percentage, fits the whole diagram.
- **In a note**: `![](Diagram.svg)` or `![[Diagram.svg]]` shows the diagram, in the colors of your theme, and follows its changes. Typing `![[` suggests the diagrams and pictures of the vault.
- **The file** is a plain SVG, saved at each change: it opens in a browser and displays on GitHub. It also carries its own source, which is what Bref reads to keep editing it.
- **Export**: the button at the right of the bar saves a PNG next to the diagram, dark on white.
- **Import**: `import` in the palette takes an Excalidraw file (`.excalidraw`) or a draw.io file (`.drawio`, saved without compression: untick *File › Properties › Compressed*). Shapes, texts, arrows and what they hold on to are kept; freehand strokes and pictures are not, and arrows are routed again by Bref.

## CSV and TSV tables

A `.csv` or `.tsv` file of the vault shows in the tree and opens in a grid, in place of the note. The file is read and written directly: there is no import, and the rows you did not touch are written back byte for byte.

- **Detection**: the encoding comes from the byte order mark, else UTF-8 if the file is valid UTF-8, else Windows-1252. The delimiter (`,` `;` tab `|` `:` or space) is the one that splits most lines into the same number of fields, quotes respected. The first row is a header unless it holds numbers.
- **Settings**: the bar above the grid shows the delimiter, the encoding and the header row; click one, or search `table` in the palette (`Ctrl + P`), to change it. The file is then read again that way, and the choice is kept for that file. *Detect automatically* forgets it.
- **Moving**: the arrows, `Tab` and `Shift + Tab`, `Page Up` and `Page Down`, `Home` and `End` along the row, `Ctrl + Home` and `Ctrl + End` to the corners, `Ctrl + Up` and `Ctrl + Down` to the ends of the column. Click a cell, or use the wheel (`Shift` + wheel sideways) and the scroll bars.
- **Selecting**: `Shift` + arrows, or drag with the mouse. Click a row number or a column header for the whole row or column, the corner or `Ctrl + A` for everything. `Ctrl + click` (on a cell, a row number or a header) keeps what is selected and starts another selection: copy and cut take them all, one block under the other, `Delete` empties them all and `Ctrl + Delete` removes all their rows. Typing and pasting go to the last one.
- **Writing**: typing replaces the cell, `Enter` or `F2` opens it as it is, `Enter` validates and goes down, `Tab` validates and goes right, `Esc` cancels. `Delete` empties the selection.
- **Rows**: `Ctrl + Enter` inserts one below, `Ctrl + Shift + Enter` above, `Ctrl + Delete` removes the selected ones. `Ctrl + Z` and `Ctrl + Shift + Z` undo and redo, a paste in one step.
- **Copy and paste**: `Ctrl + C` copies the selection as tab-separated text, which spreadsheets and the tables of your notes paste as they are; `Ctrl + X` cuts it. `Ctrl + V` pastes cells from a spreadsheet, a Markdown table or CSV text from the selected cell, adding the rows that are missing; one value fills the whole selection. In a note, selecting cells of a Markdown table and copying gives the same tab-separated text.
- **Copy as, export**: `copy table as` and `export table` in the palette (or the buttons of the bar) offer TSV, CSV (with the delimiter of the file), Markdown and JSON, for the selection, or the whole table when only one cell is selected. An export is written next to the table, under a name that is still free.
- **Saved** about half a second after the last change, in the background, atomically, in the encoding and with the line endings of the file. A character the encoding cannot hold (a `€` in ISO-8859-1) is reported and the file is left alone. If another program rewrites the file and nothing waits to be saved, the grid reads it again.
- **Limits**: files up to 1 GiB, read whole in memory (about the size of the file); no columns to add or remove, no sorting or formulas.

## Syncing a vault

A vault is a folder of plain files: there are two ways to have the same notes on every computer, and safely copied somewhere else. Pick one, never both: a git repository inside a cloud folder ends up corrupted.

### With git: one action

`Ctrl + Shift + S`, or **Sync the vault** in the palette, does everything, in the background, while you keep typing:

1. the open notes are saved and everything that changed is committed;
2. what the server has is fetched and merged;
3. the result is sent.

A message says what was sent and received, or why it failed.

- **Conflicts**: two computers that changed different notes, or different parts of the same note, are merged without a question. When the same lines changed on both sides, both versions are kept, as Dropbox and Syncthing do: the note keeps the version of this computer, the other one is written next to it as `Note (conflict 2026-10-09 laptop).md`, and the sync completes. You never get `<<<<<<<` markers in a note. A note deleted on one side and edited on the other stays, edited.
- **A vault that is not in git yet**: the same command asks for the address of an *empty* repository (create it on GitHub, GitLab, Codeberg, or on any server you reach by SSH with `git init --bare notes.git`), then connects the vault and sends its notes. The trash (`.trash`) is left out of the repository, through a `.gitignore`.
- **On another computer**: **Clone a vault from a git address…**, in the palette or on the welcome screen, asks for the address and for a folder, and opens the clone as the vault.
- **Requirements**: `git` must be installed, and able to reach the repository without asking anything (an SSH key, or a credential helper for HTTPS). Bref never shows a password prompt: a refused sign-in is reported. Without a name and an email in the git configuration, the commits are signed `Bref`.
- **What it refuses**, with the reason and without touching anything: a merge or a rebase in progress, a detached HEAD, a branch that follows no branch of the server, a locked index, and a vault that is only a folder of a larger repository.
- **Limits**: syncing is done when you ask, not on a timer nor when quitting. git syncs computers: there is no Bref on a phone, where the repository stays readable with another app.

### With a cloud folder: nothing to do

Put the vault in a folder synced by iCloud, Dropbox, Google Drive or Syncthing: Bref follows what changes there (see below), nothing more is needed.

## Notes and vault

- A vault is an ordinary folder. Notes in subfolders are found too; hidden folders (`.git`, `.obsidian`…) are ignored. New notes are created at the top of the vault, or in the selected folder when the tree is on display.
- CSV and TSV files are part of the vault too: the tree lists them with the notes of their folder, and selecting one opens its grid in place of the note (see [CSV and TSV tables](#csv-and-tsv-tables)). They are not in the graph, and a note does not link to them. They can be renamed (they keep their extension), moved, duplicated or trashed like a note.
- Pictures are part of the vault: the tree lists them under the notes of their folder, and the graph shows them as squares, linked to the notes that display them. Selecting one shows it in place of the note; it can be renamed (the notes that display it follow), moved or trashed like a note.
- What other programs change in the vault (a sync tool, a script, another editor) is picked up within a second, without restarting: Bref asks the system to report changes (inotify, FSEvents, ReadDirectoryChangesW).
- A note is saved shortly after you stop typing, and when you switch note or quit. Saving is atomic: a crash never leaves a half-written file.
- A YAML front matter (a block that starts on the first line with `---` and ends with `---` or `...`) is shown dimmed and is never interpreted or rewritten: the title of the note is the first line after it, and renaming a note or rewriting its links leaves the block byte for byte as it was. Two of its keys are read: `tags` (`tags: [a, b]`, `tags: a, b` or a dashed list) count like `#tags` in the text, and each of the `aliases` is another name for the note, suggested after `[[` and followed by links, backlinks and the graph. Other keys and nested YAML are ignored.
- **Backup**: click the arrow into a tray at the bottom of the rail (or type `back up` in the palette) and choose a folder: Bref writes there one archive of the whole vault, trash included, named after the vault and the date (`Notes 2026-10-09.tar.gz`), numbered if that name is taken. It uses the `tar` program of the system (Linux, macOS, Windows 10 and later) and refuses a folder inside the vault.
- **Terminal**: type `terminal` in the palette to open the terminal of your system in the folder of the vault (Windows Terminal or the console on Windows, Terminal on macOS; on Linux the one named in `$TERMINAL`, else the one of the desktop). Bref has no terminal of its own.
- **Status messages**: the outcome of an operation (a backup saved, a note restored, a failure) shows at the bottom right, with a spinner while it runs. A message goes away after four seconds, or as soon as you click it.
- A note opens with the cursor at the start of its body, under its title and its front matter: what you type first goes into the text, it does not rename the note.
- A new note is named after its first line: `# Groceries` becomes `Groceries.md`, and the file is renamed when you change that line. A new note left empty leaves no file behind. A note whose file name did not already match its first line (typical of an existing vault) keeps its name, until you add or change a `# ` title on its first line: the file then takes that name, as in Obsidian.
- `[[Groceries]]` finds the note by file name, in any subfolder, ignoring case.
- When a note is renamed, from the tree or by changing its first line, the `[[links]]` to its old name are rewritten in every note, keeping their `|alias` and `#heading`. After a change of title, this happens when you leave the note, not at each keystroke. If another note still carries the old name, the links are left alone: they may be meant for it.
- The vault and the notes you opened, most recent first, are remembered in a small text file (the navigation panel in a file named `layout` next to it, and the [appearance](#appearance) in `settings`):

  | System | File |
  |---|---|
  | Linux | `~/.config/bref/config` |
  | macOS | `~/Library/Application Support/bref/config` |
  | Windows | `%APPDATA%\bref\config` |

## Updates

Once a day, in the background, Bref asks GitHub whether a newer version exists. If so, a discreet banner at the bottom of the window says so.

Where Bref can install itself, the banner offers **Update** (also `Update Bref to …` in the palette): it downloads the new version, installs it, and restarts, with your notes saved first.

| Installed with | What Update does |
|---|---|
| Windows installer | runs the new installer silently; it replaces `bref.exe` and opens Bref again |
| macOS `.dmg` | replaces `Bref.app` in place (it needs write access to the folder that holds it, `/Applications` for most people) and opens it again |
| Arch, prebuilt package | installs the new package with `pacman -U`, through `pkexec`, which asks for your password in a desktop window |

Elsewhere (Windows zip, compiled from source, `bref-git`, other Linux distributions) the banner links to the download page: follow [Upgrading](#upgrading). If an update fails, Bref stays as it was and says why.

Bref checks nothing but HTTPS: the files are not signed (see [Windows](#windows) and [macOS](#macos)), so an update is as trustworthy as downloading the same file by hand from this repository's releases.

The check is the only network request the app makes until you press Update. It runs `curl` against `api.github.com/repos/alarboulletmarin/bref/releases/latest`, which sees your IP address and the client name `bref`, and nothing else. Without `curl`, or offline, nothing is shown. To check right away, without waiting for the next day, type `check for updates now` in the palette (`Ctrl + P`): Bref answers that it is up to date, or shows the banner. This works even when the daily check is off. Type `updates` in the palette to stop the daily check, or to resume: the choice is kept as `updates=off` in the `settings` file, and the time of the last check in the `update` file, both next to `config`.

## Compatibility

| System | Status |
|---|---|
| Linux, GNOME on Wayland | first-class: this is what Bref is built and used on (Arch Linux) |
| Linux, other desktops, Wayland or X11 | should work, with the same built-in title bar. Not tested |
| macOS, Windows | built and tested by CI on every commit, and the downloads are built there too; not used day to day by the author |

- **Linux** needs a working Vulkan driver (Mesa or the vendor one). Bref draws its own title bar, as Zed does: GNOME under Wayland draws none for applications. Drag the window by the pill at the top right, by the title of the panel, by the empty part of the rail or of a table's bar (a double click maximizes it), or hold `Alt` and drag from anywhere. On macOS and Windows the system title bar is used.
- **Language**: French if the system language is French, English otherwise. Bref asks the system (Windows, macOS) or reads `LC_ALL`, `LC_MESSAGES` and `LANG` (Linux, or any system when started from a terminal).
- **Fonts**: IBM Plex Sans and Plex Mono, built into the app.

## Troubleshooting

**The window takes two seconds to open (Linux).** Another Vulkan driver is slow to load. Bref already skips the NVIDIA one when the machine has no NVIDIA card. To check what remains, run `VK_LOADER_DEBUG=driver Bref` in a terminal.

**The window does not open, with an error about a surface or an adapter.** No usable Vulkan driver: install the one for your GPU (`vulkan-intel`, `vulkan-radeon`, `nvidia-utils`… on Arch; `mesa-vulkan-drivers` on Ubuntu).

**Bold text is not bold.** The font you picked is a variable font, which the text engine cannot embolden. [Pick another font](#appearance), or none to go back to the built-in ones.

**A note was not renamed after I changed its title.** Either another note already has that name, or the file name did not match the first line to begin with and that line is not a `# ` title you just added or changed, see [Notes and vault](#notes-and-vault).

**I changed the vault from another program while Bref was open.** Bref is told by the system when the vault changes, and also looks at it every 30 seconds in case an event got lost (every two seconds when the system cannot watch the folder, for instance on some network drives, or when Linux has run out of inotify watches: raise `fs.inotify.max_user_watches`). Notes and folders added, renamed or removed elsewhere show up in the tree, the palette and the graph, and the note on display is read again when its file changes. If you were typing in that note at that moment, your version is kept and written back.

**Limits.** The name field of a new folder or a rename only edits at its end: type, or erase with Backspace. Links written as plain text inside a code block are not followed when a note is renamed.

## How it works

- **Editor**: a custom text element drawn directly with GPUI's text system. Each line is classified (heading, list item, quote, code…) and shaped with its own size and style runs; the text itself is never transformed. The layout is redone only when the text, the width, the theme or the cursor change, and then only for the lines an edit touched (the others are kept and shifted); a table of more than 1,500 rows is laid out light, with cells shaped only near the screen. A test compares every reused layout with a full one on thousands of random edits.
- **CSV and TSV** (`src/table.rs`, `src/sheet.rs`): the file is decoded once and kept as text with one small index entry per row, so a million rows cost about 12 MB; fields are read on demand, and a row you did not edit is written back as it was. The grid builds only the cells on screen. Encoding and delimiter detection, parsing, writing and the copy and export formats are pure functions with unit tests.
- **Typing rules** (`src/markdown.rs`): pure functions decide what Enter and Tab do on a line and renumber the list around the cursor. They are unit-tested without any UI.
- **Vault** (`src/vault.rs`): the note list, its tags and its text are indexed off the UI thread when the vault opens, then kept up to date on each save. Changes made elsewhere are reported by the `notify` crate; the vault is also checked on a slow timer, which becomes the only mechanism when watching is not possible.
- **Palette**: name matches first (substring, ranked by position, then subsequence; ties keep the most recently opened note first), then notes whose text contains every word, most recent first. The whole text of the vault is kept in memory for that.
- **Updates** (`src/update.rs`): the latest release comes from the GitHub API through `curl`, at most once a day, off the UI thread, and so does the download. Reading the answer and comparing versions are pure functions with unit tests; swapping `Bref.app` from a disk image is tested on a real macOS by CI. `BREF_RELEASES_API` points the check at a local `file://` release description, to try an update without publishing one.
- **Navigation** (`src/nav.rs`): the tree is rebuilt from the paths of the indexed notes, and only the visible rows are drawn. 
- **Graph** (`src/graph.rs`): a force-directed layout computed off the UI thread when the links change, then drawn as plain lines and discs. Links to notes that do not exist yet are left out.
- **Diagrams** (`src/diagram.rs`, `src/canvas.rs`, `src/import.rs`): shapes and arrows are plain data; the same outlines are drawn on the canvas and written to the SVG file, whose `<metadata>` holds one line of source per element.
- **Startup**: the last note is read before the first frame, so the window appears with its content. Icons are embedded in the binary.
- **Window**: on Linux Bref draws its own title bar, shadow and resize edges (client-side decorations); on macOS and Windows the system does.

## Development

```bash
cargo run                 # debug build
cargo run --release       # what gets installed
make test                 # cargo test --locked
```

The tests include an end-to-end run driven by simulated keystrokes (typing, lists, autosave, palette, links, navigation panel, file operations, CSV tables), using GPUI's test platform: no display needed. Ignored benchmarks measure vaults of thousands of notes, CSV files of 300,000 rows and notes with a table of 300,000 rows; run them with `cargo test --release --locked -- --ignored --nocapture`, each one reads its parameters from environment variables named in its doc comment.

`Cargo.lock` started as a copy of the one GPUI 0.2.2 was published with: newer versions of some of its dependencies no longer build together. Update dependencies one at a time.

Releasing (maintainer): `scripts/release.sh X.Y.Z` bumps the version, tags, pushes, creates the GitHub release (the Arch package is attached by `.github/workflows/arch-package.yml`) and pins the PKGBUILD checksum.

## License

MIT
