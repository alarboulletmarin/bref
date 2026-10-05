# Maintainer: Andrea Larboullet Marin <a.larboulletmarin@gmail.com>
pkgname=bref
pkgver=0.1.7
pkgrel=1
pkgdesc="Fast, minimal Markdown note-taking app"
arch=('x86_64' 'aarch64')
url="https://github.com/alarboulletmarin/bref"
license=('MIT')
depends=('libxcb' 'libxkbcommon' 'libxkbcommon-x11' 'wayland' 'vulkan-icd-loader')
makedepends=('cargo' 'fontconfig' 'freetype2')
optdepends=('vulkan-driver: a Vulkan driver for your GPU is required to open the window')
# The binary is stripped at build time: a -debug package would be empty, and its
# file would clash between bref and bref-git.
options=('!debug')
# The app was called encre until 0.1.4.
conflicts=('encre')
replaces=('encre')
# Pinned by scripts/release.sh once the release tarball exists.
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('2de1efcb93ab70dd42cc6d3af45b0265695c7dbef88e0d9a24b308764587ddf3')

prepare() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

package() {
    cd "$pkgname-$pkgver"
    make DESTDIR="$pkgdir" PREFIX=/usr install
}
