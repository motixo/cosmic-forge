#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Verify three-repository reminders in private XDG without installing app or engine."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from unittest.mock import patch
from PIL import ImageChops
from Xlib import X
from qa_native import Session, ROOT


with tempfile.TemporaryDirectory(prefix='forge-updates-', dir=ROOT / 'build') as tmp:
    fixture = Path(tmp)
    tools = fixture / 'tools'
    tools.mkdir()

    def publish(name, repo, asset, data):
        (fixture / (name + '-asset')).write_bytes(data)
        (fixture / (name + '-metadata')).write_text(json.dumps({
            'tag_name': 'glass-v99999' if name == 'effects' else 'engine-v99999',
            'html_url': f'https://github.com/{repo}/releases/tag/test',
            'body': f'Changes for {name}.\nA release note with useful details.',
            'assets': [{
            'name': asset, 'size': len(data), 'digest': 'sha256:' + hashlib.sha256(data).hexdigest(),
            'browser_download_url': f'https://github.com/{repo}/releases/download/test/{asset}',
        }]}))

    subprocess.run(['python3', str(ROOT / 'shaders/scripts/package_effects.py'), '--revision', '99999',
                    '--output', str(fixture / 'effects')], check=True, capture_output=True)
    publish('effects', 'motixo/cosmic-forge-shaders', 'cosmic-forge-glass.json', (fixture / 'effects').read_bytes())
    subprocess.run(['python3', str(ROOT / 'engine/scripts/package_engine.py'), '--output', str(fixture / 'engine')],
                   check=True, capture_output=True)
    publish('engine', 'motixo/cosmic-forge-engine', 'cosmic-forge-engine.json', (fixture / 'engine').read_bytes())
    (fixture / 'app-metadata').write_text(json.dumps({
        'tag_name': 'v99.0.0',
        'html_url': 'https://github.com/motixo/cosmic-forge/releases/tag/v99.0.0',
        'body': 'Changes for the app.\nA release note with useful details.',
    }))
    opener = tools / 'xdg-open'
    opener.write_text('#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\nPath(' + repr(str(fixture / 'opened')) + ').write_text(sys.argv[1])\n')
    opener.chmod(0o755)
    curl = tools / 'curl'
    curl.write_text('#!/usr/bin/python3\nimport sys,time\nfrom pathlib import Path\nroot=Path(' + repr(str(fixture)) + ')\n'
        'url=sys.argv[-1]\n'
        'name="effects" if "cosmic-forge-shaders" in url else "engine" if "cosmic-forge-engine" in url else "app"\n'
        'source=name+("-metadata" if url.endswith("/releases/latest") else "-asset")\n'
        'with (root/"audit").open("a") as f:f.write(source+"\\n")\n'
        'time.sleep(.3)\nPath(sys.argv[sys.argv.index("--output")+1]).write_bytes((root/source).read_bytes())\nsys.stdout.write("200")\n')
    curl.chmod(0o755)
    with patch.dict(os.environ, {'PATH': str(tools) + ':' + os.environ['PATH']}):
        session = Session(ROOT / 'target/release/cosmic-forge', ROOT / 'build/qa-updates', with_engine=False)
    try:
        assert not (fixture / 'audit').exists(), 'Automatic checks must default to off'
        state = session.path / 'state/cosmic-forge'
        state.mkdir(parents=True, exist_ok=True)
        (state / 'engine-installation.json').write_text('{"test":"installed engine for update check"}')
        legacy = {
            session.path / 'data/cosmic-forge/mods/custom.json': b'{"existing_mod":"preserve"}',
            state / 'panel-installation.json': b'{"legacy":true}',
            session.config / 'cosmic/com.system76.CosmicPanel.Dock/v1/anchor_gap': b'true\n',
            session.config / 'cosmic/com.system76.CosmicGreeter/v1/wallpaper': b'"existing wallpaper"\n',
        }
        for path, data in legacy.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        before = {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        session.click(816, 24)
        target = session.path / 'data/cosmic-forge/packs/cosmic-glass.json'
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if target.exists() and (fixture / 'audit').exists() and len((fixture / 'audit').read_text().splitlines()) >= 5:
                break
            time.sleep(.1)
        else:
            raise AssertionError('The header did not check all three repositories')
        time.sleep(1)
        collapsed = session.capture('updates-collapsed')
        assert (fixture / 'audit').read_text().splitlines() == ['app-metadata', 'engine-metadata', 'engine-asset', 'effects-metadata', 'effects-asset']
        assert json.loads(target.read_text())['revision'] == 99999
        assert all(path.read_bytes() == data for path, data in legacy.items())
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        assert not (Path(session.environment['HOME']) / '.local/bin/cosmic-comp').exists()
        assert not (state / 'engine-compositor-receipt.json').exists()

        session.click(330, 277)
        assert (fixture / 'opened').read_text() == 'https://github.com/motixo/cosmic-forge/releases/tag/v99.0.0'
        for name, button_y, crop in [
            ('app', 326, (234, 350, 790, 400)),
            ('engine', 472, (234, 496, 790, 546)),
        ]:
            session.click(330, button_y)
            opened = session.capture(name + '-changelog')
            assert ImageChops.difference(collapsed.crop(crop), opened.crop(crop)).getbbox(), name
            session.click(330, button_y)
            closed = session.capture(name + '-changelog-closed')
            assert not ImageChops.difference(collapsed.crop(crop), closed.crop(crop)).getbbox(), name
        session.click(330, 618)
        session.capture('shader-changelog-expanded')
        session.click(310, 423)
        session.capture('compositor-only-settings')

        setting = session.config / 'cosmic-forge/automatic-updates.json'
        assert not setting.exists()
        session.click(960, 663)
        assert json.loads(setting.read_text()) is True
        time.sleep(3)
        assert len((fixture / 'audit').read_text().splitlines()) == 10
        session.capture('automatic-updates-enabled')
        session.key('Escape')

        def restart():
            session.key('q', True)
            session.app.wait(timeout=5)
            session.app = subprocess.Popen(['dbus-run-session', '--', str(session.binary)],
                                           env=session.environment, stdout=session.log, stderr=session.log)
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                windows = [w for w in session.display.screen().root.query_tree().children
                           if w.get_wm_name() == 'COSMIC Forge']
                if windows:
                    session.window = windows[0]
                    break
                time.sleep(.1)
            else:
                raise AssertionError('Forge did not reopen')
            time.sleep(3)
            session.window.set_input_focus(X.RevertToParent, X.CurrentTime)
            session.display.sync()

        restart()
        assert len((fixture / 'audit').read_text().splitlines()) == 15, 'Every enabled launch must check all three repositories'
        session.capture('startup-reminder')
        session.key('Escape')
        session.click(856, 24)
        session.click(960, 663)
        session.capture('automatic-updates-disabled')
        assert json.loads(setting.read_text()) is False
        restart()
        assert len((fixture / 'audit').read_text().splitlines()) == 15, 'Disabled reminders must not check on startup'
        app_release = json.loads((fixture / 'app-metadata').read_text())
        app_release['body'] = '\n'.join(f'Change {i}: useful release details.' for i in range(100))
        (fixture / 'app-metadata').write_text(json.dumps(app_release))
        session.click(816, 24)
        time.sleep(3)
        assert len((fixture / 'audit').read_text().splitlines()) == 20, 'The header must still check with reminders off'
        session.capture('manual-check-with-reminders-off')
        session.window.configure(width=700, height=500)
        session.display.sync()
        time.sleep(.6)
        small = session.capture('updates-small-window')
        session.click(170, 178)
        session.capture('updates-small-long-changelog')
        session.key('Escape')
        assert ImageChops.difference(small, session.capture('updates-small-closed')).getbbox()

        assert all(path.read_bytes() == data for path, data in legacy.items())
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file() and p != setting}
        assert not (Path(session.environment['HOME']) / '.local/bin/cosmic-comp').exists()
        assert not (state / 'engine-compositor-receipt.json').exists()
        (session.path / 'results.json').write_text(json.dumps({
            'header_checks_all_three': True, 'shaders_updated_automatically': True,
            'app_opens_release_page': True, 'engine_opens_patches': True,
            'collapsible_changelogs': True, 'automatic_startup_check': True,
            'opt_out_persists': True, 'manual_check_works_when_disabled': True,
            'no_implicit_installation': True, 'legacy_mod_data_preserved': True,
        }, indent=2) + '\n')
        print('PASS three-repository dialog, changelogs, actions, startup reminder, opt-out and shader updates; no app/engine installation')
    finally:
        session.close()
