# Updates

Once a day, in the background, Bref asks GitHub whether a newer version exists. If so, a discreet banner at the bottom of the window says so.

## Updating

Where Bref can install itself, the banner offers **Update** (also **Update Bref to …** in the palette). It downloads the new version, installs it, and restarts. Your notes are saved first.

| Installed with | What Update does |
|---|---|
| Windows installer | runs the new installer silently. It replaces `bref.exe` and opens Bref again. |
| macOS `.dmg` | replaces `Bref.app` in place, and opens it again. It needs write access to the folder that holds it: `/Applications` for most people. |
| Arch, prebuilt package | installs the new package with `pacman -U`, through `pkexec`, which asks for your password in a desktop window |

Elsewhere (Windows zip, compiled from source, `bref-git`, other Linux distributions), the banner links to the download page. Follow [Upgrading](install.md#upgrading).

If an update fails, Bref stays as it was and says why.

## Trust

Bref checks nothing but HTTPS. The files are not signed (see [Install](install.md#macos)), so an update is as trustworthy as downloading the same file by hand from the repository's releases.

## Privacy

The check is the only network request the app makes until you press **Update**. It runs `curl` against `api.github.com/repos/alarboulletmarin/bref/releases/latest`, which sees your IP address and the client name `bref`, and nothing else. Without `curl`, or offline, nothing is shown.

## Controls

- **Check now**: type `check for updates now` in the palette. Bref answers that it is up to date, or shows the banner. This works even when the daily check is off.
- **Stop or resume the daily check**: type `updates` in the palette. The choice is kept as `updates=off` in the [`settings` file](settings.md#the-settings-file), and the time of the last check in the `update` file, both in the [configuration folder](vault.md#configuration-files).
