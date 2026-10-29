# First steps

## Choose a vault

On first launch, Bref asks for a **vault**: the folder that holds your notes.

- **Start in ~/Documents/Notes** creates it there, or in your home folder if you have no `Documents`. A folder already there is opened as it is.
- **Open another folder…** picks an existing one. Dropping a folder on the welcome screen does the same.
- **Clone a vault from a git address…** fetches a vault kept in git. See [Syncing a vault](sync.md).

That is the only setup. Bref remembers the vault and the last note you had open. `Ctrl + O` changes the vault at any time.

On Linux, you can put Bref one key away: bind the command `bref` in your desktop settings (GNOME: Settings → Keyboard → Custom Shortcuts).

## Write

Start typing. The first line is the title of the note, and the name of its file. There is no save button: notes are written to disk as you type. See [Files on disk](vault.md) for how files are named and saved.

`Ctrl + N` creates a new note. `Ctrl + J` opens today's note, or creates it.

The bottom right of the note counts its words and characters, or those of the selection. Characters are counted as written, Markdown markers included, line ends excluded.

## The blank page

An empty note shows, under the line you write on:

- today's date (a click opens the note of the day);
- the keys that matter;
- what to read again: the daily notes of a week, a month and a year ago, and one note you have not touched for a month or more, a different one each day.

A click opens any of them. The first key you type clears the page.

## Find anything: the palette

`Ctrl + P` opens the palette. Type to search, `Enter` opens the selected note.

- Notes whose name matches come first. CSV tables, pictures and diagrams are found by their file name too (`contacts.csv`).
- Then come the notes whose text contains every word you typed (from two letters), with the line where the first word appears.
- Each row shows the note's folder and, on the right, how long ago it was written. A line of keys under the list reminds the main gestures.
- If no note has that name, the last row creates it.
- `#` lists the notes carrying a tag.
- `>` switches to the command palette: `>bold`, `>delete row`.

With nothing typed, on a CSV table or in a table of a note, the palette offers their gestures under the recent notes.

## Every command: the command palette

`F10` (or `Ctrl + Shift + P`, or the first button of the rail) opens the command palette. It lists every command the current view can do, with an icon, a short name and its keys. Commands are grouped, and those of what you are on (a block, a table, a diagram) come first.

Type to find a command by what it does, then `Enter` runs it where you were. Any word of its name works, accents ignored:

- `folder` offers **New folder**;
- `table` offers the settings of the CSV table on display (delimiter, encoding, header row, copy as, export);
- `diagram`, `theme`, `font` and `update` offer theirs too.

## Every shortcut: the help panel

`F1` (or `Ctrl + /`) opens the list of shortcuts, on the keys of what is on display: a CSV table, a diagram, a table of the note.

- A link per section sits at the top.
- Type to search, by key or by effect (`ctrl+p`, `graph`). `Esc` empties the search, then closes.
- Click the keys of a row to [change them](settings.md#changing-a-shortcut).
- At its foot: the version of Bref, and links to its source and to its Ko-fi page.

The full list is also in [Keyboard shortcuts](shortcuts.md).

## Menus

A right click in a note opens a menu: open the link under the pointer or copy its address, cut, copy, paste, bold, italic, underline, strikethrough, highlight, code, make or edit a link, comment, turn the lines into a list.

`Shift + F10` (or the `Menu` key) opens the same menu at the cursor. In the panel, it opens the menu of the selected line. Arrows, `Home` and `End` walk it, `Enter` runs, `Esc` closes and gives the keys back.

The menus show, beside each action, the shortcut that does the same. Pointing at an icon of the rail or of a bar names it, with its shortcut.

## Quit

`Ctrl + Q` quits. Bref reopens with the panel, tabs and panes as you left them.
