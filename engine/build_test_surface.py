#!/usr/bin/env python3
"""Build the optional native Wayland render-test fixture."""
import os
from pathlib import Path
import shlex
import subprocess

root = Path(__file__).resolve().parent
cargo = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
protocols = sorted((cargo / 'registry/src').glob('*/wayland-protocols-0.32.*/protocols'))
if not protocols:
    raise SystemExit('Build the Rust sources first to populate the protocol cache')
output = root / 'build/test-protocols'
output.mkdir(parents=True, exist_ok=True)
for name, xml in [('xdg-shell', 'stable/xdg-shell/xdg-shell.xml'),
                  ('background-effect', 'staging/ext-background-effect/ext-background-effect-v1.xml')]:
    for mode, suffix in [('client-header', 'client.h'), ('private-code', 'protocol.c')]:
        subprocess.run(['wayland-scanner', mode, str(protocols[-1] / xml), str(output / (name + '-' + suffix))], check=True)
flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'wayland-client', 'cairo'], text=True))
binary = root / 'build/bin/test-surface'
binary.parent.mkdir(parents=True, exist_ok=True)
subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-I' + str(output),
                str(root / 'tests/surface.c'), str(output / 'xdg-shell-protocol.c'),
                str(output / 'background-effect-protocol.c'), '-o', str(binary) + '.new', *flags], check=True)
os.replace(str(binary) + '.new', binary)
