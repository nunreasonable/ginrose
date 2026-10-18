#!/bin/sh
# Build the files Ginrose is published as, into dist/:
#
#   ginrose-<version>-x86_64.tar.gz   what the Flathub manifest downloads:
#     ginrose-<version>/ginrose
#     ginrose-<version>/LICENSE
#     ginrose-<version>/me.daeese.apps.ginrose.desktop
#     ginrose-<version>/me.daeese.apps.ginrose.player.desktop
#     ginrose-<version>/me.daeese.apps.ginrose.metainfo.xml
#     ginrose-<version>/icons/<size>.png      16 to 512
#   me.daeese.apps.ginrose.flatpak    a bundle made from that same folder plus
#                                      a Rosemary binary, for testing here
#
#   tools/flatpak.sh path/to/rosemary [--install]
#
# No SDK is needed: the binaries are built on the host, and GNOME 51's runtime
# has a newer glibc than they need plus SDL 3.4, OpenAL, Vulkan, GTK 4 and
# libadwaita. --install also installs the bundle for this user.
set -eu
cd "$(dirname "$0")/.."
rosemary=${1:?usage: tools/flatpak.sh path/to/rosemary [--install]}
id=me.daeese.apps.ginrose
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
build=target/flatpak/build
repo=target/flatpak/repo
stage=target/flatpak/stage/ginrose-$version
cargo build --release

rm -rf "$stage"
install -Dm755 target/release/ginrose "$stage/ginrose"
install -Dm644 LICENSE "$stage/LICENSE"
for file in "$id.desktop" "$id.player.desktop" "$id.metainfo.xml"; do
    install -Dm644 "flatpak/$file" "$stage/$file"
done
for icon in assets/icons/*.png; do
    install -Dm644 "$icon" "$stage/icons/$(basename "$icon")"
done
mkdir -p dist
archive="dist/ginrose-$version-x86_64.tar.gz"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 \
    -C "$(dirname "$stage")" -cf - "ginrose-$version" | gzip -9n > "$archive"

rm -rf "$build"
flatpak build-init "$build" "$id" org.gnome.Platform org.gnome.Platform 51
files="$build/files"
install -Dm755 "$stage/ginrose" "$files/bin/ginrose"
install -Dm755 "$rosemary" "$files/bin/rosemary"
for icon in "$stage"/icons/*.png; do
    size=$(basename "$icon" .png)
    install -Dm644 "$icon" "$files/share/icons/hicolor/${size}x${size}/apps/$id.png"
done
install -Dm644 "$stage/$id.desktop" "$files/share/applications/$id.desktop"
install -Dm644 "$stage/$id.player.desktop" "$files/share/applications/$id.player.desktop"
install -Dm644 "$stage/$id.metainfo.xml" "$files/share/metainfo/$id.metainfo.xml"
install -Dm644 "$stage/LICENSE" "$files/share/licenses/$id/LICENSE"
flatpak build-finish "$build" \
    --command=ginrose \
    --share=network \
    --share=ipc \
    --socket=wayland \
    --socket=fallback-x11 \
    --socket=pulseaudio \
    --device=dri \
    --device=input \
    --filesystem=xdg-run/discord-ipc-0 \
    --filesystem=xdg-run/app/com.discordapp.Discord:create \
    --require-version=1.16.0
flatpak build-export "$repo" "$build" stable
mkdir -p dist
flatpak build-bundle "$repo" "dist/$id.flatpak" "$id" stable \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
sha256sum "$archive" "dist/$id.flatpak"
if [ "${2:-}" = --install ]; then
    flatpak install --user -y --noninteractive --reinstall "dist/$id.flatpak"
fi
