{
  description = "Ginrose and Ginrose Studio: Roblox and Roblox Studio launchers for Linux, built around Rosemary";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # Ginrose's build of Rosemary, as the Flatpaks ship. Point it at
    # github:microlift/rosemary for the upstream one.
    rosemary = {
      url = "github:nunreasonable/rosemary/ginrose-0.3.2";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rosemary, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      build =
        flavor:
        pkgs.callPackage ./packaging/nix/package.nix {
          inherit flavor;
          rosemary = rosemary.packages.${system}.default;
        };
    in
    {
      packages.${system} = rec {
        ginrose = build "ginrose";
        ginrose-studio = build "ginrose-studio";
        default = ginrose;
      };

      overlays.default = final: prev: {
        ginrose = final.callPackage ./packaging/nix/package.nix {
          rosemary = rosemary.packages.${system}.default;
        };
        ginrose-studio = final.callPackage ./packaging/nix/package.nix {
          flavor = "ginrose-studio";
          rosemary = rosemary.packages.${system}.default;
        };
      };
    };
}
