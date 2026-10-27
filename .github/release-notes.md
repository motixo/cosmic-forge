Native COSMIC mod manager with glass effects, Penumbra, panel and dock colors, lock screen wallpapers and media controls.

Install or update for the current user:

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Ubuntu/Pop!_OS 24.04+: `.deb`. Fedora 43+: `.rpm`. Arch Linux: `.pkg.tar.zst`.
Ubuntu/Pop and Fedora builds support x86_64 and ARM64; Arch packages support x86_64.
Portable archives require glibc 2.35+ and Python 3.11+ for installation.

Application installation preserves preferences and installs only Forge. Install optional compositor, panel and lock screen engines from Settings. Sign out and back in after engine installation.

`SHA256SUMS` covers the release assets. The source archive includes pinned engine, shader and mod sources and excludes agent files and skills.
