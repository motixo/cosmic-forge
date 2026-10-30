## Changes since v0.11.2

### Added

- **One update window for all three repositories.** The header's update button checks COSMIC Forge, the compositor engine and the official shader pack together. Each available update has an expandable changelog. App updates link to their GitHub release; engine updates open Settings → Patches; verified shader updates install automatically.
- **Optional automatic update reminders.** Enable the switch at the top of Settings to check on every app launch and once an hour while Forge stays open. The option is off by default. Manual checks always remain available.
- **Per-effect Animation switch.** Pause or resume animated shaders independently. The choice survives effect switching, restarts and pack updates. Reset controls restores the shader's default.
- **Support for the latest shader inputs.** Forge detects `window_age`, `output_size` and `surface_position` requirements, enabling Snow's per-window accumulation and Void's monitor-aware perspective with a compatible engine.
- Source releases now pin [Forge Engine engine-v0.6.0](https://github.com/motixo/cosmic-forge-engine/releases/tag/engine-v0.6.0) and [official shader pack glass-v9](https://github.com/motixo/cosmic-forge-shaders/releases/tag/glass-v9), including Minecraft Glass, Void, Ember and Snow.

### Fixed

- **About shows actual installed GitHub release tags** for the engine and official shader pack. Renderer revision 7 is no longer presented as an engine release version. Release records are tied to the installed content; unrecognized local builds display “Unknown release” instead of a guessed version.
- Correct vertical alignment of Snow accumulation and other controls without descriptions.
- Move Updates to the top of Settings. The update window's engine action scrolls directly to Patches.
- Keep completed update results inside the update window, removing duplicate status lines from the footer.

### Compatibility

- Use **engine-v0.6.0** for all current shaders. `window_age` requires renderer revision 6; monitor geometry inputs require revision 7. Shader ABI 1 remains unchanged.
- Updating Forge does not replace or restart the desktop compositor. Install engine updates explicitly through **Settings → Patches**, then sign out and back in to activate them.
- Existing effect preferences, per-effect overrides, imported community effects and recovery backups are preserved. An official pack with locally modified contents must use a new revision; the updater continues to reject conflicting packs at the same revision.
- Ember retains its upstream attribution and CC BY-NC-SA 3.0 license; see the shader repository's [credits](https://github.com/motixo/cosmic-forge-shaders/blob/glass-v9/CREDITS.md).

### Install or update

Install or update for the current user:

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Ubuntu/Pop!_OS 24.04+: `.deb`. Fedora 43+: `.rpm`. Arch Linux: `.pkg.tar.zst`.
Ubuntu/Pop and Fedora support x86_64 and ARM64; Arch packages support x86_64.
Portable archives require glibc 2.35+ and Python 3.11+ for installation.

`SHA256SUMS` covers the release assets. The source archive includes pinned engine and shader sources.

### Validation

Rust unit and integration tests, formatting, Clippy with warnings denied, the release build and Python installer/release tests are checked before publication. Native UI checks cover the three-repository update window, reminder persistence, release links, collapsible changelogs, small windows, About release tags, Snow alignment and compositor actions during update checks. Release CI builds both architectures and installs, opens and removes the native packages on Ubuntu, Fedora and Arch before publishing.

[Full source changes](https://github.com/motixo/cosmic-forge/compare/v0.11.2...v0.12.0)
