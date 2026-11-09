# Ginrose and Ginrose Studio from source, for Fedora, openSUSE and other RPM
# distributions. The crates come vendored in Source1, so the build needs no
# network: packaging/rpm/Makefile makes both sources (COPR runs it with
# "make srpm", where the network is allowed).

# The release profile is built without debug info.
%global debug_package %{nil}

Name:           ginrose
Version:        0.3.5
Release:        1%{?dist}
Summary:        Roblox launcher for Linux, built around Rosemary
# Ginrose is GPL-3.0-or-later; the rest are the statically linked crates'.
License:        GPL-3.0-or-later AND MIT AND Apache-2.0 AND Unicode-3.0 AND ISC AND BSD-3-Clause AND CDLA-Permissive-2.0
URL:            https://github.com/nunreasonable/ginrose
Source0:        %{url}/archive/refs/tags/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz
ExclusiveArch:  x86_64

BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.14
BuildRequires:  pkgconfig(libadwaita-1) >= 1.6
Recommends:     xdg-utils
%if 0%{?fedora}
# What a Rosemary that Ginrose downloads needs, as Fedora names it.
Suggests:       SDL3 >= 3.4
Suggests:       openal-soft
Suggests:       vulkan-loader
%endif

%description
Ginrose runs the official macOS Roblox client on Linux through Rosemary, a
macOS compatibility layer. It downloads the client straight from Roblox, keeps
it up to date, sets it up to run well and shows what you are playing on
Discord. The client is never modified, and Ginrose refuses to run one that
was.

Rosemary is not part of this package: Ginrose uses one on PATH, or downloads
one in its setup.

%package -n ginrose-studio
Summary:        Roblox Studio launcher for Linux, built around Rosemary
Recommends:     xdg-utils
%if 0%{?fedora}
Suggests:       SDL3 >= 3.4
Suggests:       openal-soft
Suggests:       vulkan-loader
Suggests:       webkitgtk6.0
Suggests:       glib-networking
%endif

%description -n ginrose-studio
Ginrose Studio runs the official macOS Roblox Studio on Linux through
Rosemary. It downloads Studio straight from Roblox and keeps it up to date,
and lets you set any FastFlag, environment variable or Rosemary build.

%prep
%autosetup -n %{name}-%{version}
tar -xJf %{SOURCE1}
mkdir -p .cargo
cat > .cargo/config.toml <<'CONFIG'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CONFIG

%build
export RUSTFLAGS="%{?build_rustflags}"
cargo build --release --offline --locked

%check
cargo test --release --offline --locked --lib

%install
packaging/install-files.sh ginrose target/release/ginrose %{buildroot} %{_prefix}
packaging/install-files.sh ginrose-studio target/release/ginrose-studio %{buildroot} %{_prefix}
# %%license puts them where the distribution keeps licenses.
rm -r %{buildroot}%{_datadir}/licenses

%files
%license LICENSE
%doc README.md
%{_bindir}/ginrose
%{_datadir}/applications/me.daeese.apps.ginrose.desktop
%{_datadir}/applications/me.daeese.apps.ginrose.player.desktop
%{_datadir}/applications/com.roblox.RobloxPlayer.desktop
%{_datadir}/icons/hicolor/*/apps/me.daeese.apps.ginrose.png
%{_datadir}/metainfo/me.daeese.apps.ginrose.metainfo.xml

%files -n ginrose-studio
%license LICENSE
%{_bindir}/ginrose-studio
%{_datadir}/applications/me.daeese.apps.ginrose.studio.desktop
%{_datadir}/applications/me.daeese.apps.ginrose.studio.editor.desktop
%{_datadir}/applications/com.Roblox.RobloxStudio.desktop
%{_datadir}/icons/hicolor/*/apps/me.daeese.apps.ginrose.studio.png
%{_datadir}/metainfo/me.daeese.apps.ginrose.studio.metainfo.xml

%changelog
* Sun Nov 08 2026 nunreasonable <rafaelpasquarella@gmail.com> - 0.3.5-1
- The Roblox and Rosemary downloads finish on slower connections

* Fri Nov 06 2026 nunreasonable <rafaelpasquarella@gmail.com> - 0.3.4-1
- The rosemary command reports a configuration it could not save

* Fri Nov 06 2026 nunreasonable <rafaelpasquarella@gmail.com> - 0.3.3-1
- Presented handlers run when the frame is on screen, without holding back presentation

* Thu Nov 05 2026 nunreasonable <rafaelpasquarella@gmail.com> - 0.3.2-1
- Rosemary with smoother frame pacing and Wayland pointer warps

* Sat Oct 31 2026 nunreasonable <rafaelpasquarella@gmail.com> - 0.3.1-1
- Native packages
