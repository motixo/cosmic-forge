#!/usr/bin/env python3
"""Verify Aero on real nested COSMIC output, including live system theme changes."""
import json
import os
from pathlib import Path
import time

from PIL import ImageChops, ImageGrab, ImageStat

ROOT = Path(__file__).resolve().parents[1]
(ROOT / 'build/assets').mkdir(parents=True, exist_ok=True)
(ROOT / 'build/logs').mkdir(parents=True, exist_ok=True)
RUN = Path(os.environ['CRYSTAL_QA_DIR'])
STATE = json.loads((RUN / 'state.json').read_text())
CONFIG = RUN / 'config/cosmic/com.system76.CosmicComp/v1'
RESULTS = []
saved = {}


def write(path, value):
    if path not in saved:
        saved[path] = path.read_bytes() if path.exists() else None
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value + '\n')
    time.sleep(0.65)


def setting(key, value):
    write(CONFIG / key, json.dumps(value))


def grab(name=None):
    image = ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')
    if name:
        image.save(ROOT / 'build/assets' / (name + '.png'))
    return image


def difference(a, b, box):
    return sum(ImageStat.Stat(ImageChops.difference(a.crop(box), b.crop(box))).mean) / 3


def aligned_difference(a, b, relative):
    # Switching system modes may reposition floating windows. Compare opaque
    # foreground pixels in surface coordinates, excluding the accent outline.
    crops = []
    for image in (a, b):
        pixels = image.load()
        points = [(x, y) for y in range(image.height) for x in range(image.width)
                  if pixels[x, y] == (11, 14, 20)]
        left = min(x for x, _ in points)
        top = min(y for _, y in points)
        crops.append(image.crop((left + relative[0], top + relative[1],
                                 left + relative[2], top + relative[3])))
    return sum(ImageStat.Stat(ImageChops.difference(*crops)).mean) / 3


def check(name, condition):
    RESULTS.append({'test': name, 'passed': bool(condition)})
    print(('PASS ' if condition else 'FAIL ') + name, flush=True)
    if not condition:
        raise AssertionError(name)


def accent(color):
    fields = ['base', 'hover', 'pressed', 'selected', 'selected_text', 'focus',
              'divider', 'on', 'disabled', 'on_disabled', 'border', 'disabled_border']
    component = '(' + ', '.join(f'{field}: "{color}"' for field in fields) + ')'
    write(RUN / 'config/cosmic/com.system76.CosmicTheme.Dark/v2/accent', component)


try:
    setting('glass_effect', 'crystal')
    setting('crystal_everyday', False)
    setting('crystal_thickness', 0)
    setting('crystal_blur_amount', 70)
    setting('crystal_refraction', 150)
    setting('crystal_density', 100)
    crystal = grab()
    pixels = crystal.load()
    points = [(x, y) for y in range(crystal.height) for x in range(crystal.width)
              if pixels[x, y] == (11, 14, 20)]
    assert points, 'Opaque Wayland calibration header not found'
    left = min(x for x, _ in points)
    top = min(y for _, y in points)
    body = (left + 170, top + 220, left + 700, top + 420)
    header = (left + 20, top + 8, left + 800, top + 80)
    control = (left + 45, top + 500, left + 120, top + 505)

    setting('aero_blur_amount', 45)
    setting('aero_color', 55)
    setting('aero_reflections', 65)
    check('Aero preferences leave Crystal unchanged', difference(crystal, grab(), body) < 0.05)
    setting('glass_effect', 'aero')
    aero = grab('aero-glass')
    check('Aero selection changes the backdrop live', difference(crystal, aero, body) > 2)
    check('Aero preserves the opaque title and text', difference(crystal, aero, header) == 0)
    check('Aero preserves opaque foreground controls', difference(crystal, aero, control) == 0)
    check('Reflections remain stable between frames', difference(aero, grab(), body) < 0.05)
    setting('crystal_enabled', False)
    check('Named Aero selection takes precedence over the legacy switch', difference(aero, grab(), body) < 0.05)
    setting('aero_reflections', 0)
    no_reflections = grab()
    check('Reflection strength changes the glass live', difference(aero, no_reflections, body) > 0.5)
    setting('aero_reflections', 100)
    full_reflections = grab()
    setting('aero_reflections', 255)
    check('Reflections are bounded at 100 percent', difference(full_reflections, grab(), body) < 0.05)
    setting('aero_reflections', 65)
    setting('aero_color', 0)
    no_color = grab()
    check('Theme color intensity updates live', difference(aero, no_color, body) > 2)
    setting('aero_color', 100)
    full_color = grab()
    setting('aero_color', 255)
    check('Color intensity is bounded at 100 percent', difference(full_color, grab(), body) < 0.05)
    setting('aero_color', 55)
    setting('aero_blur_amount', 46)
    check('Adjacent Aero blur percentages produce different rendering', difference(aero, grab(), body) > 0.01)
    setting('aero_blur_amount', 0)
    clear = grab('aero-clear')
    setting('aero_blur_amount', 100)
    blurred = grab('aero-blurred')
    check('Aero blur spans clear to diffuse transmission', difference(clear, blurred, body) > 2)
    setting('aero_blur_amount', 255)
    check('Aero blur is bounded at 100 percent', difference(blurred, grab(), body) < 0.05)
    setting('aero_blur_amount', 45)
    restored = grab()
    check('Restoring Aero controls restores identical glass', difference(aero, restored, body) < 0.05)
    setting('crystal_blur_amount', 0)
    setting('crystal_thickness', 100)
    setting('crystal_refraction', 0)
    setting('crystal_density', 250)
    setting('crystal_everyday', True)
    check('Crystal controls leave Aero unchanged', difference(restored, grab(), body) < 0.05)

    setting('glass_effect', 'frosted')
    stock = grab()
    setting('aero_reflections', 0)
    setting('aero_color', 0)
    setting('aero_blur_amount', 0)
    check('Aero controls leave stock Frosted Glass unchanged', difference(stock, grab(), body) < 0.05)
    setting('crystal_everyday', False)
    setting('crystal_thickness', 0)
    setting('crystal_blur_amount', 70)
    setting('crystal_refraction', 150)
    setting('crystal_density', 100)
    setting('glass_effect', 'crystal')
    check('Returning to Crystal preserves its exact prior appearance', difference(crystal, grab(), body) < 0.05)

    setting('crystal_enabled', True)
    setting('glass_effect', 'unknown-effect')
    check('Unknown effect names retain the legacy Crystal preference', difference(crystal, grab(), body) < 0.05)
    (CONFIG / 'glass_effect').unlink()
    time.sleep(0.65)
    check('Missing effect names retain the legacy Crystal preference', difference(crystal, grab(), body) < 0.05)
    setting('glass_effect', 'aero')
    setting('aero_reflections', 65)
    setting('aero_color', 55)
    setting('aero_blur_amount', 45)
    mode = RUN / 'config/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark'
    write(mode, 'true')
    accent('#378BE6FF')
    blue = grab('aero-theme-blue')
    accent('#55C982FF')
    green = grab('aero-theme-green')
    accent('#BA70E6FF')
    violet = grab('aero-theme-violet')
    check('Aero follows a changed system accent live', difference(blue, green, body) > 2)
    check('Aero follows another accent without restarting', difference(green, violet, body) > 2)
    check('Theme accent changes preserve foreground text', aligned_difference(blue, violet, (20, 8, 800, 80)) == 0)
    setting('aero_color', 0)
    neutral_a = grab()
    accent('#378BE6FF')
    check('Zero color intensity removes accent tint', difference(neutral_a, grab(), body) < 0.05)
    setting('aero_color', 55)
    dark = grab('aero-theme-dark')
    write(mode, 'false')
    light = grab('aero-theme-light')
    check('Glass tint follows light and dark system modes', difference(dark, light, body) > 3)
    check('Theme mode changes preserve opaque content', aligned_difference(dark, light, (45, 500, 120, 505)) == 0)
finally:
    for path, content in reversed(list(saved.items())):
        if content is None:
            path.unlink(missing_ok=True)
        else:
            path.write_bytes(content)
    (ROOT / 'build/logs/aero-render-results.json').write_text(json.dumps(RESULTS, indent=2) + '\n')

print(f'{len(RESULTS)} Aero render checks passed')
