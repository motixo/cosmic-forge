<p align="center"><img src="resources/icons/io.github.motixo.CosmicForge.svg" width="96" alt="COSMIC Forge icon"></p>
<h1 align="center">COSMIC Forge</h1>

A native mod manager for the **COSMIC desktop**: glass shaders, Penumbra, panel
and dock colors, lock screen wallpapers and media controls.

## Install

Ubuntu/Pop!_OS 24.04+, Fedora 43+ and current Arch Linux. Requires COSMIC,
Python 3.11+ and curl. Ubuntu/Pop and Fedora support x86_64 and ARM64;
Arch packages support x86_64.

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Installs the latest verified binary in `~/.local`, without sudo or Rust.
Run the same command to update. Open **COSMIC Forge** from your app menu or run
`~/.local/bin/cosmic-forge`.

Native packages are available in [Releases](https://github.com/motixo/cosmic-forge/releases/latest).
Download the package for your CPU:

| System | Package | Install |
| --- | --- | --- |
| Ubuntu / Pop!_OS | `.deb` | `sudo apt install ./cosmic-forge_*.deb` |
| Fedora | `.rpm` | `sudo dnf install ./cosmic-forge-*.rpm` |
| Arch Linux | `.pkg.tar.zst` | `sudo pacman -U ./cosmic-forge-*.pkg.tar.zst` |

To remove a user installation, or switch to a native package:

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh -s -- --uninstall
```

Preferences and engine installations are retained. Application installation adds
only Forge; install optional engines from **Settings → Patches** and sign out and
back in after engine installation. Prebuilt engine pins currently cover COSMIC
1.10/1.11; lock screen binaries require glibc 2.39+.

## Build from source

Requires Rust 1.93+, Python 3.11+ and native COSMIC development libraries.
Release archives require glibc 2.35+.

```sh
git clone --recurse-submodules https://github.com/motixo/cosmic-forge.git
cd cosmic-forge
cargo build --release --locked
python3 scripts/install.py
```

The header update button refreshes [shaders](https://github.com/motixo/cosmic-forge-shaders)
and the [mod catalog](https://github.com/motixo/cosmic-forge-mods), and checks for
[engine](https://github.com/motixo/cosmic-forge-engine) updates independently of app releases.

[Adding effects](docs/adding-effects.md) · GPL-3.0-only
