#!/usr/bin/env python3
"""Regression checks against falsely green parity reports."""
import copy
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CHECKER = runpy.run_path(str(ROOT / 'scripts/check-jpa-parity.py'))
VALIDATE = CHECKER['validate']


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / 'plugins/jpa-assist/parity/requirements.json').read_text())

    def test_pinned_inventory_is_valid_but_has_no_full_parity_claim(self):
        self.assertEqual(VALIDATE(self.ledger), [])
        self.assertTrue(any(e['status'] != 'verified-parity' for e in self.ledger['requirements']))

    def test_removing_an_open_requirement_cannot_make_the_gate_green(self):
        self.ledger['requirements'].pop(0)
        self.assertTrue(any('Missing pinned' in e for e in VALIDATE(self.ledger)))

    def test_success_requires_differential_evidence_not_a_status_change(self):
        item = self.ledger['requirements'][0]
        item['status'] = 'verified-parity'
        item['acceptance'] = [{'scenario': 'source edit', 'result': 'pass'}]
        self.assertTrue(any('acceptance' in e or 'evidence' in e for e in VALIDATE(self.ledger)))

    def test_duplicate_alias_and_unspecified_blocker_are_rejected(self):
        self.ledger['requirements'].append(copy.deepcopy(self.ledger['requirements'][0]))
        self.ledger['requirements'][1].update(status='blocked')
        errors = VALIDATE(self.ledger)
        self.assertTrue(any('Duplicate' in e for e in errors))
        self.assertTrue(any('exact blocker' in e for e in errors))

    def test_mapping_fields_are_validated(self):
        reqs = self.ledger['requirements']
        reqs[0]['mappedTo'] = ['G999']
        self.assertTrue(any('unknown id' in e for e in VALIDATE(self.ledger)))
        reqs[0].pop('mappedTo')
        reqs[0]['notApplicable'] = '  '
        self.assertTrue(any('non-empty reason' in e for e in VALIDATE(self.ledger)))
        reqs[0].update(notApplicable='internal', mappedTo=['G002'])
        self.assertTrue(any('mutually exclusive' in e for e in VALIDATE(self.ledger)))

    def test_not_applicable_entries_stay_open_gaps(self):
        na = [e for e in self.ledger['requirements'] if e.get('notApplicable')]
        self.assertTrue(na)
        self.assertTrue(all(e['status'] != 'verified-parity' for e in na))


SHA = 'a' * 64
EXISTING = 'scripts/check-jpa-parity.py'


class EvidenceLevelAndDocumentationTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / 'plugins/jpa-assist/parity/requirements.json').read_text())
        self.row = self.ledger['requirements'][0]
        self.empty_claims = tempfile.TemporaryDirectory()
        self.addCleanup(self.empty_claims.cleanup)

    def validate(self, claims=None):
        return VALIDATE(self.ledger, claims or self.empty_claims.name)

    def verify_row(self, case):
        self.row.update(status='verified-parity', implementation=['x'], evidence=[EXISTING], acceptance=[case])

    def docs_case(self, **overrides):
        case = {
            'scenario': 'doc', 'implementationEvidence': EXISTING, 'test': 't', 'scope': 's', 'result': 'pass',
            'referenceType': 'documentation', 'referenceSources': ['https://example.com/doc'],
            'referenceClaims': ['Docs section X says Y'], 'docsRetrievedOn': '2026-11-11',
        }
        case.update(overrides)
        return case

    def test_valid_evidence_level_is_accepted_without_status_change(self):
        self.row['evidenceLevel'] = {
            'L0': {'status': 'pass', 'tests': ['t'], 'commit': 'abc'},
            'L1': {'status': 'pass', 'harness': EXISTING, 'receipt': EXISTING, 'wasmSha256': SHA},
            'L2': {'status': 'none'},
            'L3': {'status': 'fail', 'receipts': [EXISTING]},
        }
        self.assertEqual(self.validate(), [])
        self.assertEqual(CHECKER['coverage'](self.ledger['requirements'])['L1'], 1)

    def test_invalid_evidence_level_shapes_are_rejected(self):
        bad = [
            [], {'L9': {'status': 'pass'}}, {'L0': {'status': 'ok'}}, {'L0': {'status': 'pass'}},
            {'L1': {'status': 'pass', 'harness': EXISTING, 'receipt': EXISTING, 'wasmSha256': 'zz'}},
            {'L2': {'status': 'pass', 'receipts': []}}, {'L3': {'status': 'pass', 'receipts': ['no/such/file.json']}},
            {'L1': {'status': 'fail', 'receipt': 'no/such/file.json'}},
        ]
        for level in bad:
            self.row['evidenceLevel'] = level
            self.assertTrue(any('evidenceLevel' in e for e in self.validate()), level)

    def test_partial_row_with_passing_l3_stays_an_open_gap(self):
        self.row['evidenceLevel'] = {'L3': {'status': 'pass', 'receipts': [EXISTING]}}
        self.assertEqual(self.validate(), [])
        self.assertNotEqual(self.row['status'], 'verified-parity')
        self.assertEqual(CHECKER['coverage'](self.ledger['requirements'])['L3'], 1)

    def test_valid_documentation_case_verifies_row_and_is_counted_separately(self):
        self.verify_row(self.docs_case())
        self.assertEqual(self.validate(), [])
        observed, documentation = CHECKER['verified_split'](self.ledger['requirements'])
        self.assertEqual((observed, documentation), ([], [self.row['id']]))

    def test_observed_case_counts_as_observed(self):
        self.verify_row({'scenario': 's', 'referenceEvidence': 'r', 'implementationEvidence': 'i', 'test': 't', 'scope': 's', 'result': 'pass'})
        self.assertEqual(self.validate(), [])
        self.assertEqual(CHECKER['verified_split'](self.ledger['requirements']), ([self.row['id']], []))

    def test_documentation_case_requires_sources_claims_date_and_file(self):
        for override in (
            {'referenceSources': []}, {'referenceSources': ['http://example.com']}, {'referenceSources': 'https://x'},
            {'referenceClaims': []}, {'referenceClaims': ['']}, {'docsRetrievedOn': 'yesterday'},
            {'implementationEvidence': 'no/such/file'}, {'result': 'fail'}, {'test': ''},
        ):
            self.verify_row(self.docs_case(**override))
            self.assertTrue(self.validate(), override)
        self.verify_row(self.docs_case(referenceType='guess'))
        self.assertTrue(any('referenceType' in e for e in self.validate()))

    def test_every_documented_claim_needs_an_acceptance_case(self):
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory) / f"{self.row['id']}.json").write_text(json.dumps({'claims': [{'id': 'c1'}, {'id': 'c2'}]}))
            self.verify_row(self.docs_case(claimIds=['c1']))
            self.assertTrue(any('c2' in e for e in self.validate(directory)))
            self.verify_row(self.docs_case(claimIds=['c1', 'c2']))
            self.assertEqual(self.validate(directory), [])

    def test_missing_claims_file_is_tolerated(self):
        self.verify_row(self.docs_case())
        self.assertEqual(self.validate(), [])

    def test_release_still_fails_and_reports_counts(self):
        script = str(ROOT / 'scripts/check-jpa-parity.py')
        self.assertEqual(subprocess.run([sys.executable, script, '--release'], capture_output=True).returncode, 1)
        out = json.loads(subprocess.run([sys.executable, script, '--json'], capture_output=True, text=True).stdout)
        for key in ('evidenceCoverage', 'verifiedObservedReference', 'verifiedDocumentationReference'):
            self.assertIn(key, out)
        self.assertEqual(len(out['openGaps']), 355)


if __name__ == '__main__':
    unittest.main()
