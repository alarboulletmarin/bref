# Settings and appearance

Every choice is a command of the palette: type its name, and see the result at once.

## Theme

The half-filled circle at the bottom of the rail, or `theme` in the palette.

- 13 themes: Bref Dark and Bref Light, Dracula, One Dark, Gruvbox (dark and light), Nord, Catppuccin (Mocha and Latte), Tokyo Night, Rosé Pine, Solarized (dark and light).
- Each theme is applied as you move through the list, so you see it before choosing. `Enter` keeps it, `Esc` goes back to the one you had.
- `Default` follows the light or dark setting of your system, with Bref Light or Bref Dark.
- In every theme, links, secondary text and the titles of colored panels are adjusted to a contrast of at least 4.5:1 (WCAG AA).

**The look.** The window is a warm ground, a little darker than the page in every theme. The note, the panel and the right dock lie on it as rounded sheets, eight pixels apart. Page titles are in a serif, while note titles follow your font.

- The active pane of a split has a thin accent ring.
- The open row of a list, a tag in the text, a commented passage and the current tab take a quiet tint of the accent.
- Tables and code blocks have a rounded band.
- Menus, the palette, help and notices float above, with a shadow.

## Fonts

Bref carries IBM Plex Sans (text), IBM Plex Mono (code) and IBM Plex Serif (page titles), under the SIL Open Font License (`assets/fonts/`). So it looks the same everywhere, with nothing to install.

- Type `font` in the palette to choose another font, for the app or for the code, among those installed on your system. Type in the list to filter it.
- Ideograms and emoji, which Plex does not draw, come from a font of your system.
- A variable font cannot be emboldened. See [Troubleshooting](troubleshooting.md#bold-text-is-not-bold).

## Text size

`Ctrl + +` and `Ctrl + -` change the size, `Ctrl + 0` restores the default.

Headings, code and the width of the text column scale with it. The interface around the note (panel, rail, menus, palette, help) follows too, from 85 % to 150 %, so it stays usable at either end.

## Page width

`page width` in the palette sets how wide the text column is: narrow, normal, wide, or the full window.

`wide tables` in the palette chooses what a table wider than the text does:

- **center** (the default): it spills out on both sides of the column, centered on the window;
- **column**: it stays in the column, and scrolls sideways.

A small table keeps its own size, and nothing goes past the window.

## Formatting bar and animations

- `formatting bar` in the palette shows or hides the [formatting bar](writing.md#the-formatting-bar) under the note.
- `animations` in the palette turns the short fades off. When on, the panel, the formatting bar and a folder of the tree fade in over about 0.13 s as they open. Nothing moves on typing, scrolling or the caret, and no fade delays a result. Hovers and closing are instant.

## Language

French if the system language is French, English otherwise. Bref asks the system (Windows, macOS), or reads `LC_ALL`, `LC_MESSAGES` and `LANG` (Linux, or any system when started from a terminal).

## The settings file

These choices are kept in a file named `settings`, in the [configuration folder](vault.md#configuration-files). It holds one `key=value` per line:

| Key | Values | Set by |
|---|---|---|
| `theme` | the name of a theme | `theme` |
| `font`, `mono` | the font of the app, of the code | `font` |
| `size` | the text size | `Ctrl + +`, `Ctrl + -`, `Ctrl + 0` |
| `page` | `narrow`, `normal`, `wide`, `full` | `page width` |
| `wide` | `column`, `center` | `wide tables` |
| `bar` | `on`, `off` | `formatting bar` |
| `motion` | `on`, `off` | `animations` |
| `updates` | `off` to stop the daily check | `updates`, see [Updates](updates.md) |

## Changing a shortcut

The help panel (`F1`) lists every shortcut, and lets you give any of them other keys.

- **Change**: click the keys of a row, or move to it with `Down` and press `Enter`. The row waits, and the next combination you press becomes its shortcut, at once. `Esc` cancels. `Backspace` leaves the action with no shortcut.
- **Conflicts**: a combination already used where the action applies is not taken. The row says by which action. `Enter` (**Replace**) gives it to the new one, and the other is left without a shortcut. The same keys in two views that never meet (a table and a diagram) are not a conflict.
- **Refused keys**: a key alone that types text, and the keys the text and the views need (`Enter`, `Tab`, the arrows…), are refused, with the reason. A combination your system usually keeps for itself is accepted with a warning.
- **Reset**: a changed row is highlighted and has a **default** button. **Reset all**, at the top of the panel, puts every shortcut back.
- The right-click menus and the palette show the keys in effect next to each action, so a change made here shows there too.
- Rows without a button are not key bindings (a click, a palette command, something you type). The search finds them, but they cannot be changed.

**The `keys` file.** Only what you changed is kept, one `action=keys` line each, in the `keys` file of the configuration folder, next to `settings`. `secondary` stands for `Cmd` on macOS and `Ctrl` elsewhere. A line that cannot be used (unknown action, unreadable keys, keys already taken) is ignored and reported at launch; the default stays.
