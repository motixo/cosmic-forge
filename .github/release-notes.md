## Changes since v0.10.0

### Added

- **Mica in the official shader catalog:** the bundled shader repository now
  includes Mica's matte wallpaper material, light/dark themes, inactive-window
  fallback and independent Blur and Wallpaper color controls. Install official
  shader pack 7 through **Check for updates** to use it.
- **Official catalog ordering:** the menu follows the order supplied by the
  official shader pack, placing Mica immediately above Aero. Community effects
  keep their relative order after the official presets.
- **About now reports installed component versions:** the Forge app version,
  the installed compositor's COSMIC version and renderer revision, and the
  official shader pack's revision. Bundled shaders and a missing Forge
  compositor are identified explicitly. Reopening About refreshes these values.
- **Per-effect background selection:** choose **Behind window** or **Wallpaper
  only** through the new **Backdrop source** picker. The pack supplies the
  default; your override is saved independently for each effect and survives
  restarts, effect switching and pack updates. **Reset controls** restores the
  pack default. Built-in presets have the same picker.
- **Animated shader pack support:** JSON and JSON + `.frag` imports can declare
  `shader.animated`. The renderer supplies time, frame, window position and
  velocity inputs on all custom bases (`glass`, `aero` and `crystal`). Animation
  behavior is defined by the shader; existing effects remain static by default.
- **Wallpaper and window-state pack support:** optional `shader.backdrop` accepts
  `scene` or `wallpaper`. Shaders can use the engine's `window_active` and
  `is_dark` inputs without creating pack controls for them.
- Updated [shader authoring documentation](https://github.com/motixo/cosmic-forge/blob/v0.11.1/docs/adding-effects.md)
  with the animation, motion, background and window-state contracts.

### Compatibility

- **Animation requires renderer revision 3 or newer. Wallpaper backgrounds and
  window-state inputs require renderer revision 4.** Install
  [Forge Engine engine-v0.5.0](https://github.com/motixo/cosmic-forge-engine/releases/tag/engine-v0.5.0)
  through **Settings → Patches** to use all of these features, then sign out and
  back in. Updating Forge alone does not replace the desktop compositor.
- Forge detects the renderer requirement of imported packs and saved wallpaper
  choices. Existing static packs retain their previous requirements; omitted
  `shader.backdrop` keeps the scene background. Shader ABI 1 is unchanged.
- The source archive pins the compositor-only engine, including the animation,
  background and state-input changes. Panel, dock and lock screen patches are
  not included in that engine source.

### Install or update

Install or update for the current user:

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Ubuntu/Pop!_OS 24.04+: `.deb`. Fedora 43+: `.rpm`. Arch Linux: `.pkg.tar.zst`.
Ubuntu/Pop and Fedora builds support x86_64 and ARM64; Arch packages support x86_64.
Portable archives require glibc 2.35+ and Python 3.11+ for installation.

Application installation preserves effect preferences and installs only Forge.

`SHA256SUMS` covers the release assets. The source archive includes pinned engine and shader sources.

### Validation

Local validation passed: 41 Rust tests, `cargo fmt --check`, Clippy with warnings
denied, the release build, and native About/background-picker UI checks. Engine
rendering checks are detailed in its separate release notes.

[Full source changes](https://github.com/motixo/cosmic-forge/compare/v0.10.0...v0.11.1)
