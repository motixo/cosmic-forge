#!/usr/bin/env python3
"""Build the glass renderer separately; never install or restart the desktop."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent


def progress(stage):
    print('FORGE_PROGRESS:' + stage, flush=True)


def prepare(source, version, local=None):
    release = json.loads((ROOT / 'release.json').read_text())
    pin = release['sources'][version]
    files = [ROOT / 'patches/glass.patch', ROOT / 'patches/runtime.patch',
             ROOT / 'runtime.rs', ROOT / 'loader.rs']
    expected = {'commit': pin, 'sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in files}}
    marker = source / '.forge-source.json'
    if marker.exists():
        if json.loads(marker.read_text()) != expected:
            raise RuntimeError('Renderer source changed: use a fresh --work-dir')
        return
    if source.exists():
        raise RuntimeError('Refusing to modify an existing unmarked source directory')
    source.parent.mkdir(parents=True, exist_ok=True)
    if local:
        subprocess.run(['git', 'clone', '--shared', '--no-checkout', str(local), str(source)], check=True)
        subprocess.run(['git', 'checkout', '--detach', pin], cwd=source, check=True)
    else:
        subprocess.run(['git', 'init', '-q', str(source)], check=True)
        subprocess.run(['git', 'fetch', '--depth', '1', release['upstream'], pin], cwd=source, check=True)
        subprocess.run(['git', 'checkout', '--detach', 'FETCH_HEAD'], cwd=source, check=True)
    for patch in files[:2]:
        subprocess.run(['git', 'apply', '--check', str(patch)], cwd=source, check=True)
        subprocess.run(['git', 'apply', str(patch)], cwd=source, check=True)
    destination = source / 'src/backend/render/wayland'
    shutil.copy2(ROOT / 'runtime.rs', destination / 'forge_runtime.rs')
    shutil.copy2(ROOT / 'loader.rs', destination / 'forge_loader.rs')
    (source / 'build.rs').write_text(f'fn main() {{ println!("cargo:rustc-env=GIT_HASH={pin}"); }}\n')
    marker.write_text(json.dumps(expected, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cosmic-version', choices=['1.10.0', '1.11.0'], required=True)
    parser.add_argument('--source', type=Path, help='Use an existing upstream Git checkout as a local clone source')
    parser.add_argument('--work-dir', type=Path)
    parser.add_argument('--target-dir', type=Path)
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--jobs', type=int, default=3)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    work = (args.work_dir or ROOT.parent / 'build/engine-sources' / args.cosmic_version).resolve()
    progress('preparing')
    prepare(work, args.cosmic_version, args.source)
    if args.prepare_only:
        return
    target = (args.target_dir or ROOT.parent / 'build/engine-target' / args.cosmic_version).resolve()
    command = ['cargo', 'build', '--release', '--locked', '--bin', 'cosmic-comp', '-j', str(args.jobs)]
    if args.offline:
        command.append('--offline')
    progress('building')
    subprocess.run(command, cwd=work, env=dict(os.environ, CARGO_TARGET_DIR=str(target),
                   CARGO_PROFILE_RELEASE_LTO='thin'), check=True)
    output = ROOT / 'build/bin'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=output, delete=False) as temporary:
        pending = Path(temporary.name)
    try:
        shutil.copy2(target / 'release/cosmic-comp', pending)
        os.replace(pending, output / 'cosmic-comp')
    finally:
        pending.unlink(missing_ok=True)
    print('Renderer built: ' + str(output / 'cosmic-comp'))


if __name__ == '__main__':
    main()
