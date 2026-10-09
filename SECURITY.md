# Security policy

## Supported versions

Only the latest release receives fixes. Bref can update itself (see *Updates* in the README); please check that you run the latest version before reporting.

## Reporting a vulnerability

Do not open a public issue. Use the [private report form](https://github.com/alarboulletmarin/bref/security/advisories/new) (*Security* tab, *Report a vulnerability*): only the maintainer sees it.

Please include the version (`bref --version`, or the help panel), the operating system, and the steps or the file that trigger the problem.

You can expect a first answer within a week. A confirmed vulnerability is fixed in a new release, and the advisory is published with it, crediting you unless you prefer otherwise.

## What matters most

- The updater: the file it downloads, the checksum it verifies, and what it runs.
- Files that Bref opens: notes, images, CSV and TSV tables, Excalidraw and draw.io imports, SVG diagrams.
- Anything that writes outside the vault and the configuration folder, or that destroys or overwrites a file.
