# Comments

`Ctrl + Shift + M` annotates a word, a line or the line of an image, without changing its text. Without a selection, it comments the line.

## In the file

The comment is kept in the note itself, as [CriticMarkup](https://criticmarkup.com/):

```text
{==the text==}{>>the comment<<}
```

The file stays plain Markdown. In Bref, the text is highlighted and the comment follows it, faded.

## The comments panel

A panel to the right of the note lists its comments.

- Click a comment to go to it.
- Click its check mark (it shows under the pointer) to resolve it: the markup goes, the text stays.
- The comment of the passage under the caret is on a tint, and the panel scrolls to it.
- The speech bubble at the bottom right of the note, next to the copy button, closes and opens the panel. So does the panel's cross.
- A badge on the bubble counts the comments (`9+` beyond nine).
- `comments` in the palette lists them, to jump to one from the keyboard.

On a comment, `Ctrl + Shift + M` resolves it. On a line of its own (see below), the line goes.

## In a table

- **A cell**: the comment is on the text of the cell. A selection over several cells gets one comment per cell, never across a `|`. A `|` typed in the comment is written `\|`.
- **A row, a column or the whole table**, taken by its grip: the comment goes on a line of its own under the table, named by its content. A row is named by its first cell, a column by its header: `{>>Column "Age": in years<<}`.

Because the comment is named by content, a sort leaves it where it belongs:

- Renaming that first cell or header renames the comment.
- Deleting the row, the column or the table deletes it.
- Moving, inserting, duplicating, sorting or swapping rows and columns keeps it with what it is about. One `Ctrl + Z` brings both back.

How it shows:

- A commented cell is tinted, with a mark in its corner.
- A commented row, column or table is outlined, with a dot on its grip. It is stronger while the cursor is in the comment.
- The panel says where each comment is: `Cell C2`, `Row Léa`, `Column Age`, `Table`.

A comment on an empty cell is refused: there is nothing to highlight.
