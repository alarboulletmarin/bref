# Links, embeds and files

## Links between notes

- `[[` suggests your notes. A link to a note that does not exist creates it.
- `[[Groceries]]` finds the note by file name, in any subfolder, ignoring case.
- `[[Note|alias]]` shows another text. `[[Note#Heading]]` points at a heading.
- `@` suggests days (`@today`, `@tomorrow`, `@monday`, `@2026-10-09`) and links to the daily note of that day.
- `Ctrl + click` opens a link. `Ctrl + Shift + click` opens it in the pane beside.
- Each name in the `aliases` of a note's front matter is another name for it: suggested after `[[`, and followed by links, backlinks and the graph.

When a note is renamed, the links to it follow. See [Renaming](vault.md#names-and-renaming).

## Backlinks

The **Backlinks** view of the panel (`Ctrl + L`) lists the notes that link to the open one, each with the line that holds the link.

- Links written with an alias, and links inside tables, count.
- Embeds count too, marked *embedded*.
- The list follows the note you open, not the one you preview while moving through the list. So you can look at each linking note in turn.

## Tags

A `#tag` in the text, or a `tags` key in the front matter, tags a note.

- `#` in the palette lists the notes carrying a tag.
- The **Tags** view of the panel (`Ctrl + T`) lists every tag of the vault with its number of notes.
- `Ctrl + click` opens a tag of the text.

## Embedded sections

A line holding only `![[Note#Heading]]` shows that heading, and what is under it up to the next heading of the same level, in place and live.

| Write | Shows |
|---|---|
| `![[Note#Heading]]` | the section under that heading |
| `![[Note]]` | the whole note, without its front matter |
| `![[Note#^id]]` | the block that ends with `^id` (a common Markdown convention) |

- **Look**: the section is drawn in a frame, with a thin bar on its left in the accent color and the name of its note at the top right. It follows the theme and wraps to the page.
- **Open the source**: click the name, double click the frame, or press `Alt + Enter` on its line. The note opens at that section, and `Alt + Left` comes back. `Ctrl + click` on a `[[Note#Heading]]` link does the same.
- **Suggestions**: typing `![[Note#` suggests the headings of the note, and `![[Note#^` its blocks. This works after `[[` too.
- **Read-only**: to change the text, change it in its note.
- **Loops**: an embed that leads back to a note already in the chain says *circular embed*. Three levels are shown at most.
- **Renames**: renaming the note rewrites the embeds and links to it. Renaming the heading (the same headings but one, saved) rewrites `#Heading` in the other notes.
- **Only alone on its line**: inside a sentence, a list item or a code block, it is a plain link. `![[picture.png]]` is still a picture.

The file stays plain Markdown: other apps show the line as a link.

## Pictures

`![](picture.png)` or `![[picture.png]]` shows the picture under its line. Bref looks for it next to the note, then at the top of the vault, then by its file name anywhere in the vault.

- `Ctrl + V` with a picture in the clipboard saves it next to the note and inserts its `![](…)` line.
- Typing `![[` suggests the diagrams and pictures of the vault.
- **Enlarge**: point at a picture (PNG, JPEG, GIF, WebP…), a formula, an SVG or a Mermaid diagram, and a magnifier button appears at its corner, or beside it when it is too small to hold it. It opens a viewer: `Ctrl + wheel` zooms around the pointer, and dragging moves the picture. See [the viewer](diagrams.md#svg-files-from-elsewhere).
- A click on the picture itself still places the cursor. A missing or unreadable picture has no button.

Pictures are part of the vault. See [Files on disk](vault.md#pictures-csv-and-other-files).

## Dropping files

Drag files or folders from your file manager onto the window. The originals never move, and nothing is overwritten: a name already taken becomes `name 2`.

**On a note.** A line shows between two lines of the text, never inside a table, a code block or the YAML header. A label next to the pointer says where the items will land. They are copied next to the note, like a pasted picture, and each gets a line at that place:

| Dropped | Line written |
|---|---|
| a picture | `![[photo.png]]` |
| a Markdown note | `[[note]]` |
| a file the vault lists | `[[rapport.pdf]]` |
| a folder | `[[Folder]]` |

One `Ctrl + Z` takes the lines back; the copies stay.

**On the tree.** The folder under the pointer is tinted (on a file, the folder that holds it), and the label names it. A drop between rows goes to the root of the vault. The new files are shown and selected, and a message says how many files were copied and how many items were left out.

**On the welcome screen**, a dropped folder becomes the vault.

**What is copied**: folders with everything in them, except hidden files and symbolic links, which are counted, not copied. A folder is never copied into itself. The copy runs in the background, so a large video does not freeze the window.

**Files the vault lists**: pictures, CSV and TSV tables, and attachments (`pdf`, video and audio, archives, Office and OpenDocument files, `txt`, `json`…). They show in the tree and the palette. `[[name.pdf]]` opens them in the application of your system, and renaming one rewrites the links to it. Other types are copied but not listed, so no link is written for them.

`[[Folder]]` opens the folder as a [list page](boards.md#list-pages) when no note has that name.

**Not possible**: pasting files copied in a file manager (the toolkit only passes text and pictures), and playing video or audio inside a note.
