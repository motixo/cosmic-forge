#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check renderer progress, installation reuse and stock restoration in private XDG/HOME."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from unittest.mock import patch
from PIL import ImageChops
from qa_native import Session

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    parser.add_argument('--engine-binary', type=Path, default=ROOT / 'engine/build/bin/cosmic-comp')
    args = parser.parse_args()
    packages = {name: subprocess.check_output(['/usr/bin/pacman', '-Q', name], text=True).strip()
                for name in ('cosmic-comp', 'cosmic-settings', 'cosmic-panel', 'cosmic-greeter')}
    artifacts = ROOT / 'build/qa-engine-progress'
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-progress-', dir=ROOT / 'build') as temporary:
        tools = Path(temporary) / 'tools'
        tools.mkdir()
        wrapper = tools / 'pacman'
        wrapper.write_text('#!/usr/bin/python3\nimport sys,time\n'
                           'time.sleep(0.6)\nprint(' + repr(packages) + '[sys.argv[-1]])\n')
        wrapper.chmod(0o755)
        # Slow only the test's package checks so the animation can be observed.
        with patch.dict(os.environ, {'PATH': str(tools) + ':' + os.environ['PATH']}):
            session = Session(args.binary.resolve(), Path(temporary) / 'session', with_engine=False)
        try:
            env = dict(session.environment, PATH=os.environ['PATH'])
            launcher = Path(env['HOME']) / '.local/bin/cosmic-comp'
            launcher.parent.mkdir(parents=True)
            legacy = b'#!/bin/sh\n# Managed by cosmic-crystal\nexit 77\n'
            launcher.write_bytes(legacy)
            launcher.chmod(0o755)
            settings = launcher.with_name('cosmic-settings')
            settings.write_bytes(b'keep custom Settings launcher\n')
            legacy_record = Path(env['XDG_STATE_HOME']) / 'cosmic-crystal/installation.json'
            legacy_record.parent.mkdir(parents=True)
            legacy_record.write_text(json.dumps({'shims': {'cosmic-comp': legacy.decode()}}))
            preserved = {p: p.read_bytes() for p in [settings, legacy_record]}
            subprocess.run(['python3', str(ROOT / 'engine/install.py'), '--binary', str(args.engine_binary.resolve())],
                           env=env, check=True, capture_output=True)
            state = Path(env['XDG_STATE_HOME']) / 'cosmic-forge'
            record_path = state / 'engine-installation.json'
            record = json.loads(record_path.read_text())
            paths = [record_path, Path(record['launcher']), Path(record['destination']) / 'cosmic-comp']
            before = {p: (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mtime_ns) for p in paths}
            session.click(856, 24)
            session.scroll_to_bottom()
            session.click(450, 738, wait=0.35)
            session.scroll_to_bottom()
            first = session.capture('progress-active')
            first.save(artifacts / 'progress-active.png')
            time.sleep(0.45)
            second = session.capture('progress-animated')
            assert ImageChops.difference(first.crop((344, 700, 984, 810)),
                                        second.crop((344, 700, 984, 810))).getbbox(), 'Progress bar did not animate'
            log = state / 'engine-build.log'
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                if log.exists() and 'FORGE_PROGRESS:already_installed' in log.read_text():
                    break
                time.sleep(0.2)
            else:
                raise AssertionError('The installed renderer was not recognized: ' + (log.read_text() if log.exists() else 'no setup log'))
            time.sleep(0.5)
            session.capture('progress-finished').save(artifacts / 'progress-finished.png')
            assert before == {p: (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mtime_ns) for p in paths}
            text = log.read_text()
            assert 'FORGE_PROGRESS:building' not in text and 'FORGE_PROGRESS:installing' not in text
            recovery = state / 'engine-previous-compositor'
            engine = Path(record['destination']) / 'cosmic-comp'
            original = Path('/usr/bin/cosmic-comp')
            original_before = hashlib.sha256(original.read_bytes()).hexdigest(), original.stat().st_mtime_ns
            for legacy_only in (False, True):
                if legacy_only:
                    launcher.write_bytes(legacy)
                    launcher.chmod(0o755)
                session.scroll_to_bottom()
                session.click(450, 733)
                deadline = time.monotonic() + 10
                while launcher.exists() and time.monotonic() < deadline:
                    time.sleep(0.1)
                assert not launcher.exists(), 'Restore kept the patched compositor launcher'
                assert shutil.which('cosmic-comp', path=str(launcher.parent) + os.pathsep + '/usr/bin') == str(original)
                assert not record_path.exists(), 'Restore kept the Forge renderer installation record'
                time.sleep(0.4)
                assert 'Original COSMIC compositor restored' in log.read_text()
                assert preserved == {p: p.read_bytes() for p in preserved}
                assert recovery.read_bytes() == legacy
                assert before[engine] == (hashlib.sha256(engine.read_bytes()).hexdigest(), engine.stat().st_mtime_ns)
                assert original_before == (hashlib.sha256(original.read_bytes()).hexdigest(), original.stat().st_mtime_ns)
                session.capture('restore-legacy' if legacy_only else 'restore-forge').save(
                    artifacts / ('restore-legacy.png' if legacy_only else 'restore-forge.png'))
            (artifacts / 'results.json').write_text(json.dumps({'animated_progress': True, 'already_installed': True,
                                                             'no_rebuild_or_reinstall': True,
                                                             'restore_forge_to_system': True,
                                                             'restore_legacy_to_system': True,
                                                             'settings_and_recovery_preserved': True}, indent=2) + '\n')
            print('PASS animated progress; matching renderer skipped; Forge/legacy Restore selects stock; Settings/recovery preserved')
        finally:
            session.close()


if __name__ == '__main__':
    main()
