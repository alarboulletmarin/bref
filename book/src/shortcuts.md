# Keyboard shortcuts

<!-- Written by the test `shortcuts_page_is_current` from `help_sections` (src/shell/keymap.rs).
     Do not edit by hand: run `make docs`. -->

Every line of the help panel (`F1`), with the default keys of Linux and Windows. On macOS, read `Cmd` for `Ctrl`, and `Alt` for `Ctrl` when moving by word. Moving between panes is `Cmd + Alt + arrows`, and back and forward are `Cmd + [` and `Cmd + ]`.

`Ctrl+P › word` means: open the palette and type that word. A line without keys has no default shortcut. Any line that names an action can be given keys in the help panel: see [Changing a shortcut](settings.md#changing-a-shortcut).

## Notes

| Keys | Action |
|---|---|
| `Ctrl+P` | Find by name or text, create, filter by \#tag |
| `F10` / `Ctrl+Shift+P` | Command palette: every command, found by what it does (also the first button of the rail, or \> in the palette) |
| `Ctrl+F` | Find in the note |
| `Ctrl+R` | Find and replace in the note |
| `Ctrl+Shift+F` | Search the text of the whole vault, in the panel: matches note by note, replace |
| `F3` | Search bar: next match (Enter too) |
| `Shift+F3` / `Shift+Enter` | Search bar: previous match |
| `Alt+C` | Search bar: match case |
| `Alt+W` | Search bar: whole words |
| `Alt+R` | Search bar: regular expression ($1 in the replacement) |
| `Tab` | Search bar: replace field |
| `Ctrl+Enter` | Search bar: replace all (Enter: this match) |
| `Ctrl+Shift+Enter` | Search bar: replace in the whole vault, after showing the count |
| `Ctrl+Shift+O` | Outline: jump to a heading of the note |
| `Ctrl+N` | New note |
|  | New note from a template (a note of the Templates folder: {{title}} {{date}} {{time}} {{cursor}}) |
| `Ctrl+P › template` | Insert a template at the cursor, or make a template of the open note |
| `Ctrl+J` | Today's note: open it, or create it |
| `Ctrl+P › capture` | Quick capture: sets up a shortcut of your desktop (Super+Shift+N on GNOME) that opens a one-line window anywhere; what you type goes to today's note |
| `Ctrl+O` | Change vault |
| `Ctrl+P › terminal` | Open a terminal in the vault |
| `Ctrl+Shift+S` | Sync the vault with git: commit, receive, merge, send; a conflict keeps both versions as two notes |
| `Ctrl+P › clone` | Clone a vault from a git address (also on the welcome screen) |
| A cloud folder | iCloud, Dropbox, Google Drive, Syncthing: put the vault there, nothing else; never a git repository inside one |
| `Ctrl+Shift+C` | Copy the code block, else the note |
| `Ctrl+click` | Open a \[\[link\]\], \#tag or URL |
| `F1` / `Ctrl+/` | This help |
| `Ctrl+Q` | Quit |

## Navigation

| Keys | Action |
|---|---|
| `Alt+Left` | Back: what was shown before, at the same place (also the mouse button, and the rail) |
| `Alt+Right` | Forward: what was shown after going back |
| `Ctrl+\` | Split the pane: a new one on its right, opened on the palette to choose what goes there |
| `Ctrl+Alt+\` | Split the pane: a new one below it |
| `Alt+Shift+Left` | Go to the pane on the left |
| `Alt+Shift+Right` | Go to the pane on the right |
| `Alt+Shift+Up` | Go to the pane above |
| `Alt+Shift+Down` | Go to the pane below |
| `Ctrl+1` | Go to the first pane |
| `Ctrl+2` | Go to the second pane |
|  | Move the tab to the pane on the left (split off if there is none) |
|  | Move the tab to the pane on the right (split off if there is none) |
|  | Move the tab to the pane above (split off if there is none) |
|  | Move the tab to the pane below (split off if there is none) |
|  | Give every pane the same room |
| `Alt+Shift+Enter` | The pane on the whole window, and back (the others keep what they hold) |
|  | Move the panel's view to the other dock (left or right of the panes); the rail shows or hides it where it lives |
|  | Open the graph or the calendar as a tab; closing the tab gives it back to its dock |
| `Shift+click on the rail, title of a view` | Shift+click stacks the view under those shown in its dock; drag its title to another place in a dock, or to the right edge to open the right dock. Escape lets go |
| Dragging a tab | Onto a tab bar: it moves there; onto the middle of a pane: it joins it; near an edge of a pane: a new pane opens there with it. A file from the tree does the same. Escape lets go without moving anything |
| Border between panes | Drag it to share the room; double click: even shares |
| Opening a file | Click in the tree, choose in the palette, follow a link: the file opens in a tab of its own, or its tab comes to the front if it is already open; the arrows in the tree preview notes in a single italic tab |
| `Ctrl+W` | Close the tab that has the focus; the pane goes with its last tab |
| `Ctrl+Alt+T` | Reopen the last closed tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Ctrl+Shift+Page Up` | Move the tab one place to the left |
| `Ctrl+Shift+Page Down` | Move the tab one place to the right |
| Middle click | On a tab: close it |
| `Ctrl+Enter` | In the palette: open the chosen note in the other pane |
| `Ctrl+Shift+click` | Open a \[\[link\]\] in the other pane |
| Drag from the tree | Onto a pane: open the file there; onto the right half of a single pane: open a second one |
| `Ctrl+E` | Vault tree |
| `Ctrl+Shift+T` | Recent notes |
| `Ctrl+G` | Graph of the notes |
| `Ctrl+T` | Tags |
| `Ctrl+L` | Backlinks: the notes that link to this one |
| `Ctrl+Shift+J` | Calendar: arrows, Page Up/Down for the month, Enter opens the day (a dot marks the days with a note) |
| A picture | Shown in place of the note; a square in the graph |
| A .csv or .tsv | A grid in place of the note; not in the graph |
| `Ctrl+M` | Panel on the whole window |
| `Arrows` / `Tab` | Select and preview / linked notes (graph) |
| `Enter` / `Esc` | Open the note / back to the note |
| `Ctrl+Shift+N` | New folder (also in the palette) |
| Point at an icon | Its name and its shortcut |
| `Alt + drag` | Move the window from anywhere (Linux) |
| `F2` | Panel: rename |
| `Ctrl+D` | Panel: duplicate |
| `Delete` | Panel: move to the trash |
| `Ctrl+click` / `Shift+click` | Select several rows: move, duplicate, trash them together |
| Right click | Copy the link or the path, set an icon, show a folder or a tag as a list… |
| `Tab` | From the panel: go from button to button (Shift+Tab: back); Enter or Space presses it |
| `Shift+Tab` | Back to the previous button |
| `Shift+F10` / `Menu` | The same menu from the keys, for the selected row or the cursor: arrows, Enter, Esc |
| In a list | Type to filter (key:text for one column), click a cell to rewrite it |
| Drag a node | Move it in the graph, linked notes follow |

## Diagrams

| Keys | Action |
|---|---|
| `Ctrl+Shift+D` | New diagram: an SVG file in the vault |
| Dock, bar over the selection | The dock at the bottom: select, shapes, arrows and relationships (K), text, library (S), with their names; select something and a bar floats over it: colors, fill, dashes, the relationship of an arrow, its heads, route and texts, duplicate, delete |
| 1 … 6 | The colour of the selection, as the palette gives them; the menus (arrow keys, Enter) and the library are walked with the keys |
| Handles, right click | Point at a shape: four handles, drag one to another shape to draw an arrow; a right click opens the menu of what is under it (write in it, turn an arrow around, route, duplicate, delete) |
| R U O D C P N T | Rectangle, rounded, ellipse, diamond, cylinder, person, note, text |
| Selected shape, loop button | A transition from a state to itself, its label written at once; \`trigger \[guard\] / effect\`, the guard is in italics |
| Sequence: arrows between lifelines | A message's height follows its order: drag it up or down to reorder; a call that waits opens a bar until its answer; the bar of the bar numbers them; a fragment (S: alt, opt, loop, par…) picked with messages selected wraps them, the first line of its text is the operator, each line after it an operand |
| Activity (S): actions, decisions, bars, pins | A pin dropped on the border of a shape sits on it, follows it and is removed with it; swimlanes (G F W) are columns or rows; the guard of a flow is its text |
| Components (S): components, ports, nodes | A port sits on the border of a component and goes with it; a node, a device and an execution environment hold the artifacts and nodes dropped on them; the ball and the socket (K B, K Q) are its interfaces |
| G F W | Package, frame, swimlane: they hold the shapes dropped on them, and move, copy and remove them together |
| `S` | Shape library: classes, use cases… with their text; type to search, Enter picks, drag a row onto the canvas; starting points open a new diagram |
| `K` | Relationships: a letter picks one (A N X G C H Z D, U I S L E M for a dependency with its stereotype, R association class, B ball, Q socket, J V «deploy» «manifest», F O W P T the messages of a sequence diagram, Y n-ary association); on selected arrows it changes them |
| `A` / `L` / `V` | Arrow / line, held by the shapes they join / select |
| Enter, double click | Write in the shape or on the arrow; Esc when done |
| `Tab` | Arrow: the text at its start, in its middle, at its end (a role, a multiplicity); drag one along the stroke |
| `---` | Alone on a line of a box: a compartment (UML class) |
| `«interface»  + static x: int  abstract f()` | In a class: a stereotype line, then members; static is underlined, abstract in italics; name : Type is an object |
| `Shift+click` / `Ctrl+click` | Add a shape to the selection, or take it out |
| `Delete` | Remove the selection |
| `Ctrl+D` | Diagram: duplicate the selection |
| `Ctrl+Z` | Diagram: undo |
| `Ctrl+Shift+Z` | Diagram: redo |
| Edge of a shape | An arrow end dropped there stays there; in the middle, it follows the other end |
| `Wheel` / `+ - 0` | Move the view / zoom in, out, fit all |
| `![](Schéma.svg)` | Show the diagram in a note |
| `.svg` | An SVG from elsewhere (draw.io, Mermaid.js…) opens in a viewer: drag or wheel to move, + - 0 to zoom |
| Picture, formula, diagram | Point at one in a note: a button at its corner shows it full size, to zoom and move around (Esc goes back) |
| `Ctrl+P › import` | Bring in an Excalidraw or draw.io file |

## CSV tables

| Keys | Action |
|---|---|
| .csv .tsv | Opens in a grid; encoding and delimiter are detected, written straight into the file |
| Arrows, Tab | Move; with Shift (or drag), select a range; click a row number or a header |
| `Ctrl+click` | Add another selection: copy, cut, empty or remove their rows together |
| Type, Enter, F2 | Replace the cell / validate and go down / open the cell; Esc cancels |
| `Ctrl+C` | Table: copy as tab-separated text |
| `Ctrl+X` | Table: cut |
| `Ctrl+V` | Table: paste cells from a spreadsheet, Markdown or CSV |
| `Shift+Space` | Table: select the whole rows of the selection |
| `Ctrl+Space` | Table: select the whole columns of the selection |
| `Ctrl+Enter` | Table: insert a row below (a column to the right when whole columns are selected) |
| `Ctrl+Shift+Enter` | Table: insert a row above (a column to the left when whole columns are selected) |
| `Ctrl+Delete` | Table: remove the selected rows, or the columns when whole columns are selected |
| `Ctrl+Z` | Table: undo |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Table: redo |
| `Ctrl+P › table` | Delimiter, encoding, header row, copy as…, export (CSV, TSV, Markdown, JSON) |

## Appearance

| Keys | Action |
|---|---|
| `Ctrl+P › theme` | Theme, previewed as you browse |
| `Ctrl+P › font` | Font of the app / of the code |
| `Ctrl+P › page width` | Width of the text column: narrow, normal, wide, full |
| `Ctrl+P › wide tables` | A table wider than the text: in the column, or centered and wider |
| `Ctrl+P › animations` | Short fades (about 0.13 s) when the panel, the formatting bar or a folder opens; none on typing, scrolling or hover. Off: everything is instant (motion=off in settings) |
| `Ctrl+=` / `Ctrl++` | Bigger text; the panel, menus and help follow, up to 150 % |
| `Ctrl+-` | Smaller text; the interface follows, down to 85 % |
| `Ctrl+0` | Default text size |

## Tables in a note

| Keys | Action |
|---|---|
| `Grips` | Over a table: a small grip above the column and left of the row under the pointer, a round one at the corner for the whole table. Click one to take the column, the row or the table (copy, cut, erase, or any gesture below), drag it to move it, Esc lets go; the + between two grips inserts there |
|  | Insert a row above |
|  | Insert a row below |
|  | Duplicate the row |
| `Alt+Up` | Move the row up |
| `Alt+Down` | Move the row down |
|  | Delete the row |
|  | Insert a column on the left |
|  | Insert a column on the right |
|  | Duplicate the column |
|  | Move the column left |
|  | Move the column right |
|  | Delete the column |
|  | Sort the rows by this column, A to Z |
|  | Sort the rows by this column, Z to A |
|  | Swap rows and columns |
|  | Delete the table |
| `Ctrl+Shift+Backspace` | Remove the row, or the columns taken by their grip |
| `Formulas` | A cell that starts with = computes: =B\*C, =sum(above), =avg(B1:B9), =round(B/C, 2), =if(B\>10, 1, 0). Columns are letters, rows are numbers from the first row under the header (C2, B1:B10); B alone is column B of this row; above, left and column are ranges. Functions: sum avg min max count round abs if. The cell shows its result (right-aligned, a different color); the formula shows when the cursor is in it, and the file keeps it. A cell with no result shows \#ERR: point at it for the reason. Completion offers functions, ranges and columns after = |
| `Alt+=` | Write a total in the cell: the sum of the cells above, or on its left |
| `Ctrl+Alt+C` | Copy the cells (the taken ones, or the table) with the results of their formulas, for a spreadsheet |

## Typing

| Keys | Action |
|---|---|
| `/` | Components: headings, lists, panel, table, code, formula… |
| `/table` | Table: pick its size on the grid (mouse, arrows), or type it: 12x5 |
| \> \[!NOTE\] | Colored panel: NOTE, TIP, IMPORTANT, WARNING, CAUTION |
| -  | Bullet list |
| 1.  | Numbered list |
| \[\]  | Task |
| \# \#\# \#\#\# | Headings; the first \# names the file |
| \>  | Quote |
| `` ```rust `` | Code block: colors, copy icon |
| `---` | Divider |
| `[[` | Link to a note |
| `Ctrl+P › kanban` | New kanban board: cards you type, drag and tick; double click to rewrite |
| Board: arrows | Go from card to card, to a column's title, to Add a card or Add a column; Space ticks the card |
| `Enter` / `F2` | Board: write the card or the title (Enter also adds what is chosen) |
| `Delete` | Board: remove the card, or the column with its cards (undone with the board's undo) |
| `Alt+Up` | Board: carry the card up |
| `Alt+Down` | Board: carry the card down |
| `Alt+Left` | Board: carry the card to the column on the left |
| `Alt+Right` | Board: carry the card to the column on the right |
| `@` | Link to the note of a day: @today, @monday, @2026-10-09 |
| `![](image.png)` | Picture, under its line |
| `![[` | Suggests the pictures and diagrams of the vault |
| Drop files | Drag files or folders from your file manager onto a note (a line shows where) or onto the tree (the folder under the pointer): they are copied, never overwritten, and a note gets a link per item |
| `Double` / triple click | Select a word / a line; drag to extend by words / lines |
| `` ```mermaid `` | Diagram, under its block; its magnifier shows it full size |
| $x^2$  $$…$$ | LaTeX formula, under its line |
| -\> != \<= =\> | Shown as → ≠ ≤ ⇒ |

## Editing

| Keys | Action |
|---|---|
| `Enter` | Continue the list, or leave it on an empty item |
| `Tab` / `Shift+Tab` | Indent / outdent |
| `Tab` / Enter in a table | Next cell / new row; columns stay aligned; + buttons add a row or a column |
| `Ctrl+Shift+L` | Align the column of the cursor to the left |
| `Ctrl+Shift+E` | Center the column of the cursor |
| `Ctrl+Shift+R` | Align the column of the cursor to the right |
| Pasting a table | Spreadsheet cells or Markdown fill the grid, or become a table |
| `Ctrl+Shift+B` | Turn the selected lines into a list, then the next kind: bullets, numbered, tasks, plain text |
|  | Selected lines: bulleted list (again: plain text) |
|  | Selected lines: numbered list (again: plain text) |
|  | Selected lines: task list (again: plain text) |
|  | Selected lines: plain text |
| `Ctrl+Enter` | Check / uncheck a task or a \[ \] box; in a table cell, add one |
| `Ctrl+B` | Bold |
| `Ctrl+I` | Italic |
| `Ctrl+U` | Underline |
| `Ctrl+Shift+X` | Strikethrough |
| `Ctrl+Shift+H` | Highlight |
| `Ctrl+Shift+K` | Inline code |
| `Ctrl+P › formatting bar` | Under the note, centered: bold, italic, underline, strikethrough, highlight (five colors), code, link, lists, tasks, table (in a table: add or delete a row or a column); a lit button means the selection has the style, click again to remove it; only the active pane shows it; in a narrow note it folds into a button at the bottom, in the middle, that opens it; hide it with the palette |
| `Ctrl+D` | Several cursors: select the word, then its next occurrence (Esc: back to one) |
| `Alt+click` | Add a cursor |
| `Ctrl+K` | Link: \[text\](address) from the selection; on a link, change its address |
| Embed a section | !\[\[Note\#Heading\]\] alone on a line shows that heading and what is under it, up to the next heading of the same level, in place and live: !\[\[Note\]\] the whole note (without its front matter), !\[\[Note\#^id\]\] the block that ends with ^id. Framed, with the source's name at the top right: a click on it, or a double click on the frame, opens the note there (Alt+Left comes back). Typing !\[\[Note\# offers the headings of the note, \#^ its blocks. Read-only; an embed that leads back to itself says so, and three levels are shown at most. Renaming the note or the heading rewrites the embeds and links to it |
| `Alt+Enter` | On the line of an embed: open its note, at the section |
| `Ctrl+Shift+M` | Comment the selection or the line (in a table: the cell, or the taken row, column or table); on a comment, resolve it |
| `Ctrl+P › comments` | Comments: show their panel, jump to one (a badge on the speech bubble counts them) |
| Right click | Menu of the note: link, comment, cut, copy, paste, bold, italic |
| `Ctrl+C` | Copy |
| `Ctrl+X` | Cut |
| `Ctrl+V` | Paste text or a picture; an address over a selection makes it a link |
| `Ctrl+A` | Select all |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Redo |
| `Ctrl+Left` / `Right` | Move by word |

## Blocks

| Keys | Action |
|---|---|
| `Grip` | Left of the line under the pointer: drag it to move the block (a line shows where it lands), click it to select the block and open its menu; the + left of it adds a block under it and opens the / list |
|  | Block: turn into text |
|  | Block: turn into heading 1 |
|  | Block: turn into heading 2 |
|  | Block: turn into heading 3 |
|  | Block: turn into a bulleted list |
|  | Block: turn into a numbered list |
|  | Block: turn into a task list |
|  | Block: turn into a quote |
|  | Block: turn into a code block |
|  | Block: duplicate |
| `Alt+Up` | Block: move up (in a table, the row) |
| `Alt+Down` | Block: move down (in a table, the row) |
|  | Block: delete |
