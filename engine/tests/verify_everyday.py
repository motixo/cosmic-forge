#!/usr/bin/env python3
"""Verify Everyday mode and text contrast on real compositor output."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from PIL import Image, ImageChops, ImageGrab, ImageStat

ROOT = Path(__file__).resolve().parents[1]
ASSETS = Path(os.environ.get('CRYSTAL_QA_ASSETS_DIR', ROOT / 'build/assets'))
REPORTS = Path(os.environ.get('CRYSTAL_QA_REPORT_DIR', ROOT / 'build/logs'))
ASSETS.mkdir(parents=True, exist_ok=True)
REPORTS.mkdir(parents=True, exist_ok=True)
RUN = Path(os.environ['CRYSTAL_QA_DIR'])
STATE = json.loads((RUN / 'state.json').read_text())
CONFIG = RUN / 'config/cosmic/com.system76.CosmicComp/v1'
results = []
measurements = []


def setting(key, value):
    (CONFIG / key).write_text(str(value).lower() + '\n')
    time.sleep(0.6)


def grab(name):
    image = ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')
    image.save(ASSETS / (name + '.png'))
    return image


def difference(a, b, box):
    return sum(ImageStat.Stat(ImageChops.difference(a.crop(box), b.crop(box))).mean) / 3


def origin(image):
    points = [(x, y) for y in range(image.height) for x in range(image.width)
              if image.getpixel((x, y)) == (11, 14, 20)]
    assert points, 'Reading surface not found'
    return min(x for x, _ in points), min(y for _, y in points)


def check(name, condition):
    results.append({'test': name, 'passed': bool(condition)})
    if not condition:
        raise AssertionError(name)


def luminance(rgb):
    channels = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in rgb]
    return sum(a * b for a, b in zip(channels, (0.2126, 0.7152, 0.0722)))


setting('crystal_enabled', True)
setting('crystal_everyday', True)
setting('crystal_thickness', 40)
setting('crystal_blur_amount', 0)
setting('crystal_density', 100)
setting('crystal_refraction', 0)
zero = grab('everyday-zero-refraction')
left, top = origin(zero)
center = (left + 180, top + 220, left + 700, top + 420)
edge = (left + 8, top + 220, left + 65, top + 420)
header = (left + 20, top + 8, left + 800, top + 80)
setting('crystal_refraction', 250)
strong = grab('everyday-edge-refraction')
check('Everyday refraction leaves the broad reading center undistorted', difference(zero, strong, center) < 0.05)
check('Everyday mode retains visible refraction near the perimeter', difference(zero, strong, edge) > 0.5)
check('Everyday refraction does not alter opaque foreground content', difference(zero, strong, header) == 0)
setting('crystal_blur_amount', 70)
everyday = grab('everyday-balanced')
setting('crystal_everyday', False)
expressive = grab('everyday-expressive')
check('Everyday mode changes the background live', difference(everyday, expressive, center) > 3)
setting('crystal_everyday', True)
check('Everyday mode switches back deterministically', difference(everyday, grab('everyday-restored'), center) < 0.05)
setting('crystal_blur_amount', 100)
matte = grab('everyday-fully-frosted')
setting('crystal_blur_amount', 255)
check('Everyday blur remains bounded at 100 percent', difference(matte, grab('everyday-blur-clamped'), center) < 0.05)
setting('crystal_enabled', False)
stock = grab('everyday-stock')
setting('crystal_everyday', False)
check('Everyday mode does not alter stock Frosted Glass', difference(stock, grab('everyday-stock-unchanged'), center) < 0.05)
setting('crystal_enabled', True)
setting('crystal_everyday', True)
setting('crystal_blur_amount', 70)
setting('crystal_refraction', 150)
os.killpg(STATE['surface_pid'], signal.SIGTERM)
time.sleep(0.3)

wallpapers = {}
for name, color in [('bright', (255, 255, 255)), ('dark', (0, 0, 0))]:
    path = RUN / (name + '.png')
    Image.new('RGB', (1280, 900), color).save(path)
    wallpapers[name] = path
busy = Image.new('RGB', (1280, 900))
pixels = busy.load()
for y in range(900):
    for x in range(1280):
        pixels[x, y] = (255, 255, 255) if (x // 8 + y // 8) % 2 else (0, 0, 0)
busy.save(RUN / 'busy.png')
wallpapers['busy'] = RUN / 'busy.png'
mode_path = RUN / 'config/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark'
mode_path.parent.mkdir(parents=True, exist_ok=True)
background = RUN / 'config/cosmic/com.system76.CosmicBackground/v1/all'

for mode in ['dark', 'light']:
    mode_path.write_text(str(mode == 'dark').lower() + '\n')
    time.sleep(1)
    process = subprocess.Popen([str(ROOT / 'build/bin/test-surface'), '--readability-' + mode],
                               env=STATE['env'], stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        time.sleep(0.8)
        for name, path in wallpapers.items():
            background.write_text('(output: "all", source: Path(' + json.dumps(str(path)) +
                '), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)\n')
            time.sleep(1)
            image = grab('reading-' + mode + '-' + name)
            left, top = origin(image)
            # These strips sit between the actual 14 px text lines, sampling
            # the backdrop without counting anti-aliased glyph pixels as it.
            samples = [image.getpixel((x, y)) for row in [252, 284, 316, 348, 380]
                       for y in range(top + row + 5, top + row + 10)
                       for x in range(left + 180, left + 700)]
            foreground = (0.95, 0.97, 1.0) if mode == 'dark' else (0.06, 0.065, 0.08)
            ink = luminance(foreground)
            ratios = [(max(ink, lum) + 0.05) / (min(ink, lum) + 0.05)
                      for lum in (luminance(tuple(v / 255 for v in pixel)) for pixel in samples)]
            minimum = min(ratios)
            measurements.append({'theme': mode, 'wallpaper': name, 'minimum_text_contrast': minimum})
            check(f'{mode.title()} text contrast is at least 4.5:1 over {name} wallpaper', minimum >= 4.5)
    finally:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)

(REPORTS / 'everyday-render-results.json').write_text(json.dumps(results, indent=2) + '\n')
(REPORTS / 'everyday-contrast-results.json').write_text(json.dumps(measurements, indent=2) + '\n')
print(json.dumps({'checks': results, 'contrast': measurements}, indent=2))
