# Glass effect packs

Users can open the settings gear in the header, choose **Import effect**,
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

`base` is `glass`, `aero` or `crystal`. Generic `glass` requires Forge 0.5+
and renderer revision 2 or newer. Start with the matching shader in `shaders/shaders/`.
Keep Smithay's texture, clipping, alpha and debug variants. The shader receives
the existing uniforms from that base pipeline. Aero supplies `aero_color`,
`aero_reflections`, `blur_amount`, `theme_tint`, `theme_accent`, geometry, rounded
corners and the sharp backdrop. Crystal supplies its refraction, facet density,
thickness and everyday-mode uniforms instead. Common sampling and clipping follow
the supplied source. The pipeline captures and blurs the backdrop before running
the custom shader; it does not add arbitrary render passes or CPU plugins.

Additional user-controlled uniforms must be scalar `float`, start with `forge_`, and map to a
control in the same effect. They receive the raw control value (integer converted
to float, boolean as 0/1). Use a unique `forge_<effect>_<name>` preference key.
Do not reuse another creator's keys unless the setting is intentionally shared.
There are at most 32 controls/uniforms, shader source is at most 64 KiB, and the
whole pack is at most 2 MiB. Controls may reuse the documented Crystal/Aero keys.
Other compositor preferences are excluded. Opacity uses the existing theme
`alpha_map` control and stays shared; surface switches are managed by COSMIC Settings.

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

## Generic glass base

Use `shader.base: "glass"` for a neutral pipeline with no Aero reflections,
Crystal facets, tint or refraction built in. `shaders/shaders/glass.frag` is the
editable template. The importable pair in `shaders/examples/plain-glass/` provides
independent blur plus shared opacity:

```sh
cosmic-forge --import-pack shaders/examples/plain-glass/plain-glass.json \
  shaders/examples/plain-glass/plain-glass.frag
```

The example is optional and is not added to the built-in catalog. Importing it
adds **Plain Glass** to the sidebar. The app prompts for a compositor update if
renderer revision 2 is not installed. Activate the updated compositor at the next
login; subsequent shader edits only need reimporting.

Map the reserved float uniform name `forge_blur` to your own integer control key:

```json
"shader": {
  "base": "glass",
  "uniforms": { "forge_blur": "forge_my_effect_blur" }
}
```

Define `forge_my_effect_blur` in `controls` with integer bounds inside 0–100.
The engine reads this parameter before blurring; its default is 45 if omitted.
Use a different key for each effect to retain independent values. No Aero or
Crystal preferences are changed. Opacity remains the existing shared theme value.
Other custom float uniforms work as before. Animated shaders use the opt-in
contract below. External images and extra render passes are not supplied.

| Shader input | Meaning |
| --- | --- |
| `tex` | Captured background after the selected blur; sharp at blur 0 |
| `sharp_tex` | Original captured background before blur |
| `v_coords` | Texture UV for the captured background |
| `input_to_geo` | Transform `vec3(v_coords, 1)` into surface-local UV |
| `geo_size` | Surface size in logical units |
| `output_size` | Current rendering monitor size in logical units, renderer 7+ |
| `surface_position` | Surface top-left relative to that output, in logical units, renderer 7+ |
| `corner_radius` | Surface corner radii |
| `uv_per_pixel` | UV step per logical pixel of the extended background capture |
| `output_scale` | Output scale factor |
| `blur_amount` | Selected blur normalized to 0–1 |
| `theme_tint`, `theme_accent` | Current surface palette and accent RGB |
| `alpha` | Compositor opacity multiplier |

`output_size` follows the output being rendered, including rotation and fractional
scale. Each part of a window spanning outputs receives that output's dimensions;
it is not the primary monitor or combined desktop size. It is also available for
static shaders without enabling continuous animation. Multiply by `output_scale`
for transformed physical pixel dimensions. Resolution or scale changes update it
on the next render; it is zero when there is no current output mode.
`surface_position + geo_size * 0.5 - output_size * 0.5` gives the window-center
offset on that monitor, including rotated or scaled outputs, without framebuffer
axis assumptions. Both geometry inputs are available when animation is disabled.

Preserve the template's clipping, premultiplied alpha and texture variants. Clamp
custom sampling to the available capture; this base has no automatic refraction
margin beyond the blur capture. An invalid shader falls back to neutral glass at
the selected blur. Aero and Crystal packs continue to use their existing paths.

To produce a self-contained community pack:

```sh
python shaders/scripts/package_effects.py \
  --catalog shaders/examples/plain-glass/plain-glass.json \
  --shader-dir shaders/examples/plain-glass --pack-id plain-glass --revision 1 \
  --output build/plain-glass.json
```

## Animated shaders

Forge 0.11+ and renderer revision 3 add animation to every custom shader base.
Add `"animated": true` inside the effect's `shader` object, alongside `base`,
`source` and `uniforms`. This works with either a self-contained pack or a
separate JSON and `.frag` pair:

```json
"shader": {
  "base": "glass",
  "animated": true,
  "uniforms": { "forge_speed": "forge_my_effect_speed" }
}
```

Declare only the engine inputs your fragment shader uses:

```glsl
uniform float time;
uniform float window_age;
uniform float delta_time;
uniform int frame_index;
uniform vec2 window_position;
uniform vec2 window_velocity;
uniform float forge_speed;
// In main(), after sampling the backdrop and before clipping:
float phase = time * forge_speed / 100.0;
color.rgb *= 0.85 + 0.15 * sin(phase);
```

`time` is monotonic seconds since the compositor's first output frame, shared
across windows and outputs. `delta_time` is the interval between this output's
rendered frames, in seconds, capped at 0.25 after a pause. `frame_index` is an
integer counter for that output and wraps on overflow. All surfaces on the same
output receive the same values during a frame. These inputs are provided by the
engine; do not map them in `shader.uniforms` or define controls for them.

Renderer revision 6 also supplies `window_age`: seconds since this custom effect
first rendered on the individual glass surface. A newly opened window starts at
zero even while older windows keep their age. The clock stays with the surface
across resizing, movement, hidden intervals and output changes. Applying another
custom effect starts a new clock; changing its controls or source does not.
Like `time`, its shader value is zero when animation is disabled. The stored clock
continues running while animation is off. Declare it only for effects requiring
the newer renderer; existing shaders retain their current requirements.

`window_position` is the top-left of the effect surface in logical desktop
coordinates (X right, Y down), including the output's desktop offset. It does
not include the extra backdrop capture margin. `window_velocity` is its motion
in logical units per second, sampled once per output frame. Velocity starts at
zero, returns to zero at rest and resets after a gap longer than 0.25 seconds
or when a surface first appears on an output. Each surface and output keeps its
own motion history; closed surfaces are removed from that history. These inputs
also apply to popups and compositor-owned transparent surfaces.

For position-driven parallax, use `window_position / geo_size` to shift a
procedural pattern's vanishing point. For a response that settles when dragging
stops, use a bounded value such as
`window_velocity / (abs(window_velocity) + vec2(800.0))`.

Animated surfaces request frames at the output's refresh rate while visible.
Fully covered, minimized or inactive-workspace surfaces do not keep an output
rendering. Turning off animation or selecting an invalid shader stops continuous
redraw; invalid GLSL uses the existing static fallback. Time continues while
hidden, so a restored surface resumes at the current phase. Shader source stays
compiled while time and parameters change.

Omitting `animated`, or setting it to `false`, keeps the existing static behavior;
the five animation and motion inputs are zero. Existing static packs remain
compatible with older engines. New animation formulas only require editing and reimporting the
shader. The existing single-pass pipeline supplies time, geometry, parameters
and captured backgrounds; previous-frame buffers, external textures and CPU code
are separate capabilities.

## Backdrop sources and window state

Forge 0.11.2+ and renderer revision 5 support background selection on every custom base:
`glass`, `aero` and `crystal`. They work with static and animated shaders.

```json
"shader": {
  "base": "glass",
  "backdrop": "wallpaper",
  "uniforms": { "forge_blur": "forge_my_effect_blur" }
}
```

`shader.backdrop` accepts only `"scene"` and `"wallpaper"`. Omitting it means
`"scene"`, which preserves existing packs. `scene` samples the composited content
beneath the surface. `wallpaper` samples only the output's COSMIC wallpaper layer,
excluding windows, panels and other desktop content. In both modes, `tex` is the
blurred source and `sharp_tex` is its original, including at maximum blur.

Wallpaper sampling follows the window's position and each output's wallpaper
layout, scale and transform. The engine uses the actual `wallpaper` background
layer rendered by COSMIC, including image transitions and wallpaper settings.
It caches the full output image and its blur; moving windows changes sample
coordinates, without rerasterizing or reblurring an unchanged wallpaper. Each
output and graphics context has its own cache. A missing wallpaper layer produces
a black backdrop, rather than falling back to windows or panels.

Forge's **Backdrop source → Behind window / Wallpaper only** picker overrides
the pack default for that effect. Choices survive app restarts, effect switching
and pack updates. **Reset controls** removes the override and restores the pack
default. The built-in presets have the same picker. It is hidden without a compatible
Forge compositor. Wallpaper choices require renderer revision 5, which fixes
wallpaper rendering on built-in Frosted and Aero. The state inputs below require
revision 4. Older static scene packs retain their previous renderer requirements.

The following inputs come directly from the engine. Do not add them to
`shader.uniforms` or create pack controls for them:

```glsl
uniform float window_active;
uniform float is_dark;
```

`window_active` is `1.0` for an activated window and `0.0` for an inactive one.
Subsurfaces, popups and transient children follow their owner. Ownerless surfaces,
including panels, receive `1.0`.
`is_dark` follows the system theme mode: `1.0` for dark and `0.0` for light.
It is read from the theme mode setting and is independent of palette brightness.

Focus and theme changes request redraws and update uniforms without recompiling
the shader. They also work without `animated: true`. The engine applies no
inactive-window styling: the shader decides what these values look like.
For example, after sampling the backdrop and before applying the clip mask:

```glsl
float transmission = mix(0.7, 1.0, window_active);
vec3 inactive_tint = mix(vec3(0.9), vec3(0.15), is_dark);
color.rgb = mix(inactive_tint * color.a, color.rgb, transmission);
```

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
data, GLSL, examples and pack tooling; compositor setup lives in the separate
[engine repository](https://github.com/motixo/cosmic-forge-engine).
The app connects it as the `shaders/` Git submodule, pinned to a tested commit.
Run `git submodule update --init --recursive` after cloning the app. Installed apps
fetch release assets without Git. A `glass-v<N>` tag in the shader repository
runs its checks and publishes revision N independently of app releases.

## Engine requirement

Existing installed Crystal/Aero binaries render only the built-in effects. New
source and custom parameters require the Forge renderer ABI 1 in `engine/` once.
On Arch Linux, Fedora, Ubuntu or Pop!_OS with a supported COSMIC release from
1.10 onward, open Settings and choose **Install compositor**. Published pins
currently cover 1.10.0 and 1.11.0; future tested versions are added to the engine
release independently of app releases. Forge downloads the
latest stable `cosmic-forge-engine.json` release from **motixo/cosmic-forge-engine**.
The app checks the repository URL, asset size, GitHub SHA-256 digest, bundle paths
and ABI before using its installer. Engine fixes and compatible renderer revisions
can be published independently of app releases. The source checkout is not needed
at runtime. The Install action downloads an XZ-compressed prebuilt executable matched to
COSMIC and the CPU architecture. It requires Python 3, curl, glibc 2.35+ and the
COSMIC runtime libraries. The asset repository, SHA-256, size, decompression limit,
ELF architecture and embedded revision are checked before installation. A missing
or incompatible binary reports an error without starting a source build.

To compile from source, run `python3 install.py --build` in the engine repository.
This downloads pinned upstream source and Cargo dependencies and requires
Python 3, Rust, Git and the compositor build
libraries. Build caches are shared across engine releases at
`$XDG_CACHE_HOME/cosmic-forge/engine-target/<version>`. The app never silently
starts a source build from the Install action. Both methods recheck the captured
COSMIC packages before changing the launcher. Build progress is logged to
`$XDG_STATE_HOME/cosmic-forge/engine-build.log`. The operation runs in the background
and never stops the current session. A native progress bar shows the current stage
(downloading the engine package or binary, checking, installing or restoring). Download
phases use an animated bar because their duration is not known; completion fills
the bar. After downloading the small engine package and before downloading a
compositor, fetching upstream source or compiling, Forge checks the renderer revision,
exact COSMIC package versions, launcher ownership and binary checksum.
Package queries and launcher guards use pacman, rpm or dpkg-query as appropriate.
Pop snapshot packages can be matched by their pinned compositor commit. A matching
installation finishes immediately without rebuilding or rewriting its files.

A missing, incompatible or damaged engine release leaves the installed renderer
unchanged. Restore uses an embedded recovery helper and needs no internet. Both
engine and shader repositories are pinned as Git submodules for development.

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
