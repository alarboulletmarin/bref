# Vault health

A vault you have kept for years gathers small problems you never see: links to notes never written, or renamed outside Bref, pictures nothing shows any more, empty notes, sync conflict copies, two notes with the same title. The vault health page finds them and fixes them in a click.

## Opening it

- Type `health` in the palette, and choose **Vault health**.
- The page opens in a tab of its own. Your notes open beside it, so you can fix one thing and come back.
- A small dot on the palette command means new broken links appeared since you last looked at the page.
- The page looks at the vault only while it is open, off to the side, so typing never waits. It looks again whenever a note changes.

## What it finds

Each kind is a group with its count. Click a group's header to fold it.

| Kind | What it means | What you can do |
|---|---|---|
| **Broken links** | A `[[link]]` names no note, alias, file or folder. | **Open** the note at that line. **Create** the note. **Relink to** a close existing name, shown before you click. **Remove the brackets**: `[[Name\|shown]]` becomes `shown`. |
| **Links to a template** | A `[[link]]` names only a note of the templates folder, which no link opens. | **Open** the note, or the template. |
| **Same name in several notes** | Two notes, or a note and an alias, answer the same name: a link to it is ambiguous. | **Open** each note. **Rename** one; the links follow. |
| **Sync conflict copies** | A copy named `Note (conflict 2026-10-09 laptop).md`, written by a [git sync](sync.md#conflicts). | **Compare side by side** in two panes. **Keep this copy**: the original goes to the trash and the copy takes its name. **Keep the original**: the copy goes to the trash. |
| **Unreadable front matter** | The header between the `---` lines does not close, or has a line that is not `key: value`. | **Open** the note at that line. |
| **Empty notes** | Nothing written besides the header and a title. | **Open**, or **Move to the trash**. |
| **Unused pictures and attachments** | A picture, PDF, video or archive that no note shows or links. Tables are left out. | **Open**, or **Move to the trash**. |
| **Heavy pictures** | A picture over 5 MB. | See its size. **Open**, or **Show the folder** it is in. |
| **Orphan notes** | A note no link reaches and that links to nothing. Daily notes, named after their date, are left out. | **Open**, or **Show in the graph**. |

Links inside code blocks are not read, as everywhere in Bref.

## Fixing safely

- Nothing is deleted. What you remove goes to the vault's [trash](vault.md), and the message that says so has an **Undo** button.
- A rename goes through the same rename as the panel, which rewrites the links to the note.
- **Fix all** only does what needs no choice: it relinks each broken link that has exactly one close name. A note is rewritten once, however many of its links change.
- A note open in a tab changes in its editor: `Ctrl + Z` there undoes the fix in one step. A note that is not open is rewritten on disk.

## What it does not check

- Spelling and style.
- Web links: checking them would need the network.
