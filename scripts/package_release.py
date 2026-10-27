#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Package a portable Linux build and native DEB, RPM and Arch packages."""
import argparse
import gzip
import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version']
APP_ID = 'io.github.motixo.CosmicForge'
ARCHES = {'x86_64': ('amd64', 62), 'aarch64': ('arm64', 183)}
EPOCH = int(os.environ.get('SOURCE_DATE_EPOCH', time.time()))
DESKTOP = f'''[Desktop Entry]
Type=Application
Name=COSMIC Forge
Name[fa]=کازمیک فورج
Comment=Mod manager for the COSMIC desktop
Exec=cosmic-forge
Icon={APP_ID}
Terminal=false
Categories=Settings;DesktopSettings;
Keywords=cosmic;mods;glass;panel;wallpaper;
StartupNotify=true
StartupWMClass={APP_ID}
'''


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def copy(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def archive(path, prefix, files):
    with tarfile.open(path, 'w:xz') as tar:
        for name, source in sorted(files.items()):
            info = tar.gettarinfo(source, arcname=f'{prefix}/{name}' if prefix else name)
            info.uid = info.gid = 0
            info.uname = info.gname = 'root'
            info.mtime = EPOCH
            with source.open('rb') as stream:
                tar.addfile(info, stream)


def portable(binary, output, arch):
    files = {name: ROOT/name for name in [
        'Cargo.toml', 'LICENSE', 'README.md', 'scripts/install.py',
        f'resources/icons/{APP_ID}.svg',
    ]}
    files['bin/cosmic-forge'] = binary
    archive(output/f'cosmic-forge-{VERSION}-linux-{arch}.tar.xz', f'cosmic-forge-{VERSION}', files)


def payload(directory, binary):
    copy(binary, directory/'usr/bin/cosmic-forge', True)
    icon = f'resources/icons/{APP_ID}.svg'
    copy(ROOT/icon, directory/f'usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg')
    desktop = directory/f'usr/share/applications/{APP_ID}.desktop'
    desktop.parent.mkdir(parents=True)
    desktop.write_text(DESKTOP)
    copy(ROOT/'LICENSE', directory/'usr/share/licenses/cosmic-forge/LICENSE')


def deb(binary, output, arch, temporary):
    stage = temporary/'deb'
    payload(stage, binary)
    copy(ROOT/'LICENSE', stage/'usr/share/doc/cosmic-forge/copyright')
    control = stage/'DEBIAN/control'
    control.parent.mkdir()
    control.write_text(f'''Package: cosmic-forge
Version: {VERSION}
Section: utils
Priority: optional
Architecture: {ARCHES[arch][0]}
Maintainer: motixo <motixo@users.noreply.github.com>
Homepage: https://github.com/motixo/cosmic-forge
Depends: libc6 (>= 2.35), libgcc-s1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-egl1, libegl1, libfontconfig1, libx11-6, libx11-xcb1, libxcb1, libxi6, python3 (>= 3.11), curl
Recommends: libvulkan1, xdg-desktop-portal
Description: Native mod manager for the COSMIC desktop
 Manage glass effects, panel and dock mods, and lock screen mods.
''')
    (stage/'DEBIAN/md5sums').write_text(''.join(
        hashlib.md5(p.read_bytes(), usedforsecurity=False).hexdigest() + '  ' + str(p.relative_to(stage)) + '\n'
        for p in sorted((stage/'usr').rglob('*')) if p.is_file()
    ))
    run('dpkg-deb', '--build', '--root-owner-group', str(stage), str(output/f'cosmic-forge_{VERSION}_{ARCHES[arch][0]}.deb'))


def rpm(binary, output, arch, temporary):
    top = temporary/'rpm'
    stage = top/'payload'
    payload(stage, binary)
    spec = top/'cosmic-forge.spec'
    spec.write_text(f'''Name: cosmic-forge
Version: {VERSION}
Release: 1
Summary: Native mod manager for the COSMIC desktop
License: GPL-3.0-only
URL: https://github.com/motixo/cosmic-forge
BuildArch: {arch}
Requires: glibc >= 2.35, libgcc, libxkbcommon, libxkbcommon-x11, libwayland-client, libwayland-egl, libglvnd-egl, fontconfig, libX11, libX11-xcb, libxcb, libXi, python3 >= 3.11, curl
Recommends: vulkan-loader, xdg-desktop-portal
%global debug_package %{{nil}}
%global __os_install_post %{{nil}}
%description
Manage glass effects, panel and dock mods, and lock screen mods.
%install
mkdir -p %{{buildroot}}
cp -a {stage}/usr %{{buildroot}}/
%files
/usr/bin/cosmic-forge
/usr/share/applications/{APP_ID}.desktop
/usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg
%license /usr/share/licenses/cosmic-forge/LICENSE
''')
    run('rpmbuild', '-bb', '--define', f'_topdir {top}', '--define', f'_rpmdir {output}', str(spec))
    for package in (output/arch).glob('*.rpm'):
        package.replace(output/package.name)
    (output/arch).rmdir()


def arch_package(binary, output, arch, temporary):
    stage = temporary/'arch'
    payload(stage, binary)
    size = sum(p.stat().st_size for p in stage.rglob('*') if p.is_file())
    (stage/'.PKGINFO').write_text(f'''pkgname = cosmic-forge
pkgbase = cosmic-forge
pkgver = {VERSION}-1
pkgdesc = Native mod manager for the COSMIC desktop
url = https://github.com/motixo/cosmic-forge
builddate = {EPOCH}
packager = motixo
size = {size}
arch = {arch}
license = GPL-3.0-only
depend = glibc>=2.35
depend = gcc-libs
depend = libxkbcommon
depend = libxkbcommon-x11
depend = wayland
depend = libglvnd
depend = fontconfig
depend = libx11
depend = libxcb
depend = libxi
depend = python>=3.11
depend = curl
optdepend = vulkan-icd-loader: Vulkan rendering
optdepend = xdg-desktop-portal: file chooser integration
''')
    mtree = subprocess.check_output(['bsdtar', '-cf', '-', '--format=mtree', '--options=!all,use-set,type,uid,gid,mode,time,size,sha256,link', '--uid', '0', '--gid', '0', '-C', str(stage), '.'])
    (stage/'.MTREE').write_bytes(gzip.compress(mtree, mtime=0))
    run('bsdtar', '--zstd', '--uid', '0', '--gid', '0', '-cf', str(output/f'cosmic-forge-{VERSION}-1-{arch}.pkg.tar.zst'), '-C', str(stage), '.PKGINFO', '.MTREE', 'usr')


def source(output):
    names = subprocess.check_output(['git', 'ls-files', '--recurse-submodules'], cwd=ROOT, text=True).splitlines()
    files = {}
    for name in names:
        path = Path(name)
        if path.name in ('AGENTS.md', 'SKILL.md') or any(p in ('.agents', '.codex', 'skills') for p in path.parts):
            continue
        if (ROOT/path).is_file() and not (ROOT/path).is_symlink():
            files[name] = ROOT/path
    archive(output/f'cosmic-forge-{VERSION}-source.tar.xz', f'cosmic-forge-{VERSION}', files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/cosmic-forge')
    parser.add_argument('--output', type=Path, default=ROOT/'build/release')
    parser.add_argument('--source-only', action='store_true')
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.source_only:
        source(args.output)
        return
    arch = platform.machine()
    if arch not in ARCHES:
        raise SystemExit('Unsupported architecture: ' + arch)
    header = args.binary.read_bytes()[:20]
    if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != ARCHES[arch][1]:
        raise SystemExit('Expected a native 64-bit Linux executable')
    versions = re.findall(r'GLIBC_(\d+)\.(\d+)', subprocess.check_output(['readelf', '--version-info', str(args.binary)], text=True))
    if not versions or max(tuple(map(int, v)) for v in versions) > (2, 35):
        raise SystemExit('Build the release on Ubuntu 22.04 (glibc 2.35), not the host system')
    version = subprocess.check_output([str(args.binary.resolve()), '--version'], text=True).strip()
    if version != f'COSMIC Forge {VERSION} (Rust / libcosmic)':
        raise SystemExit('The executable version does not match Cargo.toml')
    with tempfile.TemporaryDirectory(prefix='forge-package-') as directory:
        temporary = Path(directory)
        binary = temporary/'cosmic-forge'
        copy(args.binary, binary, True)
        run('strip', '--strip-unneeded', str(binary))
        portable(binary, args.output, arch)
        deb(binary, args.output, arch, temporary)
        rpm(binary, args.output, arch, temporary)
        if arch == 'x86_64':
            arch_package(binary, args.output, arch, temporary)
    print('Packages:', args.output)


if __name__ == '__main__':
    main()
