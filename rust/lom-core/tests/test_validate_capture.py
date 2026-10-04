#!/usr/bin/env python3
"""Development-only capture of the legacy validation contract (no runtime use).

Run from any directory. Inputs, outcomes and effective catalog overrides from
all validation-bearing legacy suites become fixtures for native Rust tests.
"""
from pathlib import Path
import copy
import importlib
import json
import os
import sys
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / 'compiler'))
import lomc
import lomc.validate as validation
import lomc.compiler as compiler
import lomc.dice_data as data
pack = importlib.import_module('lomc.pack')
original_story = validation.validate_story
original_manifest = validation.validate_manifest
records, non_json, seen = [], [], set()
active_test = ''
builtin = json.loads((ROOT / 'data/editor_data.json').read_text())

def effective_catalog():
    portrait = data.load_portrait_table()
    fields = {
        'characters': None if portrait is None else [
            {'id': key, 'portraits': values} for key, values in sorted(portrait.items())
        ],
        'views': None if data.load_view_ids() is None else sorted(data.load_view_ids()),
        'combat_talents': None if data.load_combat_talents() is None else list(data.load_combat_talents().values()),
        'dice_meta': data.load_dice_meta(),
        **data.load_editor_ids(),
    }
    return fields

baseline = copy.deepcopy(effective_catalog())

def record(kind, value, valid, warnings, catalog=None):
    item = {'kind': kind, 'input': copy.deepcopy(value), 'valid': valid, 'warnings': warnings}
    if catalog:
        item['catalog_overrides'] = catalog
    try:
        encoded = json.dumps(item, sort_keys=True, ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as error:
        non_json.append({'test': active_test, 'valid': valid, 'reason': str(error)})
        return
    if encoded in seen:
        return
    seen.add(encoded)
    item['test'] = active_test
    item['input_json'] = json.dumps(item.pop('input'), ensure_ascii=False, allow_nan=False)
    records.append(item)

def story_wrapper(story, source='story.json', warnings=None):
    actual_warnings = []
    catalog = effective_catalog()
    overrides = {key: value for key, value in catalog.items() if value != baseline[key]}
    valid = False
    try:
        result = original_story(story, source=source, warnings=actual_warnings)
        valid = True
        if warnings is not None:
            warnings.extend(actual_warnings)
        return result
    finally:
        record('story', story, valid, actual_warnings, overrides)

def manifest_wrapper(manifest, source='manifest.json'):
    valid = False
    try:
        result = original_manifest(manifest, source=source)
        valid = True
        return result
    finally:
        record('manifest', manifest, valid, [])

validation.validate_story = lomc.validate_story = compiler.validate_story = pack.validate_story = story_wrapper
validation.validate_manifest = lomc.validate_manifest = pack.validate_manifest = manifest_wrapper

class CaptureResult(unittest.TextTestResult):
    def startTest(self, test):
        global active_test
        active_test = str(test)
        super().startTest(test)

modules = [
    'test_lomc', 'test_campaign_v3_regressions', 'test_combat_node',
    'test_free_trigger', 'test_gameplay_checks', 'test_gameplay_composites',
    'test_localization',
]
suite = unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName('tests.' + name) for name in modules)
result = unittest.TextTestRunner(verbosity=1, resultclass=CaptureResult).run(suite)
output = Path(__file__).with_name('test_validate_fixtures.json')
payload = {
    'provenance': 'Captured from existing Python validation-bearing unittest suites; Python is only a development fixture generator.',
    'tests_run': result.testsRun,
    'test_failures': len(result.failures),
    'test_errors': len(result.errors),
    'non_json_cases': non_json,
    'cases': records,
}
output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + '\n')
print(f'Captured {len(records)} unique calls: {sum(r["valid"] for r in records)} valid, {sum(not r["valid"] for r in records)} invalid; {len(non_json)} non-JSON parser cases')
print(output)
sys.exit(not result.wasSuccessful())
