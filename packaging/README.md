# Packaging

Native packages of Ginrose and Ginrose Studio, for distributions that would
rather not use the Flatpaks. Each one installs the binary, its menu entries,
icons and AppStream metadata. Ginrose sees that the menu entries came with a
package and does not write its own copies into your home folder.

Rosemary is not part of these packages. Ginrose uses a `rosemary` on `PATH`
(from a distribution package, for example), or downloads one in its setup into
its own folder. A downloaded Rosemary needs SDL 3.4 or newer and OpenAL, and
Ginrose Studio's login needs WebKitGTK 6.0.

All of them need GTK 4.14 and libadwaita 1.6 or newer.

| Distribution | Recipe | What it builds |
| --- | --- | --- |
| Arch Linux and derivatives | [`arch/ginrose`](arch/ginrose/PKGBUILD) | `ginrose` and `ginrose-studio`, from source |
| | [`arch/ginrose-bin`](arch/ginrose-bin/PKGBUILD) | `ginrose-bin` and `ginrose-studio-bin`, the official builds |
| Debian 13, Ubuntu 25.04 and later | [`tools/native-packages.sh`](../tools/native-packages.sh) | `.deb` of the official builds |
| Fedora, openSUSE Tumbleweed | [`rpm/ginrose.spec`](rpm/ginrose.spec) | `ginrose` and `ginrose-studio`, from source |
| | [`tools/native-packages.sh`](../tools/native-packages.sh) | `.rpm` of the official builds |
| NixOS and Nix | [`../flake.nix`](../flake.nix), [`nix/package.nix`](nix/package.nix) | `ginrose` and `ginrose-studio`, from source, with Rosemary |
| Gentoo | [`gentoo/games-util/ginrose`](gentoo/games-util/ginrose) | `games-util/ginrose`, from source; USE `player` and `studio` |

Every from-source recipe calls [`install-files.sh`](install-files.sh) to put
the files in place, and the official release tarballs carry the same script.
The menu entries in [`desktop/`](desktop) are the ones Ginrose writes itself,
with the command run from `PATH`; a test keeps them the same.

## Arch Linux

The PKGBUILDs are what goes to the AUR. To build one here:

```sh
cd packaging/arch/ginrose
makepkg -si
```

## Debian and Ubuntu

Debian's Rust is too old to build Ginrose (it needs 1.88), so the `.deb`
packages hold the official builds. `tools/flatpak.sh` makes the release
tarballs in `dist/`, then:

```sh
tools/native-packages.sh
sudo apt install ./dist/ginrose_*_amd64.deb
```

The binaries need glibc 2.39: Ubuntu 24.04 has that but only libadwaita 1.5,
so it is not supported.

## Fedora and openSUSE

The spec builds from source with the crates vendored, so the build itself
needs no network. To make a source RPM and build it:

```sh
make -f packaging/rpm/Makefile srpm outdir=dist
rpmbuild --rebuild dist/ginrose-*.src.rpm
```

For COPR, choose the "make srpm" method with `packaging/rpm/Makefile`.

## NixOS

The flake builds Ginrose and Ginrose Studio with Ginrose's build of Rosemary on
their `PATH`; a Rosemary that the setup downloads cannot run on NixOS. To try
it:

```sh
nix run github:nunreasonable/ginrose
nix run github:nunreasonable/ginrose#ginrose-studio
```

In a system flake, add `ginrose.url = "github:nunreasonable/ginrose"` to the
inputs and `ginrose.packages.x86_64-linux.ginrose` to
`environment.systemPackages`, or use `ginrose.overlays.default`. To use the
upstream Rosemary instead, set
`ginrose.inputs.rosemary.url = "github:microlift/rosemary"`.

## Gentoo

The ebuild goes into an overlay with its `Manifest`
(`ebuild ginrose-<version>.ebuild manifest`). Its crate list comes from
`Cargo.lock`, so it is made again for every release (`pycargoebuild` does
that). Rosemary for Gentoo is `app-emulation/rosemary` in
[rosemary-gentoo-overlay](https://github.com/nunreasonable/rosemary-gentoo-overlay).
