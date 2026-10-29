# Files on disk

A vault is an ordinary folder of plain files. Bref adds nothing to your notes that another Markdown app could not read.

## Notes

- One `.md` file per note. Notes in subfolders are found too. Hidden folders (`.git`…) are ignored.
- New notes are created at the top of the vault, or in the selected folder when the tree is on display.
- A note is saved shortly after you stop typing, and when you switch note or quit. Saving is atomic: a crash never leaves a half-written file.
- A note opens with the cursor at the start of its body, under its title and its front matter. What you type first goes into the text; it does not rename the note.

## Names and renaming

- A new note is named after its first line: `# Groceries` becomes `Groceries.md`. The file is renamed when you change that line.
- A new note left empty leaves no file behind.
- A note whose file name did not already match its first line (typical of an existing vault) keeps its name. It takes the name of its first line only once you add or change a `# ` title there.
- Renaming a note from the tree also rewrites its first line, when that line is its name.
- When a note is renamed, from the tree or by changing its first line, the `[[links]]` to its old name are rewritten in every note. They keep their `|alias` and `#heading`. After a change of title, this happens when you leave the note, not at each keystroke.
- If another note still carries the old name, the links are left alone: they may be meant for it.
- Nothing is ever overwritten. A name already taken is refused.

## Front matter

A YAML front matter is a block that starts on the first line with `---` and ends with `---` or `...`.

- It is shown dimmed, and never interpreted or rewritten. Renaming a note or rewriting its links leaves the block byte for byte as it was.
- The title of the note is the first line after it.
- Two keys are read:
  - `tags` (`tags: [a, b]`, `tags: a, b`, or a dashed list) count like `#tags` in the text;
  - each name in `aliases` is another name for the note, suggested after `[[` and followed by links, backlinks and the graph.
- Other keys and nested YAML are ignored, except by [list pages](boards.md#list-pages) and [kanban boards](boards.md#kanban-boards) (`kanban: true`).

`/meta` inserts a front matter. See [Slash commands](writing.md#slash-commands).

## Pictures, CSV and other files

- **Pictures** are part of the vault. The tree lists them under the notes of their folder, and the graph shows them as squares, linked to the notes that display them. Selecting one shows it in place of the note. It can be renamed (the notes that display it follow), moved or trashed like a note.
- **CSV and TSV** files are listed with the notes of their folder and open in a [grid](csv.md).
- **Pictures** shown and listed: `png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `bmp`, `tif`, `tiff`, `avif`.
- **Attachments** are listed, found in the palette, linked with `[[name.pdf]]`, and open in the application of your system: `pdf`; video and audio (`mp4`, `m4v`, `mov`, `mkv`, `webm`, `avi`, `mp3`, `m4a`, `wav`, `ogg`, `opus`, `flac`); archives (`zip`, `7z`, `tar`, `gz`, `rar`); documents (`doc`, `docx`, `xls`, `xlsx`, `ppt`, `pptx`, `odt`, `ods`, `odp`, `epub`, `rtf`); text (`txt`, `json`, `xml`, `html`, `log`). See [Dropping files](links.md#dropping-files).
- **Other files** stay in the folder, but Bref does not list them.

## Changes from other programs

What other programs change in the vault (a sync tool, a script, another editor) is picked up in about half a second, without restarting. Bref asks the system to report changes (inotify, FSEvents, ReadDirectoryChangesW).

- Notes and folders added, renamed or removed elsewhere show up in the tree, the palette and the graph.
- The note on display is read again when its file changes. If you were typing in that note at that moment, your version is kept and written back.
- Bref also looks at the vault every 30 seconds, in case an event got lost. When the system cannot watch the folder, it looks every two seconds. See [Troubleshooting](troubleshooting.md#changes-made-elsewhere-show-up-late).

## Backup

Click the arrow into a tray at the bottom of the rail, or type `back up` in the palette, and choose a folder. Bref writes there one archive of the whole vault, trash included, named after the vault and the date: `Notes 2026-10-09.tar.gz`. It is numbered if that name is taken.

A folder inside the vault is refused.

## Terminal

Type `terminal` in the palette to open the terminal of your system in the folder of the vault:

- Windows: Windows Terminal, or the console;
- macOS: Terminal;
- Linux: the one named in `$TERMINAL`, else the one of the desktop.

Bref has no terminal of its own.

## Messages

- **Status**: the outcome of an operation (a backup saved, a note restored, a failure) shows at the bottom right, with a spinner while it runs. A message goes away after four seconds (a long one stays a little longer), or as soon as you click it.
- **Failures** stay twelve seconds, until you click them or press `Esc`. They name the file and say why in words (`permission denied`, `not found`, `the disk is full`), never the raw system error.
- **Unreadable things say so**: a Mermaid diagram or a `$$` formula that cannot be drawn, and a picture that cannot be read (a corrupt file, one of 0 bytes), show « Unreadable figure », « Unreadable formula » or « Unreadable image » under their source. A picture opened alone says « Loading… » while a big one is read.
- **A missing or unreadable vault**: a vault that is no longer where the config left it is reported on the welcome page. A vault that cannot be read (no permission) says so in the panel, with the reason and the folder, instead of looking empty, and offers another vault. A folder inside it that cannot be read is marked « unreadable: » with the reason; the rest of the vault opens.
- **Empty lists say so**: an empty tree, tag list or recent list, a note nothing links to, a palette search with no result (or with one letter, too little for the text search), a list page with no row, and a board with no column each show one soft line instead of a blank page.

## Configuration files

Bref keeps its own state in a configuration folder, outside the vault:

| System | Folder |
|---|---|
| Linux | `~/.config/bref/` |
| macOS | `~/Library/Application Support/bref/` |
| Windows | `%APPDATA%\bref\` |

| File | Holds |
|---|---|
| `config` | the vault, and the notes you opened, most recent first |
| `layout` | the navigation panel and its docks |
| `panes` | the panes and their tabs |
| `settings` | the [appearance and other choices](settings.md#the-settings-file) |
| `tables` | the delimiter, encoding and header row you chose for a [CSV file](csv.md#detection) |
| `keys` | the [shortcuts you changed](settings.md#changing-a-shortcut) |
| `update` | the time of the last [update check](updates.md) |
| `crash` | the last crash, with the version of Bref, to attach to a bug report |

In the vault itself, Bref only writes `.bref-icons` (the [icons](navigation.md#icons) you gave), the `.trash` folder, and, for a vault it [connects to git](sync.md#a-vault-not-in-git-yet), a `.gitignore` that leaves the trash out.
