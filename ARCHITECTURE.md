# Architecture

Bref is one Rust crate built on [GPUI](https://www.gpui.rs/) (pinned at 0.2.2). This page is the
map: where things live, which way the dependencies go, and the rules the code keeps.

## Layers

```
src/
  main.rs      entry point
  lang.rs      tr("English", "Français"), language detection, date and search-key names
  logic/       pure functions: no gpui, no disk, no window
  platform/    the outside world: disk, processes, network, OS (no gpui)
  ui/          the look shared by every view: theme, fonts and icons, widgets, figures
  views/       gpui entities: editor, side panel, palette, grid, canvas, graph, viewer, board
  tests/       the whole app driven by simulated keystrokes (end_to_end/ is one session,
               one file per chapter); benches.rs holds the measures run by hand
```

A layer only uses the layers below it:

```
views ──► ui ──► logic
  │              ▲
  └──► platform ─┘        lang: usable everywhere
```

The test `layers_only_depend_downwards` fails when a file breaks this rule.

## Where to look

| To change… | Go to |
|---|---|
| how Markdown is read (headings, links, tags, lists, tables) | `logic/markdown.rs` |
| CSV/TSV parsing and writing | `logic/table.rs` |
| diagrams (shapes, arrows, SVG with its source) and their import | `logic/diagram.rs`, `logic/import.rs` |
| the vault on disk (scan, save, rename, trash, config files) | `platform/vault.rs` |
| git sync, update check | `platform/git.rs`, `platform/update.rs` |
| colours, themes, page width | `ui/theme.rs` |
| icons and fonts | `ui/assets.rs` |
| the text editor | `views/editor.rs` |
| the tree, recent notes, calendar, context menu | `views/nav.rs` |
| the command palette | `views/palette.rs` |
| the CSV/TSV grid | `views/sheet.rs` |
| the diagram canvas | `views/canvas.rs` |
| keyboard shortcuts | `keys.rs`, `defaults()` and `help_sections` in `main.rs` |

## Rules

- **Nothing is destroyed or overwritten on disk**: atomic writes (`vault::write`), a `.trash`
  folder, and a taken name is refused.
- **Interface text** always goes through `tr("English", "Français")`. Tests run in French.
- **Pure first**: logic that can be a pure function lives in `logic/` and is unit-tested there;
  every user journey is tested end to end with simulated keystrokes (GPUI's test platform).
- **No new dependency** without asking: `Cargo.lock` comes from GPUI 0.2.2's and is fragile.
