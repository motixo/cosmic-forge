## Changes since v0.12.0

This patch makes engine setup failures easier to diagnose and includes the
login PATH fix from [Engine 0.6.1](https://github.com/motixo/cosmic-forge-engine/releases/tag/engine-v0.6.1).

### Fixed

- **Check the compositor actually serving this desktop.** An installed Forge
  binary or a marker from another compositor no longer implies that shaders
  are active in the current Wayland session.
- **Replace the misleading ABI warning with actionable messages.** Distinguish
  a missing or damaged installation, another running compositor, an older Forge
  build, a missing runtime marker and an unverifiable desktop session.
- Show the session state in **Settings → Patches**, **About** and the effect
  footer. It refreshes while the app is open, including after engine actions.
- Extend **`cosmic-forge --status`** with the installed engine release, session
  state, running compositor PID and executable path, and runtime marker PID.
- Include the engine's managed login-profile cleanup in offline **Restore**.
  Removing Forge's PATH block preserves user edits outside it and requires no
  source checkout or network connection.
- Use **Log out** consistently in the app and setup instructions.

### Engine and shader compatibility

- Source builds now pin **engine-v0.6.1**. Its installer checks the account's
  login shell with a clean PATH and repairs existing installations that would
  otherwise keep launching the system compositor.
- The engine remains at **renderer revision 7 / shader ABI 1**, supporting pinned
  COSMIC **1.10.0 and 1.11.0**. The official shader pack remains **glass-v9**.
- To apply the PATH fix to an existing installation, run the engine installer
  again through **Settings → Patches**, then **log out and back in**.
- Updating the app does not install or restart the compositor. Existing effect
  preferences, imported packs and recovery backups are preserved.

### Install or update

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Ubuntu/Pop!_OS 24.04+: `.deb`. Fedora 43+: `.rpm`. Arch Linux: `.pkg.tar.zst`.
Ubuntu/Pop and Fedora support x86_64 and ARM64; Arch packages support x86_64.
Portable archives require glibc 2.35+ and Python 3.11+ for installation.

`SHA256SUMS` covers the release assets. The source archive includes pinned engine
and shader sources.

### Validation

48 Rust unit and integration tests pass, including session identification and
source-free offline recovery. Formatting, Clippy with warnings denied and the
release build pass. Native UI checks cover all seven diagnostic states, live
status refresh, missing-engine guidance and compositor actions during update
checks. Engine 0.6.1 also passes 45 Python tests across real login shells.
Release CI builds both architectures and installs, opens and removes the native
packages on Ubuntu, Fedora and Arch before publishing.

[Full source changes](https://github.com/motixo/cosmic-forge/compare/v0.12.0...v0.12.1)
