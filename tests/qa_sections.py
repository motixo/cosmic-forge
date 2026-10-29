#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check shader navigation and preservation of existing mod preferences."""
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

    def capture(name):
        return session.capture(name).crop((300, 48, 1035, 800))

    def same(first, second):
        return ImageChops.difference(first, second).getbbox() is None

    try:
        legacy = {
            session.config / 'cosmic/com.system76.CosmicPanel.Dock/v1/anchor_gap': 'true\n',
            session.config / 'cosmic/com.system76.CosmicGreeter/v1/wallpaper': '"existing wallpaper"\n',
            session.path / 'data/cosmic-forge/mods/custom.json': '{"legacy": true}\n',
            session.path / 'state/cosmic-forge/panel-installation.json': '{"legacy": true}\n',
        }
        for path, value in legacy.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value)
        before = {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        session.select_effect(1)
        shader = capture('crystal')
        session.click(856, 24)
        session.capture('compositor-only-settings')
        session.click(856, 24)
        assert same(shader, capture('crystal-after-settings')), 'Settings lost the selected shader page'
        for y in (72, 282, 410):
            session.click(100, y)
            assert same(shader, capture(f'sidebar-{y}')), 'A removed section or heading navigated away'
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        subprocess.run([str(session.binary), '--import-pack', str(ROOT / 'shaders/examples/prism.json')],
                       env=session.environment, check=True, capture_output=True)
        time.sleep(1.3)
        assert same(shader, capture('crystal-after-import')), 'Import lost the selected shader page'
        session.select_effect(0)
        assert session.get('glass_effect') == '"frosted"'
        for path, value in legacy.items():
            assert path.read_text() == value, f'Existing mod data changed: {path}'
        (session.path / 'results.json').write_text(json.dumps({
            'shader_navigation': True, 'settings_return': True, 'mod_sections_removed': True,
            'imports_preserve_selection': True, 'legacy_mod_data_preserved': True,
        }, indent=2) + '\n')
        print('PASS shader-only navigation; Settings return; imports; existing mod data preserved')
    finally:
        session.close()


if __name__ == '__main__':
    main()
