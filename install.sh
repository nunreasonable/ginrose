#!/bin/sh
# Install Ginrose for the current user, without Flatpak.
#
#   ./install.sh              download the latest release from ginrose-flatpak
#   ./install.sh --source     build from this checkout instead (needs cargo)
#   ./install.sh --studio     Ginrose Studio instead of Ginrose (either way)
#
# Add --with-rosemary to also install Ginrose's build of Rosemary, the latest
# release plus a few fixes, which needs SDL 3.4 and a recent distro. Without
# it, the setup offers that build or the official AppImage.
#
# Nothing is written outside your home folder.
set -eu

REPO="nunreasonable/ginrose-flatpak"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"

say() { printf '\033[1;35m::\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m!!\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Linux ] || die "Ginrose runs on Linux."
[ "$(uname -m)" = x86_64 ] || die "Ginrose is built for x86_64 for now."

missing=""
for lib in libgtk-4.so.1 libadwaita-1.so.0; do
    ldconfig -p 2>/dev/null | grep -q "$lib" || missing="$missing $lib"
done
if [ -n "$missing" ]; then
    say "Missing:$missing"
    say "Fedora: sudo dnf install gtk4 libadwaita"
    say "Debian/Ubuntu: sudo apt install libgtk-4-1 libadwaita-1-0"
    say "Arch: sudo pacman -S gtk4 libadwaita"
    die "Install those, then run this again."
fi

from_source=0
with_rosemary=0
name=ginrose
for arg in "$@"; do
    case "$arg" in
        --source) from_source=1 ;;
        --with-rosemary) with_rosemary=1 ;;
        --studio) name=ginrose-studio ;;
        *) die "Unknown option: $arg" ;;
    esac
done
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/$name"

mkdir -p "$BIN_DIR" "$DATA_DIR/rosemary"
latest() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -o "\"browser_download_url\": *\"[^\"]*$1\"" \
        | head -n1 | sed 's/.*"\(https[^"]*\)"/\1/'
}

if [ "$from_source" = 1 ]; then
    command -v cargo >/dev/null || die "--source needs Rust (https://rustup.rs)."
    cd "$(dirname "$0")"
    say "Building $name"
    cargo build --release
    install -m755 "target/release/$name" "$BIN_DIR/$name"
else
    command -v curl >/dev/null || die "curl is needed to download Ginrose."
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Finding the latest release"
    # A digit after the name, so Ginrose's never matches Ginrose Studio's.
    url=$(latest "$name-[0-9][^\"]*x86_64\\.tar\\.gz")
    [ -n "$url" ] || die "No $name release found for $REPO."
    say "Downloading $(basename "$url")"
    curl -fL --progress-bar "$url" -o "$tmp/$name.tar.gz"
    tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
    # At the top of the archive, or in a folder named after the release.
    binary=$(find "$tmp" -type f -name "$name" | head -n1)
    [ -n "$binary" ] || die "The archive has no $name in it."
    install -m755 "$binary" "$BIN_DIR/$name"
fi

if [ "$with_rosemary" = 1 ]; then
    command -v curl >/dev/null || die "curl is needed to download Rosemary."
    url=$(latest 'rosemary-ginrose-x86_64')
    [ -n "$url" ] || die "No Rosemary build in the latest release of $REPO."
    say "Downloading Ginrose's Rosemary"
    curl -fL --progress-bar "$url" -o "$DATA_DIR/rosemary/rosemary.new"
    chmod 755 "$DATA_DIR/rosemary/rosemary.new"
    mv -f "$DATA_DIR/rosemary/rosemary.new" "$DATA_DIR/rosemary/rosemary"
fi

"$BIN_DIR/$name" install-desktop
say "$name is installed in $BIN_DIR"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "Add $BIN_DIR to your PATH to run $name from a terminal." ;;
esac

if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
    say "Opening the setup"
    nohup "$BIN_DIR/$name" setup >/dev/null 2>&1 &
else
    say "Run '$name setup' from your desktop to finish."
fi
