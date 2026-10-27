#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Install or remove the current user's native COSMIC Forge launcher."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import tomllib
ROOT = Path(__file__).resolve().parents[1]
APP_ID = 'io.github.motixo.CosmicForge'
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
DEFAULT_BINARY = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'release/cosmic-forge'
MANAGED_FILES = (
    'bin/cosmic-forge',
    f'share/applications/{APP_ID}.desktop',
    f'share/icons/hicolor/scalable/apps/{APP_ID}.svg',
)

def atomic(path, data, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.' + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data.encode() if isinstance(data, str) else data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)

def read_record(prefix):
    path = prefix / 'state/cosmic-forge/installation.json'
    if path.is_symlink():
        raise RuntimeError('Preserved a replaced installation record: ' + str(path))
    if not path.exists():
        return (path, None)
    record = json.loads(path.read_text())
    allowed = {str(prefix / name) for name in MANAGED_FILES}
    files = record.get('files') if isinstance(record, dict) else None
    if not isinstance(files, dict) or any(
        name not in allowed or not isinstance(digest, str)
        or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest)
        for name, digest in files.items()
    ):
        raise RuntimeError('Invalid installation record: ' + str(path))
    return (path, record)

def check_managed(files):
    for location, expected in files.items():
        path = Path(location)
        if path.is_symlink() or (path.exists() and hashlib.sha256(path.read_bytes()).hexdigest() != expected):
            raise RuntimeError('Preserved a modified file: ' + str(path))

def install(prefix, binary):
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise RuntimeError('Build the Rust app first: cargo build --release --locked')
    binary_data = binary.read_bytes()
    if binary_data[:4] != b'\x7fELF':
        raise RuntimeError('Expected a native COSMIC Forge executable')
    version_text = subprocess.run([str(binary), '--version'], check=True, capture_output=True, text=True).stdout.strip()
    if version_text != f'COSMIC Forge {VERSION} (Rust / libcosmic)':
        raise RuntimeError(f'Expected COSMIC Forge {VERSION}; rebuild from this checkout')
    digest = hashlib.sha256(binary_data).hexdigest()
    destination = prefix / 'share/cosmic-forge/versions' / digest[:16]
    launcher, desktop, icon = (prefix / name for name in MANAGED_FILES)
    record_file, record = read_record(prefix)
    expected = record['files'] if record else {}
    for path in (launcher, desktop, icon):
        if (path.exists() or path.is_symlink()) and str(path) not in expected:
            raise RuntimeError('Preserved a file owned by another installation: ' + str(path))
    check_managed(expected)
    installed_binary = destination / 'cosmic-forge'
    if installed_binary.exists() or installed_binary.is_symlink():
        check_managed({str(installed_binary): digest})
    else:
        atomic(installed_binary, binary_data, 0o755)
    launcher_text = '#!/bin/sh\n# Managed by cosmic-forge\n' + f'exec {shlex.quote(str(installed_binary))} "$@"\n'
    # Desktop Exec uses desktop-entry quoting, rather than shell parsing.
    executable = '"' + str(launcher).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'
    desktop_text = (
        '[Desktop Entry]\nType=Application\nName=COSMIC Forge\n'
        'Name[fa]=کازمیک فورج\nComment=Choose and fine tune your desktop effects\n'
        f'Exec={executable}\nIcon={APP_ID}\nTerminal=false\n'
        'Categories=Settings;DesktopSettings;\nKeywords=glass;crystal;aero;shader;effects;\n'
        f'StartupNotify=true\nStartupWMClass={APP_ID}\n'
    )
    payload = {
        launcher: launcher_text.encode(),
        desktop: desktop_text.encode(),
        icon: (ROOT / 'resources/icons' / f'{APP_ID}.svg').read_bytes(),
    }
    payload[record_file] = (json.dumps({
        'version': VERSION, 'runtime': 'Rust / libcosmic', 'destination': str(destination),
        'files': {str(path): hashlib.sha256(data).hexdigest() for path, data in payload.items()},
    }, indent=2) + '\n').encode()
    previous = {
        path: (path.read_bytes(), path.stat().st_mode & 0o777) if path.exists() else None
        for path in payload
    }
    completed = []
    try:
        for path, data in payload.items():
            atomic(path, data, 0o755 if path == launcher else 0o644)
            completed.append(path)
    except OSError:
        for path in reversed(completed):
            before = previous[path]
            if before is None:
                path.unlink(missing_ok=True)
            else:
                atomic(path, before[0], before[1])
        raise
    if shutil.which('update-desktop-database'):
        subprocess.run(['update-desktop-database', str(desktop.parent)], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    print('Installed COSMIC Forge: ' + str(launcher))

def uninstall(prefix):
    record_file, record = read_record(prefix)
    if record is None:
        print('COSMIC Forge is not installed.')
        return
    check_managed(record['files'])
    for location in record['files']:
        Path(location).unlink(missing_ok=True)
    record_file.unlink()
    print('Removed COSMIC Forge launchers. Desktop effect preferences and backups were retained.')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--uninstall', action='store_true')
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--prefix', type=Path, default=Path.home() / '.local')
    args = parser.parse_args()
    try:
        prefix = args.prefix.resolve()
        if '\n' in str(prefix) or '\r' in str(prefix):
            raise RuntimeError('Installation paths cannot contain line breaks')
        if args.uninstall:
            uninstall(prefix)
        else:
            install(prefix, args.binary.resolve())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + '\n')
if __name__ == '__main__':
    main()
