# Architecture

Bref is one Rust crate built on [GPUI](https://www.gpui.rs/) (pinned at 0.2.2). This page is the
map: where things live, which way the dependencies go, and the rules the code keeps.

## Layers

```
src/
  main.rs      entry point: declares the layers, calls shell::run()
  lang.rs      tr("English", "Français"), language detection, date and search-key names
  actions.rs   the app's actions, dispatched by shortcuts, menus and the palette
  logic/       pure functions: no gpui, no disk, no window
  platform/    the outside world: disk, processes, network, OS (no gpui)
  ui/          the look shared by every view: theme, fonts and icons, widgets, shortcut names
  views/       gpui entities: editor, side panel, palette, grid, canvas, graph, viewer, board
  shell/       the window: owns the panes and the views, routes events, runs the commands
    tests/     the whole app driven by simulated keystrokes
```

A layer only uses the layers below it:

```
shell ──► views ──► ui ──► logic
  │         │              ▲
  └─────────┴──► platform ─┘        lang, actions: usable everywhere
```

The test `layers_only_depend_downwards` fails when a file breaks this rule, and when a view
reaches into the shell.

## The shell

`Shell` (`shell/mod.rs`) is the window's state. Its methods are grouped by subject, one file
each, all `impl Shell`:

| File | Subject |
|---|---|
| `pane.rs` | the one or two panes, which is active, the file a pane shows (`FileView`) |
| `notes.rs` | opening, previewing, creating, saving notes; links and wikilinks |
| `library.rs` | the vault: choosing, scanning, watching, icons, backup, trash |
| `sync.rs` | git sync and cloning |
| `sidebar.rs`, `files.rs`, `menu.rs`, `listing.rs` | the side panel, file operations, the context menu, list pages |
| `palette_open.rs` | every way the palette opens: notes, commands, outline, questions |
| `help.rs`, `keymap.rs` | the help panel; the default shortcuts and their sections |
| `travel.rs` | back and forward |
| `search.rs`, `templates.rs`, `tables.rs`, `diagrams.rs`, `board.rs`, `drops.rs`, `capture.rs` | one feature each |
| `settings.rs`, `toasts.rs`, `updates.rs` | settings, status messages, new versions |
| `render.rs` | drawing the window |

The active pane is `self.pane()` / `self.pane_mut()`; `each_pane` and `in_pane` reach the others.
A view talks to the shell only through its events (`EditorEvent`, `SheetEvent`, `CanvasEvent`…).

## Where to look

| To change… | Go to |
|---|---|
| how Markdown is read (headings, links, tags, styles) | `logic/markdown/` (`mod.rs`; `tables.rs`, `lists.rs`, `front.rs`, `dates.rs`, `search.rs`) |
| CSV/TSV parsing and writing | `logic/table.rs` |
| diagrams (shapes, arrows, SVG with its source) and their import | `logic/diagram.rs`, `logic/import.rs` |
| the vault on disk (scan, save, rename, trash, config files) | `platform/vault.rs` |
| git commands, update check | `platform/git.rs`, `platform/update.rs` |
| colours, themes, page width | `ui/theme.rs` |
| icons and fonts | `ui/assets.rs` |
| the text editor | `views/editor/`: `edit.rs` (changes, undo), `cursor.rs`, `style.rs`, `table.rs`, `blocks.rs`, `find.rs`, `completion.rs`, `links.rs` (links and comments, in a table too), `notes.rs` (which rows, columns and cells of a table carry a comment, for the marks drawn on it), `format_bar.rs`, `mouse.rs`, `clipboard.rs`, `input.rs` (IME) |
| what a keystroke costs | `views/editor/layout.rs` (`plan` redoes only the damaged lines) then `paint.rs`; the test `layout_reuse_matches_full_layout` checks every reuse against a full layout |
| the tree, recent notes, calendar | `views/nav.rs` (state), `shell/sidebar.rs` (drawing) |
| the command palette | `views/palette.rs`, `shell/palette_open.rs` |
| the CSV/TSV grid | `views/sheet/` (`disk.rs` loading and saving, `edit.rs`, `clipboard.rs`, `mouse.rs`, `render.rs`) |
| a shortcut | `shell/keymap.rs` (`defaults`, `help_sections`), `ui/keys.rs` (how keys are read and shown) |
| the test of a feature | `shell/tests/` (`end_to_end/` is one session, one file per chapter) |

## Rules

- **Nothing is destroyed or overwritten on disk**: atomic writes (`vault::write`), a `.trash`
  folder, and a taken name is refused.
- **Interface text** always goes through `tr("English", "Français")`. Tests run in French.
- **Pure first**: logic that can be a pure function lives in `logic/` and is unit-tested there;
  every user journey is tested end to end with simulated keystrokes (GPUI's test platform).
- **No new dependency** without asking: `Cargo.lock` comes from GPUI 0.2.2's and is fragile.
