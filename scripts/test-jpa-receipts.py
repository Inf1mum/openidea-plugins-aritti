#!/usr/bin/env python3
import contextlib
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('check', Path(__file__).with_name('check-jpa-receipts.py'))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

ACCEPTANCE = '| ID | Rows |\n|---|---|\n| AC-001 | G001, G002 | x |\n| AC-002 | S001 | y |\n'
LEDGER = {'requirements': [
    {'id': 'G001', 'status': 'partial'}, {'id': 'G002', 'status': 'partial'},
    {'id': 'S001', 'status': 'partial'}, {'id': 'G003', 'status': 'unverified'},
]}
GOOD = {
    'scenario': 'AC-001', 'rows': ['G001'],
    'host': {'version': '1', 'os': 'mac', 'pluginBuild': 'a' * 64},
    'fixture': 'f', 'result': 'pass', 'observations': '', 'artifacts': [],
    'checks': {k: 'pass' for k in check.CHECK_KEYS}, 'tester': 't', 'date': '2026-01-01',
}


class Receipts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.parity = Path(self.tmp.name)
        (self.parity / 'receipts').mkdir()
        (self.parity / 'ACCEPTANCE.md').write_text(ACCEPTANCE)
        self.ledger_text = json.dumps(LEDGER)
        (self.parity / 'requirements.json').write_text(self.ledger_text)

    def tearDown(self):
        self.tmp.cleanup()

    def put(self, name, data):
        (self.parity / 'receipts' / name).write_text(json.dumps(data))

    def report(self):
        return check.run(self.parity)

    def test_valid_receipt_counts_as_pass(self):
        self.put('a.json', GOOD)
        report = self.report()
        self.assertEqual(report['errors'], [])
        self.assertEqual(report['coverage']['G001']['passed'], ['AC-001'])
        self.assertEqual(report['partialWithoutPass'], ['G002', 'S001'])
        self.assertEqual(report['partialWithoutScenario'], [])

    def test_malformed_receipt(self):
        bad = copy.deepcopy(GOOD)
        del bad['host']
        bad['result'] = 'maybe'
        bad['extra'] = 1
        self.put('a.json', bad)
        self.put('b.json', GOOD | {'date': 'yesterday'})
        (self.parity / 'receipts' / 'c.json').write_text('{nope')
        errors = '\n'.join(self.report()['errors'])
        for needle in ('missing field host', 'result must be', 'unknown field extra', 'date must be', 'invalid JSON'):
            self.assertIn(needle, errors)

    def test_pass_with_failed_check_is_an_error(self):
        self.put('a.json', GOOD | {'checks': GOOD['checks'] | {'undo_exact': 'fail'}})
        self.assertIn('contradicts', '\n'.join(self.report()['errors']))

    def test_artifact_hash(self):
        artifact = self.parity / 'receipts' / 'p.txt'
        artifact.write_text('hello')
        digest = hashlib.sha256(b'hello').hexdigest()
        self.put('a.json', GOOD | {'artifacts': [{'path': 'p.txt', 'sha256': digest}]})
        self.assertEqual(self.report()['errors'], [])
        self.put('a.json', GOOD | {'artifacts': [{'path': 'p.txt', 'sha256': '0' * 64}]})
        self.assertIn('hash mismatch', '\n'.join(self.report()['errors']))
        self.put('a.json', GOOD | {'artifacts': [{'path': 'missing.txt', 'sha256': digest}]})
        self.assertIn('does not exist', '\n'.join(self.report()['errors']))
        self.put('a.json', GOOD | {'artifacts': [{'path': '../requirements.json', 'sha256': digest}]})
        self.assertIn('escapes', '\n'.join(self.report()['errors']))

    def test_unknown_scenario_and_rows(self):
        self.put('a.json', GOOD | {'scenario': 'AC-099'})
        self.put('b.json', GOOD | {'rows': ['G999']})
        self.put('c.json', GOOD | {'rows': ['S001']})
        errors = '\n'.join(self.report()['errors'])
        self.assertIn('unknown scenario AC-099', errors)
        self.assertIn('unknown ledger row G999', errors)
        self.assertIn('row S001 is not covered by scenario AC-001', errors)

    def test_examples_do_not_count(self):
        self.put('a.json', GOOD | {'example': True})
        report = self.report()
        self.assertEqual(report['errors'], [])
        self.assertEqual(report['exampleReceipts'], 1)
        self.assertEqual(report['coverage']['G001']['passed'], [])

    def test_uncovered_partial_row_reported(self):
        (self.parity / 'ACCEPTANCE.md').write_text(ACCEPTANCE.replace('| AC-002 | S001 |', '| AC-002 | - |'))
        self.assertEqual(self.report()['partialWithoutScenario'], ['S001'])

    def test_duplicate_scenario_and_unknown_row_in_script(self):
        (self.parity / 'ACCEPTANCE.md').write_text(ACCEPTANCE + '| AC-001 | G777 | z |\n')
        errors = '\n'.join(self.report()['errors'])
        self.assertIn('duplicate scenario AC-001', errors)
        self.assertIn('unknown row G777', errors)

    def test_exit_codes_and_require_pass(self):
        args = ['--parity-dir', str(self.parity)]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(check.main(args), 0)
            self.assertEqual(check.main(args + ['--require-pass']), 1)
        for scenario, row in (('AC-001', 'G001'), ('AC-001', 'G002'), ('AC-002', 'S001')):
            self.put(f'{scenario}-{row}.json', GOOD | {'scenario': scenario, 'rows': [row]})
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(check.main(args + ['--require-pass']), 0)
            self.put('bad.json', {'scenario': 'AC-001'})
            self.assertEqual(check.main(args), 1)

    def test_ledger_is_never_modified(self):
        self.put('a.json', GOOD)
        with contextlib.redirect_stdout(io.StringIO()):
            check.main(['--parity-dir', str(self.parity), '--json'])
        self.assertEqual((self.parity / 'requirements.json').read_text(), self.ledger_text)


class FixtureReferences(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.parity = Path(self.tmp.name)
        (self.parity / 'fixtures' / 'f' / 'src').mkdir(parents=True)
        (self.parity / 'fixtures' / 'f' / 'src' / 'A.java').write_text('class A {}')
        (self.parity / 'fixtures' / 'f' / 'GOLDEN.md').write_text('These codes exist: `jpa-legacy-rule`, `src/checks.rs`.\n')
        (self.parity / 'differential' / 'scenarios').mkdir(parents=True)

    def tearDown(self):
        self.tmp.cleanup()

    def acceptance(self, text):
        (self.parity / 'ACCEPTANCE.md').write_text(text)

    def scenario(self, **fields):
        data = {'fixture': 'f', 'inputFile': 'src/A.java'} | fields
        (self.parity / 'differential' / 'scenarios' / 'DF-001-x.json').write_text(json.dumps(data))
        return check.check_fixtures(self.parity)

    def test_existing_and_missing_acceptance_paths(self):
        self.acceptance('| AC-001 | G001 | f : `fixtures/f/src/A.java`, `fixtures/f/src/` | s | e | read-only |\n')
        self.assertEqual(check.check_fixtures(self.parity), [])
        self.acceptance('| AC-001 | G001 | f : `fixtures/f/src/Missing.java`, `src/not/checked.java` | s | e | read-only |\n')
        self.assertEqual(check.check_fixtures(self.parity), ['ACCEPTANCE.md: missing fixture path fixtures/f/src/Missing.java'])

    def test_legacy_only_code_in_scenario_row_is_an_error(self):
        self.acceptance('| AC-001 | G001 | f | s | `jpa-legacy-rule` Info | read-only |\n| AC-002 | G001 | f | s | `jpa-legacy-rule-other` | read-only |\n')
        self.assertEqual(check.check_fixtures(self.parity), ['ACCEPTANCE.md: AC-001 expects jpa-legacy-rule, which the live provider never emits'])
        self.acceptance('| Rule | Rows |\n| `jpa-legacy-rule` | G001 |\n')
        self.assertEqual(check.check_fixtures(self.parity), [])

    def test_scenario_json_paths(self):
        self.assertEqual(self.scenario(), [])
        self.assertEqual(self.scenario(inputFiles=['src/A.java', 'fixtures/f/GOLDEN.md'], goldenRef='GOLDEN.md', swap=[{'from': 'src/A.java', 'step': 'cp'}]), [])
        self.assertEqual(self.scenario(inputFile='src/Nope.java'), ['DF-001-x.json: missing fixture path src/Nope.java'])
        errors = self.scenario(inputFiles=['src/B.java'], goldenRef='NOPE.md', swap=[{'from': 'x.xml', 'step': 'cp'}])
        self.assertEqual(len(errors), 3)
        self.assertEqual(self.scenario(fixture='absent'), ['DF-001-x.json: missing fixture absent'])
        self.assertTrue(self.scenario(inputFile='src/(new file)'))

    def test_run_and_cli_report_missing_paths(self):
        (self.parity / 'receipts').mkdir()
        (self.parity / 'requirements.json').write_text(json.dumps(LEDGER))
        self.acceptance(ACCEPTANCE + '| AC-003 | G001 | f : `fixtures/f/gone.txt` | s | e | read-only |\n')
        report = check.run(self.parity)
        self.assertEqual(report['missingFixturePaths'], 1)
        self.assertIn('ACCEPTANCE.md: missing fixture path fixtures/f/gone.txt', report['errors'])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(check.main(['--parity-dir', str(self.parity), '--check-fixtures']), 1)
            self.acceptance(ACCEPTANCE)
            self.assertEqual(check.main(['--parity-dir', str(self.parity), '--check-fixtures']), 0)


if __name__ == '__main__':
    unittest.main()
