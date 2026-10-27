#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check imported shaders, live uniforms, source updates and invalid-source fallback."""
import json
import os
from pathlib import Path
import time
from PIL import ImageChops, ImageGrab, ImageStat

ROOT = Path(__file__).resolve().parents[2]
RUN = Path(os.environ['CRYSTAL_QA_DIR'])
STATE = json.loads((RUN / 'state.json').read_text())
CONFIG = RUN / 'config/cosmic/com.system76.CosmicComp/v1'
pack = json.loads((ROOT / 'shaders/examples/prism.json').read_text())
effect = pack['effects'][0]
saved = {key: (CONFIG / key).read_bytes() if (CONFIG / key).exists() else None
         for key in ['forge_runtime', 'glass_effect']}
results = []


def write(key, text):
    temporary = CONFIG / ('.' + key + '.new')
    temporary.write_text(text + '\n')
    temporary.replace(CONFIG / key)
    time.sleep(0.8)


def runtime(source, value, base='aero'):
    return '(abi:1,id:"prism",base:' + json.dumps(base) + ',source:' + json.dumps(source) + \
        ',parameters:[(key:"forge_prism_amount",name:"forge_prism",value:' + str(float(value)) + ')])'


def grab():
    return ImageGrab.grab(xdisplay=STATE['display']).convert('RGB')


def difference(first, second):
    return sum(ImageStat.Stat(ImageChops.difference(first, second)).mean) / 3


def check(name, passed):
    results.append({'test': name, 'passed': bool(passed)})
    print(('PASS ' if passed else 'FAIL ') + name, flush=True)
    (RUN / 'pack-render-results.json').write_text(json.dumps(results, indent=2) + '\n')
    assert passed, name


try:
    write('glass_effect', '"aero"')
    baseline = grab()
    write('forge_runtime', runtime(effect['shader']['source'], 0))
    write('glass_effect', '"prism"')
    neutral = grab()
    check('Imported shader with zero custom strength matches built-in Aero', difference(baseline, neutral) < 0.4)
    write('forge_runtime', runtime(effect['shader']['source'], 100))
    shifted = grab()
    check('A custom uniform changes real compositor output live', difference(neutral, shifted) > 1)
    shifted.save(RUN / 'prism.png')
    source = effect['shader']['source'].replace('transmitted.brg', 'vec3(0.8, 0.12, 0.3)')
    write('forge_runtime', runtime(source, 100))
    updated = grab()
    check('Replacing downloaded shader source changes output without restart', difference(shifted, updated) > 1)
    write('forge_runtime', runtime('This is invalid GLSL', 100))
    fallback = grab()
    check('Invalid GLSL falls back to built-in glass', difference(baseline, fallback) < 0.4)
    write('forge_runtime', runtime(effect['shader']['source'], 100))
    check('A corrected pack recovers without restarting the engine', difference(shifted, grab()) < 0.4)
    write('glass_effect', '"crystal"')
    crystal = grab()
    source = (ROOT / 'shaders/shaders/crystal.frag').read_text()
    write('forge_runtime', runtime(source, 0, 'crystal'))
    write('glass_effect', '"prism"')
    check('Downloaded Crystal source retains the built-in glass rendering', difference(crystal, grab()) < 0.4)
finally:
    for key, data in saved.items():
        if data is None:
            (CONFIG / key).unlink(missing_ok=True)
        else:
            (CONFIG / key).write_bytes(data)
