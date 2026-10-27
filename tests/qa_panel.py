#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise native panel colors, independent profiles and downloadable mod entries."""
import json
from pathlib import Path
import time
from qa_native import Session, ROOT

s=Session(ROOT/'target/release/cosmic-forge',ROOT/'build/qa-panel',with_panel=True)
results={}
try:
    colors=lambda name:(s.config/f'cosmic/com.system76.CosmicPanel.{name}/v1/forge_colors').read_text().replace(' ','')
    def color(value):
        s.click(980,294);s.scroll_to_bottom();s.click(450,683);s.key('a',True)
        for c in value:s.key(c,wait=.03)
        s.click(480,789)
    s.click(100,322)
    def panel_files():
        return {p:p.read_bytes() for d in (s.config/'cosmic').glob('com.system76.CosmicPanel.*') for p in d.rglob('*') if p.is_file()}
    before=panel_files()
    s.click(980,294);s.scroll_to_bottom()
    s.click(450,500,wait=1.3)
    assert panel_files()==before,'Canvas click applied the color before Apply'
    s.click(580,632,wait=1.3)
    assert panel_files()==before,'Hue slider applied the color before Apply'
    s.click(450,683);s.key('a',True)
    for c in 'red':s.key(c,wait=.03)
    s.key('Return',wait=1.3)
    assert panel_files()==before,'Editing the color applied it before Apply'
    s.capture('picker-draft-remains-open')
    s.click(480,789)
    assert 'light:Some((1.0,0.0,0.0))' in colors('Panel')
    before=panel_files()
    s.click(980,294);s.scroll_to_bottom();s.click(450,500)
    s.click(480,747)
    assert panel_files()==before,'Cancel saved the draft color'
    results['picker_edits_wait_for_apply_and_cancel_preserves_color']=True
    s.click(970,239);s.click(965,317);color('blue')
    assert 'light:Some((1.0,0.0,0.0))' in colors('Panel') and 'dark:Some((0.0,0.0,1.0))' in colors('Panel')
    s.click(380,348)
    assert 'dark:None' in colors('Panel') and 'light:Some((1.0,0.0,0.0))' in colors('Panel')
    results['native_picker_and_per_theme_reset']=True
    s.click(970,187);s.click(965,267);color('green')
    assert 'dark:Some((0.0,0.5019608,0.0))' in colors('Dock'),colors('Dock')
    assert 'dark:None' in colors('Panel')
    s.capture('independent-dock-color')
    results['independent_panel_profiles']=True
    s.click(100,282);s.click(980,244)
    gradient=s.config/'cosmic/com.system76.CosmicPanel.Dock/v1/top_edge_gradient'
    assert 'enabled:true' in gradient.read_text()
    s.click(992,301)
    assert 'height:97' in gradient.read_text()
    assert (gradient.parent/'anchor_gap').read_text().strip()=='false'
    assert 'dark:Some' in colors('Dock')
    s.capture('penumbra-controls')
    results['penumbra_preserves_colors']=True
    catalog=json.loads((ROOT/'mods/catalog.json').read_text());catalog['revision']+=1
    catalog['mods'].append({'id':'new-panel-preset','section':'panel','name':'New panel preset','description':'Downloaded without rebuilding Forge','engine_revision':1,'controls':[{'key':'panel_opacity','kind':'integer','label':'Panel opacity','min':0,'max':100,'unit':'%'}],'preset':{'panel_opacity':37}})
    target=s.path/'data/cosmic-forge/mods/cosmic-forge-mods.json'
    before={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    target.write_text(json.dumps(catalog));time.sleep(1.3)
    assert before=={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    s.click(100,362);s.capture('new-catalog-mod')
    s.click(385,330)
    assert (gradient.parent/'opacity').read_text().strip()=='0.37'
    s.capture('new-preset-applied')
    results['catalog_reload_and_explicit_preset_without_rebuild']=True
    (s.path/'results.json').write_text(json.dumps(results,indent=2)+'\n')
    print('PASS picker drafts, Apply and Cancel; per-theme reset; profiles; Penumbra; dynamic catalog and preset')
finally:s.close()
