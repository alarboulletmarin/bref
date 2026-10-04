#!/usr/bin/env bash
# Usage: scripts/release.sh X.Y.Z [notes-file]
#
# Bumps the version, tags, pushes, creates the GitHub release, then pins the
# tarball checksum in the PKGBUILD and regenerates .SRCINFO. Run it on main,
# clean and in sync with origin. Without a notes file, the notes are the
# commit subjects since the previous tag (chore/build commits left out).
set -euo pipefail
cd "$(dirname "$0")/.."

die() { echo "release: $*" >&2; exit 1; }

ver=${1:?usage: scripts/release.sh X.Y.Z [notes-file]}
[[ $ver =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must look like 1.2.3"
[ "$(git branch --show-current)" = main ] || die "not on main"
git diff --quiet && git diff --cached --quiet || die "uncommitted changes"
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main differs from origin/main"
git rev-parse -q --verify "refs/tags/v$ver" >/dev/null && die "tag v$ver already exists"

# First release: there is no previous tag, the notes cover the whole history.
prev=$(git describe --tags --abbrev=0 2>/dev/null || git rev-list --max-parents=0 HEAD)
notes=$(mktemp)
if [ -n "${2:-}" ]; then
    cp "$2" "$notes"
else
    { echo "## Changes"; echo
      git log "$prev..HEAD" --no-merges --pretty='- %s' | grep -vE '^- (chore|build)(\(.*\))?: ' || true
      echo; echo "## Install and upgrade"; echo
      echo "See the [README](https://github.com/alarboulletmarin/encre#install). On Arch, download \`encre-x86_64.pkg.tar.zst\` from the assets below, then \`sudo pacman -U ./encre-x86_64.pkg.tar.zst\`."
    } >"$notes"
fi

sed -i "0,/^version = .*/s//version = \"$ver\"/" Cargo.toml
sed -i "s/^pkgver=.*/pkgver=$ver/" PKGBUILD
cargo update -p encre --offline -q
cargo test --locked -q

git add Cargo.toml Cargo.lock PKGBUILD
# Nothing to commit when the files already carry this version (first release).
git diff --cached --quiet || git commit -q -m "chore: release $ver"
git tag -a "v$ver" -m "encre $ver"
git push origin main "v$ver"
gh release create "v$ver" --title "encre $ver" --notes-file "$notes"

# The tarball GitHub serves for the tag is what the PKGBUILD downloads: hash that one.
url="https://github.com/alarboulletmarin/encre/archive/refs/tags/v$ver.tar.gz"
sha=
for _ in 1 2 3 4 5; do
    sha=$(curl -fsL "$url" | sha256sum | cut -d' ' -f1) && [ -n "$sha" ] && break
    sleep 3
done
[ -n "$sha" ] || die "could not hash $url; pin it by hand"
sed -i "s/^sha256sums=.*/sha256sums=('$sha')/" PKGBUILD
makepkg --printsrcinfo >.SRCINFO
git add PKGBUILD .SRCINFO
git commit -q -m "build: pin v$ver checksum for the AUR"
git push origin main
echo "released v$ver ($sha)"
