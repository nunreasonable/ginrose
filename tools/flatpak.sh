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
# With --studio, the same for Ginrose Studio instead:
#
#   ginrose-studio-<version>-x86_64.tar.gz
#     ginrose-studio-<version>/ginrose-studio, LICENSE, icons/<size>.png,
#     me.daeese.apps.ginrose.studio{,.editor}.desktop and its metainfo
#   me.daeese.apps.ginrose.studio.flatpak
#
#   tools/flatpak.sh [--studio] path/to/rosemary [--install]
#
# No SDK is needed: the binaries are built on the host, and GNOME 51's runtime
# has a newer glibc than they need plus SDL 3.4, OpenAL, Vulkan, GTK 4 and
# libadwaita. --install also installs the bundle for this user.
set -eu
cd "$(dirname "$0")/.."
studio=0
if [ "${1:-}" = --studio ]; then
    studio=1
    shift
fi
rosemary=${1:?usage: tools/flatpak.sh [--studio] path/to/rosemary [--install]}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
if [ "$studio" = 1 ]; then
    id=me.daeese.apps.ginrose.studio
    name=ginrose-studio
    client=editor
    icons=assets/studio-icons
    target=target/flatpak/studio
else
    id=me.daeese.apps.ginrose
    name=ginrose
    client=player
    icons=assets/icons
    target=target/flatpak
fi
build=$target/build
repo=$target/repo
stage=$target/stage/$name-$version
# Reproducible: the same commit and toolchain give the same binary wherever it
# is built, so a release can be checked against its source.
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$PWD=/ginrose --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
cargo build --release --locked

rm -rf "$stage"
install -Dm755 "target/release/$name" "$stage/$name"
install -Dm644 LICENSE "$stage/LICENSE"
for file in "$id.desktop" "$id.$client.desktop" "$id.metainfo.xml"; do
    install -Dm644 "flatpak/$file" "$stage/$file"
done
for icon in "$icons"/*.png; do
    install -Dm644 "$icon" "$stage/icons/$(basename "$icon")"
done
mkdir -p dist
archive="dist/$name-$version-x86_64.tar.gz"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 \
    -C "$(dirname "$stage")" -cf - "$name-$version" | gzip -9n > "$archive"

rm -rf "$build"
flatpak build-init "$build" "$id" org.gnome.Platform org.gnome.Platform 51
files="$build/files"
install -Dm755 "$stage/$name" "$files/bin/$name"
install -Dm755 "$rosemary" "$files/bin/rosemary"
for icon in "$stage"/icons/*.png; do
    size=$(basename "$icon" .png)
    install -Dm644 "$icon" "$files/share/icons/hicolor/${size}x${size}/apps/$id.png"
done
install -Dm644 "$stage/$id.desktop" "$files/share/applications/$id.desktop"
install -Dm644 "$stage/$id.$client.desktop" "$files/share/applications/$id.$client.desktop"
install -Dm644 "$stage/$id.metainfo.xml" "$files/share/metainfo/$id.metainfo.xml"
install -Dm644 "$stage/LICENSE" "$files/share/licenses/$id/LICENSE"
if [ "$studio" = 1 ]; then
    # Ginrose's permissions, less Discord's socket (Studio shows nothing on
    # Discord), plus the home folder: Studio opens and saves places, plugins
    # and model files wherever the user keeps them, tools like Rojo write
    # into project folders there, and a Rosemary chosen in the settings is
    # usually somewhere in it too. The network covers Rojo's local server,
    # pulseaudio and input devices playtesting.
    flatpak build-finish "$build" \
        --command=ginrose-studio \
        --share=network \
        --share=ipc \
        --socket=wayland \
        --socket=fallback-x11 \
        --socket=pulseaudio \
        --device=dri \
        --device=input \
        --filesystem=home \
        --require-version=1.16.0
else
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
fi
flatpak build-export "$repo" "$build" stable
mkdir -p dist
flatpak build-bundle "$repo" "dist/$id.flatpak" "$id" stable \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
sha256sum "$archive" "dist/$id.flatpak"
if [ "${2:-}" = --install ]; then
    flatpak install --user -y --noninteractive --reinstall "dist/$id.flatpak"
fi
