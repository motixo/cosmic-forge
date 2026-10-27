#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check native Frosted Glass and missing-compositor dialogs in private XDG/HOME."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time
from PIL import ImageChops
from qa_native import Session

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    artifacts = ROOT / 'build/qa-missing-engine'
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-missing-', dir=ROOT / 'build') as directory:
        session = Session(args.binary.resolve(), Path(directory) / 'session', with_engine=False)
        try:
            initial = session.capture('frosted-live')
            initial.save(artifacts / 'frosted-live.png')
            for index in (1, 2):
                session.select_effect(index)
                assert session.get('glass_effect') == '"frosted"'
                assert session.get('forge_runtime') is None
                dialog = session.capture('missing-compositor')
                dialog.save(artifacts / 'missing-compositor.png')
                assert ImageChops.difference(initial.crop((250, 350, 800, 470)),
                                            dialog.crop((250, 350, 800, 470))).getbbox()
                session.click(615, 499)
                after = session.capture('cancelled')
                assert ImageChops.difference(initial.crop((250, 350, 800, 470)),
                                            after.crop((250, 350, 800, 470))).getbbox() is None
                assert session.get('glass_effect') == '"frosted"'
            session.select_effect(1)
            session.click(720, 499)
            session.capture('settings').save(artifacts / 'settings.png')
            panel = session.config / 'cosmic/com.system76.CosmicTheme.Light/v2/frosted_panel'
            before = panel.read_text().strip()
            session.click(960, 302)
            assert panel.read_text().strip() != before, 'Go to settings did not open the Settings page'
            session.click(856, 24)
            assert session.get('glass_effect') == '"frosted"'
            subprocess.run([str(session.binary), '--import-pack', str(ROOT / 'shaders/examples/prism.json')],
                           env=session.environment, check=True, capture_output=True)
            time.sleep(1.3)
            session.select_effect(3)
            assert session.get('glass_effect') == '"frosted"'
            assert session.get('forge_runtime') is None
            session.key('Escape')
            session.click(896, 24)
            session.capture('about').save(artifacts / 'about.png')
            session.key('Escape')
            (artifacts / 'results.json').write_text(json.dumps({
                'frosted_without_custom_engine': True,
                'crystal_and_aero_require_compositor': True,
                'cancel_preserves_selection': True,
                'go_to_settings': True,
                'community_shader_requires_forge': True,
                'about_displayed': True,
            }, indent=2) + '\n')
            print('PASS Frosted live; built-in/community compositor prompts; Cancel; Settings; About')
        finally:
            session.close()


if __name__ == '__main__':
    main()
