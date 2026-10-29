#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Download and verify the latest Forge release; install for the current user."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

REPO = 'motixo/cosmic-forge'
API = f'https://api.github.com/repos/{REPO}/releases/latest'
ICON = 'io.github.motixo.CosmicForge.svg'


def download(url, path, maximum):
    request = urllib.request.Request(url, headers={'User-Agent': 'cosmic-forge-installer', 'Accept': 'application/vnd.github+json' if url == API else 'application/octet-stream'})
    with urllib.request.urlopen(request, timeout=120) as response, path.open('wb') as stream:
        size = 0
        while chunk := response.read(1024 * 1024):
            size += len(chunk)
            if size > maximum:
                raise RuntimeError('Download exceeds its size limit')
            stream.write(chunk)


def asset(release, arch):
    if not isinstance(release, dict):
        raise RuntimeError('Invalid release metadata')
    tag = release.get('tag_name', '')
    if release.get('draft') or release.get('prerelease') or not isinstance(tag, str) or not re.fullmatch(r'v\d+\.\d+\.\d+', tag):
        raise RuntimeError('Expected a stable Forge release')
    version = tag[1:]
    name = f'cosmic-forge-{version}-linux-{arch}.tar.xz'
    assets = release.get('assets')
    if not isinstance(assets, list):
        raise RuntimeError('Invalid release assets')
    found = next((a for a in assets if isinstance(a, dict) and a.get('name') == name), None)
    if found is None:
        raise RuntimeError('No prebuilt release for ' + arch)
    if found.get('browser_download_url') != f'https://github.com/{REPO}/releases/download/{tag}/{name}':
        raise RuntimeError('Unexpected download repository or filename')
    if type(found.get('size')) is not int or not 0 < found['size'] <= 256 * 1024 * 1024:
        raise RuntimeError('Invalid release size')
    if not isinstance(found.get('digest'), str) or not re.fullmatch(r'sha256:[a-f0-9]{64}', found['digest']):
        raise RuntimeError('Missing release SHA-256 digest')
    return version, found


def extract(archive, destination, version):
    prefix = 'cosmic-forge-' + version
    allowed = {'bin/cosmic-forge', 'scripts/install.py', 'Cargo.toml', 'LICENSE', 'README.md', f'resources/icons/{ICON}'}
    with tarfile.open(archive, 'r|xz') as tar:
        names = set()
        size = 0
        for member in tar:
            path = PurePosixPath(member.name)
            relative = str(PurePosixPath(*path.parts[1:]))
            if path.is_absolute() or not path.parts or path.parts[0] != prefix or '..' in path.parts or relative not in allowed or relative in names or not member.isfile():
                raise RuntimeError('Unsafe release archive path or file type')
            size += member.size
            if size > 256 * 1024 * 1024:
                raise RuntimeError('Unpacked release exceeds its size limit')
            names.add(relative)
            path = destination/member.name
            path.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as source, path.open('wb') as target:
                while chunk := source.read(1024 * 1024):
                    target.write(chunk)
            path.chmod(0o755 if path.name == 'cosmic-forge' else 0o644)
        if names != allowed:
            raise RuntimeError('Incomplete release archive')
    return destination/prefix


def install(prefix, uninstall=False):
    if sys.version_info < (3, 11):
        raise RuntimeError('Python 3.11 or newer is required')
    if os.geteuid() == 0:
        raise RuntimeError('Run this installer as your normal user, without sudo')
    distro = platform.freedesktop_os_release()
    ids = {distro.get('ID', ''), *distro.get('ID_LIKE', '').split()}
    if not ids.intersection({'arch', 'fedora', 'ubuntu', 'pop'}):
        raise RuntimeError('Supported systems: Arch Linux, Fedora, Ubuntu and Pop!_OS')
    arch = {'x86_64': 'x86_64', 'aarch64': 'aarch64', 'arm64': 'aarch64'}.get(platform.machine())
    if arch is None:
        raise RuntimeError('Supported architectures: x86_64 and ARM64')
    libc = os.confstr('CS_GNU_LIBC_VERSION') or ''
    if not libc.startswith('glibc ') or tuple(map(int, libc.split()[1].split('.'))) < (2, 35):
        raise RuntimeError('glibc 2.35 or newer is required')
    with tempfile.TemporaryDirectory(prefix='cosmic-forge-install-') as directory:
        temporary = Path(directory)
        metadata = temporary/'release.json'
        print('Checking the latest COSMIC Forge release…', flush=True)
        download(API, metadata, 1024 * 1024)
        version, info = asset(json.loads(metadata.read_text()), arch)
        archive = temporary/'release.tar.xz'
        print(f'Downloading COSMIC Forge {version} for {arch}…', flush=True)
        download(info['browser_download_url'], archive, info['size'])
        if archive.stat().st_size != info['size'] or 'sha256:' + hashlib.sha256(archive.read_bytes()).hexdigest() != info['digest']:
            raise RuntimeError('Release checksum or size mismatch; nothing was installed')
        root = extract(archive, temporary/'unpacked', version)
        binary = root/'bin/cosmic-forge'
        check = subprocess.run([str(binary), '--version'], capture_output=True, text=True)
        if check.returncode or check.stdout.strip() != f'COSMIC Forge {version} (Rust / libcosmic)':
            raise RuntimeError('This binary cannot run. Install COSMIC runtime libraries (libxkbcommon, Wayland, EGL and fontconfig), then retry.\n' + check.stderr.strip())
        command = [sys.executable, str(root/'scripts/install.py'), '--binary', str(binary), '--prefix', str(prefix)]
        if uninstall:
            command.append('--uninstall')
        subprocess.run(command, check=True)
    if not uninstall:
        print(f'Launch COSMIC Forge from your app menu or run {prefix}/bin/cosmic-forge. Install the shader compositor from Settings → Patches.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, default=Path.home()/'.local')
    parser.add_argument('--uninstall', action='store_true')
    args = parser.parse_args()
    try:
        install(args.prefix.resolve(), args.uninstall)
    except (OSError, ValueError, RuntimeError, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()
