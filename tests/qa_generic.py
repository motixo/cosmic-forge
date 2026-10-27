#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check generic glass import, revision gating and controls in private XDG."""
import argparse
import json
from pathlib import Path
import subprocess
import time
from qa_native import ROOT, Session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/cosmic-forge')
    args = parser.parse_args()
    session = Session(args.binary.resolve(), ROOT / 'build/qa-generic')
    try:
        before = {key: session.get(key) for key in ['aero_blur_amount', 'aero_color', 'aero_reflections']}
        example = ROOT / 'shaders/examples/plain-glass'
        subprocess.run([str(session.binary), '--import-pack', str(example / 'plain-glass.json'),
                        str(example / 'plain-glass.frag')], env=session.environment, check=True, capture_output=True)
        time.sleep(1.3)
        session.click(100, 192)
        assert session.get('glass_effect') == '"aero"', 'Revision 1 accepted a generic effect'
        session.capture('revision-one-needs-update')
        session.key('Escape')
        record_path = session.path / 'state/cosmic-forge/engine-installation.json'
        record = json.loads(record_path.read_text())
        record['renderer_revision'] = 2
        record_path.write_text(json.dumps(record))
        session.click(100, 192)
        assert session.get('glass_effect') == '"plain-glass"'
        session.capture('plain-glass')
        assert 'base:"glass"' in session.get('forge_runtime')
        session.click(968, 282)
        assert session.get('forge_plain_blur') == '46', session.get('forge_plain_blur')
        assert 'name:"forge_blur",value:46.0' in session.get('forge_runtime')
        session.capture('independent-blur')
        session.click(100, 152)
        assert session.get('glass_effect') == '"aero"'
        assert before == {key: session.get(key) for key in before}
        session.click(100, 192)
        assert session.get('forge_plain_blur') == '46'
        assert 'name:"forge_blur",value:46.0' in session.get('forge_runtime')
        (session.path / 'results.json').write_text(json.dumps({
            'paired_import': True, 'revision_one_requires_update': True,
            'revision_two_selects_glass': True, 'blur_uniform_updates': True,
            'aero_preferences_preserved': True, 'blur_survives_switching': True,
        }, indent=2) + '\n')
        print('PASS generic import; revision gate; live blur parameter; independent preferences')
    finally:
        session.close()


if __name__ == '__main__':
    main()
