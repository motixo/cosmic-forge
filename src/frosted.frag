// SPDX-License-Identifier: GPL-3.0-only
// Native Frosted bridge, including the strongest COSMIC blur step.
// Geometry clipping follows COSMIC's clipped_surface.frag.
#version 100
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform sampler2D sharp_tex;
uniform float alpha;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform vec2 geo_size;
uniform vec4 corner_radius;
uniform mat3 input_to_geo;
uniform float blur_amount;
uniform vec2 uv_per_pixel;
uniform float output_scale;
uniform float forge_frosted_extra_blur;
uniform vec3 theme_tint;
uniform vec3 theme_accent;

void main() {
    if (alpha <= 0.0) discard;
    vec2 g = (input_to_geo * vec3(v_coords, 1.0)).xy;
    if (g.x < 0.0 || g.y < 0.0 || g.x > 1.0 || g.y > 1.0) discard;
    vec2 p = g * geo_size;
    float radius = g.x < 0.5
        ? (g.y < 0.5 ? corner_radius.x : corner_radius.w)
        : (g.y < 0.5 ? corner_radius.y : corner_radius.z);
    vec2 center = p - geo_size * 0.5;
    vec2 q = abs(center) - geo_size * 0.5 + radius;
    float inside = -(length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius);
    float mask = smoothstep(-0.5, 0.5, inside);
    if (mask <= 0.0) discard;

    vec4 color = texture2D(tex, v_coords);
    if (forge_frosted_extra_blur > 0.0) {
        // The generic renderer stops one blur step below native Frosted's maximum.
        vec2 step_uv = uv_per_pixel * forge_frosted_extra_blur / max(output_scale, 0.1);
        vec2 inset = uv_per_pixel * 0.5;
        color = color * 0.5
            + texture2D(tex, clamp(v_coords + vec2(step_uv.x, 0.0), inset, 1.0 - inset)) * 0.125
            + texture2D(tex, clamp(v_coords - vec2(step_uv.x, 0.0), inset, 1.0 - inset)) * 0.125
            + texture2D(tex, clamp(v_coords + vec2(0.0, step_uv.y), inset, 1.0 - inset)) * 0.125
            + texture2D(tex, clamp(v_coords - vec2(0.0, step_uv.y), inset, 1.0 - inset)) * 0.125;
    }
    #if defined(NO_ALPHA)
    color.a = 1.0;
    #endif
    color *= mask * alpha;
    #if defined(DEBUG_FLAGS)
    if (tint == 1.0) color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
    #endif
    gl_FragColor = color;
}
