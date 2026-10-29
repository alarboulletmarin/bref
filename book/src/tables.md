# Tables and formulas

## Make a table

- **`/table`** opens a grid under the cursor. Move the pointer over it, or use the arrows, to choose the number of columns and rows, then click or press `Enter`. The empty table is written, and the cursor waits in its first cell. For a table larger than the grid, type its size while the grid is open, columns first: `12x5`.
- **By hand**: `| a | b |` then `Tab` is enough. The line of dashes is written for you.
- **The formatting bar** has a table button too.

## How a table looks

A table is drawn as a grid, like on the web:

- Columns share the width of the page. A long cell wraps inside its column, and its row grows. A short column keeps its natural width.
- The header row is in bold. `:--`, `:-:` and `--:` under it align a column.
- The text underneath is plain Markdown. The `|`, the line of dashes and the padding are not drawn.
- A literal `|` in a cell is written `\|` and shown as `|`.
- The columns of the text are aligned as you type, so the file reads well in other editors. Wide characters (ideograms, emoji) count as two. A table too wide to stay aligned is written compactly.
- A table wider than the text can spill out on both sides of the column, or scroll inside it. See **wide tables** in [Settings](settings.md#page-width). Only a table with more columns than the page can show, even at their narrowest, scrolls sideways (`Shift + wheel` or a horizontal swipe), following the cursor.

## Move and type

- `Tab` and `Shift + Tab` go to the next and previous cell, and select its content: typing replaces it. After the last cell, `Tab` adds a row.
- `Enter` adds a row under the current one. On a last row left empty, it removes that row and leaves the table.
- Arrows move from cell to cell without stopping on the `|`. `Up` and `Down` follow the lines of a wrapped cell, then go to the cell below.
- `Backspace` and `Delete` stop at the edge of a cell.
- Selecting several cells and erasing, cutting or typing empties them, without merging the columns.
- `Ctrl + Shift + L`, `E` and `R` align the column of the cursor left, centered or right.

## Rows and columns

Two `+` buttons appear when the pointer or the cursor is on a table: the one below adds a row, the one on the right adds a column.

**Grips.** With the pointer over a table, a small grip (three dots) shows above the column and left of the row under it. A round one sits near the top left corner. A click takes the column, the row or the whole table, and outlines it. Then:

- `Ctrl + C` and `Ctrl + X` copy and cut it as a Markdown table. It pastes as a table in another note, an issue or a mail.
- `Delete` empties its cells. The shape of the table stays.
- Drag a column or row grip to move it. A line shows where it will land, and `Esc` lets go. The header row stays first.
- The `+` between two grips inserts a column or a row right there.
- A right click on a grip takes it and opens the menu.

**The menu.** Right click in a table, or type the name of the gesture in the palette (`row`, `column`, `sort`, `swap`):

- insert a row above or below, or a column left or right;
- duplicate, move or delete a row or a column;
- sort the rows on the column of the cursor, A to Z or Z to A: numbers by value, empty cells last, the header stays put;
- swap rows and columns;
- delete the whole table.

Gestures apply to every row and column the selection touches. One `Ctrl + Z` takes a gesture back.

**Keys.** `Alt + Up` and `Alt + Down` move the row (the header stays first). `Ctrl + Shift + Backspace` deletes it, or the columns taken by their grip. Every gesture is in the help panel, where it can get a key of its own.

Deleting the header makes the next row the header. Deleting the last column or row deletes the table. To remove a column by hand, delete its cells.

## Paste

- **In a table**, text goes into one cell, on one line, its `|` escaped. Cells copied from a spreadsheet or a web page (tab-separated), or from another Markdown table, fill the grid from the current cell. Rows and columns are added as needed.
- **Under a table**, a pasted line becomes part of it, and its columns line up.
- **Anywhere else**, a pasted Markdown table or tab-separated cells become a table on their own lines, aligned, with the line of dashes added.

## Formulas

A cell that starts with `=` computes. The file keeps the formula; other apps show its text.

```text
=B*C
=sum(above)
=avg(B1:B9)
=round(B/C, 2)
=if(B>10, "big", "small")
```

**References**

- Columns are letters. Rows are numbers, counted from the first row under the header, as in a spreadsheet: `C2`, `B1:B10`.
- `B` alone is column B of the formula's own row, so the same formula works on every line.
- `above`, `left` and `column` (the other cells of the column) are ranges by position.

**Functions and operators**

- Functions: `sum`, `avg` (or `average`), `min`, `max`, `count`, `round`, `abs`, `if`.
- Operators: `+ - * / ^ %`. `15%` is a hundredth, `7 % 3` a remainder.
- Comparisons: `= <> < <= > >=`. Parentheses group.
- Arguments are separated by `,` or `;`. Decimals in a formula use a point.

**Numbers in cells** are read as written: `1 234,5`, `1,234.5`, `12.50 €`, `$3`, `15%`. With a single separator, it is the decimal one: `1,234` is 1.234. Write `1 234` or `1,234.00` for a thousand. The sort of a table reads numbers the same way. Text and empty cells are left out of sums.

**Results**

- The result shows in the cell, right-aligned, in another color. With the cursor in the cell, the formula shows instead, ready to edit.
- A cell with no result shows `#ERR`. Pointing at it says why: unknown function, reference out of the table, a cell that depends on itself, a text where a number is needed, a division by zero…
- Results follow what you type. Only a table that has formulas is computed, and not a very large one. A table of 1,400 rows with a running total in each row takes about 25 ms per keystroke.

**Helpers**

- `Alt + =` writes a total in the cell: the sum of the cells above, or on its left when only those hold numbers. A cell with something else in it is not overwritten.
- After `=`, completion offers functions, ranges, and the columns by letter or header.
- `Ctrl + C` on cells keeps the formulas: they paste back as formulas.
- `Ctrl + Alt + C`, or **copy the cells with their results** in the palette, copies the taken cells (or the table) with their results, as tabs and line breaks, for a spreadsheet.

Formulas are for the tables of a note, not for the [CSV grid](csv.md).

## Very large tables

Past 1,500 rows (a whole CSV turned into Markdown, for instance), a table is no longer laid out as one block:

- Its columns are sized from its first 300 rows.
- Cells stop wrapping, and what overflows is cut, so every row has the same height.
- Only the rows near the screen are drawn.

A table of 300,000 rows opens in about 40 ms, and typing in it takes under 10 ms.

Such a table keeps `Tab`, `Shift + Tab` and `Enter`, but has no grips, no row and column gestures, and its columns are not realigned as you type.
