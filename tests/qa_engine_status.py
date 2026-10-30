#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check live session diagnostics through real Unix sockets in private XDG directories."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from PIL import ImageChops
from qa_native import ROOT, Session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    session = Session(args.binary.resolve(), ROOT / 'build/qa-engine-status')
    child = None
    record_path = session.path / 'state/cosmic-forge/engine-installation.json'
    record = json.loads(record_path.read_text())
    binary = Path(record['destination']) / 'cosmic-comp'
    binary.write_bytes(Path(sys.executable).read_bytes())
    runtime = session.path / 'runtime'
    marker = runtime / 'cosmic-forge-engine-v1'
    socket = runtime / 'wayland-0'
    server = ('import socket,sys; s=socket.socket(socket.AF_UNIX); '
              's.bind(sys.argv[1]); s.listen();\n'
              'while True:\n c,_=s.accept(); c.close()')
    results = []

    def stop():
        nonlocal child
        if child:
            child.terminate()
            child.wait(timeout=5)
            child = None
        socket.unlink(missing_ok=True)

    def start(executable):
        nonlocal child
        stop()
        child = subprocess.Popen([str(executable), '-c', server, str(socket)],
                                 stdout=session.log, stderr=session.log)
        deadline = time.monotonic() + 4
        while not socket.exists():
            assert child.poll() is None, 'The socket fixture exited'
            assert time.monotonic() < deadline, 'The socket fixture timed out'
            time.sleep(.05)

    def check(expected):
        result = subprocess.run([str(session.binary), '--status'], env=session.environment,
                                capture_output=True, text=True, check=True)
        data = json.loads(result.stdout)['engine']
        assert data['state'] == expected, data
        assert data['message'] or expected == 'active'
        results.append(data)
        time.sleep(1.2)
        screenshot = session.capture(expected)
        (session.path / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print('PASS', expected, data['running'], flush=True)
        return screenshot, data

    try:
        before = {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}
        start(sys.executable)
        _, data = check('different_compositor')
        assert data['running']['pid'] == child.pid
        assert data['running']['executable'] != str(binary)
        marker.write_text(str(os.getpid()))
        check('different_compositor')
        start(binary)
        marker.unlink(missing_ok=True)
        unconfirmed, _ = check('runtime_unconfirmed')
        marker.write_text(str(child.pid))
        active, data = check('active')
        assert data['running']['pid'] == child.pid
        assert data['running']['executable'] == str(binary)
        area = (300, active.height - 80, active.width, active.height)
        assert ImageChops.difference(active.crop(area), unconfirmed.crop(area)).getbbox(), \
            'The footer did not refresh when the marker appeared'
        marker.write_text('invalid')
        check('runtime_unconfirmed')
        marker.write_text(str(child.pid))
        replacement = session.path / 'replacement'
        replacement.write_bytes(binary.read_bytes())
        replacement.chmod(0o755)
        replacement.replace(binary)
        check('restart_required')
        session.click(856, 24)
        session.capture('settings-restart-required')
        session.click(856, 24)
        stop()
        check('session_unverified')
        launcher = Path(record['launcher'])
        original = launcher.read_bytes()
        launcher.write_bytes(b'changed launcher')
        check('installation_invalid')
        launcher.write_bytes(original)
        record_path.unlink()
        check('not_installed')
        assert before == {p: p.read_bytes() for p in session.config.rglob('*') if p.is_file()}, \
            'Diagnostics changed preferences'
        print('PASS native diagnostic refresh and read-only --status for all seven states', flush=True)
    finally:
        stop()
        session.close()


if __name__ == '__main__':
    main()
