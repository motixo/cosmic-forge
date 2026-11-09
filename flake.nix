{
  description = "COSMIC Forge and its patched COSMIC compositor";

  inputs = {
    self.submodules = true;
    nixpkgs.url = "github:NixOS/nixpkgs/f4a03c008967841f2a0b3cb7c436fc6fc31671f6";
  };

  outputs = { self, nixpkgs }: {
    overlays.default = final: prev: {
      cosmic-forge = final.callPackage ./nix/package.nix { };
      cosmic-comp = final.callPackage ./nix/compositor.nix {
        inherit (prev) cosmic-comp;
        pinnedCompositor = nixpkgs.legacyPackages.${final.stdenv.hostPlatform.system}.cosmic-comp;
      };
    };

    packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        };
      in
      {
        default = pkgs.cosmic-forge;
        inherit (pkgs) cosmic-forge;
        cosmic-comp-forge = pkgs.cosmic-comp;
      }
    );

    nixosModules.default = import ./nix/module.nix self.overlays.default;
  };
}
