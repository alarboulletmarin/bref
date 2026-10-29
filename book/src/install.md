# Install

| Your system | How |
|---|---|
| Arch Linux | [Prebuilt package](#arch-linux) |
| macOS 11 or later | [Disk image](#macos) |
| Windows 10 or 11, 64-bit | [Installer or zip](#windows) |
| Ubuntu, Linux Mint, Debian | [Build from source](#ubuntu-linux-mint-debian) |
| Fedora | [Build from source](#fedora) |
| Another Linux distribution | [Build from source](#other-linux-distributions) |

Arch, macOS and Windows have a ready-made download. Everywhere else, Bref is compiled on your machine. The first build takes a few minutes.

Compiling needs a recent stable Rust. If `rustc --version` shows less than 1.88, install Rust with [rustup](https://rustup.rs).

## Arch Linux

Every release ships a ready-to-install package. No compiler is needed, and pacman tracks it as `bref`.

```bash
curl -LO https://github.com/alarboulletmarin/bref/releases/latest/download/bref-x86_64.pkg.tar.zst
sudo pacman -U ./bref-x86_64.pkg.tar.zst
```

Download the file first, as above. The package is not signed: given a URL, pacman looks for a `.sig` file and warns when there is none.

**To follow `main`**, build the development package from a checkout. It needs `cargo` and `git`, and conflicts with `bref`.

```bash
git clone https://github.com/alarboulletmarin/bref.git
cd bref/aur/bref-git
makepkg -si
```

**To build the latest tagged release yourself**, run `makepkg -si` from the root of the repository.

Bref is not on the AUR yet.

## macOS

Download [`bref-macos.dmg`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-macos.dmg). It runs on Apple Silicon and Intel. Open it and drag Bref to Applications.

The app is not notarized, because that needs a paid Apple Developer account. So macOS blocks the first launch:

1. Open **System Settings**, then **Privacy & Security**.
2. Scroll down and click **Open Anyway**.

Or run this once in a terminal:

```bash
xattr -dr com.apple.quarantine /Applications/Bref.app
```

**To compile it yourself**, you need Xcode (the full app, for the Metal shader compiler) and Rust:

```bash
git clone https://github.com/alarboulletmarin/bref.git
cd bref
cargo install --path . --locked
```

This installs the `bref` command in `~/.cargo/bin`, without the `.app` bundle. Start it from a terminal.

## Windows

Download [`bref-windows-x86_64-setup.exe`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-windows-x86_64-setup.exe) and run it. It installs for your user only, with no administrator rights, and adds a Start menu entry and an uninstaller.

To install nothing, take [`bref-windows-x86_64.zip`](https://github.com/alarboulletmarin/bref/releases/latest/download/bref-windows-x86_64.zip) instead. Unzip it anywhere and run `bref.exe`.

The files are not signed, because a code-signing certificate costs a few hundred euros a year. SmartScreen may show "Windows protected your PC": click **More info**, then **Run anyway**.

**To compile it yourself**, you need the Visual Studio C++ build tools with the Windows SDK, and Rust with the MSVC toolchain:

```powershell
git clone https://github.com/alarboulletmarin/bref.git
cd bref
cargo install --path . --locked
```

This installs `bref.exe` in `%USERPROFILE%\.cargo\bin`.

## Ubuntu, Linux Mint, Debian

1. Install the build tools and libraries:

   ```bash
   sudo apt install build-essential pkg-config libfontconfig-dev libfreetype-dev \
     libwayland-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 \
     fonts-noto-color-emoji
   ```

2. Install Rust with [rustup](https://rustup.rs). The `cargo` from apt is usually too old.

3. Build and install:

   ```bash
   git clone https://github.com/alarboulletmarin/bref.git
   cd bref
   make
   sudo make install
   ```

## Fedora

Follow the [steps above](#ubuntu-linux-mint-debian) with these packages. Their names have not been tested.

```bash
sudo dnf install gcc pkgconf-pkg-config fontconfig-devel freetype-devel \
  wayland-devel libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel vulkan-loader \
  google-noto-color-emoji-fonts
```

## Other Linux distributions

You need:

- the development files of fontconfig, freetype, Wayland, libxcb and libxkbcommon (with its X11 part);
- a Vulkan driver for your GPU;
- an emoji font, such as Noto Color Emoji, for 🎉 in notes and names;
- Rust.

Then:

```bash
make                    # cargo build --release --locked
sudo make install       # PREFIX=/usr/local by default, PREFIX=/usr for a system-wide install
```

`make install` adds the `bref` command, an entry in your application menu, and its icon. `sudo make uninstall` removes them.

## Upgrading

On Arch (prebuilt package), macOS and Windows (installer), Bref [updates itself](updates.md): click **Update** in the banner. Otherwise:

| Installed with | Upgrade |
|---|---|
| Arch, prebuilt package | the **Update** button of the banner, or run the two commands of [Arch Linux](#arch-linux) again |
| macOS, `.dmg` | the **Update** button of the banner, or download the new `.dmg` and drop Bref on Applications again |
| Windows, installer | the **Update** button of the banner, or run the new installer: it upgrades in place |
| Windows, `.zip` | unzip the new version over the old one |
| Arch, `bref-git` | `git pull`, then `makepkg -si` in `aur/bref-git` |
| `make install` | `git pull`, then `make` and `sudo make install` |
| `cargo install` | `git pull`, then `cargo install --path . --locked` |

## Coming from encre

Bref was called *encre* until version 0.1.4. Your notes are untouched. Settings saved under the old name are still read, and written under the new one the next time they change.

| Installed encre with | Before upgrading |
|---|---|
| Arch, either package | nothing: the `bref` packages replace `encre` and `encre-git` |
| `make install` | `sudo make uninstall` in the old checkout, before `git pull` |
| `cargo install` | `cargo uninstall encre` |
