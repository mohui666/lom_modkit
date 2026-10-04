#!/usr/bin/env python3
"""Capture only legacy synthetic read-state tests; never discover or open real saves."""
import json
from pathlib import Path
import sys
import unittest
ROOT=Path(__file__).resolve().parents[3]
sys.dont_write_bytecode=True
sys.path[:0]=[str(ROOT/'editor'),str(ROOT/'compiler'),str(ROOT/'editor/tests')]
import game_install as game
cases=[]
for kind in ('dat','json'):
    original=getattr(game,'_reset_universe_'+kind)
    def capture(path,mod_id,keys,kind=kind,original=original):
        if not path.is_file(): return original(path,mod_id,keys)
        before=path.read_bytes()
        try:
            result=original(path,mod_id,keys)
        except Exception:
            # Existing suite separately injects atomic-write faults; Rust tests inject
            # filesystem failures independently, rather than encoding a fake success.
            raise
        cases.append({'kind':kind,'mod_id':mod_id,'keys':keys,'input':before.hex(),'output':path.read_bytes().hex(),'count':result})
        return result
    setattr(game,'_reset_universe_'+kind,capture)
import game_install_test
suite=unittest.defaultTestLoader.loadTestsFromTestCase(game_install_test.ResetStoryReadStateTest)
result=unittest.TextTestRunner(verbosity=1).run(suite)
unique=list({json.dumps(c,sort_keys=True):c for c in cases}.values())
Path(__file__).with_name('game_read_state_fixtures.json').write_text(json.dumps({'failures':len(result.failures),'errors':len(result.errors),'cases':unique},ensure_ascii=False,indent=2)+'\n')
print('read-state cases:',len(unique))
raise SystemExit(not result.wasSuccessful())
