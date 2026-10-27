#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the Rust/libcosmic window through XTest in private XDG directories."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import time
from PIL import ImageGrab
from Xlib import X, XK, display
from Xlib.ext import xtest
ROOT = Path(__file__).resolve().parents[1]

class Session:

    def __init__(self, binary, path=None, with_engine=True, with_panel=False, with_greeter=False):
        self.app = self.xvfb = self.log = None
        self.display = None
        try:
            self.start(binary, path, with_engine, with_panel, with_greeter)
        except BaseException:
            self.close()
            raise

    def start(self, binary, path=None, with_engine=True, with_panel=False, with_greeter=False):
        self.path = path or ROOT / 'build/qa-forge-native'
        self.path.mkdir(parents=True, exist_ok=True)
        (self.path / 'screenshots').mkdir(exist_ok=True)
        self.config = self.path / 'config'
        if self.config.exists():
            shutil.rmtree(self.config)
        for source in (Path.home() / '.config/cosmic').glob('com.system76.CosmicTheme.*'):
            if source.is_dir():
                shutil.copytree(source, self.config / 'cosmic' / source.name)
        mode = self.config / 'cosmic/com.system76.CosmicTheme.Mode/v1/is_dark'
        mode.parent.mkdir(parents=True, exist_ok=True)
        mode.write_text('false\n')
        if (self.path / 'state').exists():
            shutil.rmtree(self.path / 'state')
        if (self.path / 'data').exists():
            shutil.rmtree(self.path / 'data')
        packs = self.path / 'data/cosmic-forge'
        packs.mkdir(parents=True)
        (packs / 'last-check').write_text(str(int(time.time())))
        (packs / 'mods').mkdir()
        (packs / 'mods/last-check').write_text(str(int(time.time())))
        self.comp = self.config / 'cosmic/com.system76.CosmicComp/v1'
        self.comp.mkdir(parents=True)
        self.put('glass_effect', '"aero"' if with_engine else '"frosted"')
        for key, value in [('aero_blur_amount', 45), ('aero_color', 55), ('aero_reflections', 65)]:
            self.put(key, str(value))
        runtime = self.path / 'runtime'
        runtime.mkdir(exist_ok=True, mode=0o700)
        home = self.path / 'home'
        home.mkdir(exist_ok=True)
        if with_engine:
            launcher = home / '.local/bin/cosmic-comp'
            launcher.parent.mkdir(parents=True, exist_ok=True)
            launcher.write_bytes(b'#!/bin/sh\n# UI test renderer fixture\nexit 0\n')
            destination = self.path / 'data/cosmic-forge/engine/versions/ui-fixture'
            destination.mkdir(parents=True)
            engine = destination / 'cosmic-comp'
            engine.write_bytes(b'UI test renderer fixture')
            engine.chmod(0o755)
            state = self.path / 'state/cosmic-forge'
            state.mkdir(parents=True)
            (state / 'engine-installation.json').write_text(json.dumps({
                'abi': 1, 'destination': str(destination), 'launcher': str(launcher),
                'launcher_sha256': hashlib.sha256(launcher.read_bytes()).hexdigest(),
            }))
        if with_panel:
            import sys
            sys.path.insert(0, str(ROOT / 'engine'))
            from platforms import package_manager, package_query
            manager = package_manager()
            package = subprocess.check_output(package_query(manager, 'cosmic-panel'), text=True).strip()
            profile = self.config / 'cosmic/com.system76.CosmicPanel.Dock/v1'
            profile.mkdir(parents=True)
            (profile / 'anchor_gap').write_text('true\n')
            launcher = home / '.local/bin/cosmic-panel'
            launcher.parent.mkdir(parents=True, exist_ok=True)
            launcher.write_bytes(b'#!/bin/sh\n# Private UI panel fixture\nexit 0\n')
            destination = self.path / 'data/cosmic-forge/panel/versions/0123456789abcdef'
            destination.mkdir(parents=True)
            binary_fixture = destination / 'cosmic-panel'
            binary_fixture.write_bytes(b'\x7fELFforge-panel-engine-v1 UI fixture')
            binary_fixture.chmod(0o755)
            state = self.path / 'state/cosmic-forge'
            state.mkdir(parents=True, exist_ok=True)
            (state / 'panel-installation.json').write_text(json.dumps({
                'abi': 1, 'revision': 1, 'destination': str(destination), 'launcher': str(launcher),
                'launcher_sha256': hashlib.sha256(launcher.read_bytes()).hexdigest(),
                'binary_sha256': hashlib.sha256(binary_fixture.read_bytes()).hexdigest(),
                'package_manager': manager, 'packages': {'cosmic-panel': package},
            }))
        if with_greeter:
            import sys
            sys.path.insert(0, str(ROOT / 'engine'))
            from platforms import package_manager, package_query
            manager = package_manager()
            package = subprocess.check_output(package_query(manager, 'cosmic-greeter'), text=True).strip()
            launcher = home / '.local/bin/cosmic-greeter'
            launcher.parent.mkdir(parents=True, exist_ok=True)
            launcher.write_bytes(b'#!/bin/sh\n# Private UI locker fixture\nexit 0\n')
            destination = self.path / 'data/cosmic-forge/greeter/versions/0123456789abcdef'
            destination.mkdir(parents=True)
            binary_fixture = destination / 'cosmic-greeter'
            binary_fixture.write_bytes(b'\x7fELFforge-lockscreen-engine-v1 UI fixture')
            binary_fixture.chmod(0o755)
            state = self.path / 'state/cosmic-forge'
            state.mkdir(parents=True, exist_ok=True)
            (state / 'greeter-installation.json').write_text(json.dumps({
                'abi': 1, 'revision': 1, 'destination': str(destination), 'launcher': str(launcher),
                'launcher_sha256': hashlib.sha256(launcher.read_bytes()).hexdigest(),
                'binary_sha256': hashlib.sha256(binary_fixture.read_bytes()).hexdigest(),
                'package_manager': manager, 'packages': {'cosmic-greeter': package},
            }))
        self.log = (self.path / 'app.log').open('w')
        self.xvfb = subprocess.Popen(['Xvfb', '-displayfd', '1', '-screen', '0', '1100x1000x24', '-nolisten', 'tcp', '-ac'], stdout=subprocess.PIPE, stderr=self.log, text=True)
        self.screen = ':' + self.xvfb.stdout.readline().strip()
        self.xvfb.stdout.close()
        self.display = display.Display(self.screen)
        env = dict(
            os.environ, DISPLAY=self.screen, HOME=str(home),
            XDG_CONFIG_HOME=str(self.config), XDG_DATA_HOME=str(self.path / 'data'),
            XDG_STATE_HOME=str(self.path / 'state'), XDG_CACHE_HOME=str(self.path / 'cache'),
            XDG_RUNTIME_DIR=str(runtime), WINIT_UNIX_BACKEND='x11',
            COSMIC_SINGLE_INSTANCE='false', LIBGL_ALWAYS_SOFTWARE='1', ICED_BACKEND='wgpu',
        )
        for key in ['WAYLAND_DISPLAY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_ACTIVATION_TOKEN', 'DESKTOP_STARTUP_ID']:
            env.pop(key, None)
        self.environment = env
        self.binary = binary
        self.app = subprocess.Popen(['dbus-run-session', '--', str(binary)], env=env, stdout=self.log, stderr=self.log)
        self.window = None
        for _ in range(150):
            for window in self.display.screen().root.query_tree().children:
                if window.get_wm_name() == 'COSMIC Forge':
                    self.window = window
                    break
            if self.window:
                break
            if self.app.poll() is not None:
                raise RuntimeError((self.path / 'app.log').read_text())
            time.sleep(0.1)
        if self.window is None:
            raise RuntimeError('Native window did not appear')
        time.sleep(2)
        self.window.set_input_focus(X.RevertToParent, X.CurrentTime)
        self.display.sync()
        (self.path / 'session.json').write_text(json.dumps({'display': self.screen, 'window': self.window.id, 'pid': self.app.pid}))

    def put(self, key, value):
        (self.comp / key).write_text(value + '\n')

    def get(self, key):
        return (self.comp / key).read_text().strip() if (self.comp / key).exists() else None

    def opacity(self):
        path = self.config / 'cosmic/com.system76.CosmicTheme.Light/v2/alpha_map'
        if not path.exists():
            return 50
        data = path.read_text()
        low = float(re.search('extremely_low:\\s*([0-9.eE+-]+)', data).group(1))
        high = float(re.search('extremely_high_2:\\s*([0-9.eE+-]+)', data).group(1))
        amount = 0.5 * (low - 0.9) / 0.1 + 0.5 if low > 0.9 else 0.5 - 0.5 * (0.6 - high) / 0.6 if high < 0.6 else 0.5
        return round(amount * 100)

    def click(self, x, y, wait=0.45):
        xtest.fake_input(self.display, X.MotionNotify, x=x, y=y)
        xtest.fake_input(self.display, X.ButtonPress, 1)
        xtest.fake_input(self.display, X.ButtonRelease, 1)
        self.display.sync()
        time.sleep(wait)

    def key(self, key, ctrl=False, wait=0.5):
        self.keycode(self.display.keysym_to_keycode(XK.string_to_keysym(key)), ctrl, wait)

    def keycode(self, code, ctrl=False, wait=0.5):
        control = self.display.keysym_to_keycode(XK.string_to_keysym('Control_L'))
        if ctrl:
            xtest.fake_input(self.display, X.KeyPress, control)
        xtest.fake_input(self.display, X.KeyPress, code)
        xtest.fake_input(self.display, X.KeyRelease, code)
        if ctrl:
            xtest.fake_input(self.display, X.KeyRelease, control)
        self.display.sync()
        time.sleep(wait)

    def capture(self, name):
        geom = self.window.get_geometry()
        image = ImageGrab.grab(xdisplay=self.screen).crop((0, 0, geom.width, geom.height))
        image.save(self.path / 'screenshots' / f'{name}.png')
        return image

    def select_effect(self, index):
        self.click(100, 112 + 40 * index)

    def scroll_to_top(self):
        xtest.fake_input(self.display, X.MotionNotify, x=750, y=650)
        for _ in range(30):
            xtest.fake_input(self.display, X.ButtonPress, 4)
            xtest.fake_input(self.display, X.ButtonRelease, 4)
        self.display.sync()
        time.sleep(0.2)

    def scroll_to_bottom(self):
        xtest.fake_input(self.display, X.MotionNotify, x=750, y=650)
        for _ in range(20):
            xtest.fake_input(self.display, X.ButtonPress, 5)
            xtest.fake_input(self.display, X.ButtonRelease, 5)
        self.display.sync()
        time.sleep(0.2)

    def close(self):
        if self.app and self.app.poll() is None:
            if self.display:
                subprocess.run(['setxkbmap', '-display', self.screen, '-layout', 'us'], check=False)
                self.key('q', True)
            try:
                self.app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.app.terminate()
                self.app.wait(timeout=5)
        if self.display:
            self.display.close()
        if self.xvfb:
            self.xvfb.stdout.close()
            self.xvfb.terminate()
            self.xvfb.wait(timeout=5)
        if self.log:
            self.log.close()

def exercise(session):
    results = []

    def check(name, condition):
        results.append({'test': name, 'passed': bool(condition)})
        print(('PASS ' if condition else 'FAIL ') + name, flush=True)
        (session.path / 'results.json').write_text(json.dumps(results, indent=2))
        if not condition:
            raise AssertionError(name)
    check('Native window opens without changing preferences', session.get('glass_effect') == '"aero"' and (not (session.path / 'state/cosmic-forge/backups').exists()))
    session.capture('cosmic-forge-light')
    session.capture('cosmic-forge')
    session.select_effect(1)
    check('Sidebar selects Crystal', session.get('glass_effect') == '"crystal"')
    session.capture('cosmic-forge-crystal')
    session.select_effect(2)
    check('Sidebar returns to Aero', session.get('glass_effect') == '"aero"')
    session.click(968, 282)
    check('Native spin button saves a precise step', session.get('aero_blur_amount') == '46')
    session.key('z', True)
    check('Ctrl+Z no longer triggers Undo', session.get('aero_blur_amount') == '46')
    undo_key = session.display.keysym_to_keycode(XK.string_to_keysym('z'))
    quit_key = session.display.keysym_to_keycode(XK.string_to_keysym('q'))
    subprocess.run(['setxkbmap', '-display', session.screen, '-layout', 'ir'], check=True)
    time.sleep(0.4)
    session.keycode(undo_key, ctrl=True)
    check('Undo remains disabled with the Persian keyboard layout', session.get('aero_blur_amount') == '46')
    subprocess.run(['setxkbmap', '-display', session.screen, '-layout', 'us'], check=True)
    time.sleep(0.4)
    session.click(800, 327)
    blur = int(session.get('aero_blur_amount'))
    check('Native slider saves a bounded value', 65 <= blur <= 80)
    session.click(968, 376)
    check('Aero color is independent of blur', session.get('aero_color') == '56' and int(session.get('aero_blur_amount')) == blur)
    session.key('z', True)
    check('Ctrl+Z leaves the current control unchanged', session.get('aero_color') == '56' and int(session.get('aero_blur_amount')) == blur)
    selected = session.get('glass_effect')
    session.click(856, 24)
    check('Header settings button opens Settings without switching effect', session.get('glass_effect') == selected)
    session.capture('cosmic-forge-surfaces')
    check('Opening Settings preserves glass surface preferences', all(p.exists() for p in session.config.glob('cosmic/com.system76.CosmicTheme.*/v2/frosted_panel')))
    session.click(856, 24)
    check('Header settings button returns to the selected effect', session.get('glass_effect') == selected)
    session.put('aero_blur_amount', '18')
    time.sleep(1.2)
    session.click(968, 282)
    check('Controls follow external preference changes', session.get('aero_blur_amount') == '19')
    session.put('glass_effect', '"frosted"')
    time.sleep(1.2)
    before = (session.config / 'cosmic/com.system76.CosmicTheme.Light/v2/frosted').read_text().strip()
    session.click(968, 261)
    strengths = list(session.config.glob('cosmic/com.system76.CosmicTheme.*/v2/frosted'))
    check('External selection updates the visible controls', len(strengths) == 4 and len({p.read_text().strip() for p in strengths}) == 1 and (strengths[0].read_text().strip() != before))
    opacity = session.opacity()
    step = 1 if opacity < 100 else -1
    session.click(968 if step == 1 else 888, 355)
    check('Frosted opacity saves through its native spin control', session.opacity() == opacity + step)
    maps = list(session.config.glob('cosmic/com.system76.CosmicTheme.*/v2/alpha_map'))
    check('Opacity updates light, dark and both theme builders', len(maps) == 4 and len({p.read_bytes() for p in maps}) == 1)
    session.click(888 if step == 1 else 968, 355)
    check('Opacity can be adjusted back through the visible control', session.opacity() == opacity)
    session.capture('cosmic-forge-frosted')
    session.select_effect(2)
    session.click(968 if step == 1 else 888, 564)
    check('Aero exposes the same shared opacity control', session.opacity() == opacity + step)
    session.select_effect(1)
    check('Opacity survives switching to Crystal', session.opacity() == opacity + step)
    session.select_effect(2)
    mode = session.config / 'cosmic/com.system76.CosmicTheme.Mode/v1/is_dark'
    mode.parent.mkdir(parents=True, exist_ok=True)
    mode.write_text('true\n')
    time.sleep(1.6)
    dark = session.capture('cosmic-forge-dark')
    check('Native widgets follow dark mode after an external change', sum(dark.getpixel((10, 500))[:3]) / 3 < 120)
    session.window.configure(width=780, height=650)
    session.display.sync()
    time.sleep(0.8)
    session.capture('cosmic-forge-compact')
    check('Native window supports compact sizing', session.window.get_geometry().width == 780)
    session.window.configure(width=1040, height=820)
    session.display.sync()
    time.sleep(0.6)
    subprocess.run([str(session.binary), '--import-pack', str(ROOT / 'shaders/examples/prism-files/prism.json'),
                    str(ROOT / 'shaders/examples/prism-files/prism.frag')],
                   env=session.environment, check=True, capture_output=True)
    time.sleep(1.6)
    check('Importing a community pack preserves the current effect', session.get('glass_effect') == '"aero"')
    session.select_effect(3)
    check('Imported glass appears in the sidebar without reopening Forge', session.get('glass_effect') == '"prism"')
    check('Selecting the imported effect sends shader source to the renderer',
          'forge_prism_amount' in session.get('forge_runtime'))
    session.capture('cosmic-forge-community')
    session.click(380, 750)
    check('Removing the active imported effect selects Frosted Glass', session.get('glass_effect') == '"frosted"')
    check('Removing the imported effect archives its source and clears the shader',
          session.get('forge_runtime') is None and
          not (session.path / 'data/cosmic-forge/packs/prism-demo.json').exists() and
          (session.path / 'data/cosmic-forge/packs/prism-demo.previous').exists())
    session.capture('cosmic-forge-after-removal')
    session.select_effect(2)
    check('Returning to a built-in effect clears the runtime shader', session.get('forge_runtime') is None)
    subprocess.run(['setxkbmap', '-display', session.screen, '-layout', 'ir'], check=True)
    time.sleep(0.4)
    session.click(968, 282, wait=0)
    previous = int(session.get('aero_blur_amount'))
    session.keycode(quit_key, ctrl=True, wait=0)
    session.app.wait(timeout=10)
    check('Closing flushes a pending control change', int(session.get('aero_blur_amount')) == previous + 1)
    check('Ctrl+Q closes cleanly with the Persian keyboard layout', session.app.returncode == 0)
    return results
if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(os.environ.get('FORGE_BINARY', ROOT / 'target/release/cosmic-forge')))
    parser.add_argument('--inspect', action='store_true')
    args = parser.parse_args()
    session = Session(args.binary.resolve())
    try:
        session.capture('cosmic-forge-native-initial')
        print(json.dumps({'display': session.screen, 'path': str(session.path)}), flush=True)
        if args.inspect:
            deadline = time.monotonic() + 300
            while session.app.poll() is None and time.monotonic() < deadline:
                time.sleep(1)
        else:
            exercise(session)
    finally:
        session.close()
