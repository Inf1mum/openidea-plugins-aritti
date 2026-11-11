#!/usr/bin/env python3
"""Regression checks against falsely green parity reports."""
import copy
import json
from pathlib import Path
import runpy
import unittest

ROOT = Path(__file__).resolve().parents[1]
VALIDATE = runpy.run_path(str(ROOT / 'scripts/check-jpa-parity.py'))['validate']


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


if __name__ == '__main__':
    unittest.main()
