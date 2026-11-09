# Glass effect packs

## Import an effect

Install the Forge compositor, then log out and back in. [Installation guide](../README.md#install)

Open **Settings → Import effect**. Select one bundled JSON pack, or select one
JSON settings file and one `.frag` file together. Select the imported effect in
Forge's sidebar. To uninstall a local effect, select it and click **Remove effect**.

Try both import formats with Prism Glass. The bundled JSON is ready to import.
The separate files are easier to edit. [Prism pack](https://github.com/motixo/cosmic-forge-shaders/blob/main/examples/prism.json) · [Editable Prism files](https://github.com/motixo/cosmic-forge-shaders/tree/main/examples/prism-files)

The same imports work from the command line. The paths below are relative to the
Forge checkout with its submodules initialized.

```sh
cosmic-forge --import-pack shaders/examples/prism.json
cosmic-forge --import-pack shaders/examples/prism-files/prism.json shaders/examples/prism-files/prism.frag
```

## Creating a pack

1. Copy **Plain Glass** for a neutral starting point, or use the editable Prism
   files to see how a slider drives an Aero shader. [Plain Glass template](https://github.com/motixo/cosmic-forge-shaders/tree/main/examples/plain-glass)
2. Pick unique lowercase IDs for your pack and effect. Give the effect a `name`,
   `tagline`, `description`, `controls` array, and `shader` object. Set `shader.base`
   to `glass`, `aero`, or `crystal`. The neutral `glass` base needs renderer revision 2+.
3. Edit the GLSL ES 1.00 fragment shader. Keep the template's texture variants,
   rounded-corner clipping, and premultiplied alpha output.
4. Reimport the JSON and `.frag` together to try each change. Keep the same IDs
   and control keys to keep the effect and its settings when you reimport.

For a custom control, use a unique `forge_` key and map a `forge_` uniform name
to that key in `shader.uniforms`. Prism maps `forge_prism` to `forge_prism_amount`
and declares `uniform float forge_prism;` in GLSL. Integer controls pass their raw
value, so divide by `100.0` for a 0–100 slider. Boolean controls pass `0.0` or `1.0`.

For `glass`, map `forge_blur` to an integer control in 0–100 to drive the
compositor's blur. Without that mapping, blur defaults to 45. Use `alpha_map`
with `target` set to `theme` for the shared COSMIC opacity control.

A single-file pack contains `format: 1`, a unique `id`, a positive `revision`, and
an `effects` array with GLSL in each `shader.source`. For an editable file pair,
the JSON can instead contain just one effect object. GLSL is limited to 64 KiB per
effect and the pack to 2 MiB. Each effect can have up to 32 controls and 32 mapped
uniforms.

## Shader inputs

Declare engine inputs directly in GLSL. Use `shader.uniforms` for your own controls.

| Input | Meaning |
| --- | --- |
| `tex`, `sharp_tex` | Blurred and original backdrop textures |
| `v_coords`, `input_to_geo` | Backdrop UV and matrix that transforms it to surface-local UV |
| `geo_size`, `corner_radius` | Surface size and corner radii in logical units |
| `uv_per_pixel`, `output_scale` | Capture UV step per logical pixel and output scale factor |
| `blur_amount`, `alpha` | Normalized blur and compositor opacity multiplier |
| `theme_tint`, `theme_accent` | COSMIC surface and accent colors |
| `window_active`, `is_dark` | Focus and dark theme as `0.0` or `1.0` (renderer revision 4+) |
| `output_size`, `surface_position` | Current output size and surface top-left relative to that output, in logical units (revision 7+) |

Set `shader.backdrop` to `wallpaper` to sample only the current output's wallpaper
(renderer revision 5+). The default `scene` samples what's behind the surface.
The app's **Backdrop source** setting can override the pack default.

## Animated shaders

Set `shader.animated` to `true` (renderer revision 3+) to use these inputs and
request redraws while the effect is visible. Leave it off for static shaders.

| Input | Meaning |
| --- | --- |
| `float time` | Compositor clock in seconds |
| `float delta_time`, `int frame_index` | Seconds since the previous output frame and per-output frame counter |
| `vec2 window_position`, `vec2 window_velocity` | Desktop-global position and velocity in logical units and logical units per second |
| `float window_age` | Seconds since the effect first rendered on this surface (revision 6+) |

For smooth speed changes, see how Rain Glass maps `forge_animation_speed_percent`
and reads `forge_animation_clock.x` (Engine 0.6.3+). [Rain Glass source](https://github.com/motixo/cosmic-forge-shaders/blob/main/shaders/rain-glass.frag)

## Test and share

Apply the imported effect to a transparent window and check that the controls
change the render. If the effect is unavailable, `cosmic-forge --status` reports
whether the engine is `active`. If it falls back to built-in glass after a GLSL
compile error, look for `Forge shader rejected` in the compositor log.

To share a single JSON file, use the pack builder. Name each `.frag` after its
matching effect ID when bundling. This example builds the editable Prism pair.

```sh
python shaders/scripts/package_effects.py --catalog shaders/examples/prism-files/prism.json \
  --shader-dir shaders/examples/prism-files --pack-id prism-demo --revision 1 \
  --output build/prism.json
cosmic-forge --import-pack build/prism.json
```

Keep the pack and effect IDs when publishing an update and increase `revision`.
