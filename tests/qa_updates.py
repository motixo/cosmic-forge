#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Verify shader updates and compositor notification without installing anything."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from unittest.mock import patch
from qa_native import Session, ROOT


with tempfile.TemporaryDirectory(prefix='forge-updates-', dir=ROOT / 'build') as tmp:
    fixture = Path(tmp)
    tools = fixture / 'tools'
    tools.mkdir()

    def publish(name, repo, asset, data):
        (fixture / (name + '-asset')).write_bytes(data)
        (fixture / (name + '-metadata')).write_text(json.dumps({'assets': [{
            'name': asset, 'size': len(data), 'digest': 'sha256:' + hashlib.sha256(data).hexdigest(),
            'browser_download_url': f'https://github.com/{repo}/releases/download/test/{asset}',
        }]}))

    subprocess.run(['python3', str(ROOT / 'shaders/scripts/package_effects.py'), '--revision', '99999',
                    '--output', str(fixture / 'effects')], check=True, capture_output=True)
    publish('effects', 'motixo/cosmic-forge-shaders', 'cosmic-forge-glass.json', (fixture / 'effects').read_bytes())
    subprocess.run(['python3', str(ROOT / 'engine/scripts/package_engine.py'), '--output', str(fixture / 'engine')],
                   check=True, capture_output=True)
    publish('engine', 'motixo/cosmic-forge-engine', 'cosmic-forge-engine.json', (fixture / 'engine').read_bytes())
    curl = tools / 'curl'
    curl.write_text('#!/usr/bin/python3\nimport sys,time\nfrom pathlib import Path\nroot=Path(' + repr(str(fixture)) + ')\n'
        'url=sys.argv[-1]\n'
        'name="effects" if "cosmic-forge-shaders" in url else "engine" if "cosmic-forge-engine" in url else "unexpected"\n'
        'with (root/"audit").open("a") as f:f.write(name+"\\n")\n'
        'source=name+("-metadata" if url.endswith("/releases/latest") else "-asset")\n'
        'time.sleep(.3)\nPath(sys.argv[sys.argv.index("--output")+1]).write_bytes((root/source).read_bytes())\nsys.stdout.write("200")\n')
    curl.chmod(0o755)
    with patch.dict(os.environ, {'PATH': str(tools) + ':' + os.environ['PATH']}):
        session = Session(ROOT / 'target/release/cosmic-forge', ROOT / 'build/qa-updates', with_engine=False)
    try:
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
            if target.exists() and (fixture / 'audit').exists() and len((fixture / 'audit').read_text().splitlines()) >= 4:
                break
            time.sleep(.1)
        else:
            raise AssertionError('The header did not check shaders and compositor')
        time.sleep(1)
        session.capture('compositor-update-message')
        assert (fixture / 'audit').read_text().splitlines() == ['effects', 'effects', 'engine', 'engine']
        assert json.loads(target.read_text())['revision'] == 99999
        assert all(path.read_bytes() == data for path, data in legacy.items())
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        assert not (Path(session.environment['HOME']) / '.local/bin/cosmic-comp').exists()
        assert not (state / 'engine-compositor-receipt.json').exists()
        session.key('Escape')
        session.click(856, 24)
        session.capture('compositor-only-settings')
        (session.path / 'results.json').write_text(json.dumps({
            'header_updates_shaders_only': True, 'compositor_release_checked': True,
            'no_implicit_installation': True, 'legacy_mod_data_preserved': True,
        }, indent=2) + '\n')
        print('PASS shader updater; compositor notification; no mod requests; no implicit installation')
    finally:
        session.close()
