#!/usr/bin/env python3
"""Exercise the running nested compositor through real Wayland surfaces."""
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
RUN = Path(os.environ.get('CRYSTAL_QA_DIR', ROOT / 'build/preview'))
STATE = json.loads((RUN / 'state.json').read_text())
CONFIG = RUN / 'config/cosmic/com.system76.CosmicComp/v1'
results = []


def setting(key, value):
    (CONFIG / key).write_text(str(value).lower() + '\n')
    time.sleep(0.6)


def capture(name):
    image = ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')
    image.save(ASSETS / (name + '.png'))
    return image


def difference(a, b, box):
    return sum(ImageStat.Stat(ImageChops.difference(a.crop(box), b.crop(box))).mean) / 3


def strong_edge_pixels(image, box):
    image = image.crop(box).convert('L')
    horizontal = ImageChops.difference(image, ImageChops.offset(image, 1, 0))
    vertical = ImageChops.difference(image, ImageChops.offset(image, 0, 1))
    contrast = ImageChops.lighter(horizontal, vertical)
    histogram = contrast.crop((1, 1, image.width - 1, image.height - 1)).histogram()
    return sum(histogram[6:])


def check(name, condition):
    results.append({'test': name, 'passed': bool(condition)})
    if not condition:
        raise AssertionError(name)


setting('crystal_enabled', True)
setting('crystal_everyday', False)
setting('crystal_thickness', 0)
setting('crystal_blur_amount', 0)
setting('crystal_refraction', 150)
setting('crystal_density', 100)
clear = capture('crystal-clear')
# Locate the opaque title bar by its exact RGB value, independent of placement.
coords = [(x, y) for y in range(clear.height) for x in range(clear.width)
          if clear.getpixel((x, y)) == (11, 14, 20)]
if not coords:
    raise RuntimeError('Test surface title bar not found')
left = min(x for x, _ in coords)
top = min(y for _, y in coords)
body = (left + 170, top + 220, left + 700, top + 420)
header = (left + 20, top + 8, left + 800, top + 80)
text = (left + 45, top + 500, left + 120, top + 505)
setting('crystal_blur_amount', 100)
blurred = capture('crystal-blurred')
check('Crystal blur changes the background live', difference(clear, blurred, body) > 3)
check('Opaque header stays pixel-identical', difference(clear, blurred, header) == 0)
check('Opaque foreground control stays pixel-identical', difference(clear, blurred, text) == 0)
check('Maximum blur leaves no clear patches in the calibration area',
      all(difference(clear, blurred, (x, y, x + 16, y + 16)) > 0.6
          for y in range(top + 218, top + 418, 16)
          for x in range(left + 170, left + 700, 16)))
reference_dir = os.environ.get('CRYSTAL_REFERENCE_DIR')
if reference_dir:
    reference_dir = Path(reference_dir)
    check('Clear natural crystal preserves more backdrop detail than maximum blur',
          strong_edge_pixels(clear, body) > strong_edge_pixels(blurred, body) * 2)
    frosted_reference = Image.open(reference_dir / 'blurred.png')
    reference_points = [(x, y) for y in range(frosted_reference.height)
                        for x in range(frosted_reference.width)
                        if frosted_reference.getpixel((x, y))[:3] == (11, 14, 20)]
    reference_left = min(x for x, _ in reference_points)
    reference_top = min(y for _, y in reference_points)
    reference_body = (reference_left + 170, reference_top + 220,
                      reference_left + 700, reference_top + 420)
    check('Maximum blur retains the fully frosted reference background',
          difference(blurred.crop(body), frosted_reference.crop(reference_body),
                     (0, 0, body[2] - body[0], body[3] - body[1])) < 10.0)
    check('Selected crystal cut edges remain visible at maximum blur',
          strong_edge_pixels(blurred, body) > strong_edge_pixels(frosted_reference, reference_body) * 2)
    check('Maximum blur keeps edge glints sparse instead of drawing a bright mesh',
          strong_edge_pixels(blurred, body) < (body[2] - body[0]) * (body[3] - body[1]) * 0.015)
setting('crystal_blur_amount', 50)
uneven = capture('crystal-uneven')
patch_errors = [difference(clear, uneven, (x, y, x + 16, y + 16))
                for y in range(top + 218, top + 418, 16)
                for x in range(left + 170, left + 700, 16)]
check('Intermediate blur retains many clear patches',
      sum(error < 0.6 for error in patch_errors) >= 20)
check('Intermediate blur also has strongly blurred patches',
      sum(error > 6.0 for error in patch_errors) >= 20)
check('The polish pattern stays fixed between frames',
      difference(uneven, capture('crystal-polish-stable'), body) < 0.05)
setting('crystal_blur_amount', 51)
check('Adjacent percentages produce different rendering',
      difference(uneven, capture('crystal-blur-51'), body) > 0.02)
setting('crystal_blur_amount', 100)
setting('crystal_enabled', False)
frosted = capture('frosted')
check('Switching back to Frosted Glass redraws immediately', difference(frosted, blurred, body) > 3)
setting('crystal_enabled', True)
setting('crystal_blur_amount', 0)
restored = capture('crystal-restored')
check('Crystal switching is reversible and deterministic', difference(clear, restored, body) < 0.05)
setting('crystal_blur_amount', 255)
clamped = capture('crystal-clamped')
check('Out-of-range blur is bounded at maximum', difference(blurred, clamped, body) < 0.05)
setting('crystal_blur_amount', 0)
setting('crystal_blur', 13)
check('Percentage blur takes precedence over the legacy level',
      difference(clear, capture('crystal-blur-percentage-priority'), body) < 0.05)
(CONFIG / 'crystal_blur_amount').unlink()
time.sleep(0.6)
check('Legacy maximum blur remains fully frosted',
      difference(blurred, capture('crystal-blur-legacy-maximum'), body) < 0.05)
setting('crystal_blur', 7)
legacy = capture('crystal-blur-legacy-seven')
setting('crystal_blur_amount', 54)
check('Legacy blur is converted to the equivalent percentage',
      difference(legacy, capture('crystal-blur-legacy-converted'), body) < 0.05)
setting('crystal_blur_amount', 0)
setting('crystal_refraction', 0)
no_refraction = capture('crystal-refraction-zero')
setting('crystal_refraction', 250)
strong_refraction = capture('crystal-refraction-maximum')
check('Refraction strength updates the live background', difference(no_refraction, strong_refraction, body) > 3)
check('Refraction leaves foreground controls unchanged', difference(no_refraction, strong_refraction, text) == 0)
setting('crystal_refraction', 65535)
check('Refraction strength is bounded at 250 percent', difference(strong_refraction, capture('crystal-refraction-clamped'), body) < 0.05)
setting('crystal_refraction', 100)
old_strength = capture('crystal-refraction-original')
setting('crystal_refraction', 150)
check('Default refraction is stronger than the original build', difference(old_strength, capture('crystal-refraction-default'), body) > 0.4)
setting('crystal_density', 25)
sparse = capture('crystal-density-sparse')
setting('crystal_density', 250)
dense = capture('crystal-density-dense')
check('Crystal density changes faces without resizing the window', difference(sparse, dense, body) > 3)
check('Density leaves the opaque header unchanged', difference(sparse, dense, header) == 0)
setting('crystal_density', 65535)
check('Density is bounded at 250 percent', difference(dense, capture('crystal-density-upper-clamped'), body) < 0.05)
setting('crystal_density', 0)
check('Density is bounded at 25 percent', difference(sparse, capture('crystal-density-lower-clamped'), body) < 0.05)
setting('crystal_enabled', False)
stock_before = capture('frosted-before-controls')
setting('crystal_density', 250)
setting('crystal_refraction', 0)
check('Crystal controls do not change Frosted Glass', difference(stock_before, capture('frosted-after-controls'), body) < 0.05)
setting('crystal_enabled', True)
setting('crystal_refraction', 150)
setting('crystal_density', 100)
check('Restoring strength and density restores the same rendering', difference(clear, capture('crystal-controls-restored'), body) < 0.05)
process = subprocess.Popen([str(ROOT / 'build/bin/test-surface'), '--opaque'], env=STATE['env'],
    stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
try:
    time.sleep(1)
    a = capture('opaque-crystal')
    setting('crystal_enabled', False)
    b = capture('opaque-frosted')
    # Ignore the cursor and output shadow; the entire opaque interior is stable.
    check('Opaque window contents stay unchanged', difference(a, b, (left + 120, top + 120, left + 700, top + 480)) == 0)
finally:
    os.killpg(process.pid, signal.SIGTERM)
    process.wait(timeout=5)
setting('crystal_enabled', True)
setting('crystal_blur_amount', 0)
(REPORTS / 'render-results.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps(results, indent=2))
