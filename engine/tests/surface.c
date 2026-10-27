// SPDX-License-Identifier: GPL-3.0-only
// Native Wayland client: the compositor supplies all refraction and blur.
#define _GNU_SOURCE
#include <wayland-client.h>
#include <cairo.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "xdg-shell-client.h"
#include "background-effect-client.h"

static struct wl_display *display;
static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct xdg_wm_base *shell;
static struct ext_background_effect_manager_v1 *effects;
static struct wl_surface *surface;
static struct wl_buffer *buffer;
static int running = 1;
static int width = 820, height = 540;
static int light_text = 1;

static void ping(void *d, struct xdg_wm_base *s, uint32_t serial) { (void)d; xdg_wm_base_pong(s, serial); }
static const struct xdg_wm_base_listener shell_listener = { .ping = ping };
static void capabilities(void *d, struct ext_background_effect_manager_v1 *e, uint32_t f) { (void)d; (void)e; (void)f; }
static const struct ext_background_effect_manager_v1_listener effect_listener = { .capabilities = capabilities };
static void global(void *d, struct wl_registry *r, uint32_t name, const char *interface, uint32_t version) {
    (void)d; (void)version;
    if (!strcmp(interface, "wl_compositor")) compositor = wl_registry_bind(r, name, &wl_compositor_interface, 4);
    else if (!strcmp(interface, "wl_shm")) shm = wl_registry_bind(r, name, &wl_shm_interface, 1);
    else if (!strcmp(interface, "xdg_wm_base")) {
        shell = wl_registry_bind(r, name, &xdg_wm_base_interface, 1);
        xdg_wm_base_add_listener(shell, &shell_listener, NULL);
    } else if (!strcmp(interface, "ext_background_effect_manager_v1")) {
        effects = wl_registry_bind(r, name, &ext_background_effect_manager_v1_interface, 1);
        ext_background_effect_manager_v1_add_listener(effects, &effect_listener, NULL);
    }
}
static void global_remove(void *d, struct wl_registry *r, uint32_t n) { (void)d; (void)r; (void)n; }
static const struct wl_registry_listener registry_listener = { .global = global, .global_remove = global_remove };
static void configure(void *d, struct xdg_surface *s, uint32_t serial) {
    (void)d;
    xdg_surface_ack_configure(s, serial);
    wl_surface_attach(surface, buffer, 0, 0);
    wl_surface_damage_buffer(surface, 0, 0, width, height);
    wl_surface_commit(surface);
}
static const struct xdg_surface_listener surface_listener = { .configure = configure };
static void size_configure(void *d, struct xdg_toplevel *t, int32_t w, int32_t h, struct wl_array *states) {
    (void)d; (void)t; (void)w; (void)h; (void)states;
}
static void close_window(void *d, struct xdg_toplevel *t) { (void)d; (void)t; running = 0; }
static const struct xdg_toplevel_listener toplevel_listener = { .configure = size_configure, .close = close_window };
static void label(cairo_t *cr, double x, double y, double size, const char *text) {
    cairo_select_font_face(cr, "sans-serif", CAIRO_FONT_SLANT_NORMAL, CAIRO_FONT_WEIGHT_NORMAL);
    cairo_set_font_size(cr, size);
    if (light_text) cairo_set_source_rgb(cr, 0.95, 0.97, 1.0);
    else cairo_set_source_rgb(cr, 0.06, 0.065, 0.08);
    cairo_move_to(cr, x, y);
    cairo_show_text(cr, text);
}

int main(int argc, char **argv) {
    int aero = argc > 1 && !strcmp(argv[1], "--aero");
    int opaque = argc > 1 && !strcmp(argv[1], "--opaque");
    int no_effect = argc > 1 && !strcmp(argv[1], "--no-effect");
    int reading_light = argc > 1 && !strcmp(argv[1], "--readability-light");
    int reading = reading_light || (argc > 1 && !strcmp(argv[1], "--readability-dark"));
    display = wl_display_connect(NULL);
    if (!display) return 1;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!compositor || !shm || !shell || !effects) { fprintf(stderr, "Required Wayland globals missing\n"); return 2; }
    int size = width * height * 4;
    int fd = memfd_create("crystal-test-surface", MFD_CLOEXEC);
    if (fd < 0 || ftruncate(fd, size) < 0) return 3;
    void *data = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (data == MAP_FAILED) return 4;
    struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, size);
    buffer = wl_shm_pool_create_buffer(pool, 0, width, height, width * 4, WL_SHM_FORMAT_ARGB8888);
    wl_shm_pool_destroy(pool);
    close(fd);
    cairo_surface_t *canvas = cairo_image_surface_create_for_data(data, CAIRO_FORMAT_ARGB32, width, height, width * 4);
    cairo_t *cr = cairo_create(canvas);
    cairo_set_operator(cr, CAIRO_OPERATOR_SOURCE);
    if (reading_light) cairo_set_source_rgba(cr, 0.95, 0.96, 0.98, 0.16);
    else cairo_set_source_rgba(cr, 0.035, 0.045, 0.07, opaque ? 1.0 : 0.16);
    cairo_paint(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
    cairo_set_source_rgb(cr, 0.045, 0.055, 0.08);
    cairo_rectangle(cr, 0, 0, width, 88);
    cairo_fill(cr);
    label(cr, 30, 38, 24, aero ? "COSMIC Aero" : "COSMIC Crystal");
    label(cr, 30, 65, 13, "COMPOSITOR TEST SURFACE");
    light_text = !reading_light;
    label(cr, 34, 146, 28, aero ? "Glass, colored by your theme." : "Light, cut into crystal.");
    label(cr, 34, 182, 16, aero ? "Smooth blur / diagonal reflections / clear text" : "Faceted refraction / adjustable blur / clear text");
    if (reading) {
        label(cr, 180, 252, 14, "Small text should remain easy to read over glass.");
        label(cr, 180, 284, 14, "0123456789  /usr/share  --flag=value  {} [] ()");
        label(cr, 180, 316, 14, "The quick brown fox jumps over the lazy dog.");
        label(cr, 180, 348, 14, "Crystal reflections belong near the window edges.");
        label(cr, 180, 380, 14, "A calm center leaves room for the actual content.");
    }
    label(cr, 34, 473, 15, "The background is refracted. These controls stay sharp.");
    cairo_set_source_rgb(cr, 1, 1, 1);
    cairo_rectangle(cr, 34, 496, 100, 12);
    cairo_fill(cr);
    cairo_destroy(cr);
    cairo_surface_destroy(canvas);
    surface = wl_compositor_create_surface(compositor);
    struct xdg_surface *xs = xdg_wm_base_get_xdg_surface(shell, surface);
    xdg_surface_add_listener(xs, &surface_listener, NULL);
    struct xdg_toplevel *top = xdg_surface_get_toplevel(xs);
    xdg_toplevel_set_title(top, aero ? "COSMIC Aero test" : "COSMIC Crystal test");
    xdg_toplevel_set_app_id(top, "io.github.motixo.CosmicCrystal.Test");
    xdg_toplevel_set_min_size(top, width, height);
    xdg_toplevel_set_max_size(top, width, height);
    xdg_toplevel_add_listener(top, &toplevel_listener, NULL);
    if (!no_effect) {
        struct ext_background_effect_surface_v1 *fx = ext_background_effect_manager_v1_get_background_effect(effects, surface);
        struct wl_region *region = wl_compositor_create_region(compositor);
        wl_region_add(region, 0, 0, width, height);
        ext_background_effect_surface_v1_set_blur_region(fx, region);
        wl_region_destroy(region);
    }
    if (opaque) {
        struct wl_region *region = wl_compositor_create_region(compositor);
        wl_region_add(region, 0, 0, width, height);
        wl_surface_set_opaque_region(surface, region);
        wl_region_destroy(region);
    }
    wl_surface_commit(surface);
    while (running && wl_display_dispatch(display) != -1) {}
    wl_display_disconnect(display);
    munmap(data, size);
    return 0;
}
