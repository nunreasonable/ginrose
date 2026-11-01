# Ginrose or Ginrose Studio, with a Rosemary on its PATH: on NixOS the
# Rosemary binaries the setup downloads cannot run, so one has to come from
# Nix. The flake passes Ginrose's build of Rosemary; any other works.
{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  glib,
  gtk4,
  libadwaita,
  xdg-utils,
  rosemary ? null,
  flavor ? "ginrose",
}:

assert lib.elem flavor [
  "ginrose"
  "ginrose-studio"
];

rustPlatform.buildRustPackage {
  pname = flavor;
  version = (lib.importTOML ../../Cargo.toml).package.version;

  src = lib.cleanSourceWith {
    src = ../..;
    filter = path: type: !(lib.hasInfix "/target" path || lib.hasInfix "/dist" path);
  };
  cargoLock.lockFile = ../../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
  ];
  buildInputs = [
    glib
    gtk4
    libadwaita
  ];

  cargoBuildFlags = [
    "--bin"
    flavor
  ];
  cargoTestFlags = [ "--lib" ];

  postInstall = ''
    mv $out/bin/${flavor} built
    patchShebangs packaging/install-files.sh
    packaging/install-files.sh ${flavor} built "" $out
  '';

  preFixup = ''
    gappsWrapperArgs+=(--prefix PATH : ${lib.makeBinPath ([ xdg-utils ] ++ lib.optional (rosemary != null) rosemary)})
  '';

  meta = {
    description =
      if flavor == "ginrose" then
        "Roblox launcher for Linux, built around Rosemary"
      else
        "Roblox Studio launcher for Linux, built around Rosemary";
    homepage = "https://github.com/nunreasonable/ginrose";
    license = lib.licenses.gpl3Plus;
    mainProgram = flavor;
    platforms = [ "x86_64-linux" ];
  };
}
