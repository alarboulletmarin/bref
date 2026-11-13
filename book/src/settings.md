# Settings and appearance

Every choice Bref offers is in one place: the **Settings** tab. Open it with `Ctrl + ,`, the gear at the bottom of the rail, or `settings` in the palette.

## The settings tab

It opens as a tab of the active pane, like a note. Put it next to a note to try a theme on real text.

- **Sections** on the left: Appearance, Editor, Files & links, Sync & backup, Agents, Shortcuts, Updates & about. In a narrow pane they go above, as a row.
- **Each setting** shows its name, one line on what it changes when that is not obvious, and its current value: pills for a few choices, a switch for on and off, cards for the themes, a button for an action.
- **Applied at once**, and saved in the [`settings` file](#the-settings-file). There is no OK or Cancel.
- **Search**: type anywhere on the page, or click the field at the top. It finds a setting in every section, by its name, its line or one of its values (`dracula`, `wide`). `Enter` goes to the first one found.
- `Esc` clears the search, then closes the tab.
- The tab comes back after a restart, with the other tabs.
- Without a vault (welcome screen), there is no tab to open: open or create a vault first.

| Keys | On the settings page |
|---|---|
| `Up` / `Down` | the previous or next setting; past the last one of a section, into the next section |
| `Tab` / `Shift + Tab` | the next or previous section; from the search field, back to the settings |
| `Left` / `Right` | the previous or next choice: theme, page width, wide tables, text size; off or on for a switch |
| `Enter` / `Space` | change it: the next choice, the other side of a switch, or what its button does |
| a letter | starts a search |
| `Esc` | clears the search, then closes the tab |

**The palette commands stay.** `theme`, `font`, `page width`, `wide tables`, `formatting bar`, `animations` and `updates` still work: the tab is the map, the palette the shortcut. `theme` in the palette still previews each theme as you move through the list.

**Edited by hand.** If you change the `settings` file while Bref runs, opening the tab again (`Ctrl + ,`) reads it and applies it.

### What each section holds

- **Appearance**: the theme, the font of the app and of the code, the text size, the animations.
- **Editor**: the page width, wide tables, the formatting bar.
- **Files & links**: the vault (its folder, **Open another…**, **Clone…**), where new notes and daily notes go, the trash (**Restore…**), the [configuration folder](vault.md#configuration-files).
- **Sync & backup**: git (the server the vault syncs with, the time of the last sync, **Sync now**, or **Connect…** for a vault not linked yet), and a backup of the whole vault. See [Syncing a vault](sync.md).
- **Agents**: what [`bref mcp`](agents.md) gives an agent, **Read and write** or **Read only**, and the configuration to copy: the JSON for Claude Desktop, Cursor, VS Code and the others, and the command for Claude Code. Both name this copy of Bref by its full path. **Copy** puts it on the clipboard.
- **Shortcuts**: how many you changed, and the list itself (the help panel, `F1`), where you [change them](#changing-a-shortcut).
- **Updates & about**: the version, **Check now**, the daily check on or off (see [Updates](updates.md)), the guide, the code and Ko-fi.

## Theme

The cards of **Appearance** in the settings tab, the half-filled circle at the bottom of the rail, or `theme` in the palette.

- 13 themes: Bref Dark and Bref Light, Dracula, One Dark, Gruvbox (dark and light), Nord, Catppuccin (Mocha and Latte), Tokyo Night, Rosé Pine, Solarized (dark and light).
- In the settings tab, each card shows the theme's page, text and accent. A click, or `Left` and `Right` on the row, applies it at once, the tab included.
- In the palette, each theme is applied as you move through the list, so you see it before choosing. `Enter` keeps it, `Esc` goes back to the one you had.
- `Default` follows the light or dark setting of your system, with Bref Light or Bref Dark.
- In every theme, links, secondary text and the titles of colored panels are adjusted to a contrast of at least 4.5:1 (WCAG AA).

**The look.** The window is a warm ground, a little darker than the page in every theme. The note, the panel and the right dock lie on it as rounded sheets, eight pixels apart. Page titles are in a serif, while note titles follow your font.

- The active pane of a split has a thin accent ring.
- The open row of a list, a tag in the text, a commented passage and the current tab take a quiet tint of the accent.
- Tables and code blocks have a rounded band.
- Menus, the palette, help and notices float above, with a shadow.

## Fonts

Bref carries IBM Plex Sans (text), IBM Plex Mono (code) and IBM Plex Serif (page titles), under the SIL Open Font License (`assets/fonts/`). So it looks the same everywhere, with nothing to install.

- **Change…** next to **App font** or **Code font** in the settings tab, or `font` in the palette, lists the fonts installed on your system. Type in the list to filter it.
- Ideograms and emoji, which Plex does not draw, come from a font of your system.
- A variable font cannot be emboldened. See [Troubleshooting](troubleshooting.md#bold-text-is-not-bold).

## Text size

`Ctrl + +` and `Ctrl + -` change the size, `Ctrl + 0` restores the default. In the settings tab, **Text size** has the same buttons, and shows the size of the interface in percent.

Headings, code and the width of the text column scale with it. The interface around the note (panel, rail, menus, palette, help) follows too, from 85 % to 150 %, so it stays usable at either end.

## Page width

**Page width** in the **Editor** section, or `page width` in the palette, sets how wide the text column is: narrow, normal, wide, or the full window.

**Wide tables**, or `wide tables` in the palette, chooses what a table wider than the text does:

- **center** (the default): it spills out on both sides of the column, centered on the window;
- **column**: it stays in the column, and scrolls sideways.

A small table keeps its own size, and nothing goes past the window.

## Formatting bar and animations

- **Formatting bar** (a switch in **Editor**), or `formatting bar` in the palette, shows or hides the [formatting bar](writing.md#the-formatting-bar) under the note.
- **Animations** (a switch in **Appearance**), or `animations` in the palette, turns the short fades off. When on, the panel, the formatting bar and a folder of the tree fade in over about 0.13 s as they open. Nothing moves on typing, scrolling or the caret, and no fade delays a result. Hovers and closing are instant.

## Language

French if the system language is French, English otherwise. Bref asks the system (Windows, macOS), or reads `LC_ALL`, `LC_MESSAGES` and `LANG` (Linux, or any system when started from a terminal).

## The settings file

These choices are kept in a file named `settings`, in the [configuration folder](vault.md#configuration-files). It holds one `key=value` per line:

| Key | Values | In the settings tab | In the palette |
|---|---|---|---|
| `theme` | the name of a theme | Appearance › **Theme** | `theme` |
| `font`, `mono` | the font of the app, of the code | Appearance › **App font**, **Code font** | `font` |
| `size` | the text size | Appearance › **Text size** | `Ctrl + +`, `Ctrl + -`, `Ctrl + 0` |
| `page` | `narrow`, `normal`, `wide`, `full` | Editor › **Page width** | `page width` |
| `wide` | `column`, `center` | Editor › **Wide tables** | `wide tables` |
| `bar` | `on`, `off` | Editor › **Formatting bar** | `formatting bar` |
| `motion` | `on`, `off` | Appearance › **Animations** | `animations` |
| `updates` | `off` to stop the daily check | Updates & about › **Check for updates** | `updates`, see [Updates](updates.md) |

## Changing a shortcut

The help panel (`F1`, or **Open the list** in the **Shortcuts** section of the settings tab) lists every shortcut, and lets you give any of them other keys.

- **Change**: click the keys of a row, or move to it with `Down` and press `Enter`. The row waits, and the next combination you press becomes its shortcut, at once. `Esc` cancels. `Backspace` leaves the action with no shortcut.
- **Conflicts**: a combination already used where the action applies is not taken. The row says by which action. `Enter` (**Replace**) gives it to the new one, and the other is left without a shortcut. The same keys in two views that never meet (a table and a diagram) are not a conflict.
- **Refused keys**: a key alone that types text, and the keys the text and the views need (`Enter`, `Tab`, the arrows…), are refused, with the reason. A combination your system usually keeps for itself is accepted with a warning.
- **Reset**: a changed row is highlighted and has a **default** button. **Reset all**, at the top of the panel, puts every shortcut back.
- The right-click menus and the palette show the keys in effect next to each action, so a change made here shows there too.
- Rows without a button are not key bindings (a click, a palette command, something you type). The search finds them, but they cannot be changed.

**The `keys` file.** Only what you changed is kept, one `action=keys` line each, in the `keys` file of the configuration folder, next to `settings`. `secondary` stands for `Cmd` on macOS and `Ctrl` elsewhere. A line that cannot be used (unknown action, unreadable keys, keys already taken) is ignored and reported at launch; the default stays.
