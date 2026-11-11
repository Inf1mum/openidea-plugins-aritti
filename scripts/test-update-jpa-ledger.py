#!/usr/bin/env python3
import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SHA = 'a' * 64


class Fixture:
    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name).resolve()
        harness = self.root / 'scripts/runtime-smoke/src/bin/jpa_assist.rs'
        harness.parent.mkdir(parents=True)
        harness.write_text('// harness stub')
        self.parity = self.root / 'plugins/jpa-assist/parity'
        (self.parity / 'claims').mkdir(parents=True)
        self.out = self.root / 'out/receipts'
        self.out.mkdir(parents=True)
        self.rows = []

    def row(self, row_id, status='partial', claims=(), implementation=('plugins/jpa-assist/src/x.rs',)):
        self.rows.append({'id': row_id, 'family': 'F', 'capability': row_id, 'status': status, 'implementation': list(implementation)})
        (self.parity / 'claims' / f'{row_id}.json').write_text(json.dumps({
            'row': row_id, 'claims': [
                {'id': f'{row_id}-C{n}', 'statement': f'{row_id} claim {n}', 'supports': 's', 'source': 'https://example.com/doc',
                 'strength': strength, 'ourFeature': 'f', 'verifiableHeadless': headless}
                for n, (strength, headless) in enumerate(claims, 1)]}))

    def receipt(self, scenario, rows, claim_ids, status='pass', wasm=SHA):
        (self.out / f'{scenario}.json').write_text(json.dumps({
            'scenario': scenario, 'rows': rows, 'claimIds': claim_ids, 'status': status, 'wasmSha256': wasm, 'fixture': 'fx'}))

    def finish(self):
        (self.parity / 'requirements.json').write_text(json.dumps({'schemaVersion': 1, 'requirements': self.rows}))

    def clone_run(self, name, mutate=None):
        """Copy the current receipts into another run directory (optionally mutating one receipt)."""
        target = self.root / name / 'receipts'
        target.mkdir(parents=True)
        for path in self.out.glob('RS-*.json'):
            text = path.read_text()
            if mutate and path.stem == mutate[0]:
                text = mutate[1](text)
            (target / path.name).write_text(text)
        return self.root / name

    def run(self, module, *args, runs=3, extra_runs=()):
        module.ROOT = self.root
        buffer = io.StringIO()
        dirs = [self.root / 'out'] + [self.clone_run(f'out{n}') for n in range(2, runs + 1)] + list(extra_runs)
        argv = []
        for directory in dirs:
            argv += ['--runner-out', str(directory)]
        with redirect_stdout(buffer):
            code = module.main([*argv, '--parity-dir', str(self.parity), *args])
        return code, buffer.getvalue()

    def ledger(self):
        return {e['id']: e for e in json.loads((self.parity / 'requirements.json').read_text())['requirements']}


class UpdateLedgerTests(unittest.TestCase):
    def setUp(self):
        self.module = load('update_jpa_ledger', 'update-jpa-ledger.py')

    def test_dry_run_reports_closable_rows_and_changes_nothing(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True), ('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1', 'G001-C2'])
        before = (f.parity / 'requirements.json').read_text()
        code, output = f.run(self.module)
        self.assertEqual(code, 0)
        self.assertIn("closable rows", output)
        self.assertIn("['G001']", output)
        self.assertEqual((f.parity / 'requirements.json').read_text(), before)
        self.assertFalse((f.parity / 'receipts').exists())

    def test_apply_records_level_one_without_closing(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        f.run(self.module, '--apply')
        entry = f.ledger()['G001']
        self.assertEqual(entry['status'], 'partial')
        level = entry['evidenceLevel']['L1']
        self.assertEqual(level['status'], 'pass')
        self.assertEqual(level['wasmSha256'], SHA)
        self.assertEqual(level['claimsCovered'], ['G001-C1'])
        self.assertTrue((f.parity / 'receipts/L1/RS-001.json').is_file())

    def test_failing_scenario_blocks_closure_and_marks_level_failed(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'], status='fail')
        _, output = f.run(self.module, '--apply', '--close')
        self.assertEqual(f.ledger()['G001']['status'], 'partial')
        self.assertEqual(f.ledger()['G001']['evidenceLevel']['L1']['status'], 'fail')
        self.assertIn('closed: []', output)

    def test_uncovered_or_non_headless_explicit_claim_blocks_closure(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True), ('explicit', True)])
        f.row('G002', claims=[('explicit', True), ('explicit', False)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        f.receipt('RS-002', ['G002'], ['G002-C1'])
        f.run(self.module, '--apply', '--close')
        self.assertEqual(f.ledger()['G001']['status'], 'partial')
        self.assertEqual(f.ledger()['G002']['status'], 'partial')

    def test_implied_claims_do_not_count_and_rows_without_explicit_claims_stay_open(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True), ('implied', False)])
        f.row('G002', claims=[('implied', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        f.receipt('RS-002', ['G002'], ['G002-C1'])
        f.run(self.module, '--apply', '--close')
        self.assertEqual(f.ledger()['G001']['status'], 'verified-parity')
        self.assertEqual(f.ledger()['G002']['status'], 'partial')

    def test_close_requires_apply(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        with self.assertRaises(SystemExit):
            f.run(self.module, '--close')

    def test_closed_rows_satisfy_the_checker_and_stay_open_gaps_in_release_semantics(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True), ('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        f.receipt('RS-002', ['G001'], ['G001-C2'])
        f.run(self.module, '--apply', '--close', '--docs-date', '2026-11-11')
        entry = f.ledger()['G001']
        self.assertEqual(entry['status'], 'verified-parity')
        self.assertEqual({c['referenceType'] for c in entry['acceptance']}, {'documentation'})
        self.assertEqual(sorted(i for c in entry['acceptance'] for i in c['claimIds']), ['G001-C1', 'G001-C2'])
        checker = load('check_jpa_parity', 'check-jpa-parity.py')
        checker.ROOT = f.root
        checker.CLAIMS_DIR = f.parity / 'claims'
        data = json.loads((f.parity / 'requirements.json').read_text())
        data.setdefault('reference', {})
        errors = checker.validate(data, claims_dir=f.parity / 'claims')
        errors = [e for e in errors if 'reference' not in e.lower() or 'G001' in e]
        self.assertEqual([e for e in errors if e.startswith('G001')], [])

    def test_mixed_wasm_hashes_are_refused(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True), ('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'], wasm='a' * 64)
        f.receipt('RS-002', ['G001'], ['G001-C2'], wasm='b' * 64)
        with self.assertRaises(SystemExit):
            f.run(self.module, '--apply')

    def test_no_receipts_is_an_error(self):
        f = Fixture()
        f.row('G001')
        f.finish()
        self.assertEqual(f.run(self.module)[0], 2)

    def test_a_single_run_is_not_enough(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        with self.assertRaises(SystemExit):
            f.run(self.module, runs=1)

    def test_a_scenario_that_fails_in_any_run_never_passes(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        flaky = f.clone_run('flaky', mutate=('RS-001', lambda s: s.replace('"pass"', '"fail"')))
        code, output = f.run(self.module, '--apply', '--close', runs=2, extra_runs=[flaky])
        self.assertEqual(code, 0)
        self.assertEqual(f.ledger()['G001']['status'], 'partial')
        self.assertEqual(f.ledger()['G001']['evidenceLevel']['L1']['status'], 'fail')
        self.assertIn('closed: []', output)
        written = json.loads((f.parity / 'receipts/L1/RS-001.json').read_text())
        self.assertIn('flaky', written)

    def test_receipts_that_differ_between_passing_runs_are_non_deterministic(self):
        f = Fixture()
        f.row('G001', claims=[('explicit', True)])
        f.finish()
        f.receipt('RS-001', ['G001'], ['G001-C1'])
        drift = f.clone_run('drift', mutate=('RS-001', lambda s: s.replace('"fx"', '"fx2"')))
        f.run(self.module, '--apply', '--close', runs=2, extra_runs=[drift])
        self.assertEqual(f.ledger()['G001']['status'], 'partial')


if __name__ == '__main__':
    unittest.main()
