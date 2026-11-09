{
  lib,
  rustPlatform,
  libcosmicAppHook,
  copyDesktopItems,
  makeDesktopItem,
  fontconfig,
  wayland,
  dbus,
  udev,
  openssl,
  curl,
  python3,
  xdg-utils,
}:

let
  appId = "io.github.motixo.CosmicForge";
  manifest = (lib.importTOML ../Cargo.toml).package;
  engine = lib.importJSON ../engine/release.json;
in
rustPlatform.buildRustPackage {
  inherit (manifest) version;
  pname = manifest.name;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions (
      [
        ../Cargo.toml
        ../Cargo.lock
        ../src
        ../tests
        ../resources/icons/io.github.motixo.CosmicForge.svg
        ../engine/release.json
        ../engine/scripts/package_engine.py
        ../shaders/effects.json
        ../shaders/effects
        ../shaders/shaders
        ../shaders/examples
        ../shaders/scripts
      ]
      ++ map (name: ../engine + "/${name}") engine.bundle_files
    );
  };
  cargoHash = "sha256-lgRVqfEe4VPOgmvZUXRbq5JFmsH/2o31EX692f8+bGM=";

  nativeBuildInputs = [
    (libcosmicAppHook.override { includeSettings = false; })
    copyDesktopItems
  ];
  buildInputs = [
    fontconfig
    wayland
    dbus
    udev
    openssl
  ];
  nativeCheckInputs = [ python3 ];
  checkNoDefaultFeatures = true;

  desktopItems = [
    (makeDesktopItem {
      name = appId;
      desktopName = "COSMIC Forge";
      comment = "Manage and fine tune COSMIC glass shaders";
      exec = "cosmic-forge";
      icon = appId;
      startupWMClass = appId;
      categories = [
        "Settings"
        "DesktopSettings"
      ];
    })
  ];

  postInstall = ''
    install -Dm644 resources/icons/${appId}.svg \
      "$out/share/icons/hicolor/scalable/apps/${appId}.svg"
  '';
  preFixup = ''
    libcosmicAppWrapperArgs+=(--prefix PATH : ${
      lib.makeBinPath [
        curl
        python3
        xdg-utils
      ]
    })
  '';

  meta = {
    description = manifest.description;
    homepage = "https://github.com/motixo/cosmic-forge";
    license = lib.licenses.gpl3Only;
    mainProgram = "cosmic-forge";
    platforms = lib.platforms.linux;
  };
}
