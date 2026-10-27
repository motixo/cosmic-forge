# COSMIC Forge

A native mod manager for the **COSMIC desktop**. Currently supports glass shader
effects: Frosted Glass, Crystal, Aero and community effects.
The sidebar groups Shaders, Panel and Lock screen; the latter two are ready for
their first mod integrations.

Adjust effects, import JSON + `.frag` files and manage the glass compositor from
Settings. Independent repositories maintain the
[shaders](https://github.com/motixo/cosmic-forge-shaders) and
[compositor engine](https://github.com/motixo/cosmic-forge-engine).

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
verified prebuilt binary; **Build compositor** builds pinned source. Prebuilt
binaries support x86_64/ARM64 with glibc 2.35+. Sign out and back in to activate.

[Adding effects](docs/adding-effects.md) · GPL-3.0-only
