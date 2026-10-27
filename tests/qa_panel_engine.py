#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Install the released panel only inside a private HOME; exercise reuse and offline Restore."""
import json,shutil,time
from pathlib import Path
from qa_native import Session,ROOT
for name in ['cache','home']:
    path=ROOT/'build/qa-panel-engine'/name
    if path.exists():shutil.rmtree(path)
s=Session(ROOT/'target/release/cosmic-forge',ROOT/'build/qa-panel-engine',with_engine=False)
try:
    state=s.path/'state/cosmic-forge';state.mkdir(parents=True,exist_ok=True)
    record=state/'panel-installation.json';log=state/'panel-build.log'
    launcher=Path(s.environment['HOME'])/'.local/bin/cosmic-panel'
    launcher.parent.mkdir(parents=True,exist_ok=True)
    settings=launcher.with_name('cosmic-settings');settings.write_text('preserve Settings')
    compositor=launcher.with_name('cosmic-comp');compositor.write_text('preserve compositor')
    prefs={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    def wait_for(stage):
        deadline=time.monotonic()+220
        while time.monotonic()<deadline:
            if log.exists() and 'FORGE_PROGRESS:'+stage in log.read_text():time.sleep(.7);return
            if log.exists() and any(text in log.read_text() for text in ['Traceback','Preserved']):raise AssertionError(log.read_text())
            time.sleep(.2)
        raise AssertionError(log.read_text() if log.exists() else 'No panel worker log')
    s.click(100,322)
    # Go to Settings through the header, independent of the missing-engine message.
    s.click(856,24);s.scroll_to_top();s.click(400,657);wait_for('complete')
    data=json.loads(record.read_text());binary=Path(data['destination'])/'cosmic-panel'
    assert 'FORGE_PROGRESS:downloading_binary' in log.read_text()
    assert prefs=={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    before={p:(p.read_bytes(),p.stat().st_mtime_ns) for p in [record,launcher,binary]}
    s.capture('panel-installed');log.unlink()
    s.scroll_to_top();s.click(540,657);wait_for('already_installed')
    assert 'FORGE_PROGRESS:building' not in log.read_text()
    assert before=={p:(p.read_bytes(),p.stat().st_mtime_ns) for p in before}
    s.capture('matching-panel-skipped')
    # Stock recovery is embedded and does not fetch engine sources.
    log.unlink();s.scroll_to_top();s.click(450,705);wait_for('complete')
    assert not launcher.exists() and not record.exists()
    assert 'Original COSMIC panel restored' in log.read_text()
    assert binary.read_bytes()==before[binary][0]
    assert settings.read_text()=='preserve Settings' and compositor.read_text()=='preserve compositor'
    assert prefs=={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    s.capture('original-panel-restored')
    (s.path/'results.json').write_text(json.dumps({'released_panel_download':True,'build_skips_matching_install':True,'offline_stock_recovery':True,'compositor_settings_preferences_preserved':True},indent=2))
    print('PASS released panel install; matching Build skipped; embedded stock Restore; compositor/Settings/preferences preserved')
finally:s.close()
