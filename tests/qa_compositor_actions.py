#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check both compositor buttons with an isolated release/worker fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from unittest.mock import patch
from qa_native import ROOT, Session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
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
p.add_argument('--release-tag',required=True)
a=p.parse_args()
state=Path(os.environ['XDG_STATE_HOME'])/'cosmic-forge'
with (state/'action-audit.jsonl').open('a') as f:
    f.write(json.dumps({'build':a.build,'tag':a.release_tag})+'\\n')
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
        metadata = {'tag_name': 'engine-ui-actions', 'assets': [{'name': 'cosmic-forge-engine.json',
            'size': len(bundle), 'digest': 'sha256:' + hashlib.sha256(bundle).hexdigest(), 'browser_download_url': url}]}
        (fixture / 'bundle.json').write_bytes(bundle)
        (fixture / 'release.json').write_text(json.dumps(metadata))
        curl = tools / 'curl'
        curl.write_text('#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\n'
                        'root=Path(' + repr(str(fixture)) + ')\n'
                        "source='release.json' if sys.argv[-1].endswith('/releases/latest') else 'bundle.json'\n"
                        "Path(sys.argv[sys.argv.index('--output')+1]).write_bytes((root/source).read_bytes())\n"
                        "sys.stdout.write('200')\n")
        curl.chmod(0o755)
        with patch.dict(os.environ, {'PATH': str(tools) + ':' + os.environ['PATH']}):
            session = Session(args.binary.resolve(), fixture / 'session', with_engine=False)
        try:
            state = session.path / 'state/cosmic-forge'
            audit = state / 'action-audit.jsonl'
            session.click(856, 24)
            session.scroll_to_bottom()
            session.capture('two-buttons').save(artifacts / 'two-buttons.png')
            for index, (x, y, build) in enumerate([(420, 783, False), (600, 730, True)], start=1):
                log = state / 'engine-build.log'
                log.unlink(missing_ok=True)
                session.click(x, y, wait=0.5)
                session.scroll_to_bottom()
                session.capture('build-progress' if build else 'download-progress').save(
                    artifacts / ('build-progress.png' if build else 'download-progress.png'))
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if audit.exists() and len(audit.read_text().splitlines()) == index and log.exists() and \
                       'FORGE_PROGRESS:already_installed' in log.read_text():
                        break
                    time.sleep(0.1)
                else:
                    raise AssertionError('Compositor button did not invoke its worker: ' + str((x, y)))
                call = json.loads(audit.read_text().splitlines()[-1])
                assert call == {'build': build, 'tag': 'engine-ui-actions'}, call
                stage = 'building' if build else 'downloading_binary'
                assert 'FORGE_PROGRESS:' + stage in log.read_text()
                time.sleep(0.3)
            log.unlink()
            session.click(420, 730)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if log.exists() and 'COMPOSITOR_TEST_FAILURE' in log.read_text():
                    break
                time.sleep(0.1)
            else:
                raise AssertionError('The error fixture did not run')
            time.sleep(0.5)
            session.scroll_to_bottom()
            session.capture('compositor-error').save(artifacts / 'compositor-error.png')
            (artifacts / 'results.json').write_text(json.dumps({'install_downloads': True, 'build_is_explicit': True,
                                                               'same_release_tag': True}, indent=2) + '\n')
            print('PASS separate native Install/Build buttons and progress; release tag preserved')
        finally:
            session.close()


if __name__ == '__main__':
    main()
