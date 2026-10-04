#!/usr/bin/env python3
"""Development-only pack compatibility fixture capture; Rust tests use data only."""
from pathlib import Path
import importlib
import hashlib
import json
import sys
import unittest
import zipfile
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'compiler'))
import lomc
pack = importlib.import_module('lomc.pack')
original = pack.pack_mod
records, seen = [], set()
active_test = ''

def bytes_value(data):
    if len(data)>1024 and data.count(data[:1])==len(data):
        return {'repeat_byte': data[0], 'count': len(data)}
    try:
        return {'utf8': data.decode('utf-8')}
    except UnicodeDecodeError:
        return {'hex': data.hex()}

def capture_files(root):
    files = {}
    if not root.exists():
        return None
    for file in sorted(root.rglob('*')):
        relative = file.relative_to(root).as_posix()
        if file.is_symlink():
            actual=file.resolve()
            try:
                target=actual.relative_to(root.resolve()).as_posix()
                files[relative]={'symlink_inside':target}
            except ValueError:
                files[relative]={'symlink_outside':bytes_value(actual.read_bytes())}
        elif file.is_file():
            files[relative]=bytes_value(file.read_bytes())
        elif file.is_dir():
            files[relative]={'directory':True}
    return files

def wrapper(mod_dir, output=None):
    root=Path(mod_dir)
    item={'root_name':root.name,'valid':False}
    if root.resolve()==(ROOT/'samples/showcase3').resolve():
        item['source_project']='samples/showcase3'
    else:
        item['files']=capture_files(root)
    try:
        result=original(mod_dir, output=output)
        item['valid']=True
        with zipfile.ZipFile(result) as archive:
            item['entries']={}
            for name in sorted(archive.namelist()):
                data=archive.read(name)
                item['entries'][name]={'sha256':hashlib.sha256(data).hexdigest(),'size':len(data)}
                if name.endswith(('.json','.lua','.sha256')):
                    item['entries'][name]['utf8']=data.decode('utf-8')
        return result
    finally:
        encoded=json.dumps(item,ensure_ascii=False,sort_keys=True)
        if encoded not in seen:
            seen.add(encoded);item['test']=active_test;records.append(item)

pack.pack_mod=lomc.pack_mod=wrapper
class CaptureResult(unittest.TextTestResult):
    def startTest(self,test):
        global active_test
        active_test=str(test)
        super().startTest(test)
modules=['test_lomc','test_free_trigger','test_localization','test_campaign_v3_regressions','test_combat_node']
suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName('tests.'+name) for name in modules)
result=unittest.TextTestRunner(verbosity=1,resultclass=CaptureResult).run(suite)
# Repository acceptance sample is an additional end-to-end contract fixture.
active_test='repository samples/showcase3'
import tempfile
with tempfile.TemporaryDirectory() as destination:
    wrapper(str(ROOT/'samples/showcase3'), str(Path(destination)/'showcase.lommod'))
output=Path(__file__).with_name('test_package_fixtures.json')
output.write_text(json.dumps({'tests_run':result.testsRun,'test_failures':len(result.failures),'test_errors':len(result.errors),'cases':records},ensure_ascii=False,indent=2)+'\n')
print(f'Captured {len(records)} package cases: {sum(x["valid"] for x in records)} valid, {sum(not x["valid"] for x in records)} invalid')
print(output)
sys.exit(not result.wasSuccessful())
