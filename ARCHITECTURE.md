# Architecture

Bref is one Rust crate built on [GPUI](https://www.gpui.rs/) (pinned at 0.2.2). This page is the
map: where things live, which way the dependencies go, and the rules the code keeps.

## Layers

```
src/
  main.rs      entry point: declares the layers, sends `bref mcp` to mcp::run(), then shell::run()
  lang.rs      tr("English", "Français"), language detection, date and search-key names
  actions.rs   the app's actions, dispatched by shortcuts, menus and the palette
  logic/       pure functions: no gpui, no disk, no window
  platform/    the outside world: disk, processes, network, OS (no gpui)
  ui/          the look shared by every view: theme, fonts and icons, widgets, shortcut names
  views/       gpui entities: editor, side panel, palette, grid, canvas, graph, viewer, board
  shell/       the window: owns the panes and the views, routes events, runs the commands
    tests/     the whole app driven by simulated keystrokes
  mcp/         `bref mcp`: JSON-RPC over stdio for an agent. Uses logic, platform and the pure
               parts of ui. Nothing uses it. Its texts stay in English and do not go through tr
```

A layer only uses the layers below it:

```
shell ──► views ──► ui ──► logic
  │         │              ▲
  └─────────┴──► platform ─┘        lang, actions: usable everywhere
mcp ────────────► ui, logic, platform
```

The test `layers_only_depend_downwards` fails when a file breaks this rule, and when a view
reaches into the shell.

## The shell

`Shell` (`shell/mod.rs`) is the window's state. Its methods are grouped by subject, one file
each, all `impl Shell`:

| File | Subject |
|---|---|
| `pane.rs` | the panes and their tabs, which one is active, the file a tab shows (`FileView`), splitting, moving a tab between panes, the saved layout; the panes are placed by the tree `logic/layout.rs` (`Node`, leaves = groups), drawn by `render_layout` |
| `tabs.rs` | the tab bar of a pane |
| `notes.rs` | opening, previewing, creating, saving notes; links and wikilinks |
| `library.rs` | the vault: choosing, scanning, watching, icons, backup, trash |
| `sync.rs` | git sync and cloning |
| `sidebar.rs`, `files.rs`, `menu.rs`, `listing.rs` | the side panel, file operations, the context menu, list pages |
| `docks.rs` | where each view of the panel lives: the left dock, the right dock, or a tab (`Pane::tool`). Each dock stacks its shown views (`Nav::left_shown`, `Nav::right_shown`, `Nav::folded`); each view has its own rows and scroll: the active one is in `Nav`, the others wait in `Nav::parked`, and drawing a section, its rows, or a press in it swaps its view in (`Nav::activate`). A header dragged (`ToolDragged`) lands with `place_tool` |
| `palette_open.rs` | every way the palette opens: notes, commands, outline, questions |
| `help.rs`, `keymap.rs` | the help panel; the default shortcuts and their sections |
| `travel.rs` | back and forward |
| `search.rs`, `templates.rs`, `tables.rs`, `diagrams.rs`, `board.rs`, `drops.rs`, `capture.rs` | one feature each |
| `settings.rs`, `toasts.rs`, `updates.rs` | settings, status messages, new versions |
| `render.rs` | drawing the window |

A `Pane` is what **one tab** holds (a note, or a file in its place); `Shell::panes` lists every
tab of the window, shown or not, so what must reach every open note (saving, renames, the watcher,
the theme) loops over it. A `Group` is a pane as the user sees it: its tabs in order and the one it
shows. A tab out of sight calls `Editor::rest` and keeps its text, cursor and undo only.
What the user opens goes through `Shell::open`: the tab that already shows the file, otherwise a
new tab (a preview or an empty tab is used in place); `open_note` loads into the active tab and
is for code that has already chosen it (restoring, the history).
The active tab is `self.pane()` / `self.pane_mut()`; `each_pane` and `in_pane` reach the others.
A view talks to the shell only through its events (`EditorEvent`, `SheetEvent`, `CanvasEvent`…).

## Where to look

| To change… | Go to |
|---|---|
| how Markdown is read (headings, links, tags, styles) | `logic/markdown/` (`mod.rs`; `tables.rs`, `lists.rs`, `front.rs`, `dates.rs`, `search.rs`) |
| CSV/TSV parsing and writing | `logic/table.rs` |
| embedded sections (`![[Note#Heading]]`: reading the line, finding the section, rewriting links to a renamed heading) | `logic/embed.rs`; laid out and drawn by `views/editor/embed.rs` (a read-only `Editor` per section, framed under its line) |
| formulas of the cells of a table (parser, evaluator, numbers as written) | `logic/formula.rs`; shown by `views/editor/grid.rs` (`results`, `Shown`), written and completed by `views/editor/formulas.rs` |
| diagrams (shapes, arrows, SVG with its source) and their import | `logic/diagram/` (`file.rs`: the SVG and its source; `hold.rs`: what a package or a frame holds); Mermaid class, sequence and state diagrams turned into canvas diagrams (`logic/mermaid.rs`); relationships (`logic/relation.rs`), class notation (`logic/uml.rs`), `logic/import.rs`; styled text of their labels: `logic/richtext.rs` (the marks of `markdown::emphasis`); the editor: `views/canvas/` |
| the vault on disk (scan, save, rename, trash, config files) | `platform/vault.rs` |
| git commands, update check | `platform/git.rs`, `platform/update.rs` |
| the window's door for `bref mcp` (`show`) | `platform/link.rs` (127.0.0.1 and a secret in `<config>/bref/agent`), `Shell::listen` in `shell/library.rs` |
| a note written elsewhere while typed in | `logic/merge.rs` (three-way, by lines), `Shell::take_outside` in `shell/notes.rs` |
| colours, themes, page width | `ui/theme.rs` |
| icons and fonts | `ui/assets.rs` |
| the text editor | `views/editor/`: `edit.rs` (changes, undo), `cursor.rs`, `style.rs`, `table.rs`, `blocks.rs`, `find.rs`, `completion.rs`, `links.rs` (links and comments, in a table too), `notes.rs` (which rows, columns and cells of a table carry a comment, for the marks drawn on it), `embed.rs` (sections of other notes shown in place), `format_bar.rs`, `mouse.rs`, `clipboard.rs`, `input.rs` (IME) |
| what a keystroke costs | `views/editor/layout.rs` (`plan` redoes only the damaged lines) then `paint.rs`; the test `layout_reuse_matches_full_layout` checks every reuse against a full layout |
| the tree, recent notes, calendar | `views/nav.rs` (state), `shell/sidebar.rs` (drawing) |
| the command palette | `views/palette.rs`, `shell/palette_open.rs` |
| the CSV/TSV grid | `views/sheet/` (`disk.rs` loading and saving, `edit.rs`, `clipboard.rs`, `mouse.rs`, `render.rs`) |
| a shortcut | `shell/keymap.rs` (`defaults`, `help_sections`), `ui/keys.rs` (how keys are read and shown) |
| the test of a feature | `shell/tests/` (`end_to_end/` is one session, one file per chapter) |
| the user guide | `book/src/` (an mdBook); `shortcuts.md` is written from `help_sections` by the test `shortcuts_page_is_current` |
| the MCP server (`bref mcp`) | `src/mcp/` (`mod.rs` the protocol, `notes.rs`, `draw.rs`, `guide.rs`); `main` routes `mcp` before the window. A session over pipes is `tests/mcp.rs` |

## How the main parts work

- **Editor**: a custom text element drawn directly with GPUI's text system. Each line is classified
  (heading, list item, quote, code…) and shaped with its own size and style runs; the text itself is
  never transformed. The layout is redone only when the text, the width, the theme or the cursor
  change, and then only for the lines an edit touched (the others are kept and shifted). A table of
  more than 1,500 rows is laid out light, with cells shaped only near the screen. A test compares
  every reused layout with a full one on thousands of random edits.
- **Typing rules** (`logic/markdown/lists.rs`): pure functions decide what Enter and Tab do on a
  line and renumber the list around the cursor, unit-tested without any UI.
- **CSV and TSV** (`logic/table.rs`, `views/sheet/`): the file is decoded once and kept as text with
  one small index entry per row, so a million rows cost about 12 MB; fields are read on demand, and
  a row that was not edited is written back as it was. The grid builds only the cells on screen.
  Encoding and delimiter detection, parsing, writing and the copy and export formats are pure
  functions with unit tests.
- **Vault** (`platform/vault.rs`): the note list, its tags and its text are indexed off the UI
  thread when the vault opens, then kept up to date on each save. Changes made elsewhere are
  reported by the `notify` crate; the vault is also checked on a slow timer, which becomes the only
  mechanism when watching is not possible.
- **Palette**: name matches first (substring, ranked by position, then subsequence; ties keep the
  most recently opened note first), then notes whose text contains every word, most recent first.
  The whole text of the vault is kept in memory for that, as written and in lower case: it is
  lowered once, when the vault is read, not each time the palette opens.
- **Navigation** (`views/nav.rs`, `shell/sidebar.rs`): the tree is rebuilt from the paths of the
  indexed notes, and only the visible rows are drawn.
- **Graph** (`views/graph.rs`): a force-directed layout computed off the UI thread when the links
  change, then drawn as plain lines and discs. Links to notes that do not exist yet are left out.
- **Diagrams** (`logic/diagram/`, `views/canvas/`, `logic/import.rs`): shapes and arrows are plain
  data; the same outlines are drawn on the canvas and written to the SVG file, whose `<metadata>`
  holds one line of source per element.
- **Figures and SVG** (`ui/svg.rs`, `views/viewer.rs`): Mermaid diagrams and SVG files are laid out
  and rasterized off the UI thread, at most one at a time per figure, and the previous version stays
  on screen until the new one is ready; a figure never exceeds 8,192 pixels a side or 16 million
  pixels. The viewer draws tiles of 512 pixels and keeps the last 64; a picture (`Source::Raster`)
  is not tiled, the graphics card draws it whole.
- **Updates** (`platform/update.rs`): the latest release comes from the GitHub API through `curl`,
  at most once a day, off the UI thread, and so does the download. Reading the answer and comparing
  versions are pure functions with unit tests; swapping `Bref.app` from a disk image is tested on a
  real macOS by CI. `BREF_RELEASES_API` points the check at a local `file://` release description,
  to try an update without publishing one.
- **Startup**: the last note is read before the first frame, so the window appears with its content.
  Icons are embedded in the binary. `bref mcp` never reaches that: `main` runs `mcp::run()` and
  leaves before the window, one JSON-RPC message at a time, stdout reserved for the protocol.
- **Agents**: the server keeps the vault's index and rereads it when the watcher signals a change.
  It writes with the same rules as the app (atomic save, trash, a taken name refused, links
  rewritten on rename). A diagram file is produced by `Diagram::to_svg` only.
- **Window**: on Linux Bref draws its own title bar, shadow and resize edges (client-side
  decorations); on macOS and Windows the system does.

## Rules

- **Nothing is destroyed or overwritten on disk**: atomic writes (`vault::write`), a `.trash`
  folder, and a taken name is refused.
- **Interface text** always goes through `tr("English", "Français")`. Tests run in French.
- **Pure first**: logic that can be a pure function lives in `logic/` and is unit-tested there;
  every user journey is tested end to end with simulated keystrokes (GPUI's test platform).
- **No new dependency** without asking: `Cargo.lock` comes from GPUI 0.2.2's and is fragile.
- **A diagram file never loses what it says**: its source starts with `version N` (`diagram::VERSION`).
  A line this version cannot read is kept and written back; a file of a higher (or unreadable)
  version is not a diagram for this build (`Diagram::from_svg` is `None`, `Diagram::newer` is true)
  and opens read-only in the viewer. New data of an element goes on a **line of its own, keyed by
  the element's id**, never in a new column: the text is last on `shape` and `link` lines, so an
  older reader would take a new column for text. Bump `VERSION` whenever the meaning of a line changes.
  A file is written with the **lowest** version that can say what it holds (`Diagram::needed`), so a
  diagram that uses nothing new still opens in the builds before. What changes how an existing text
  reads (styled text, `rich`) is a line of its own that a diagram made before does not have: its
  texts keep their stars until the user asks.
