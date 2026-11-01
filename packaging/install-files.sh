#!/bin/sh
# Install Ginrose or Ginrose Studio into a distribution package's root, with
# its menu entries, icons, AppStream metadata and license.
#
# From a checkout, after building:
#   packaging/install-files.sh ginrose|ginrose-studio BINARY DESTDIR [PREFIX]
#
# From an unpacked release, which carries this script at its top:
#   ./install-files.sh DESTDIR [PREFIX]
#
# PREFIX defaults to /usr. The menu entries run the command from PATH, and
# Ginrose finds them in the system's data folders and does not write its own.
set -eu
here=$(cd "$(dirname "$0")" && pwd)

if [ -d "$here/icons" ]; then
    # A release: ginrose-<version>/{ginrose,desktop/,icons/,*.metainfo.xml}.
    if [ -f "$here/ginrose-studio" ]; then name=ginrose-studio; else name=ginrose; fi
    binary=$here/$name
    destdir=${1?usage: install-files.sh DESTDIR [PREFIX]}
    prefix=${2:-/usr}
    entries_dir=$here/desktop
    icons_dir=$here/icons
    metainfo_dir=$here
    license=$here/LICENSE
else
    root=$(cd "$here/.." && pwd)
    name=${1:?usage: install-files.sh ginrose|ginrose-studio BINARY DESTDIR [PREFIX]}
    binary=${2:?missing BINARY}
    destdir=${3?missing DESTDIR}
    prefix=${4:-/usr}
    entries_dir=$root/packaging/desktop
    case "$name" in
        ginrose) icons_dir=$root/assets/icons ;;
        ginrose-studio) icons_dir=$root/assets/studio-icons ;;
    esac
    metainfo_dir=$root/flatpak
    license=$root/LICENSE
fi

case "$name" in
    ginrose)
        id=me.daeese.apps.ginrose
        entries="$id.desktop $id.player.desktop com.roblox.RobloxPlayer.desktop"
        ;;
    ginrose-studio)
        id=me.daeese.apps.ginrose.studio
        entries="$id.desktop $id.editor.desktop com.Roblox.RobloxStudio.desktop"
        ;;
    *)
        echo "install-files.sh: unknown name $name" >&2
        exit 1
        ;;
esac

share="$destdir$prefix/share"
install -Dm755 "$binary" "$destdir$prefix/bin/$name"
for entry in $entries; do
    install -Dm644 "$entries_dir/$entry" "$share/applications/$entry"
done
for icon in "$icons_dir"/*.png; do
    size=$(basename "$icon" .png)
    install -Dm644 "$icon" "$share/icons/hicolor/${size}x${size}/apps/$id.png"
done
install -Dm644 "$metainfo_dir/$id.metainfo.xml" "$share/metainfo/$id.metainfo.xml"
install -Dm644 "$license" "$share/licenses/$name/LICENSE"
