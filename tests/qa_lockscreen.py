#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Check independent lock-screen mods through native controls and a private file portal."""
import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image
from qa_native import ROOT, Session


def wait_for(check, message):
    for _ in range(80):
        if check(): return
        time.sleep(.1)
    raise AssertionError(message)


def files(directory):
    return {str(p.relative_to(directory)): p.read_bytes() for p in directory.rglob('*') if p.is_file()}


missing = Session(ROOT/'target/release/cosmic-forge', ROOT/'build/qa-lockscreen-missing', with_engine=False)
try:
    before = files(missing.config)
    for y in (410, 450):
        missing.click(100,y)
        missing.capture('inline-'+str(y))
        missing.click(380,227)
        missing.capture('settings-'+str(y))
        missing.click(856,24)
    assert before == files(missing.config)
finally: missing.close()

s = Session(ROOT/'target/release/cosmic-forge', ROOT/'build/qa-lockscreen', with_greeter=True)
portal = None
try:
    before = files(s.config)
    locker = s.config/'cosmic/com.system76.CosmicLockscreen/v1'
    s.click(100,450);s.capture('media-disabled');s.click(984,192)
    assert (locker/'media_enabled').read_text().strip() == 'true'
    assert not (locker/'wallpaper').exists()
    # Join the app's own private D-Bus; never contact the host portal.
    child_ids = Path(f'/proc/{s.app.pid}/task/{s.app.pid}/children').read_text().split()
    app_pid = next(pid for pid in child_ids if (Path('/proc')/pid/'comm').read_text().strip() == 'cosmic-forge')
    app_env = dict(item.split('=',1) for item in (Path('/proc')/app_pid/'environ').read_bytes().decode().split('\0') if '=' in item)
    env = dict(s.environment, DBUS_SESSION_BUS_ADDRESS=app_env['DBUS_SESSION_BUS_ADDRESS'])
    portal = subprocess.Popen(['python3',str(ROOT/'tests/mock_file_portal.py'),str(s.path)],env=env,stdout=s.log,stderr=s.log)
    time.sleep(.5)
    source = s.path/'تصویر من.png'
    Image.new('RGB',(32,24),(30,160,80)).save(source)
    response = s.path/'portal-response.json'
    response.write_text(json.dumps({'file':str(source)}))
    s.click(100,410);s.click(385,227)
    wait_for(lambda:(locker/'wallpaper').exists(),'Choose image did not save a wallpaper')
    selected = (locker/'wallpaper').read_text()
    copies = list((s.path/'data/cosmic-lockscreen').glob('*.png'))
    assert len(copies) == 1
    assert str(copies[0]) in selected
    source.unlink()
    assert Image.open(copies[0]).getpixel((0,0)) == (30,160,80)
    assert (locker/'media_enabled').read_text().strip() == 'true'
    s.capture('background-custom')
    s.click(100,450);s.click(984,192)
    assert (locker/'media_enabled').read_text().strip() == 'false'
    assert (locker/'wallpaper').read_text() == selected
    s.click(100,410);s.click(550,227)
    assert (locker/'wallpaper').read_text().strip() == 'None'
    assert copies[0].exists()
    assert (locker/'media_enabled').read_text().strip() == 'false'
    # Cancellation and decode failures must preserve the current preferences.
    saved = files(s.config)
    for reply in [{'cancel':True},{'file':str(s.path/'bad.png')}]:
        (s.path/'bad.png').write_text('invalid image')
        response.write_text(json.dumps(reply))
        s.click(385,227);time.sleep(1.1)
        assert files(s.config) == saved
    s.capture('invalid-image-preserved')
    # A new panel entry reorders the mod catalog but must retain the visible lock-screen mod.
    catalog=json.loads((ROOT/'mods/catalog.json').read_text());catalog['revision']+=1
    entry=dict(catalog['mods'][0],id='new-panel',name='New panel')
    catalog['mods'].insert(0,entry)
    (s.path/'data/cosmic-forge/mods/cosmic-forge-mods.json').write_text(json.dumps(catalog))
    time.sleep(1.3)
    s.capture('background-after-catalog-reorder')
    assert files(s.config) == saved
    # Only lock-screen preferences have been touched, and every other original file is retained.
    after = files(s.config)
    assert all(after[name] == value for name,value in before.items())
    assert all(name in before or name.startswith('cosmic/com.system76.CosmicLockscreen/') for name in after)
    (s.path/'results.json').write_text(json.dumps({'separate_mods':True,'missing_engine_inline':True,'native_media_toggle':True,'file_portal_import':True,'independent_background':True,'source_removal_safe':True,'follow_desktop':True,'cancel_and_invalid_preserve_preferences':True,'catalog_reorder':True},indent=2)+'\n')
    print('PASS independent lock screen mods; engine gate; native toggle; image import/cancel/errors; persistent copy; preferences')
finally:
    if portal is not None:
        portal.terminate();portal.wait(timeout=5)
    s.close()
