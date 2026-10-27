#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check grouped navigation without changing the selected glass effect."""
import argparse
import json
from pathlib import Path
import subprocess
import time
from PIL import ImageChops
from qa_native import ROOT, Session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    session = Session(args.binary.resolve(), ROOT / 'build/qa-sections', with_engine=False)
    results = {}

    def capture(name):
        return session.capture(name).crop((300, 48, 1035, 800))

    def same(first, second):
        return ImageChops.difference(first, second).getbbox() is None

    try:
        before = {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        session.capture('grouped-sidebar')
        session.click(100, 282)
        panel = capture('panel')
        session.click(856, 24)
        session.click(856, 24)
        assert same(panel, capture('panel-after-settings')), 'Settings did not return to Panel'
        results['settings_returns_to_panel'] = True
        session.click(100, 370)
        lock = capture('lock-screen')
        assert not same(panel, lock), 'Lock screen did not open its own page'
        session.click(856, 24)
        session.click(856, 24)
        assert same(lock, capture('lock-after-settings')), 'Settings did not return to Lock screen'
        results['settings_returns_to_lock_screen'] = True
        session.click(100, 72)
        assert same(lock, capture('heading-is-not-an-item')), 'A heading navigated away from the page'
        results['headings_are_labels'] = True
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        results['navigation_preserves_preferences_without_engine'] = True
        session.put('glass_effect', '"crystal"')
        time.sleep(1.3)
        assert same(lock, capture('external-effect-keeps-page')), 'An external effect change replaced Lock screen'
        results['external_effect_changes_keep_current_section'] = True
        subprocess.run([str(session.binary), '--import-pack', str(ROOT / 'shaders/examples/prism.json')],
                       env=session.environment, check=True, capture_output=True)
        time.sleep(1.3)
        assert same(lock, capture('import-keeps-page')), 'Importing an effect replaced Lock screen'
        session.click(100, 322)
        assert same(panel, capture('panel-after-import')), 'Catalog reload lost the Panel item'
        results['catalog_reload_preserves_sections'] = True
        session.select_effect(0)
        assert session.get('glass_effect') == '"frosted"', 'Could not return to native Frosted Glass'
        results['return_to_shader_controls'] = True
        (session.path / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print('PASS grouped headings; independent sections; Settings return; preferences; external changes; imports')
    finally:
        session.close()


if __name__ == '__main__':
    main()
