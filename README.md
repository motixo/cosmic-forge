# COSMIC Forge

A native mod manager for the **COSMIC desktop**. Currently supports glass shader
effects: Frosted Glass, Crystal, Aero and community effects.

Adjust effects, import JSON + `.frag` files and manage the glass compositor from
Settings. Shaders are maintained in
[cosmic-forge-shaders](https://github.com/motixo/cosmic-forge-shaders).

## Build and install

Requires Linux, Rust 1.93+, Python 3.11+ and the native COSMIC build libraries.

```sh
git clone --recurse-submodules https://github.com/motixo/cosmic-forge.git
cd cosmic-forge
cargo build --release --locked
python scripts/install.py
cosmic-forge
```

The in-app compositor installer supports Arch Linux with COSMIC 1.10/1.11.
Install or restore the compositor in Settings, then sign out and back in.

[Adding effects](docs/adding-effects.md) · GPL-3.0-only
