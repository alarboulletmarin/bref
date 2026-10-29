# CSV and TSV files

A `.csv` or `.tsv` file of the vault shows in the tree and opens in a grid, in place of the note. The file is read and written directly: there is no import, and the rows you did not touch are written back byte for byte.

## Detection

- **Encoding**: from the byte order mark; else UTF-8 if the file is valid UTF-8; else Windows-1252. Bref reads and writes UTF-8, UTF-16, ISO-8859-1 and Windows-1252.
- **Delimiter**: `,` `;` tab `|` `:` or space. Bref picks the one that splits most lines into the same number of fields, quotes respected.
- **Header row**: the first row is a header unless it holds numbers.

**To change them**, click one in the bar above the grid, or search `table` in the palette. The file is read again that way, and the choice is kept for that file. **Detect automatically** forgets it.

## Move

| Key | Action |
|---|---|
| Arrows, `Tab`, `Shift + Tab` | move by one cell |
| `Page Up`, `Page Down` | move by one screen |
| `Home`, `End` | start, end of the row |
| `Ctrl + Home`, `Ctrl + End` | the corners of the table |
| `Ctrl + Up`, `Ctrl + Down` | the ends of the column |

Click a cell, or use the wheel (`Shift + wheel` sideways) and the scroll bars.

## Select

- `Shift` + arrows, or drag with the mouse.
- Click a row number or a column header for the whole row or column. The corner, or `Ctrl + A`, selects everything.
- From the keyboard, `Shift + Space` takes the whole rows of the selection, and `Ctrl + Space` its whole columns, as in a spreadsheet.
- `Ctrl + click` (on a cell, a row number or a header) keeps what is selected and starts another selection. Copy and cut take them all, one block under the other. `Delete` empties them all, and `Ctrl + Delete` removes all their rows (all their columns, when they are whole columns). Typing and pasting go to the last one.

## Write

- Typing replaces the cell. `Enter` or `F2` opens it as it is.
- `Enter` validates and goes down. `Tab` validates and goes right. `Esc` cancels.
- `Delete` empties the selection.
- A cell being written edits like any line of text: word moves, selection, paste.

## Rows and columns

**Rows**: `Ctrl + Enter` inserts one below, `Ctrl + Shift + Enter` above. `Ctrl + Delete` (or `Ctrl + Shift + Backspace`, as in a note) removes the selected ones.

With whole columns selected (`Ctrl + Space`), the same keys act on columns: as many empty columns to the right or to the left, or the selected columns removed.

**More gestures** are the same as for the tables of a note. Find them in the palette (`column`, `sort`, `row`), or give them keys in the help panel:

- insert a column left or right of the selection;
- duplicate, move or delete the selected columns;
- move or duplicate the selected rows (`Alt + Up`, `Alt + Down` move them; the header stays on top);
- sort the rows under the header on the column of the cursor, A to Z or Z to A: numbers by value, empty cells last.

**The bar above the grid** names whole columns or rows when they are selected (a click on their header, or `Ctrl + Space`), next to the name of the file. It offers their gestures as icons: for columns, insert left or right, move, sort, delete; for rows, insert above or below, move, duplicate, delete. Pointing at an icon names it, with its keys.

`Ctrl + Z` and `Ctrl + Shift + Z` undo and redo. One undo takes each gesture back, a paste included, and the file gets its exact bytes again.

The last column cannot be deleted. Columns and sorting read the whole file, which is fine up to a few hundred thousand rows. Moving columns on a row shorter than they are leaves it a few empty cells longer after the way back.

## Copy, paste, export

- `Ctrl + C` copies the selection as tab-separated text, which spreadsheets and the tables of your notes paste as they are. `Ctrl + X` cuts it.
- `Ctrl + V` pastes cells from a spreadsheet, a Markdown table or CSV text from the selected cell, adding the rows that are missing. One value fills the whole selection.
- In a note, copying cells of a Markdown table gives a Markdown table. A spreadsheet pastes its lines in one column, so use the grid's copy for a spreadsheet.
- **copy table as** and **export table** in the palette (or the buttons of the bar) offer TSV, CSV (with the delimiter of the file), Markdown and JSON. They take the selection, or the whole table when only one cell is selected. An export is written next to the table, under a name that is still free.

## Saving

The file is saved about half a second after the last change, in the background, atomically, in its own encoding and with its own line endings.

- A character the encoding cannot hold (a `€` in ISO-8859-1) is reported, and the file is left alone.
- If another program rewrites the file and nothing waits to be saved, the grid reads it again.

## Limits

- Files up to 1 GiB, read whole in memory (about the size of the file). A million rows are parsed in under a tenth of a second on the author's machine.
- No formulas: they are for the [tables of a note](tables.md#formulas).

CSV and TSV files can be renamed (they keep their extension), moved, duplicated or trashed like a note. They are not in the graph, and a note does not link to them.
