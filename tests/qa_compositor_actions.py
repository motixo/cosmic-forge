#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check compositor actions during a stalled background update in private XDG."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from unittest.mock import patch
from PIL import ImageChops
from qa_native import ROOT, Session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    button_y = 663
    artifacts = ROOT / 'build/qa-compositor-actions'
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-actions-', dir=ROOT / 'build') as temporary:
        fixture = Path(temporary)
        tools = fixture / 'tools'
        tools.mkdir()
        worker = '''import argparse,json,os,sys,time
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--build',action='store_true')
p.add_argument('--component',default='compositor')
p.add_argument('--release-tag',required=True)
a=p.parse_args()
state=Path(os.environ['XDG_STATE_HOME'])/'cosmic-forge'
with (state/'action-audit.jsonl').open('a') as f:
    f.write(json.dumps({'build':a.build,'tag':a.release_tag,'component':a.component})+'\\n')
if len((state/'action-audit.jsonl').read_text().splitlines()) == 3:
    sys.exit('COMPOSITOR_TEST_FAILURE')
print('FORGE_PROGRESS:building' if a.build else 'FORGE_PROGRESS:downloading_binary',flush=True)
time.sleep(1.5)
print('FORGE_PROGRESS:already_installed',flush=True)
'''
        bundle = json.dumps({'format': 1, 'abi': 1, 'files': {
            'install.py': worker, 'build.py': '', 'recovery.py': '',
            'release.json': '{"abi":1,"renderer_revision":2}'}}).encode()
        url = 'https://github.com/motixo/cosmic-forge-engine/releases/download/engine-ui-actions/cosmic-forge-engine.json'
        metadata = {'tag_name': 'engine-ui-actions',
            'html_url': 'https://github.com/motixo/cosmic-forge-engine/releases/tag/engine-ui-actions', 'assets': [{'name': 'cosmic-forge-engine.json',
            'size': len(bundle), 'digest': 'sha256:' + hashlib.sha256(bundle).hexdigest(), 'browser_download_url': url}]}
        (fixture / 'bundle.json').write_bytes(bundle)
        (fixture / 'release.json').write_text(json.dumps(metadata))
        curl = tools / 'curl'
        curl.write_text('#!/usr/bin/python3\nimport sys,time\nfrom pathlib import Path\n'
                        'root=Path(' + repr(str(fixture)) + ')\n'
                        "if 'cosmic-forge-shaders' in sys.argv[-1]:\n"
                        "    (root/'update-started').touch()\n"
                        "    deadline=time.monotonic()+30\n"
                        "    while not (root/'release-update').exists():\n"
                        "        if time.monotonic()>deadline:sys.exit('Test update timed out')\n"
                        "        time.sleep(.05)\n"
                        "    Path(sys.argv[sys.argv.index('--output')+1]).write_text('{}')\n"
                        "    sys.stdout.write('404')\n"
                        "    sys.exit(0)\n"
                        "source='release.json' if sys.argv[-1].endswith('/releases/latest') else 'bundle.json'\n"
                        "Path(sys.argv[sys.argv.index('--output')+1]).write_bytes((root/source).read_bytes())\n"
                        "sys.stdout.write('200')\n")
        curl.chmod(0o755)
        with patch.dict(os.environ, {'PATH': str(tools) + ':' + os.environ['PATH']}):
            session = Session(args.binary.resolve(), fixture / 'session', with_engine=False)
        try:
            session.window.configure(height=1000)
            session.display.sync()
            time.sleep(.3)
            state = session.path / 'state/cosmic-forge'
            audit = state / 'action-audit.jsonl'
            session.click(816, 24)
            deadline = time.monotonic() + 5
            while not (fixture / 'update-started').exists() and time.monotonic() < deadline:
                time.sleep(.05)
            assert (fixture / 'update-started').exists(), 'Header did not check for updates'
            launcher = Path(session.environment['HOME']) / '.local/bin/cosmic-comp'
            launcher.parent.mkdir(parents=True, exist_ok=True)
            launcher.write_text('#!/bin/sh\n# Managed by cosmic-crystal\nexit 77\n')
            launcher.chmod(0o755)
            legacy = session.path / 'state/cosmic-crystal/installation.json'
            legacy.parent.mkdir(parents=True, exist_ok=True)
            legacy.write_text(json.dumps({'shims': {'cosmic-comp': launcher.read_text()}}))
            preserved = {legacy: legacy.read_bytes(), **{
                p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()
            }}
            session.click(856, 24)
            session.scroll_to_top()
            session.capture('install-only').save(artifacts / 'install-only.png')
            session.click(450, 712)
            deadline = time.monotonic() + 5
            while launcher.exists() and time.monotonic() < deadline:
                time.sleep(.1)
            assert not launcher.exists(), 'Offline Restore waited for the background network request'
            assert all(p.read_bytes() == data for p, data in preserved.items())
            time.sleep(.3)
            for index in (1, 2):
                log = state / 'engine-build.log'
                log.unlink(missing_ok=True)
                session.click(420, button_y, wait=0.5)
                session.scroll_to_top()
                session.capture('download-progress').save(artifacts / 'download-progress.png')
                session.click(420, button_y, wait=.1)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if audit.exists() and len(audit.read_text().splitlines()) == index and log.exists() and \
                       'FORGE_PROGRESS:already_installed' in log.read_text():
                        break
                    time.sleep(0.1)
                else:
                    raise AssertionError('Install button did not invoke its worker')
                call = json.loads(audit.read_text().splitlines()[-1])
                assert call == {'build': False, 'tag': 'engine-ui-actions', 'component': 'compositor'}, call
                assert 'FORGE_PROGRESS:downloading_binary' in log.read_text()
                assert 'FORGE_PROGRESS:building' not in log.read_text()
                time.sleep(0.3)
            # Clicking the former Build button must not start another worker.
            session.click(950, button_y)
            assert len(audit.read_text().splitlines()) == 2,'A removed Build button still runs'
            log.unlink()
            session.click(420, button_y)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if log.exists() and 'COMPOSITOR_TEST_FAILURE' in log.read_text():
                    break
                time.sleep(0.1)
            else:
                raise AssertionError('The error fixture did not run')
            time.sleep(0.5)
            session.scroll_to_top()
            error = session.capture('compositor-error')
            error.save(artifacts / 'compositor-error.png')
            (fixture / 'release-update').touch()
            time.sleep(1)
            after = session.capture('error-after-update')
            assert not ImageChops.difference(error.crop((320, 586, 1008, 900)),
                                            after.crop((320, 586, 1008, 900))).getbbox(), 'Update completion replaced the compositor error'
            (artifacts / 'results.json').write_text(json.dumps({'install_downloads': True, 'build_removed': True,
                'same_release_tag': True, 'restore_during_background_update': True,
                'install_during_background_update': True, 'no_duplicate_installer': True,
                'update_preserves_compositor_error': True}, indent=2) + '\n')
            print('PASS Restore and Install during a stalled background update; single installer; independent errors and progress')
        finally:
            (fixture / 'release-update').touch()
            session.close()


if __name__ == '__main__':
    main()
