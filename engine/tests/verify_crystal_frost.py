#!/usr/bin/env python3
"""Verify clean frost, retained optical thickness, and reading contrast."""
import argparse
import json
import os
from pathlib import Path
import time

from PIL import Image, ImageChops, ImageGrab, ImageStat

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference', type=Path)
args = parser.parse_args()
ROOT = Path(__file__).resolve().parents[1]
ASSETS = Path(os.environ.get('CRYSTAL_QA_ASSETS_DIR', ROOT / 'build/assets'))
REPORTS = Path(os.environ.get('CRYSTAL_QA_REPORT_DIR', ROOT / 'build/logs'))
ASSETS.mkdir(parents=True, exist_ok=True)
REPORTS.mkdir(parents=True, exist_ok=True)
RUN = Path(os.environ['CRYSTAL_QA_DIR'])
STATE = json.loads((RUN / 'state.json').read_text())
CONFIG = RUN / 'config/cosmic/com.system76.CosmicComp/v1'
BACKGROUND = RUN / 'config/cosmic/com.system76.CosmicBackground/v1/all'
keys = ['crystal_enabled', 'crystal_everyday', 'crystal_blur_amount', 'crystal_refraction', 'crystal_density', 'crystal_thickness']
saved = {key: (CONFIG / key).read_bytes() if (CONFIG / key).exists() else None for key in keys}
old_background = BACKGROUND.read_bytes()
results = []
measurements = {}


def setting(key, value):
    (CONFIG / key).write_text(str(value).lower() + '\n')
    time.sleep(0.6)


def capture(name):
    image = ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')
    image.save(ASSETS / (name + '.png'))
    return image


def origin(image):
    points = [(x, y) for y in range(image.height) for x in range(image.width)
              if image.getpixel((x, y))[:3] == (11, 14, 20)]
    assert points, 'Surface header missing'
    return min(x for x, _ in points), min(y for _, y in points)


def variance(image, box):
    return sum(ImageStat.Stat(image.crop(box)).stddev) / 3


def difference(a, b, box):
    return sum(ImageStat.Stat(ImageChops.difference(a.crop(box), b.crop(box))).mean) / 3


def check(name, condition):
    results.append({'test': name, 'passed': bool(condition)})
    if not condition:
        raise AssertionError(name)


def wallpaper(name, rgb):
    path = RUN / (name + '.png')
    Image.new('RGB', (1280, 900), rgb).save(path)
    BACKGROUND.write_text('(output: "all", source: Path(' + json.dumps(str(path)) +
        '), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)\n')
    time.sleep(0.8)


def luminance(rgb):
    channels = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in rgb]
    return sum(a * b for a, b in zip(channels, (0.2126, 0.7152, 0.0722)))


def minimum_contrast(image, box):
    foreground = luminance((0.95, 0.97, 1.0))
    pixels = image.crop(box)
    return min((max(foreground, bg) + 0.05) / (min(foreground, bg) + 0.05)
               for bg in (luminance(tuple(v / 255 for v in pixel))
                          for pixel in pixels.getdata()))


try:
    setting('crystal_enabled', True)
    setting('crystal_everyday', True)
    setting('crystal_refraction', 150)
    setting('crystal_density', 100)
    setting('crystal_blur_amount', 100)
    setting('crystal_thickness', 0)
    wallpaper('frost-flat', (128, 136, 148))
    thin = capture('crystal-clean-frost-thin')
    left, top = origin(thin)
    center = (left + 180, top + 220, left + 700, top + 420)
    header = (left + 20, top + 8, left + 800, top + 80)
    control = (left + 45, top + 500, left + 120, top + 505)
    setting('crystal_thickness', 100)
    thick = capture('crystal-clean-frost')
    material_variance = variance(thick, center)
    measurements['clean_center_standard_deviation'] = material_variance
    check('Full frost has a smooth reading center without grain or scratches', material_variance < 0.35)
    check('Thickness does not paint a texture over a fully diffused flat backdrop', difference(thin, thick, center) < 0.05)
    if args.reference:
        previous = Image.open(args.reference).convert('RGB')
        ref_left, ref_top = origin(previous)
        old_variance = variance(previous, (ref_left + 180, ref_top + 220, ref_left + 700, ref_top + 420))
        measurements['previous_textured_center_standard_deviation'] = old_variance
        check('The previous mineral texture has been removed', material_variance < old_variance * 0.25)
    check('The clean finish stays fixed between frames', difference(thick, capture('crystal-clean-frost-stable'), center) < 0.05)

    wallpaper('frost-bright', (255, 255, 255))
    setting('crystal_blur_amount', 35)
    setting('crystal_thickness', 0)
    thin_reading = capture('crystal-reading-thin')
    setting('crystal_thickness', 100)
    thick_reading = capture('crystal-reading-thick')
    check('Thickness still updates transmission live', difference(thin_reading, thick_reading, center) > 0.5)
    check('Thickness leaves the opaque header untouched', difference(thin_reading, thick_reading, header) == 0)
    check('Thickness leaves foreground controls untouched', difference(thin_reading, thick_reading, control) == 0)
    thin_contrast = minimum_contrast(thin_reading, center)
    thick_contrast = minimum_contrast(thick_reading, center)
    measurements['thin_contrast_at_35_frost'] = thin_contrast
    measurements['thick_contrast_at_35_frost'] = thick_contrast
    check('More thickness improves reading contrast at the same frost setting', thick_contrast > thin_contrast * 1.15)
    check('Maximum thickness keeps the controlled reading sample above 4.5:1', thick_contrast >= 4.5)
    setting('crystal_thickness', 255)
    check('Thickness is bounded at 100 percent', difference(thick_reading, capture('crystal-thickness-clamped'), center) < 0.05)
    BACKGROUND.write_bytes(old_background)
    time.sleep(0.8)
    setting('crystal_everyday', False)
    setting('crystal_blur_amount', 0)
    setting('crystal_thickness', 0)
    optical_thin = capture('crystal-depth-thin')
    setting('crystal_thickness', 100)
    optical_thick = capture('crystal-depth-thick')
    check('Thickness retains layered background refraction without the frost texture', difference(optical_thin, optical_thick, center) > 0.5)
    setting('crystal_thickness', 40)
    medium = capture('crystal-thickness-default')
    setting('crystal_thickness', 41)
    check('Adjacent thickness percentages produce different rendering', difference(medium, capture('crystal-thickness-41'), center) > 0.005)
    setting('crystal_thickness', 40)
    check('Restoring thickness restores the same transmission', difference(medium, capture('crystal-thickness-restored'), center) < 0.05)
    setting('crystal_enabled', False)
    stock = capture('crystal-thickness-stock')
    setting('crystal_thickness', 0)
    check('Thickness does not alter Frosted Glass', difference(stock, capture('crystal-thickness-stock-unchanged'), center) < 0.05)
    (REPORTS / 'crystal-frost-results.json').write_text(json.dumps(results, indent=2) + '\n')
    (REPORTS / 'crystal-frost-measurements.json').write_text(json.dumps(measurements, indent=2) + '\n')
    print(json.dumps({'checks': results, 'measurements': measurements}, indent=2))
finally:
    BACKGROUND.write_bytes(old_background)
    for key, content in saved.items():
        if content is None:
            (CONFIG / key).unlink(missing_ok=True)
        else:
            (CONFIG / key).write_bytes(content)
    time.sleep(0.8)
