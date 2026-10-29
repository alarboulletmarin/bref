# Daily notes, templates and capture

## Today's note

`Ctrl + J` (or **Today's note** in the palette) opens the note of the day, `2026-10-09.md`, wherever it is in the vault. If there is none yet, it creates it, with the date as its title.

- `@today`, `@tomorrow`, `@yesterday`, `@monday` or `@2026-10-09` in the text links to the daily note of that day. See [Links between notes](links.md#links-between-notes).
- The [blank page](first-steps.md#the-blank-page) offers the daily notes of a week, a month and a year ago.
- With a [template](#templates) named `Daily`, `Journal`, `Today`, `Note du jour` or `Aujourd'hui`, `Ctrl + J` creates the note of the day from it.

## The calendar

The **Calendar** view of the panel (`Ctrl + Shift + J`) shows one month.

- Today is a round pill. A dot under a day means it has its daily note.
- The last days of the neighbouring months are dimmed, and open their note too.
- Arrows change the day. `Page Up` and `Page Down` change the month.
- `Enter` or a click opens the note of the day, or creates it.
- The week starts on Monday in French, on Sunday in English.

## Templates

Put notes in a folder named `Templates` (or `Modèles`) at the root of the vault: each one is a template.

- Case and accents of the folder name do not matter, and templates may sit in sub-folders.
- Templates are left out of the graph, the tags, the links and the notes to read again. A `[[link]]` never points at one.

**New note from a template**: **new note from a template** in the palette, or in the right-click menu of a folder of the tree.

1. Choose the template.
2. Type the name of the note.
3. The note is written in the folder selected in the tree (the root of the vault otherwise) and opened, with the cursor where the template says.

A name already taken is refused; nothing is overwritten.

**Placeholders**

| Placeholder | Becomes |
|---|---|
| `{{title}}` | the name you typed |
| `{{date}}` | `2026-10-09` |
| `{{time}}` | `09:05` |
| `{{cursor}}` | where the cursor goes (the placeholder disappears) |

Any other `{{word}}` stays as written.

**Insert a template**: **insert a template** in the palette puts one at the cursor, filled with the name of the open note. One `Ctrl + Z` takes it back.

**Make a template of a note**: **make a template of this note** in the palette saves the open note in the templates folder, created if there is none. Its first `#` title becomes `{{title}}`.

## Quick capture

An idea rarely comes while Bref is open. `bref --capture` opens a window that is one line and nothing else, as fast as the app itself.

1. Type.
2. Press `Enter`. The line is added to the note of the day as a list item (`- call Léa`). Type `- [ ] ` first to make it a task.

The note is created if the day has none yet, and found wherever you keep your daily notes in the vault. `Esc`, or a click beside the line, closes the window without writing anything.

**Put it under a key**: open the palette, type `capture` and choose **Set up the quick capture shortcut**.

- **GNOME**: done. Bref adds the shortcut `Super + Shift + N` to your keyboard settings and tells you so. If those keys are taken, it adds none and says so: set your own keys in Settings › Keyboard › Custom Shortcuts, under *Bref: capture*. Asking again never adds a second one.
- **Elsewhere** (KDE, macOS, Windows…): an app cannot take a system-wide shortcut by itself. Bref copies the command to the clipboard, ready to paste into a new shortcut of your system settings.

Without any setup, on Linux, right click the icon of Bref in the dock or the app grid: **Quick capture** is there.

If Bref is open on today's note, the line appears there a moment later. Without a vault chosen yet, the command opens Bref as usual.
