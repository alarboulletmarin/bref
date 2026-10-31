# Tabs, panes and docks

## Tabs

A pane holds as many files as you like, as in a code editor.

- **Opening**: a click on a file in the tree, a note chosen in the palette, a link followed, a new note: each opens in a tab of its own, right of the one shown. If the file is already open, its tab comes to the front instead. A note is open only once.
- **Previewing**: browsing the tree with the arrows previews notes in a single tab, in italics. The next note browsed replaces it. Opening it for good keeps it.
- **The tab bar** appears at the top of the pane as soon as it holds two tabs, each a rounded pill. A click shows a tab. Its cross, or the middle button, closes it.

| Key | Action |
|---|---|
| `Ctrl + Tab`, `Ctrl + Shift + Tab` | next, previous tab |
| `Ctrl + Shift + Page Up`, `Page Down` | move the tab along the bar |
| `Ctrl + W` | close the tab, saving what it holds first. The pane goes with its last tab. |
| `Ctrl + Alt + T` | reopen the last tab closed |

A tab can be dragged along the bar, into another pane, or to an edge to split one. See [Drag a tab](#drag-a-tab).

Tabs come back at the next start. A tab out of sight keeps its text, cursor and undo, but lets go of what it drew, so a hundred tabs cost little memory.

## Back and forward

`Alt + Left` and `Alt + Right` (`Cmd + [` and `Cmd + ]` on macOS) walk through what you opened: notes, pictures, diagrams, tables and list pages alike. So do the back and forward buttons of the mouse, the two arrows at the top of the rail, and **Back** and **Forward** in the palette.

- You land where you were: same line, same scroll.
- Back and forward go from tab to tab, and bring a closed file back in a tab.
- It works like a browser: opening something after going back drops what was ahead.
- A note only previewed (arrows in the tree, a click in the graph) does not count. A renamed file is followed, a deleted one is skipped.
- The history is not kept between two launches. The recent notes are.

## Panes

`Ctrl + \` splits the pane to the right, `Ctrl + Alt + \` below, as many times as the window has room for. Write a note while reading two others, or keep a table under a note.

- A new pane opens on the palette, to choose what goes there. Each pane shows anything Bref can show.
- A pane is split only if both halves keep room to write: 300 px wide, 200 px high. Otherwise a message says so. The very first split folds the panel onto its rail when that is enough.
- The pane with the focus is marked by a line at its top, and a thin accent ring. The palette, the tree and the shortcuts act on it.
- A note already shown in one pane is not opened twice: the focus moves to it.
- The layout comes back at the next launch.

| Key | Action |
|---|---|
| `Alt + Shift + ←` `→` `↑` `↓` | move to the pane on that side (`Cmd + Alt + arrows` on macOS) |
| `Ctrl + 1`, `Ctrl + 2` | move to the first, second pane |
| `Ctrl + W` | close the tab that has the focus, and the pane with its last tab. Its neighbour takes its room. |
| `Alt + Shift + Enter` | put the pane on the whole window, and back. The others keep what they hold. |
| `Ctrl + Enter` in the palette | open the chosen note in the pane beside |
| `Ctrl + Shift + click` on a `[[link]]` | open it in the pane beside |

**Open to the side** in the menu of the tree does the same. The palette also has **Move the tab to the pane on the left** (right, above, below; a pane is split off when there is none on that side) and **Even out the panes**.

**Sizes**: every border between panes can be dragged, down to the smallest size of a pane. A double click shares that split evenly. A window too small for every pane shows the one with the focus alone, and all of them again once it is big enough.

### Drag a tab

Drag a tab to arrange the window:

- dropped on a tab bar, it moves there (a line shows where);
- dropped on the middle of a pane, it joins it;
- dropped near one of its edges, the pane splits on that side and the tab takes the new half.

The part that would receive it is tinted while you drag. `Esc` lets go without moving anything.

A file dragged from the tree does the same. Dropped on the right half of a window that has a single pane, it opens a second pane. A folder opens as its [list page](boards.md#list-pages).

## Docks

Each view of the panel can live on either side of the notes. So the tree and the backlinks, or the graph and the calendar, can stay in sight together.

- **Move a view**: the arrow in its header, or **Move the panel's view to the other dock** in the palette, sends it to the dock on the right, and back.
- **Show or hide**: the rail shows or hides a view where it lives.
- **Stack**: `Shift + click` on its icon stacks a view under the views already shown in its dock. Each has a header that folds it (the chevron) or hides it (the cross).
- **Drag** a view by its title to put it elsewhere. A line shows where it lands in a dock. While it is dragged, a strip on the right edge of the window opens the right dock. `Esc` lets go without moving anything.
- **As a tab**: the graph and the calendar also open as a tab of a pane, to get room: the button next to the arrow, or **Open the graph or the calendar as a tab**. Closing the tab gives the view back to its dock.

The right dock hides when the window leaves the notes too little room. Where everything is comes back at the next launch.
