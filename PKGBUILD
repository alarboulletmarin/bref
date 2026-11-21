# Maintainer: Andrea Larboullet Marin <a.larboulletmarin@gmail.com>
pkgname=bref
pkgver=0.4.9
pkgrel=1
pkgdesc="Fast, minimal Markdown note-taking app"
arch=('x86_64' 'aarch64')
url="https://github.com/alarboulletmarin/bref"
license=('MIT')
depends=('libxcb' 'libxkbcommon' 'libxkbcommon-x11' 'wayland' 'vulkan-icd-loader' 'noto-fonts-emoji')
makedepends=('cargo' 'fontconfig' 'freetype2')
optdepends=('vulkan-driver: a Vulkan driver for your GPU is required to open the window'
            'poppler: search the text of PDFs from the vault search')
# The binary is stripped at build time: a -debug package would be empty, and its
# file would clash between bref and bref-git.
options=('!debug')
# The app was called encre until 0.1.4.
conflicts=('encre')
replaces=('encre')
# Pinned by scripts/release.sh once the release tarball exists.
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('fc93ac4348ed52339199f41e7955b20f7b202a390be42b0379b74b63794de523')

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
