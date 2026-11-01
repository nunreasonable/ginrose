#!/bin/sh
# Make Debian and RPM packages of the official builds, from the release
# tarballs tools/flatpak.sh left in dist/:
#
#   tools/native-packages.sh
#
# For each of Ginrose and Ginrose Studio whose tarball is there:
#   dist/<name>_<version>_amd64.deb         Debian 13, Ubuntu 25.04 and later
#   dist/<name>-<version>-1.x86_64.rpm      Fedora, openSUSE Tumbleweed
#
# The binaries need glibc 2.39, GTK 4.14 and libadwaita 1.6. Both packages
# hold the same files: the tarball's own install-files.sh puts them in place.
# Rosemary is not included; Ginrose uses one on PATH or downloads one.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
maintainer="nunreasonable <rafaelpasquarella@gmail.com>"
homepage="https://github.com/nunreasonable/ginrose"
work=target/native-packages
rm -rf "$work"
mkdir -p "$work"

reproducible_tar() {
    tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$SOURCE_DATE_EPOCH" "$@"
}

deb() {
    name=$1 root=$2 section=$3 suggests=$4 summary=$5 description=$6
    out="dist/${name}_${version}_amd64.deb"
    control="$work/$name-control"
    data="$work/$name-deb"
    rm -rf "$control" "$data"
    cp -a "$root" "$data"
    # Debian keeps the license as the package's copyright file.
    mkdir -p "$data/usr/share/doc/$name"
    mv "$data/usr/share/licenses/$name/LICENSE" "$data/usr/share/doc/$name/copyright"
    rm -r "$data/usr/share/licenses"
    mkdir -p "$control"
    cat > "$control/control" <<CONTROL
Package: $name
Version: $version
Architecture: amd64
Maintainer: $maintainer
Installed-Size: $(du -sk --apparent-size "$data" | cut -f1)
Depends: libc6 (>= 2.39), libgcc-s1, libglib2.0-0t64 | libglib2.0-0, libgtk-4-1 (>= 4.14), libadwaita-1-0 (>= 1.6)
Recommends: xdg-utils
Suggests: $suggests
Section: $section
Priority: optional
Homepage: $homepage
Description: $summary
$description
CONTROL
    (cd "$data" && find . -type f | sort | sed 's|^\./||' | xargs md5sum) > "$control/md5sums"
    reproducible_tar -C "$control" -cJf "$work/control.tar.xz" ./control ./md5sums
    reproducible_tar -C "$data" -cJf "$work/data.tar.xz" ./usr
    printf '2.0\n' > "$work/debian-binary"
    rm -f "$out"
    (cd "$work" && ar -rcD "../../$out" debian-binary control.tar.xz data.tar.xz)
    rm "$work/control.tar.xz" "$work/data.tar.xz" "$work/debian-binary"
    echo "$out"
}

rpm_package() {
    name=$1 root=$2 summary=$3 description=$4
    top="$PWD/$work/rpmbuild-$name"
    rm -rf "$top"
    mkdir -p "$top/SPECS"
    cat > "$top/SPECS/$name.spec" <<SPEC
%global debug_package %{nil}
%global __os_install_post %{nil}
%define _build_id_links none
Name: $name
Version: $version
Release: 1
Summary: $summary
License: GPL-3.0-or-later AND MIT AND Apache-2.0 AND Unicode-3.0 AND ISC AND BSD-3-Clause AND CDLA-Permissive-2.0
URL: $homepage
Packager: $maintainer
ExclusiveArch: x86_64
Recommends: xdg-utils

%description
$description

%install
cp -a "$PWD/$root/." %{buildroot}/

%files
/usr/bin/$name
/usr/share/applications/*.desktop
/usr/share/icons/hicolor/*/apps/*.png
/usr/share/metainfo/*.metainfo.xml
%license /usr/share/licenses/$name/LICENSE
SPEC
    rpmbuild -bb --quiet "$top/SPECS/$name.spec" \
        --define "_topdir $top" \
        --define "use_source_date_epoch_as_buildtime 1" \
        --define "clamp_mtime_to_source_date_epoch 1" >/dev/null
    out="dist/$name-$version-1.x86_64.rpm"
    cp "$top/RPMS/x86_64/$name-$version-1.x86_64.rpm" "$out"
    echo "$out"
}

for name in ginrose ginrose-studio; do
    archive="dist/$name-$version-x86_64.tar.gz"
    [ -f "$archive" ] || continue
    unpacked="$work/$name-release"
    mkdir -p "$unpacked"
    tar -xzf "$archive" -C "$unpacked"
    release="$unpacked/$name-$version"
    [ -x "$release/install-files.sh" ] ||
        { echo "$archive has no install-files.sh; make it with tools/flatpak.sh" >&2; exit 1; }
    root="$work/$name-root"
    "$release/install-files.sh" "$root" /usr
    case "$name" in
        ginrose)
            summary="Roblox launcher for Linux, built around Rosemary"
            section=games
            suggests="libsdl3-0 (>= 3.4), libopenal1, libvulkan1"
            text="Ginrose runs the official macOS Roblox client on Linux through Rosemary,
a macOS compatibility layer. It downloads the client straight from Roblox,
keeps it up to date, sets it up to run well and shows what you are playing
on Discord. The client is never modified, and Ginrose refuses to run one
that was. Rosemary is not part of this package: Ginrose uses one on PATH,
or downloads one in its setup."
            ;;
        ginrose-studio)
            summary="Roblox Studio launcher for Linux, built around Rosemary"
            section=devel
            suggests="libsdl3-0 (>= 3.4), libopenal1, libvulkan1, libwebkitgtk-6.0-4, glib-networking"
            text="Ginrose Studio runs the official macOS Roblox Studio on Linux through
Rosemary. It downloads Studio straight from Roblox, keeps it up to date, and
lets you set any FastFlag, environment variable or Rosemary build. Rosemary
is not part of this package: Ginrose Studio uses one on PATH, or downloads
one in its setup."
            ;;
    esac
    # A Debian description's lines start with a space; "." stands for a blank one.
    deb "$name" "$root" "$section" "$suggests" "$summary" "$(printf '%s\n' "$text" | sed 's/^/ /')"
    rpm_package "$name" "$root" "$summary" "$text"
done
