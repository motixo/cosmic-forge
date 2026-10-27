# Glass effect packs

Users can open the settings gear in the header, choose **Import glass effect…**,
and select a local JSON pack, or select a `.frag` shader and its JSON settings
together (Ctrl+click to select both files). The effect appears immediately in the sidebar.
The same operation is available without opening the GUI:

```sh
cosmic-forge --import-pack shaders/examples/prism.json
cosmic-forge --import-pack shaders/examples/prism-files/prism.json shaders/examples/prism-files/prism.frag
```

`shaders/examples/prism.json` is a complete working community example. It adds Prism Glass,
with a live color-shift control, using the Aero glass pipeline. It is not installed
by default. `shaders/examples/prism-files/` contains the same effect as separate JSON and
`.frag` files. The JSON leaves out `shader.source`; Forge copies the selected shader
into its installed pack, so the original files can be moved or deleted after import.
The two selected filenames do not need to match. The metadata can describe a pack
with exactly one effect, or a single effect object with `shader.base`, optional
`shader.uniforms` and an optional `revision` (defaults to 1). A single-effect object
uses its effect id as the installed pack id. Selecting only a `.frag` file, multiple
shaders, or metadata for several effects is rejected without changing installed
effects.

Community packs cannot replace the three built-in effects or the official
pack. They can use any other unique effect id. Older revisions are rejected. Explicit
local imports may replace the same revision, allowing the user to edit their shader
or reimport a previously removed effect. Automatic GitHub updates must increase the
revision whenever the contents change. Effect ids must remain in subsequent updates.

Pack files live at `$XDG_DATA_HOME/cosmic-forge/packs/<pack-id>.json`. Updates replace
one file atomically and retain `<pack-id>.previous`. Other packs, user preferences,
original preference backups are preserved. Failed validation,
conflicting ids, oversized files, incompatible formats and decreasing revisions
leave the working pack unchanged. Reimporting an identical revision is harmless.

## Removing an imported effect

Select the imported effect and press **Remove effect** below its controls. This
button is available only for effects installed from local files. Built-in effects
and the official GitHub catalog are protected. If the removed effect is active,
Forge switches to Frosted Glass and clears its runtime shader before removal.
If removal fails, the previous selection and shader are restored.
Other effects in the same pack stay installed. Removing the final effect removes
the active pack file. The previous contents are saved as `<pack-id>.previous` for
recovery; original preference backups and per-effect control values are retained.
Reimport the original JSON (and `.frag` if separate) to restore a removed effect.

## Creating a pack

A pack contains `format: 1`, a lowercase `id`, a positive integer `revision`, and
an `effects` array. Each effect has the same metadata and controls as
`shaders/effects.json`, plus a `shader` object:

```json
{
  "base": "aero",
  "source": "...complete GLES 2 fragment shader...",
  "uniforms": { "forge_prism": "forge_prism_amount" }
}
```

`base` is `aero` or `crystal`. Start with the matching shader in `shaders/shaders/`.
Keep Smithay's texture, clipping, alpha and debug variants. The shader receives
the existing uniforms from that base pipeline. Aero supplies `aero_color`,
`aero_reflections`, `blur_amount`, `theme_tint`, `theme_accent`, geometry, rounded
corners and the sharp backdrop. Crystal supplies its refraction, facet density,
thickness and everyday-mode uniforms instead. Common sampling and clipping follow
the supplied source. The pipeline captures and blurs the backdrop before running
the custom shader; it does not add arbitrary render passes or CPU plugins.

Additional uniforms must be scalar `float`, start with `forge_`, and map to a
control in the same effect. They receive the raw control value (integer converted
to float, boolean as 0/1). Use a unique `forge_<effect>_<name>` preference key.
Do not reuse another creator's keys unless the setting is intentionally shared.
There are at most 32 controls/uniforms, shader source is at most 64 KiB, and the
whole pack is at most 2 MiB. Controls may reuse the documented Crystal/Aero keys.
Other compositor preferences are excluded. Opacity uses the existing theme
`alpha_map` control and stays shared; surface switches remain in Forge settings.

For readable source, keep metadata and `.frag` files separately. For example,
create `effects/prism.json` with one effect object and its `shader.base` and
`shader.uniforms`, then put its source in `shaders/shaders/prism.frag`. The publisher
script combines these into a self-contained pack; no application rebuild is needed.
Community authors can use their own catalog and shader directory:

```sh
python shaders/scripts/package_effects.py --catalog /path/prism-definition.json \
  --shader-dir /path/shaders --pack-id my-glass --revision 1 --output my-glass.json
cosmic-forge --import-pack my-glass.json
```

On the renderer, shader source and parameter values arrive in one versioned RON
config value. Each graphics context compiles a source once and caches it; value
changes redraw without recompiling. Compile failures fall back to the selected
built-in glass pipeline and log `Forge shader rejected`. Corrected source recovers
without restarting. Packs are GPU code: only import shaders from creators you trust.
Forge displays descriptions and controls; it has no sample image or embedded shader preview.

## Publishing updates on GitHub

Forge checks the public **motixo/cosmic-forge-shaders** latest stable release on startup
when its last successful check is older than six hours, and every six hours while
open. A manual check is available in the gear page and via `--update-effects`.
Network work runs on a background thread. Offline failures retain all installed
effects and do not prevent the app from opening. Checks require `curl` with HTTPS.
The app itself and compositor binaries are not automatically replaced.

Generate the official asset, including any definitions in `shaders/effects/*.json`:

```sh
python shaders/scripts/package_effects.py --revision 1
```

Publish `shaders/build/effects/cosmic-forge-glass.json` as an asset named exactly
`cosmic-forge-glass.json` on a stable release in that repository. Increase revision
for every update. GitHub's release asset SHA-256 digest, download size and repository
URL are verified before installing. The official pack id is `cosmic-glass`; it can
update Crystal/Aero source and add glass effects while leaving community packs alone.
A missing repository, a repository with no stable release, or a release without
that asset is reported as “No glass effect pack published yet”. The separate shader repository is the former `cosmic-crystal`, renamed with its
history and legacy v0.8.0 release preserved. Its main branch contains only effect
data, GLSL, examples and pack tooling; compositor setup remains in Forge.
The app connects it as the `shaders/` Git submodule, pinned to a tested commit.
Run `git submodule update --init --recursive` after cloning the app. Installed apps
fetch release assets without Git. A `glass-v<N>` tag in the shader repository
runs its checks and publishes revision N independently of app releases.

## Engine requirement

Existing installed Crystal/Aero binaries render only the built-in effects. New
source and custom parameters require the Forge renderer ABI 1 in `engine/` once.
On Arch Linux and derivatives with matching COSMIC 1.10.0 or 1.11.0 packages, open
the gear page and choose **Install / update compositor**. Forge carries the
pinned patches and builder inside its executable; the source checkout does not
need to stay at a particular path. The first build downloads pinned upstream
source and Cargo dependencies and requires Python 3, Rust, Git and the compositor
build libraries. Build progress is logged to
`$XDG_STATE_HOME/cosmic-forge/engine-build.log`. The operation runs in the background
and never stops the current session. A native progress bar shows the current stage
(checking, preparing source, compiling, installing or restoring). Download/compile
phases use an animated bar because their duration is not known; completion fills
the bar. Before downloading or compiling, Forge checks the renderer revision,
exact COSMIC package versions, launcher ownership and binary checksum. A matching
installation finishes immediately without rebuilding or rewriting its files.

It installs a user-level, package-guarded
compositor launcher, keeping the old launcher available for recovery. The Settings
launcher and existing Crystal installation record remain intact.

**Restore original COSMIC compositor** removes the verified Forge or legacy Crystal
launcher so the next login uses the original `/usr/bin/cosmic-comp` supplied by
the system package. It never reactivates an older patched compositor. The stock
binary must exist and be executable; it is already installed on supported systems
and is not downloaded or rebuilt. Shader packs, preferences, old binaries, the
previous-launcher backup and COSMIC Settings are preserved. The action also works
after an earlier Forge version restored a recorded Crystal launcher.
A changed/unowned launcher, unsupported system or failed build stops setup without
replacing desktop files. The system compositor
under `/usr/bin` is never replaced. `COSMIC_FORGE_DISABLE=1` bypasses the Forge
renderer and uses the previous compositor. Set `COSMIC_FORGE_ENGINE_TARGET` to an
existing compatible Cargo target cache to reuse build artifacts.

After that engine is installed and the session restarted, importing or updating
compatible glass packs needs neither rebuilding nor signing out. Forge reports
when the current session has an older engine. See `engine/README.md` for building
and isolated testing. The app installation helper never changes desktop launchers; the renderer is
installed only when the user invokes its separate action in Forge settings.

Legacy control-only JSON files in `$XDG_DATA_HOME/cosmic-forge/effects/` continue to
load for already implemented compositor features. New shader authors should use
packs. XDG data/config/state locations are respected throughout.
