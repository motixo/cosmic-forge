#!/usr/bin/env python3
"""Run Crystal in an isolated nested COSMIC compositor. No host preferences change."""
import argparse
import json
import math
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import time

from PIL import Image

ROOT = Path(__file__).resolve().parent
(ROOT / 'logs').mkdir(exist_ok=True)
(ROOT / 'assets').mkdir(exist_ok=True)
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--effect', choices=['crystal', 'aero', 'frosted'], default='crystal',
                    help='Glass effect to preview (default: crystal)')
parser.add_argument('--headless', action='store_true', help='Use a private Xvfb display')
parser.add_argument('--backend', choices=['auto', 'wayland', 'x11'], default='auto')
parser.add_argument('--settings', action='store_true', help='Open Crystal appearance controls')
parser.add_argument('--settings-binary', type=Path, default=ROOT / 'build/bin/cosmic-settings')
parser.add_argument('--run-dir', type=Path, default=ROOT / 'build/preview')
parser.add_argument('--compositor', type=Path, default=ROOT / 'build/bin/cosmic-comp')
args = parser.parse_args()
run = args.run_dir.resolve()
processes = []
handles = []


def spawn(name, command, env, **kwargs):
    handle = (run / (name + '.log')).open('w')
    handles.append(handle)
    process = subprocess.Popen(command, env=env, stdout=handle, stderr=subprocess.STDOUT,
                               start_new_session=True, **kwargs)
    processes.append(process)
    return process


def receive(connection, count):
    result = b''
    while len(result) < count:
        part = connection.recv(count - len(result))
        if not part:
            raise RuntimeError('Nested compositor closed its session socket')
        result += part
    return result


def stop(_signum, _frame):
    raise KeyboardInterrupt


try:
    signal.signal(signal.SIGTERM, stop)
    for name in ['config', 'data', 'cache', 'sockets']:
        (run / name).mkdir(parents=True, exist_ok=True)
    (run / 'sockets').chmod(0o700)
    (run / 'stop').unlink(missing_ok=True)
    (run / 'state.json').unlink(missing_ok=True)
    # Generated calibration wallpaper, not a composited imitation of the effect.
    wallpaper = Image.new('RGB', (1280, 900))
    pixels = wallpaper.load()
    for y in range(900):
        for x in range(1280):
            wave = math.sin(x / 135 + math.sin(y / 170) * 2.6)
            wave2 = math.cos(y / 140 - x / 260)
            checker = 18 if (x // 40 + y // 40) % 2 == 0 else 0
            pixels[x, y] = (int(90 + 65 * wave + checker), int(85 + 55 * wave2 + checker),
                            int(140 + 65 * math.sin(x / 260 + y / 190) + checker))
    wallpaper.save(run / 'wallpaper.png')
    background = run / 'config/cosmic/com.system76.CosmicBackground/v1'
    background.mkdir(parents=True, exist_ok=True)
    (background / 'same-on-all').write_text('true\n')
    (background / 'all').write_text('(output: "all", source: Path(' + json.dumps(str(run / 'wallpaper.png')) +
        '), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)\n')
    config = run / 'config/cosmic/com.system76.CosmicComp/v1'
    config.mkdir(parents=True, exist_ok=True)
    for key, value in {'crystal_enabled': 'true', 'crystal_blur': '0', 'crystal_refraction': '150',
                       'crystal_blur_amount': '0', 'crystal_density': '100', 'autotile': 'false'}.items():
        (config / key).write_text(value + '\n')
    # The default keeps legacy Crystal test fixtures working.
    if args.effect != 'crystal':
        (config / 'glass_effect').write_text(json.dumps(args.effect) + '\n')
    else:
        (config / 'glass_effect').unlink(missing_ok=True)
    # Enable the existing glass categories in this private theme only.
    for mode in ['Dark', 'Light']:
        for suffix in ['', '.Builder']:
            theme = run / f'config/cosmic/com.system76.CosmicTheme.{mode}{suffix}/v2'
            theme.mkdir(parents=True, exist_ok=True)
            for key in ['frosted_windows', 'frosted_panel', 'frosted_applets', 'frosted_system_interface']:
                (theme / key).write_text('true\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(run / 'config'), XDG_DATA_HOME=str(run / 'data'),
        XDG_CACHE_HOME=str(run / 'cache'), XDG_RUNTIME_DIR=str(run / 'sockets'),
        XDG_SESSION_TYPE='wayland', XDG_CURRENT_DESKTOP='COSMIC', COSMIC_BACKEND='winit',
        WINIT_UNIX_BACKEND='x11', LANG='en_US.UTF-8', LC_ALL='en_US.UTF-8',
        NO_AT_BRIDGE='1', GTK_A11Y='none')
    parent_wayland = os.environ.get('WAYLAND_DISPLAY')
    if parent_wayland and not Path(parent_wayland).is_absolute():
        parent_wayland = str(Path(os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))) / parent_wayland)
    nested_wayland = not args.headless and args.backend != 'x11' and parent_wayland and Path(parent_wayland).exists()
    if nested_wayland:
        env['WAYLAND_DISPLAY'] = parent_wayland
        env['WINIT_UNIX_BACKEND'] = 'wayland'
        env.pop('DISPLAY', None)
    else:
        env.pop('WAYLAND_DISPLAY', None)
    # Give clients a private session bus, and avoid connecting to host services.
    bus = subprocess.check_output(['dbus-daemon', '--session', '--fork', '--print-address=1', '--print-pid=1'], text=True).splitlines()
    env['DBUS_SESSION_BUS_ADDRESS'] = bus[0]
    bus_pid = int(bus[1])
    if args.headless:
        env['LIBGL_ALWAYS_SOFTWARE'] = '1'
        readfd, writefd = os.pipe()
        spawn('xvfb', ['Xvfb', '-displayfd', str(writefd), '-screen', '0', '1280x900x24', '-nolisten', 'tcp'], env, pass_fds=(writefd,))
        os.close(writefd)
        env['DISPLAY'] = ':' + os.read(readfd, 32).decode().strip()
        os.close(readfd)
    elif not nested_wayland and not env.get('DISPLAY'):
        raise RuntimeError('An X11/Xwayland display is needed; use --headless for automated checks')
    control, child_socket = socket.socketpair()
    control.settimeout(45)
    compositor = spawn('compositor', [str(args.compositor), '--no-xwayland'],
        dict(env, COSMIC_SESSION_SOCK=str(child_socket.fileno())), pass_fds=(child_socket.fileno(),))
    child_socket.close()
    variables = json.loads(receive(control, struct.unpack('=H', receive(control, 2))[0]))['variables']
    control.close()
    client = dict(env, **variables)
    client.pop('DISPLAY', None)
    client['DBUS_SYSTEM_BUS_ADDRESS'] = 'unix:path=' + str(run / 'no-system-bus')
    spawn('background', ['/usr/bin/cosmic-bg'], client)
    if not nested_wayland:
        from Xlib import display, Xatom
        xd = display.Display(env['DISPLAY'])
        for _ in range(50):
            windows = []
            for window in xd.screen().root.query_tree().children:
                pid = window.get_full_property(xd.intern_atom('_NET_WM_PID'), Xatom.CARDINAL)
                if pid is not None and list(pid.value) == [compositor.pid]:
                    windows.append(window)
            if windows:
                width = 1280 if args.headless else min(1280, max(900, xd.screen().width_in_pixels - 100))
                height = 900 if args.headless else min(900, max(650, xd.screen().height_in_pixels - 120))
                windows[-1].configure(x=0, y=0, width=width, height=height)
                windows[-1].set_input_focus(1, 0)
                xd.sync()
                break
            time.sleep(0.2)
    surface = spawn('surface', [str(ROOT / 'build/bin/test-surface')]
                    + (['--aero'] if args.effect == 'aero' else []), client)
    if args.settings:
        spawn('settings', [str(args.settings_binary), 'appearance'], client)
    (run / 'state.json').write_text(json.dumps({'env': client, 'display': env.get('DISPLAY'),
        'backend': 'wayland' if nested_wayland else 'x11', 'surface_pid': surface.pid}, indent=2))
    print('Isolated COSMIC Crystal ready: ' + str(run), flush=True)
    start = time.monotonic()
    while compositor.poll() is None and time.monotonic() - start < 1200 and not (run / 'stop').exists():
        time.sleep(0.25)
except KeyboardInterrupt:
    pass
finally:
    for process in reversed(processes):
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
    for process in processes:
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
    if 'bus_pid' in globals():
        try:
            os.kill(bus_pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    for handle in handles:
        handle.close()
