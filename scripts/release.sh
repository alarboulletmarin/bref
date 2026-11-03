#!/usr/bin/env bash
# Usage: scripts/release.sh X.Y.Z [notes-file]
# DRY_RUN=1: bump and test offline, restoring files even when a check fails.
# RESUME=1: finish an interrupted preparation for an existing local tag.
# The release stays a draft until release-native.yml has verified every download.
set -euo pipefail
cd "$(dirname "$0")/.."

APP=bref
TITLE=Bref
REPO="alarboulletmarin/$APP"
package_files=(PKGBUILD)
managed=(Cargo.toml Cargo.lock PKGBUILD .SRCINFO)
die() { echo "release: $*" >&2; exit 1; }
ver=${1:?usage: scripts/release.sh X.Y.Z [notes-file]}
[[ $ver =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must look like 1.2.3"
[[ ${DRY_RUN:-0} =~ ^[01]$ && ${RESUME:-0} =~ ^[01]$ ]] || die "DRY_RUN and RESUME must be 0 or 1"
dry=${DRY_RUN:-0}
resume=${RESUME:-0}
[[ $dry != 1 || $resume != 1 ]] || die "DRY_RUN and RESUME cannot be combined"
tag="v$ver"
# Only the generated development recipe in Bref may already differ.
git diff --quiet -- . ':!aur/bref-git/PKGBUILD' || die "uncommitted changes"
git diff --cached --quiet || die "staged changes"
if [ "$dry" = 0 ]; then
    [ "$(git branch --show-current)" = main ] || die "not on main"
fi
if [ "$resume" = 1 ]; then
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "no local tag $tag to resume"
    git merge-base --is-ancestor "$tag" HEAD || die "$tag is not an ancestor of HEAD"
    tagged_ver=$(git show "$tag:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)
    current_ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
    [ "$tagged_ver" = "$ver" ] && [ "$current_ver" = "$ver" ] || die "tag and checkout must both carry version $ver"
else
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        die "tag $tag already exists; use RESUME=1 to finish its preparation"
    fi
fi

release_state=
if [ "$dry" = 0 ]; then
    # Listing distinguishes a missing release from a failed API request. Never
    # treat a network/authentication error as permission to change a public one.
    release_state=$(gh api --paginate "repos/$REPO/releases" --jq ".[] | select(.tag_name == \"$tag\") | .draft")
    case "$release_state" in
        false) die "$tag is already public; this script only changes drafts" ;;
        true) [ "$resume" = 1 ] || die "draft already exists; use RESUME=1" ;;
        '') ;;
        *) die "unexpected release state" ;;
    esac
    git fetch -q origin --tags
    if [ "$resume" = 1 ]; then
        git merge-base --is-ancestor origin/main HEAD || die "main is behind or diverges from origin/main"
    else
        if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
            die "remote tag $tag already exists; use RESUME=1"
        fi
        [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main differs from origin/main"
    fi
fi

scratch=$(mktemp -d)
rollback=0
snapshot() {
    for file in "${managed[@]}"; do
        mkdir -p "$scratch/original/$(dirname "$file")"
        cp "$file" "$scratch/original/$file"
    done
    rollback=1
}
cleanup() {
    code=$?
    trap - EXIT
    if [ "$rollback" = 1 ]; then
        # The index was clean on entry. Restore only the files owned by this
        # script; preserve untracked files and Bref's generated -git recipe.
        git reset -q HEAD -- "${managed[@]}" || code=1
        for file in "${managed[@]}"; do
            cp "$scratch/original/$file" "$file" || code=1
        done
    fi
    rm -rf "$scratch"
    exit "$code"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
notes="$scratch/notes.md"
if [ -n "${2:-}" ]; then
    cp "$2" "$notes"
else
    section=
    if [ -f CHANGELOG.md ]; then
        section=$(awk -v v="$ver" '/^## /{on = ($2 == v || $2 == "[" v "]")} on' CHANGELOG.md | sed 1d)
    fi
    {
        echo "## Changes"; echo
        if [ -n "$section" ]; then
            echo "$section"
        else
            # No previous tag is a valid first release. Include the root commit.
            if [ "$resume" = 1 ]; then
                prev=$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)
                end="$tag"
            else
                prev=$(git describe --tags --abbrev=0 2>/dev/null || true)
                end=HEAD
            fi
            if [ -n "$prev" ]; then range="$prev..$end"; else range="$end"; fi
            git log "$range" --no-merges --pretty='- %s' | grep -vE '^- (chore|build|refactor)(\(.*\))?: ' || true
        fi
        echo; echo "## Install and upgrade"; echo
        echo "See the [README](https://github.com/$REPO#install). All downloads are attached before this release is published."
    } >"$notes"
fi

if [ "$resume" = 0 ]; then
    snapshot
    sed -i "0,/^version = .*/s//version = \"$ver\"/" Cargo.toml
    sed -i "s/^pkgver=.*/pkgver=$ver/; s/^pkgrel=.*/pkgrel=1/" "${package_files[@]}"
    cargo update -p "$APP" --offline -q
    if [ "$dry" = 1 ]; then
        cargo test --locked --offline -q
        cat "$notes"
        git diff --stat -- "${managed[@]}"
        echo "dry run complete: restoring version files"
        exit 0
    fi
    cargo test --locked -q
    git add Cargo.toml Cargo.lock "${package_files[@]}"
    git diff --cached --quiet || git commit -q -m "chore: release $ver"
    rollback=0
    git tag -a "$tag" -m "$TITLE $ver"
fi
# Atomic push prevents a tag reaching the remote without its main commit.
# Repeating the same push also recovers an interruption after local tagging.
git push --atomic origin main "$tag"

snapshot
url="https://github.com/$REPO/archive/refs/tags/$tag.tar.gz"
sha=
for attempt in 1 2 3 4 5; do
    if curl -fsL "$url" -o "$scratch/source.tar.gz"; then
        sha=$(sha256sum "$scratch/source.tar.gz" | cut -d' ' -f1)
        break
    fi
    if [ "$attempt" != 5 ]; then sleep 3; fi
done
[[ $sha =~ ^[0-9a-f]{64}$ ]] || die "could not hash $url; rerun with RESUME=1"
sed -i "s/^sha256sums=.*/sha256sums=('$sha')/" "${package_files[@]}"
# Generate to temporary files so an interrupted makepkg cannot truncate metadata.
for file in "${package_files[@]}"; do
    dir=$(dirname "$file")
    (cd "$dir" && makepkg --printsrcinfo) >"$scratch/srcinfo"
    cp "$scratch/srcinfo" "$dir/.SRCINFO"
done
git add "${package_files[@]}" .SRCINFO
git diff --cached --quiet || git commit -q -m "build: pin $tag checksum for the AUR"
rollback=0
git push origin main

if [ -z "$release_state" ]; then
    gh release create "$tag" --repo "$REPO" --draft --verify-tag --title "$TITLE $ver" --notes-file "$notes"
fi
# Draft release events do not start packaging. Dispatch the orchestrator
# explicitly, including on recovery; never publish from this local script.
gh workflow run release-native.yml --repo "$REPO" --ref main -f "tag=$tag"
echo "Prepared $tag as a draft; release-native.yml will publish only after every download passes."
