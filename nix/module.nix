overlay:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  engine = lib.importJSON ../engine/release.json;
in
{
  options.programs.cosmic-forge.enable = lib.mkEnableOption "COSMIC Forge and its patched compositor";

  config = lib.mkIf config.programs.cosmic-forge.enable {
    assertions = [
      {
        assertion = config.services.desktopManager.cosmic.enable;
        message = "programs.cosmic-forge.enable requires services.desktopManager.cosmic.enable.";
      }
    ];

    nixpkgs.overlays = [ overlay ];
    environment.systemPackages = [ pkgs.cosmic-forge ];
    environment.etc."cosmic-forge/engine.json".text = builtins.toJSON {
      inherit (engine) abi renderer_revision;
      cosmic_version = pkgs.cosmic-comp.version;
      release = "engine-v0.6.5";
      executable = "${pkgs.cosmic-comp}/bin/.cosmic-comp-wrapped";
    };
  };
}
