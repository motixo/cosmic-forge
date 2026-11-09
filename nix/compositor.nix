{
  lib,
  cosmic-comp,
  pinnedCompositor,
}:

let
  engine = lib.importJSON ../engine/release.json;
  version = pinnedCompositor.version;
  commit = engine.sources.${version};
  patches = builtins.filter (lib.hasPrefix "patches/") engine.bundle_files;
  modules = [
    "runtime"
    "loader"
    "animation"
    "backdrop"
    "surface_state"
  ];
in
assert lib.assertMsg
  (cosmic-comp.version == version && cosmic-comp.src.outputHash == pinnedCompositor.src.outputHash)
  "COSMIC Forge requires the tested cosmic-comp ${version} source. Use Forge's pinned Nixpkgs input.";
cosmic-comp.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ map (name: ../engine + "/${name}") patches;
  env = (old.env or { }) // {
    CARGO_PROFILE_RELEASE_LTO = "thin";
  };
  cargoTestFlags = (old.cargoTestFlags or [ ]) ++ [ "--lib" ];

  postPatch = (old.postPatch or "") + ''
    ${lib.concatMapStringsSep "\n" (name: ''
      cp ${../engine + "/${name}.rs"} src/backend/render/wayland/forge_${name}.rs
    '') modules}
    cp ${../engine/glass.frag} src/backend/render/shaders/forge_glass.frag
    cat > build.rs <<'RUST'
    fn main() { println!("cargo:rustc-env=GIT_HASH=${commit}-forge-r${toString engine.renderer_revision}"); }
    RUST
  '';

  postFixup = (old.postFixup or "") + ''
    test -x "$out/bin/.cosmic-comp-wrapped"
  '';
})
