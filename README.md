# COSMIC Forge

A native mod manager for the **COSMIC desktop**. Currently supports glass shader
effects, Penumbra, theme-aware panel colors, and separate lock screen wallpapers
and a media player widget.
The sidebar groups Shaders, Panel & Dock and Lock screen.

Adjust effects, import JSON + `.frag` files and manage the glass compositor from
Settings. Lock screen mods use a separate, optional locker engine. Independent repositories maintain the
[shaders](https://github.com/motixo/cosmic-forge-shaders) and
[engine](https://github.com/motixo/cosmic-forge-engine).
The [mod catalog](https://github.com/motixo/cosmic-forge-mods) updates the list,
controls and presets without rebuilding Forge. The header update button refreshes
effects and mods and checks for engine updates. Background checks run every six
hours while the app is open.

## Build and install

Requires Linux, Rust 1.93+, Python 3.11+ and the native COSMIC build libraries.

```sh
git clone --recurse-submodules https://github.com/motixo/cosmic-forge.git
cd cosmic-forge
cargo build --release --locked
python scripts/install.py
cosmic-forge
```

Supports Arch Linux, Fedora, Ubuntu and Pop!_OS with tested COSMIC releases from
1.10 onward (currently 1.10/1.11). In Settings, **Install compositor** downloads a
verified prebuilt binary. Prebuilt
compositor/panel binaries support x86_64/ARM64 with glibc 2.35+; lock screen binaries
require glibc 2.39+ (Ubuntu/Pop!_OS 24.04+, current Arch/Fedora). Settings provides
Install and Restore for each engine; system Settings and login greeter are preserved.
Sign out and back in after engine installation.

[Adding effects](docs/adding-effects.md) · GPL-3.0-only
