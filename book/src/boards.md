# Kanban boards and list pages

## Kanban boards

Type `kanban` in the palette and choose **New kanban board**. A note opens as a board with three columns.

**With the mouse**

- **+ Add a card** opens a card to type in. `Enter` adds it and opens the next one; `Esc` stops.
- A double click rewrites a card or a column title in place. A card emptied this way is removed.
- The `+` on the right adds a column.
- Pick a card up and it follows the pointer, while a dashed slot opens where it would land: between two cards, or at the bottom of a column. Held near the edge of the window, it scrolls the board.
- Click a card's box to tick it.

**With the keys**

| Key | Action |
|---|---|
| Arrows | go from card to card, to a column's title, to **+ Add a card**, and to the `+` of a new column |
| `Alt` + arrows | carry the card up, down, or into the next column |
| `Space` | tick the card |
| `Enter`, `F2` | write the card |
| `Delete` | remove the card; on a title, the column with its cards |
| `Ctrl + Z` | undo the last gesture, right on the board |

A card being written is a real line of text: arrows, word by word with `Ctrl`, selection with `Shift`, `Ctrl + Backspace`, copy and paste, a click to place the cursor. The same goes for the palette and a cell of a CSV table.

Columns narrow to fit the window.

**In the file**, the note stays plain Markdown, in a common Markdown kanban layout: one `## heading` per column, and one task per card. The key `kanban: true` in its front matter is what makes it open as a board. Typing that key into a note leaves you in the text.

The button at the bottom right, left of the copy button, shows that text, and comes back to the board.

## List pages

Right click a folder in the tree, or a tag in the tags panel, then **Show as a list**. Its notes become the rows of a table, and the keys of their front matter (`author:`, `rating:`…) its columns.

- **Filter**: type to filter the rows. `author:austen` looks in one column only. `Esc` clears.
- **Sort**: click a column title to sort by it, again to reverse.
- **Open**: click the name of a note.
- **Edit**: click any other cell to rewrite it. `Enter` confirms, `Esc` gives up. Only that key changes in the front matter of the note: it is added if it was missing, removed if you empty it. Nothing else in the file changes.

A folder dragged from the tree onto a pane, or a `[[Folder]]` link when no note has that name, also opens as a list page.
