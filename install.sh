#!/bin/sh
# Install Ginrose for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/nunreasonable/ginrose/main/install.sh | sh
#   ./install.sh --source     build from this checkout instead (needs cargo)
#
# Add --with-rosemary to also install Ginrose's build of Rosemary, which adds
# voice chat and Ctrl for copy and paste but needs SDL 3.4 and a recent distro.
# Without it, the setup offers that build or the official one.
#   curl -fsSL .../install.sh | sh -s -- --with-rosemary
#
# Nothing is written outside your home folder.
set -eu

REPO="nunreasonable/ginrose"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ginrose"

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
for arg in "$@"; do
    case "$arg" in
        --source) from_source=1 ;;
        --with-rosemary) with_rosemary=1 ;;
        *) die "Unknown option: $arg" ;;
    esac
done

mkdir -p "$BIN_DIR" "$DATA_DIR/rosemary"
latest() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -o "\"browser_download_url\": *\"[^\"]*$1\"" \
        | head -n1 | sed 's/.*"\(https[^"]*\)"/\1/'
}

if [ "$from_source" = 1 ]; then
    command -v cargo >/dev/null || die "--source needs Rust (https://rustup.rs)."
    cd "$(dirname "$0")"
    say "Building Ginrose"
    cargo build --release
    install -m755 target/release/ginrose "$BIN_DIR/ginrose"
else
    command -v curl >/dev/null || die "curl is needed to download Ginrose."
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Finding the latest release"
    url=$(latest 'ginrose-[^"]*x86_64\.tar\.gz')
    [ -n "$url" ] || die "No release found for $REPO."
    say "Downloading $(basename "$url")"
    curl -fL --progress-bar "$url" -o "$tmp/ginrose.tar.gz"
    tar -xzf "$tmp/ginrose.tar.gz" -C "$tmp"
    install -m755 "$tmp/ginrose" "$BIN_DIR/ginrose"
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

"$BIN_DIR/ginrose" install-desktop
say "Ginrose is installed in $BIN_DIR"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "Add $BIN_DIR to your PATH to run ginrose from a terminal." ;;
esac

if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
    say "Opening the setup"
    nohup "$BIN_DIR/ginrose" setup >/dev/null 2>&1 &
else
    say "Run 'ginrose setup' from your desktop to finish."
fi
