#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Open the packaged app and Settings with a private XDG environment."""
import argparse
from pathlib import Path
import subprocess
from qa_native import Session

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
print(subprocess.check_output([str(args.binary),'--version'],text=True).strip())
s=Session(args.binary,args.output,with_engine=False)
try:
    before={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    s.capture('app')
    s.click(856,24)
    s.capture('settings')
    assert before=={p:p.read_bytes() for p in s.config.rglob('*') if p.is_file()}
    print('PASS native packaged app and Settings; preferences preserved')
finally:s.close()
