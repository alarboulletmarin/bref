# Troubleshooting

## The window takes two seconds to open (Linux)

Another Vulkan driver is slow to load. Bref already skips the NVIDIA one when the machine has no NVIDIA card. To see what remains, run `VK_LOADER_DEBUG=driver bref` in a terminal.

## The window does not open, with an error about a surface or an adapter

There is no usable Vulkan driver. Install the one for your GPU: `vulkan-intel`, `vulkan-radeon`, `nvidia-utils`… on Arch; `mesa-vulkan-drivers` on Ubuntu.

## macOS or Windows refuses to open Bref

The downloads are not notarized or signed. See [macOS](install.md#macos) and [Windows](install.md#windows) for the one-time step.

## Bold text is not bold

The font you picked is a variable font, which the text engine cannot embolden. [Pick another font](settings.md#fonts), or none to go back to the built-in ones.

## A note was not renamed after I changed its title

Either another note already has that name, or the file name did not match the first line to begin with, and that line is not a `# ` title you just added or changed. See [Names and renaming](vault.md#names-and-renaming).

## Changes made elsewhere show up late

Bref is told by the system when the vault changes, and also looks at it every 30 seconds in case an event got lost. When the system cannot watch the folder, it looks every two seconds: on some network drives, or when Linux has run out of inotify watches. In that case, raise `fs.inotify.max_user_watches`.

See [Changes from other programs](vault.md#changes-from-other-programs).

## Sync fails

`git` must reach the repository without asking anything, and the vault must be in a plain state. See [Requirements](sync.md#requirements) and [What it refuses](sync.md#what-it-refuses).

## Known limits

- The name field of a new folder or a rename only edits at its end: type, or erase with `Backspace`.
- Links written as plain text inside a code block are not followed when a note is renamed.
