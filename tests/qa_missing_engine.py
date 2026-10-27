#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check inline shader engine requirements without changing desktop preferences."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time
from PIL import ImageChops
from qa_native import ROOT, Session


def same(first, second):
    region = (300, 48, 1035, 800)
    return ImageChops.difference(first.crop(region), second.crop(region)).getbbox() is None


def preferences(session):
    return {str(p.relative_to(session.config)): p.read_bytes()
            for p in session.config.rglob('*') if p.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    artifacts = ROOT / 'build/qa-missing-engine'
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-missing-', dir=ROOT / 'build') as directory:
        session = Session(args.binary.resolve(), Path(directory) / 'session', with_engine=False)
        try:
            before = preferences(session)
            initial = session.capture('frosted-live')
            initial.save(artifacts / 'frosted-live.png')
            for index, name in ((1, 'crystal'), (2, 'aero')):
                session.select_effect(index)
                time.sleep(1.1)
                inline = session.capture(name + '-inline')
                inline.save(artifacts / (name + '-inline.png'))
                assert not same(initial, inline)
                session.key('Escape')
                assert same(inline, session.capture(name + '-escape')), 'Escape dismissed an engine modal'
                session.select_effect(0)
                assert same(initial, session.capture('back-to-frosted')), 'Shader navigation was blocked by a modal'
            assert preferences(session) == before, 'Browsing unavailable shaders changed preferences'
            assert session.get('forge_runtime') is None
            session.select_effect(1)
            crystal = session.capture('crystal-before-settings')
            session.click(415, 300)
            settings = session.capture('settings')
            settings.save(artifacts / 'settings.png')
            assert not same(crystal, settings), 'Go to settings did not open Settings'
            session.click(856, 24)
            assert same(crystal, session.capture('crystal-after-settings')), 'Settings lost the browsed shader'
            # An unrelated preference refresh must not replace the browsed shader with the active one.
            session.put('aero_color', '56')
            time.sleep(1.3)
            assert same(crystal, session.capture('crystal-after-refresh'))
            before = preferences(session)
            subprocess.run([str(session.binary), '--import-pack', str(ROOT / 'shaders/examples/prism.json')],
                           env=session.environment, check=True, capture_output=True)
            time.sleep(1.3)
            assert same(crystal, session.capture('crystal-after-import'))
            session.select_effect(3)
            community = session.capture('community-inline')
            community.save(artifacts / 'community-inline.png')
            session.click(856, 24)
            # Insert a pack before Prism while Settings is open, changing its sidebar index.
            earlier = json.loads((ROOT / 'shaders/examples/prism.json').read_text())
            earlier['id'] = 'aaa-demo'
            earlier['effects'][0]['id'] = 'aaa'
            earlier['effects'][0]['name'] = 'Earlier Glass'
            import_path = Path(directory) / 'earlier.json'
            import_path.write_text(json.dumps(earlier))
            subprocess.run([str(session.binary), '--import-pack', str(import_path)],
                           env=session.environment, check=True, capture_output=True)
            time.sleep(1.3)
            session.click(856, 24)
            assert same(community, session.capture('community-after-settings')), 'Catalog reload lost the browsed shader ID'
            assert preferences(session) == before, 'Import or Settings changed the active effect'
            session.click(896, 24)
            about = session.capture('about')
            about.save(artifacts / 'about.png')
            assert not same(community, about), 'About stopped working'
            session.key('Escape')
            assert same(community, session.capture('about-closed'))
            # Imported shaders can still be removed while their engine is unavailable.
            session.click(400, 316)
            assert not (session.path / 'data/cosmic-forge/packs/prism-demo.json').exists()
            assert (session.path / 'data/cosmic-forge/packs/prism-demo.previous').exists()
            assert preferences(session) == before
            # Startup with an unavailable stored shader must also show the inline requirement.
            session.put('glass_effect', '"crystal"')
            time.sleep(1.3)
            before = preferences(session)
            session.key('q', ctrl=True)
            session.app.wait(timeout=10)
            session.app = subprocess.Popen(['dbus-run-session', '--', str(session.binary)],
                                           env=session.environment, stdout=session.log, stderr=session.log)
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                windows = [w for w in session.display.screen().root.query_tree().children
                           if w.get_wm_name() == 'COSMIC Forge']
                if windows:
                    session.window = windows[0]
                    break
                time.sleep(0.1)
            else:
                raise AssertionError('Window did not reopen')
            time.sleep(2)
            session.window.set_input_focus(1, 0)
            session.display.sync()
            startup = session.capture('startup-inline')
            startup.save(artifacts / 'startup-inline.png')
            assert same(crystal, startup), 'Startup opened a modal or lost the selected shader'
            assert preferences(session) == before
            session.select_effect(0)
            assert session.get('glass_effect') == '"frosted"'
            (artifacts / 'results.json').write_text(json.dumps({
                'frosted_without_custom_engine': True,
                'builtin_and_community_inline_requirements': True,
                'navigation_and_escape_without_modal': True,
                'settings_returns_to_browsed_shader': True,
                'refresh_and_import_preserve_browsed_shader': True,
                'preferences_preserved': True,
                'unavailable_import_can_be_removed': True,
                'startup_inline_requirement': True,
                'about_still_opens': True,
            }, indent=2) + '\n')
            print('PASS inline built-in/community requirements; navigation; Settings return; refresh; removal; startup; unchanged preferences')
        finally:
            session.close()


if __name__ == '__main__':
    main()
