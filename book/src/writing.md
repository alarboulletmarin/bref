# Writing notes

What you type is what is saved. Bref only changes how it looks: markers stay visible but dimmed, and the file on disk is always plain Markdown.

## Markdown

| Type | You get |
|---|---|
| `# `, `## `, `### ` | headings |
| `- `, `* `, `+ ` | a bullet list |
| `1. ` | a numbered list, renumbered as you add, remove or indent items |
| `[] ` or `- [ ] ` | a task: click the box or press `Ctrl + Enter` to check it |
| `[ ]` or `[x]` | a box on its own, in a sentence or in a table cell: click it, or press `Ctrl + Enter` next to it |
| `> ` | a quote |
| `> [!NOTE]` | a colored panel: that line and the quote lines after it |
| `\| a \| b \|` | a table, see [Tables and formulas](tables.md) |
| `` ``` `` then `Enter` | a code block, closed for you |
| `---` | a divider |
| `[[Note name]]` | a link to another note, see [Links](links.md) |
| `#tag` | a tag, usable as a filter in the palette |
| `https://…` | a link, opened in your browser |
| `![](picture.png)` or `![[picture.png]]` | the picture, under its line, see [Pictures](links.md#pictures) |
| `` ```mermaid `` | a [Mermaid](https://mermaid.js.org) diagram |
| `$x^2$`, `$$…$$`, or lines between two `$$` lines | a LaTeX formula |
| `->`, `<-`, `<->`, `=>`, `<=>`, `!=`, `<=`, `>=` | →, ←, ↔, ⇒, ⇔, ≠, ≤, ≥ |

Some details:

- **Panels**: `NOTE`, `TIP`, `IMPORTANT`, `WARNING` and `CAUTION` each have their color, as on GitHub.
- **Code blocks**: name the language (`` ```rust ``) to get it colored. The copy icon on the first line copies the content of the block.
- **Mermaid**: the diagram is drawn under its block once the cursor has left it. The magnifier next to the copy button shows it full size, to zoom and move around.
- **Formulas**: a formula is drawn under its line, not within the text. Only the first one of a line is drawn.
- **Arrows**: the symbols are shown everywhere except on the line you are editing and inside code.
- **Not rendered**: footnotes and pictures given as a web address are kept as you typed them.

## Inline styles

| Type | Key | You get |
|---|---|---|
| `**bold**` | `Ctrl + B` | bold |
| `*italic*` | `Ctrl + I` | italic |
| `<u>underlined</u>` | `Ctrl + U` | underline (Markdown has none, so Bref uses the HTML tag) |
| `~~struck~~` | `Ctrl + Shift + X` | strikethrough |
| `==highlighted==` | `Ctrl + Shift + H` | highlight |
| `<mark class="green">colored</mark>` | | a colored highlight: `yellow`, `green`, `blue`, `pink` or `purple` |
| `` `code` `` | `Ctrl + Shift + K` | inline code |

- With a selection, the key wraps it in the signs. The same key takes the style off again.
- Without a selection, the signs are inserted and the cursor waits between them.
- Over several lines, each line gets its own signs. Bullets, blanks and code blocks stay as they are.
- `==` does not highlight inside a word, so the `abc==` of an address stays as it is.
- `<mark>` is plain HTML, so other apps still show the text highlighted.
- In a table, a selection over several cells, or a row, column or table taken by its grip, styles the text of each cell (`| **Léa** | **31** |`), never a `|`. Empty cells are skipped. One `Ctrl + Z` undoes the whole gesture.

## The formatting bar

A small bar at the bottom of the note, centered, applies styles with a click: bold, italic, underline, strikethrough, highlight (with a choice of five colors), code, link, then bulleted, numbered and task lists, and a table.

- A button lights up when the selection already has the style. A second click removes it.
- Inside a table, the table button gives way to row and column buttons: add, delete.
- With two panes, only the one you write in shows the bar.
- When the word count needs room, the bar slides left. When the note is too narrow for both, it folds into one button (a letter with an underline) that opens it above the text. In a very narrow window its buttons wrap onto several lines. `Esc` or a click in the text closes it.

**Formatting bar** in the palette shows or hides it. See [Settings](settings.md).

## Lists

- `- `, `1. ` and `[] ` start a list. `Enter` continues it. On an empty item, `Enter` leaves the list.
- Numbering stays in order as you add, remove or indent items.
- `Tab` and `Shift + Tab` indent and outdent the selected lines, or the current list item.
- `Ctrl + Enter` checks or unchecks a task or a `[ ]` box, and turns a bullet into a task. In a table cell, it adds a box.

`Ctrl + Shift + B` turns the selected lines (or the line of the cursor) into a list, then into the next kind each time: bullets, numbered, tasks, plain text.

- The right-click menu and the palette (`bulleted`, `numbered`, `task`, `plain text`) go straight to the kind you want. The same kind again gives back plain text.
- Indentation is kept. A numbered list restarts at 1 on each level. A ticked box stays ticked from one task list to another.
- Headings, tables, quotes and code are left alone.
- One `Ctrl + Z` undoes it. Each kind can get its own key in the help panel.

## Slash commands

Type `/` at the start of a line or of a word: a list offers what can be inserted there.

- Keep typing to filter it by name (`/h2`, `/todo`, `/table`) or by label (`/panel` lists the five panels).
- `Up` and `Down` choose, `Enter` or `Tab` inserts, `Esc` dismisses.

What lands in the note is plain Markdown.

| Command | Inserts |
|---|---|
| `/h1`, `/h2`, `/h3` | a heading |
| `/list`, `/num`, `/todo` | a bullet list, a numbered list, a task |
| `/note`, `/tip`, `/important`, `/warning`, `/caution` | a colored panel |
| `/quote`, `/rule` | a quote, a divider |
| `/code`, `/mermaid`, `/math` | a code block, a Mermaid diagram, a formula, with the cursor inside |
| `/table` | a table, after you chose its size on a grid |
| `/link`, `/image` | `[[` or `![[`, and their suggestions |
| `/date` | today's date (`2026-10-09`), in place |
| `/meta` (also `/tags`, `/alias`) | a YAML front matter at the top of the note, wherever you are, with the cursor between the brackets of `tags: []`. If the note has one, the cursor goes there. |

## Blocks

Pointing at a line shows a grip on its left (`⋮⋮`). A block is a heading, a line of text, a list item with what is indented under it, a quote or a panel, a code block, or a table.

- **Drag the grip** to move the block. A line shows where it lands.
- **Click the grip** to select the block and open its menu: turn it into text, a heading, a list, a quote or a code block; duplicate it; move it up or down; copy, cut, comment or delete it.
- **The `+`** left of the grip adds a line under the block and opens the `/` list.
- `Alt + Up` and `Alt + Down` move the block of the cursor. In a table, they move its row.

Blank lines between blocks travel with them. The first line, the title of the note, stays first.

## Links in the text

- `Ctrl + K` makes the selection a link, `[text](address)`. On a link, it selects the address to change it. It does not work over several cells of a table.
- `Ctrl + V` over a selection: a pasted address makes it a link.
- `Ctrl + click` opens a `[[link]]`, a `#tag`, a URL or a `[text](address)` link.

Links between notes are covered in [Links, embeds and files](links.md).

## Several cursors

- `Ctrl + D` selects the word under the cursor. Pressed again, it adds a cursor on its next occurrence.
- `Alt + click` adds a cursor where you click (not in a table).

What you type, delete or move then applies to every cursor, and one undo undoes it for all. `Esc` goes back to one cursor.

## Outline

`Ctrl + Shift + O` (or **Outline of the note** in the palette) lists the headings of the note, indented by level. The section the cursor is in is selected.

- Type to filter.
- `↑` and `↓` show each heading in place as you move.
- `Enter` leaves the cursor there. `Esc` puts it back where it was.

Headings inside code blocks are not listed.

## Copy

`Ctrl + Shift + C` copies the code block the cursor is in, otherwise the whole note. The icon at the bottom right of the note copies the note too.

## Editing keys

| Key | Action |
|---|---|
| `Ctrl + Z`, `Ctrl + Shift + Z` | undo, redo |
| `Ctrl + ←` `→`, `Ctrl + Backspace` | move, delete by word |
| `Ctrl + Home`, `Ctrl + End` | start, end of the note |
| `Ctrl + A` `C` `X` `V` | select all, copy, cut, paste |
| Double click, triple click | select a word, a line. Keep dragging to extend by words, by lines. |
| `Ctrl + +`, `Ctrl + -`, `Ctrl + 0` | bigger, smaller, default text size |
