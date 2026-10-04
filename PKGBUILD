# Maintainer: Andrea Larboullet Marin <a.larboulletmarin@gmail.com>
pkgname=encre
pkgver=0.1.1
pkgrel=1
pkgdesc="Fast, minimal Markdown note-taking app"
arch=('x86_64' 'aarch64')
url="https://github.com/alarboulletmarin/encre"
license=('MIT')
depends=('libxcb' 'libxkbcommon' 'libxkbcommon-x11' 'wayland' 'vulkan-icd-loader')
makedepends=('cargo' 'fontconfig' 'freetype2')
optdepends=('vulkan-driver: a Vulkan driver for your GPU is required to open the window')
# The binary is stripped at build time: a -debug package would be empty, and its
# file would clash between encre and encre-git.
options=('!debug')
# Pinned by scripts/release.sh once the release tarball exists.
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('5cf449bdd46e43315ad0b658040b66d3e39be28f2cfe6b321643947feb015bd4')

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
