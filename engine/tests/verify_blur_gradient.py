#!/usr/bin/env python3
"""Compare center and corner blur on real compositor output at identical settings."""
import argparse
import json
import os
from pathlib import Path
import time

from PIL import Image, ImageChops, ImageGrab, ImageStat

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--record-reference', action='store_true')
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


def setting(key, value):
    (CONFIG / key).write_text(str(value).lower() + '\n')
    time.sleep(0.6)


def capture(amount, suffix=''):
    setting('crystal_blur_amount', amount)
    image = ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')
    image.save(RUN / f'gradient-{amount}{suffix}.png')
    return image


def difference(a, b, box):
    return sum(ImageStat.Stat(ImageChops.difference(a.crop(box), b.crop(box))).mean) / 3


def detail(image, box):
    image = image.crop(box).convert('L')
    horizontal = ImageChops.difference(image, ImageChops.offset(image, 1, 0))
    vertical = ImageChops.difference(image, ImageChops.offset(image, 0, 1))
    interior = (1, 1, image.width - 1, image.height - 1)
    return ImageStat.Stat(horizontal.crop(interior)).mean[0] + ImageStat.Stat(vertical.crop(interior)).mean[0]


def check(name, condition):
    results.append({'test': name, 'passed': bool(condition)})
    if not condition:
        raise AssertionError(name)


try:
    wallpaper = Image.new('RGB', (1280, 900))
    pixels = wallpaper.load()
    for y in range(900):
        for x in range(1280):
            pixels[x, y] = (255, 255, 255) if (x // 64 + y // 64) % 2 else (0, 0, 0)
    path = RUN / 'gradient-wallpaper.png'
    wallpaper.save(path)
    inverse_path = RUN / 'gradient-inverse.png'
    ImageChops.invert(wallpaper).save(inverse_path)
    def use_wallpaper(file):
        BACKGROUND.write_text('(output: "all", source: Path(' + json.dumps(str(file)) +
            '), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)\n')
        time.sleep(0.7)
    BACKGROUND.write_text('(output: "all", source: Path(' + json.dumps(str(path)) +
        '), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)\n')
    setting('crystal_enabled', True)
    setting('crystal_everyday', True)
    setting('crystal_refraction', 0)
    setting('crystal_density', 100)
    setting('crystal_thickness', 40)
    clear = capture(0)
    points = [(x, y) for y in range(clear.height) for x in range(clear.width)
              if clear.getpixel((x, y)) == (11, 14, 20)]
    assert points
    left = min(x for x, _ in points)
    top = min(y for _, y in points)
    center = (left + 260, top + 220, left + 560, top + 420)
    corner = (left + 752, top + 470, left + 810, top + 520)
    body = (left + 8, top + 100, left + 810, top + 525)
    middle = capture(60)
    matte = capture(100)
    use_wallpaper(inverse_path)
    inverse_clear = capture(0, '-inverse')
    inverse_middle = capture(60, '-inverse')
    inverse_matte = capture(100, '-inverse')
    measurements = {'center_detail_at_60': detail(middle, center),
                    'corner_detail_at_60': detail(middle, corner),
                    'center_background_response_at_60': difference(middle, inverse_middle, center),
                    'corner_background_response_at_60': difference(middle, inverse_middle, corner),
                    'clear_center_background_response': difference(clear, inverse_clear, center),
                    'frosted_center_background_response': difference(matte, inverse_matte, center)}
    (RUN / 'gradient-measurements.json').write_text(json.dumps(measurements, indent=2) + '\n')
    if not args.record_reference:
        check('Everyday mode keeps more image detail at corners than in the reading center',
              measurements['corner_background_response_at_60'] > measurements['center_background_response_at_60'] * 2)
        check('The reading center is more blurred than the clear endpoint',
              measurements['center_background_response_at_60'] < measurements['clear_center_background_response'] * 0.5)
        check('Zero frost preserves clear background transmission through the center',
              measurements['clear_center_background_response'] > 150)
        check('Maximum frost suppresses background detail',
              measurements['frosted_center_background_response'] < measurements['clear_center_background_response'] * 0.05)
        if args.reference:
            old = json.loads((args.reference / 'gradient-measurements.json').read_text())
            check('The reading center is more matte than the previous version at the same setting',
                  measurements['center_detail_at_60'] < old['center_detail_at_60'] * 0.85)
            check('Corners are clearer than the previous version at the same setting',
                  measurements['corner_detail_at_60'] > old['corner_detail_at_60'] * 1.15)
            check('Zero blur retains the previous completely clear reading center',
                  difference(clear, Image.open(args.reference / 'gradient-0.png'), center) < 0.05)
            check('Maximum blur retains the previous fully frosted reading center',
                  difference(matte, Image.open(args.reference / 'gradient-100.png'), center) < 0.05)
        middle.save(ASSETS / 'everyday-blur-gradient.png')
        (REPORTS / 'blur-gradient-results.json').write_text(json.dumps(results, indent=2) + '\n')
        (REPORTS / 'blur-gradient-measurements.json').write_text(json.dumps(measurements, indent=2) + '\n')
    print(json.dumps({'checks': results, 'measurements': measurements}, indent=2))
finally:
    BACKGROUND.write_bytes(old_background)
    for key, content in saved.items():
        if content is None:
            (CONFIG / key).unlink(missing_ok=True)
        else:
            (CONFIG / key).write_bytes(content)
    time.sleep(0.8)
