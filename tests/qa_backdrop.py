#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the native backdrop picker using private preferences and engine fixtures."""
import json
from pathlib import Path
import time
from PIL import ImageChops

from qa_native import ROOT, Session


def main():
    session = Session(ROOT / 'target/release/cosmic-forge', ROOT / 'build/qa-backdrop-controls')
    results = []

    def check(name, condition):
        results.append({'test': name, 'passed': bool(condition)})
        (session.path / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print(('PASS ' if condition else 'FAIL ') + name, flush=True)
        assert condition, name

    def revision(value):
        path = session.path / 'state/cosmic-forge/engine-installation.json'
        record = json.loads(path.read_text())
        record['renderer_revision'] = value
        path.write_text(json.dumps(record))
        time.sleep(1)

    def wallpaper():
        session.click(920, 665)
        session.click(920, 623)

    try:
        revision(5)
        session.select_effect(2)
        before = {key: session.get(key) for key in ['aero_color', 'aero_reflections', 'aero_blur_amount']}
        wallpaper()
        preference = session.config / 'cosmic-forge/backdrops/aero.json'
        check('The native picker saves the selected source', session.get('forge_backdrop') == 'wallpaper' and json.loads(preference.read_text()) == 'wallpaper')
        check('Changing source preserves the existing material controls', before == {key: session.get(key) for key in before})
        session.capture('wallpaper-source')
        session.click(968, 282)
        check('Material controls retain the backdrop choice', session.get('aero_blur_amount') == '46' and session.get('forge_backdrop') == 'wallpaper')
        session.select_effect(1)
        check('Another effect starts with its own source default', session.get('forge_backdrop') is None)
        session.select_effect(2)
        check('Returning to an effect restores its saved source', session.get('forge_backdrop') == 'wallpaper')
        session.click(940, 724)
        check('Reset restores the source and material defaults', not preference.exists() and session.get('forge_backdrop') is None and session.get('aero_blur_amount') == '45')
        supported = session.capture('wallpaper-supported')
        revision(4)
        incompatible = session.capture('wallpaper-requires-update')
        region = (320, 636, 1008, 695)
        check('An incompatible renderer hides the backdrop picker',
              ImageChops.difference(supported.crop(region), incompatible.crop(region)).getbbox() is not None)
        check('Hiding the picker preserves the source preference', not preference.exists() and session.get('forge_backdrop') is None)
        revision(5)
        session.select_effect(0)
        supported = session.capture('frosted-with-engine')
        (session.path / 'home/.local/bin/cosmic-comp').unlink()
        time.sleep(1)
        missing = session.capture('frosted-without-engine')
        region = (320, 200, 1008, 620)
        check('Frosted Glass hides its backdrop picker without an engine',
              ImageChops.difference(supported.crop(region), missing.crop(region)).getbbox() is not None)
    finally:
        session.close()


if __name__ == '__main__':
    main()
