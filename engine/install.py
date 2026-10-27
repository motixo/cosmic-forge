#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Build/install the user's glass renderer, or restore the original system compositor."""
import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent
PACKAGES = ('cosmic-comp', 'cosmic-settings', 'cosmic-panel', 'cosmic-greeter')
SYSTEM_COMPOSITOR = Path('/usr/bin/cosmic-comp')


def xdg(name, fallback):
    path = Path(os.environ.get(name, Path.home() / fallback))
    return path if path.is_absolute() else Path.home() / fallback


def paths(prefix=None):
    data = prefix / 'share' if prefix else xdg('XDG_DATA_HOME', '.local/share')
    state = prefix / 'state' if prefix else xdg('XDG_STATE_HOME', '.local/state')
    launcher = (prefix or Path.home() / '.local') / 'bin/cosmic-comp'
    return data / 'cosmic-forge/engine', state, launcher


def atomic(path, data, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent, prefix='.' + path.name)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(name, mode)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def progress(stage):
    print('FORGE_PROGRESS:' + stage, flush=True)


def already_installed(record, packages, version, prefix=None, candidate_digest=None):
    if not record or record.get('cosmic_version') != version or record.get('packages') != packages:
        return False
    revision = json.loads((ROOT / 'release.json').read_text())['renderer_revision']
    # The first Forge installer predates this field and installed renderer revision 1.
    if record.get('renderer_revision', 1) != revision:
        return False
    data, _, _ = paths(prefix)
    destination = Path(record.get('destination', ''))
    if destination.parent != data / 'versions' or not re.fullmatch(r'[0-9a-f]{16}', destination.name):
        raise RuntimeError('Invalid installed renderer path')
    binary = destination / 'cosmic-comp'
    if not binary.exists():
        return False
    if binary.is_symlink() or digest(binary.read_bytes()) != record.get('binary_sha256') or not os.access(binary, os.X_OK):
        raise RuntimeError('The installed renderer changed outside Forge; preserved it')
    return candidate_digest is None or candidate_digest == record['binary_sha256']


def report_already_installed():
    progress('already_installed')
    print('The matching Forge glass renderer is already installed. No rebuild or reinstall needed.', flush=True)


def installed_packages():
    release = platform.freedesktop_os_release()
    if release.get('ID') != 'arch' and 'arch' not in release.get('ID_LIKE', '').split():
        raise RuntimeError('The in-app renderer installer currently supports Arch Linux and derivatives')
    packages = {}
    versions = set()
    for name in PACKAGES:
        result = subprocess.run(['pacman', '-Q', name], check=True, capture_output=True, text=True)
        package = result.stdout.strip()
        match = re.fullmatch(re.escape(name) + r' (?:[0-9]+:)?(1\.(?:10|11)\.0)-[^\s]+', package)
        if not match:
            raise RuntimeError('Unsupported COSMIC package: ' + package)
        packages[name] = package
        versions.add(match[1])
    if len(versions) != 1:
        raise RuntimeError('Install matching COSMIC 1.10.0 or 1.11.0 packages first')
    return packages, versions.pop()


def record_for(state, launcher):
    path = state / 'cosmic-forge/engine-installation.json'
    if path.is_symlink():
        raise RuntimeError('Preserved a replaced engine installation record')
    record = json.loads(path.read_text()) if path.exists() else None
    if path.exists() and (not isinstance(record, dict) or not record):
        raise RuntimeError('Invalid engine installation record')
    if launcher.is_symlink():
        raise RuntimeError('Preserved a compositor launcher symlink')
    current = launcher.read_bytes() if launcher.exists() else None
    if record:
        if record.get('abi') != 1 or record.get('launcher') != str(launcher) or current is None or digest(current) != record.get('launcher_sha256'):
            raise RuntimeError('The compositor launcher changed outside Forge; preserved it')
        previous = record.get('previous')
        if not isinstance(previous, dict) or 'data' not in previous or not isinstance(previous.get('mode'), int) or not 0 <= previous['mode'] <= 0o777 or (previous.get('data') is not None and not isinstance(previous['data'], str)):
            raise RuntimeError('Invalid compositor recovery metadata')
        if previous['data'] is not None:
            base64.b64decode(previous['data'], validate=True)
    elif current is not None:
        legacy = state / 'cosmic-crystal/installation.json'
        previous = json.loads(legacy.read_text()) if legacy.exists() else {}
        expected = previous.get('shims', {}).get('cosmic-comp')
        if expected is None or current != expected.encode():
            raise RuntimeError('Preserved a compositor launcher owned by another installation')
    return path, record, current


def install(binary, packages, version, prefix=None):
    data, state, launcher = paths(prefix)
    if any(c in str(launcher) + str(data) + str(state) for c in '\n\r\x00'):
        raise RuntimeError('Installation paths cannot contain line breaks or NUL')
    record_file, record, current = record_for(state, launcher)
    current_mode = launcher.stat().st_mode & 0o777 if current is not None else 0o755
    payload = binary.read_bytes()
    if payload[:4] != b'\x7fELF' or b'cosmic-forge-engine-v1' not in payload:
        raise RuntimeError('Expected the native Forge ABI 1 renderer')
    pins = json.loads((ROOT / 'release.json').read_text())['sources']
    output = subprocess.check_output([str(binary), '--version'], text=True, timeout=15)
    if pins[version] not in output:
        raise RuntimeError('Renderer does not match the installed COSMIC version')
    if already_installed(record, packages, version, prefix, digest(payload)):
        report_already_installed()
        return
    progress('installing')
    previous = record['previous'] if record else {
        'data': base64.b64encode(current).decode() if current is not None else None,
        'mode': launcher.stat().st_mode & 0o777 if current is not None else 0o755,
    }
    fallback = state / 'cosmic-forge/engine-previous-compositor'
    if previous['data'] is not None:
        original = base64.b64decode(previous['data'], validate=True)
        if record and (not fallback.exists() or fallback.is_symlink() or fallback.read_bytes() != original):
            raise RuntimeError('Previous compositor recovery file changed; preserved it')
        if not record:
            if fallback.is_symlink() or (fallback.exists() and fallback.read_bytes() != original):
                raise RuntimeError('Preserved an existing compositor recovery file')
            if not fallback.exists():
                atomic(fallback, original, previous['mode'])
    destination = data / 'versions' / digest(payload)[:16]
    executable = destination / 'cosmic-comp'
    if executable.is_symlink() or (executable.exists() and digest(executable.read_bytes()) != digest(payload)):
        raise RuntimeError('Preserved a modified renderer binary')
    if not executable.exists():
        atomic(executable, payload, 0o755)
    quote = shlex.quote
    guard = ' && '.join('[ "$(' + shlex.join(['pacman', '-Q', name]) + ' 2>/dev/null)" = ' + quote(value) + ' ]'
                        for name, value in packages.items())
    text = ('#!/bin/sh\n# Managed by cosmic-forge renderer\n'
            'if [ "${COSMIC_FORGE_DISABLE:-0}" != 1 ] && ' + guard +
            ' && [ -x ' + quote(str(executable)) + ' ]; then\n'
            '    exec ' + quote(str(executable)) + ' "$@"\nfi\n'
            'exec ' + quote(str(fallback) if previous['data'] is not None else '/usr/bin/cosmic-comp') + ' "$@"\n')
    updated = {'abi': 1, 'renderer_revision': json.loads((ROOT / 'release.json').read_text())['renderer_revision'],
               'cosmic_version': version, 'destination': str(destination),
               'launcher': str(launcher), 'launcher_sha256': digest(text.encode()),
               'binary_sha256': digest(payload), 'packages': packages, 'previous': previous}
    try:
        atomic(launcher, text.encode(), 0o755)
        atomic(record_file, (json.dumps(updated, indent=2) + '\n').encode())
    except OSError:
        if current is None:
            launcher.unlink(missing_ok=True)
        else:
            atomic(launcher, current, current_mode)
        raise
    print('Glass renderer installed. Sign out and back in to activate it.', flush=True)
    progress('complete')


def restore(prefix=None):
    progress('restoring')
    _, state, launcher = paths(prefix)
    record_file, record, current = record_for(state, launcher)
    current_mode = launcher.stat().st_mode & 0o777 if current is not None else 0o755
    if not SYSTEM_COMPOSITOR.is_file() or not os.access(SYSTEM_COMPOSITOR, os.X_OK):
        raise RuntimeError('The original COSMIC compositor is unavailable; reinstall the cosmic-comp system package first')
    with SYSTEM_COMPOSITOR.open('rb') as binary:
        if binary.read(4) != b'\x7fELF':
            raise RuntimeError('Expected the original native COSMIC system compositor; preserved the current launcher')
    try:
        if current is not None:
            launcher.unlink()
        if record:
            record_file.unlink()
    except OSError:
        if current is not None:
            atomic(launcher, current, current_mode)
        raise
    print('Original COSMIC compositor restored. Sign out and back in to activate it.', flush=True)
    progress('complete')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--restore', action='store_true')
    parser.add_argument('--binary', type=Path, help='Install an already built matching ABI 1 renderer')
    parser.add_argument('--prefix', type=Path, help='Private installation prefix for testing')
    args = parser.parse_args()
    if args.prefix:
        args.prefix = args.prefix.resolve()
    progress('checking')
    _, state, _ = paths(args.prefix)
    state.mkdir(parents=True, exist_ok=True)
    with (state / 'cosmic-forge-engine.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if args.restore:
            restore(args.prefix)
            return
        packages, version = installed_packages()
        # Check launcher ownership before starting a potentially long build.
        _, _, launcher = paths(args.prefix)
        _, record, _ = record_for(state, launcher)
        if args.binary is None and already_installed(record, packages, version, args.prefix):
            report_already_installed()
            return
        binary = args.binary
        if binary is None:
            work = ROOT / 'build/sources' / version
            if work.exists() and not (work / '.forge-source.json').exists():
                work.rename(work.with_name(version + '-incomplete-' + str(time.time_ns())))
            command = ['python3', str(ROOT / 'build.py'), '--cosmic-version', version,
                       '--work-dir', str(work)]
            if os.environ.get('COSMIC_FORGE_ENGINE_TARGET'):
                command.extend(['--target-dir', os.environ['COSMIC_FORGE_ENGINE_TARGET']])
            subprocess.run(command, check=True)
            binary = ROOT / 'build/bin/cosmic-comp'
        current_packages, current_version = installed_packages()
        if (packages, version) != (current_packages, current_version):
            raise RuntimeError('COSMIC packages changed during the build; run installation again')
        install(binary.resolve(), packages, version, args.prefix)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error))
