# The palette and the panel

The [palette](first-steps.md#find-anything-the-palette) (`Ctrl + P`) finds and creates notes. The panel shows the vault in several views.

## The rail and its views

A rail of icons stays on the left of the window. It opens a panel beside the note, in one of these views:

| View | Key | Shows |
|---|---|---|
| Tree | `Ctrl + E` | the vault and its folders. The open note is highlighted. |
| Recent | `Ctrl + Shift + T` | every note, from the most to the least recently opened |
| Graph | `Ctrl + G` | one dot per note, one line per `[[link]]` between two notes |
| Tags | `Ctrl + T` | every `#tag` of the vault with its number of notes. Unfold a tag to see them. |
| Backlinks | `Ctrl + L` | the notes that link to the open note, see [Backlinks](links.md#backlinks) |
| Calendar | `Ctrl + Shift + J` | one month of daily notes, see [The calendar](daily-notes.md#the-calendar) |
| Search | `Ctrl + Shift + F` | every match of a search in the vault, see [Search the vault](search.md#search-the-vault) |

The rail also holds, at the top: the command palette, back and forward, the views, search and new note. At the bottom: the trash, the sync of a vault kept in git (it turns while it runs, and a red dot says the last one failed), backup, theme and help.

- **Fold**: press the key of the view on display, or click its icon. The panel folds back to the rail.
- **Resize**: drag the line between the panel and the note. Dragged all the way left, the panel folds to its rail. All the way right, it takes the whole window. Double-click the line for the default width.
- **Whole window**: `Ctrl + M`, or the arrows button at the top of the panel, puts the panel on the whole window, and back.

Views can also sit on the right of the window, or stack. See [Docks](tabs-and-panes.md#docks).

## Keys in the panel

The same keys work in every view. The calendar has its own arrows.

| Key | Action |
|---|---|
| `↑` `↓` | select a note and preview it beside the panel, in a single tab in italics (in the graph, a click too) |
| `Enter`, or a click | open the note in its tab and start typing |
| `←` `→` | tree and tags: fold or unfold a folder or a tag. Graph: move to the nearest note on that side. |
| `Tab` | graph: walk through the notes linked to the selected one |
| `Tab`, `Shift + Tab` | tree, recent and tags: go along the buttons of the panel and of the rail. A ring shows which one has the keys; `Enter` or `Space` presses it. |
| `F2`, `Delete` | tree, recent and tags: rename the selected note or folder, move it to the trash |
| `Ctrl + Shift + N` | new folder, next to the selected line of the tree |
| `Esc` | back to the note |

A previewed note only counts as opened once you click or type in it. It then moves to the top of the recent list, and its tab stays.

## The tree

- **Order**: names are sorted as in a file manager: `Note 2` before `Note 10`.
- **Fold all**: the double-chevron button at the top of the tree folds every folder. When they are all folded, it unfolds them all.
- **New note**: with the tree on display, `Ctrl + N` creates the note in the folder of the selected line. The button left of the folder button, at the top of the tree, creates it at the root of the vault instead.
- **New folder**: the folder button, `Ctrl + Shift + N`, or `new folder` in the palette. The field says where the folder goes.
- **Several lines**: `Ctrl + click` adds a line to the selection or takes it out. `Shift + click` selects everything from the last line chosen. Dragging, duplicating, copying links or paths, and moving to the trash then apply to all of them.
- **Drag and drop**: drag a note or a folder onto a folder to move it there, or below the last line to move it to the top of the vault. Nothing is ever overwritten: if the name is taken, the move is refused.
- **Rename**: `F2`. Renaming a note whose first line is its name rewrites that line too. The links to it follow, see [Names and renaming](vault.md#names-and-renaming).

**The menu.** Right click a line, or press `Shift + F10` (the `Menu` key) on the selected line:

- open, or open to the side;
- new note here, new note from a template, new diagram here, new folder;
- copy the `[[link]]`, the path, or the path relative to the vault;
- show in the file manager (Finder, Explorer);
- duplicate (`Ctrl + D`), rename, move to a folder chosen in a list (what dragging does);
- set an icon;
- show as a list (on a folder, see [List pages](boards.md#list-pages));
- move to the trash.

A right click below the last line acts on the vault itself. For the open note, the same entries are in the command palette: **Open in the other pane**, **Copy the [[link]] of the note**, **Copy the path of the note**, **Copy the relative path of the note**, **Move the note to a folder…**, **Icon of the note…**, **Show the note's folder as a list**.

## Icons

Right click a note, a file or a folder in the tree, then **Icon…**. Pick one of forty in the grid (arrows and `Enter` work), or the cross to remove it.

- Without one, every row shows a small icon of its kind (folder, note, table, diagram, picture, other file), so the names line up.
- A file that is not a note shows its extension next to its name, lighter (`contacts.csv`). A long name is cut before its extension.
- The icon you give replaces the default one, before the name. It follows a rename or a move.
- Icons are kept in `.bref-icons` at the root of the vault, so they travel with it.

## The graph

One dot per note, bigger with its links.

- The open note is in the accent colour. Its neighbours and their lines are darker.
- Notes without links sit on a ring around the rest. A rounded square is an attachment, such as a picture.
- Names show for the open note, its neighbours and the most linked notes. Pointing at any dot shows its name, and lights up the notes it is linked to.
- Drag the background to move around. Scroll to zoom. The target button glides back to the whole graph.
- Drag a note to move it: the notes linked to it follow, the closest ones the most.
- With the graph on the whole window, a click only selects. `Enter` or a double click brings the note back.

Links to notes that do not exist yet are left out.

## The trash

The trash is the hidden folder `.trash` at the top of the vault. Deleting moves a note or a folder there; nothing is ever destroyed.

- **Undo**: moving something to the trash shows a message with **Undo**. It puts the item back exactly where it was. Its folder is made again if it is gone, and nothing is overwritten.
- **Restore later**: click the bin at the bottom of the rail, or type `trash` in the palette. It lists the trash, and `Enter` returns the chosen item to the top of the vault. The trash does not remember where an item came from. A name already taken gets another one.
- **Empty the trash** with your file manager.
