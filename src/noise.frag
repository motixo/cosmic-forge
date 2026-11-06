// SPDX-License-Identifier: GPL-3.0-only
#undef main
uniform float forge_surface_noise;

void main() {
    forge_glass_original_main();
    if (forge_surface_noise > 0.0) {
        // Static, monochrome grain at physical pixel resolution; no extra texture fetch.
        vec2 pixel = floor(gl_FragCoord.xy);
        vec3 h = fract(vec3(pixel.xyx) * 0.1031);
        h += dot(h, h.yzx + 33.33);
        float grain = fract((h.x + h.y) * h.z) - 0.5;
        float strength = clamp(forge_surface_noise / 100.0, 0.0, 1.0) * 0.18;
        float coverage = gl_FragColor.a;
        gl_FragColor.rgb = clamp(gl_FragColor.rgb + vec3(grain * strength * coverage),
            vec3(0.0), vec3(coverage));
    }
}
