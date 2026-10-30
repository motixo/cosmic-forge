#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check the animation switch through the native UI in private XDG directories."""
import json
import subprocess
import time

from qa_native import ROOT, Session


def main():
    session = Session(ROOT / 'target/release/cosmic-forge', ROOT / 'build/qa-animation')
    try:
        record_path = session.path / 'state/cosmic-forge/engine-installation.json'
        record = json.loads(record_path.read_text())
        record['renderer_revision'] = 5
        record_path.write_text(json.dumps(record))
        pack = json.loads((ROOT / 'shaders/examples/prism.json').read_text())
        pack['effects'][0]['shader']['animated'] = True
        fixture = session.path / 'animated.json'
        fixture.write_text(json.dumps(pack))
        subprocess.run([str(session.binary), '--import-pack', str(fixture)],
                       env=session.environment, check=True, capture_output=True)
        time.sleep(2)
        session.select_effect(3)
        session.capture('animation-controls')
        preference = session.config / 'cosmic-forge/animations/prism.json'

        def animated():
            return 'animated:true' in session.get('forge_runtime')

        assert animated(), 'The imported animation must start enabled'
        session.click(980, 275)
        assert preference.read_text() == 'false' and not animated()
        session.capture('animation-off')
        session.select_effect(2)
        session.select_effect(3)
        assert not animated(), 'Changing effects must preserve animation off'
        session.click(980, 275)
        assert preference.read_text() == 'true' and animated()
        session.capture('animation-on')
        print('PASS native animation switch disables, persists, and enables compositor animation')
    finally:
        session.close()


if __name__ == '__main__':
    main()
