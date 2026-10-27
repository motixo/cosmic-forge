#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Verify the shared header updater and engine notification without installing anything."""
import hashlib,json,os,subprocess,tempfile,time
from pathlib import Path
from unittest.mock import patch
from qa_native import Session,ROOT

with tempfile.TemporaryDirectory(prefix='forge-updates-',dir=ROOT/'build') as tmp:
    fixture=Path(tmp);tools=fixture/'tools';tools.mkdir()
    def publish(name,repo,asset,bytes):
        (fixture/(name+'-asset')).write_bytes(bytes)
        (fixture/(name+'-metadata')).write_text(json.dumps({'assets':[{'name':asset,'size':len(bytes),'digest':'sha256:'+hashlib.sha256(bytes).hexdigest(),'browser_download_url':f'https://github.com/{repo}/releases/download/test/{asset}'}]}))
    subprocess.run(['python',str(ROOT/'shaders/scripts/package_effects.py'),'--revision','99999','--output',str(fixture/'effects')],check=True,capture_output=True)
    publish('effects','motixo/cosmic-forge-shaders','cosmic-forge-glass.json',(fixture/'effects').read_bytes())
    catalog=json.loads((ROOT/'mods/catalog.json').read_text());catalog['revision']+=1;catalog['mods'][1]['name']='Updated Color'
    publish('mods','motixo/cosmic-forge-mods','cosmic-forge-mods.json',json.dumps(catalog).encode())
    subprocess.run(['python',str(ROOT/'engine/scripts/package_engine.py'),'--output',str(fixture/'engine')],check=True,capture_output=True)
    publish('engine','motixo/cosmic-forge-engine','cosmic-forge-engine.json',(fixture/'engine').read_bytes())
    curl=tools/'curl';curl.write_text('#!/usr/bin/python3\nimport sys,time\nfrom pathlib import Path\nroot=Path('+repr(str(fixture))+')\nurl=sys.argv[-1]\n'
        'name="effects" if "cosmic-forge-shaders" in url else "mods" if "cosmic-forge-mods" in url else "engine"\n'
        'source=name+("-metadata" if url.endswith("/releases/latest") else "-asset")\n'
        'with (root/"audit").open("a") as f:f.write(name+"\\n")\n'
        'time.sleep(.3)\nPath(sys.argv[sys.argv.index("--output")+1]).write_bytes((root/source).read_bytes())\nsys.stdout.write("200")\n');curl.chmod(0o755)
    with patch.dict(os.environ,{'PATH':str(tools)+':'+os.environ['PATH']}):s=Session(ROOT/'target/release/cosmic-forge',ROOT/'build/qa-updates',with_engine=False)
    try:
        state=s.path/'state/cosmic-forge';state.mkdir(parents=True,exist_ok=True)
        (state/'engine-installation.json').write_text('{"test":"installed engine for update check"}')
        before={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
        s.click(816,24)
        deadline=time.monotonic()+20
        target=s.path/'data/cosmic-forge/mods/cosmic-forge-mods.json'
        while time.monotonic()<deadline:
            if target.exists() and (fixture/'audit').exists() and len((fixture/'audit').read_text().splitlines())>=6:break
            time.sleep(.1)
        else:raise AssertionError('The header did not check all three repositories')
        time.sleep(1);s.capture('engine-update-message')
        assert json.loads(target.read_text())['revision']==catalog['revision']
        assert (s.path/'data/cosmic-forge/packs/cosmic-glass.json').exists()
        assert before=={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
        assert not (Path(s.environment['HOME'])/'.local/bin/cosmic-comp').exists()
        assert not (state/'engine-compositor-receipt.json').exists()
        s.key('Escape');s.click(856,24);s.capture('settings-without-glass-surfaces')
        (s.path/'results.json').write_text(json.dumps({'header_updates_effects_and_mods':True,'engine_release_checked':True,'engine_update_notifies_without_installing':True,'preferences_preserved':True},indent=2))
        print('PASS shared header updater; both catalogs; engine notification; no implicit installation or preference edits')
    finally:s.close()
