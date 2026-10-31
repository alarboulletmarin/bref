# Search and replace

## Find in the note

`Ctrl + F` opens a search bar at the top right of the note. `Ctrl + R` opens it with the replace field.

- Matches are highlighted as you type. The current one is stronger, with its rank (`3 / 12`).
- The search runs on the text of the file, so Markdown markers (`**`, `[[`) can be searched too. It works inside tables and code blocks.
- A selection held on one line becomes the search. The first key typed replaces it.
- A selection over several lines keeps the search to those lines, as long as the **in the selected lines** button is on.
- The search and the replacement are kept when the bar closes, and come back selected.
- Both fields edit like any other: cursor, selection, word moves, paste, and the input method of your language (Japanese, dead keys). A long search scrolls in its field.

| Key | In the search bar |
|---|---|
| `Enter`, `Shift + Enter` (or `F3`, `Shift + F3`) | next, previous match, wrapping around |
| `Alt + C`, `Alt + W` | match case, whole words (also the `Aa` and `“ab”` buttons) |
| `Alt + R` | regular expression (also the `.*` button) |
| `Ctrl + R` | show the *Replace with* field (also the chevron on the left of the bar) |
| `Tab` | move between the two fields |
| `Enter` in *Replace with* | replace the current match and go to the next (also the first button beside the field) |
| `Ctrl + Enter` | replace every match (also the second button). One `Ctrl + Z` in the note brings them all back. |
| `Ctrl + Shift + Enter` | replace across the whole vault (also the **Vault…** button), see below |
| `Esc` | close. The cursor stays on the current match. |

- With the bar open, `Ctrl + R` and `Ctrl + F` switch modes, and the keys go to the matching field. The search and its options are kept.
- **Regular expressions**: `^` and `$` are the ends of each line, and the replacement can reuse groups with `$1`. An expression that does not compile says « Invalid expression » in the bar.
- **Edit while searching**: click in the note to edit it while the bar stays open. The matches follow the text. `F3`, `Alt + C` and `Alt + W` still work, and `Esc` closes the bar.
- With no bar open, `F3` opens it.

## Search the vault

`Ctrl + Shift + F` (or the rail's icon with lines and a magnifier) shows the search of the whole vault in the panel. If the note's search bar is open, it starts from its search.

- It takes the same options as the bar: `Alt + C`, `Alt + W`, `Alt + R`, or the `Aa`, `“ab”` and `.*` buttons.
- Under the count of matches and notes, each note lists its matches, one line each, with the line number and the match marked. A note's header folds its matches.
- `↑` `↓` walk the list. `Enter` or a click opens the note on that match, with its search bar on the same search. `Esc` gives the keys back to the note.
- The search runs in the background as you type. It follows the notes as they change, from outside the app too.

## Replace in the vault

From the vault search, `Ctrl + R` (or the chevron) shows the *Replace with* field. From the note's search bar, `Ctrl + Shift + Enter` does the same job.

- The field's button, `Ctrl + Enter`, or `Enter` in the field replaces in every note.
- Bref first shows how many matches in how many notes, and writes nothing until you accept.
- The button on a note's header replaces in that note only.
- The cross at the end of a match sets it aside: it is neither listed nor replaced.
- A note that cannot be written is reported; the others are replaced.

**Undo the replacement in the vault**, in the palette, puts every note back, except those changed since.
