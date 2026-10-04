#!/usr/bin/env python3
"""Development-only migration/content-pack parity fixture capture from legacy suites."""
import copy
import importlib
import json
import os
from pathlib import Path
import sys
import unittest
import zipfile
ROOT = Path(__file__).resolve().parents[3]
sys.dont_write_bytecode = True
sys.path[:0] = [str(ROOT / 'editor'), str(ROOT / 'compiler')]
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import migration
import content_pack
import content_registry

cases = []
for kind in ('story', 'manifest', 'content'):
    original = getattr(migration, 'migrate_' + kind)
    def capture(value, kind=kind, original=original):
        case = {'kind': kind, 'input': copy.deepcopy(value)}
        try:
            result = original(value)
            case.update(valid=True, result=copy.deepcopy(result.__dict__))
            return result
        except Exception as exc:
            case.update(valid=False, error=str(exc))
            raise
        finally:
            cases.append(case)
    setattr(migration, 'migrate_' + kind, capture)
    migration._MIGRATORS[kind] = capture

def registry():
    root = content_registry.repository_root()
    return {p.relative_to(root).as_posix(): p.read_bytes().hex()
            for p in sorted(root.rglob('*')) if p.is_file() and
            (p.relative_to(root).parts[0] == 'assets' or p.name == 'registry.json')}

packs = []
original_inspect = content_pack.inspect_content_pack
def inspect(path):
    item = {'mode': 'inspect', 'registry': registry(), 'archive': Path(path).read_bytes().hex()}
    try:
        result = original_inspect(path)
        data = dict(result.__dict__)
        data.pop('path')
        item.update(valid=True, result=data)
        return result
    except Exception as exc:
        item.update(valid=False, error=str(exc))
        raise
    finally:
        packs.append(item)
content_pack.inspect_content_pack = inspect
original_export = content_pack.export_content_pack
def export(path, content_id, **kwargs):
    item = {'mode': 'export', 'registry': registry(), 'id': content_id, 'args': kwargs}
    try:
        result = original_export(path, content_id, **kwargs)
        with zipfile.ZipFile(path) as z:
            item.update(valid=True, entries={name:z.read(name).hex() for name in z.namelist()})
        return result
    except Exception as exc:
        item.update(valid=False, error=str(exc))
        raise
    finally:
        packs.append(item)
content_pack.export_content_pack = export

suite = unittest.TestSuite()
for name in ('migration_test', 'content_pack_test'):
    sys.path.insert(0, str(ROOT / 'editor/tests'))
    suite.addTests(unittest.defaultTestLoader.loadTestsFromName(name))
result = unittest.TextTestRunner(verbosity=1).run(suite)
unique = lambda values: list({json.dumps(v, sort_keys=True, ensure_ascii=False): v for v in values}.values())
payload = {'test_failures':len(result.failures), 'test_errors':len(result.errors), 'migrations':unique(cases), 'packs':unique(packs)}
target = Path(__file__).with_name('test_library_fixtures.json')
target.write_text(json.dumps(payload, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
print(f'migrations={len(payload["migrations"])} packs={len(payload["packs"])}')
raise SystemExit(not result.wasSuccessful())
