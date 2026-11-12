#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('inherit', SCRIPTS / 'inherit-jpa-ledger.py')
inherit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inherit)


def row(row_id, status='unverified', **extra):
    return {'id': row_id, 'family': 'F', 'capability': row_id, 'status': status, **extra}


class InheritTests(unittest.TestCase):
    def ledger(self, rows):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / 'requirements.json'
        path.write_text(json.dumps({'schemaVersion': 1, 'requirements': rows}))
        return path

    def test_inherits_only_when_every_target_is_verified_by_its_own_evidence(self):
        verified = row('G001', 'verified-parity', implementation=['i'], evidence=['e'], acceptance=[{'referenceType': 'documentation'}])
        path = self.ledger([verified, row('G002'), row('E001', mappedTo=['G001']), row('E002', mappedTo=['G001', 'G002']),
                            row('E003', notApplicable='x'), row('E004'), row('G003', mappedTo=['G001'])])
        self.assertEqual(inherit.main(['--ledger', str(path), '--apply']), 0)
        by_id = {e['id']: e for e in json.loads(path.read_text())['requirements']}
        self.assertEqual(by_id['E001']['status'], 'verified-parity')
        self.assertEqual(by_id['E001']['acceptance'][0]['inheritedFrom'], ['G001'])
        self.assertEqual(by_id['E001']['implementation'], ['i'])
        for stays in ('E002', 'E003', 'E004', 'G003'):
            self.assertEqual(by_id[stays]['status'], 'unverified', stays)

    def test_a_row_inherited_from_an_inherited_row_does_not_chain(self):
        inherited = row('E001', 'verified-parity', mappedTo=['G001'], acceptance=[{'referenceType': 'inherited'}])
        path = self.ledger([inherited, row('E002', mappedTo=['E001'])])
        inherit.main(['--ledger', str(path), '--apply'])
        by_id = {e['id']: e for e in json.loads(path.read_text())['requirements']}
        self.assertEqual(by_id['E002']['status'], 'unverified')

    def test_a_known_gap_never_inherits(self):
        verified = row('G001', 'verified-parity', acceptance=[{'referenceType': 'documentation'}])
        path = self.ledger([verified, row('E001', mappedTo=['G001'], knownGap='capability missing')])
        inherit.main(['--ledger', str(path), '--apply'])
        self.assertEqual(json.loads(path.read_text())['requirements'][1]['status'], 'unverified')

    def test_dry_run_changes_nothing(self):
        verified = row('G001', 'verified-parity', acceptance=[{'referenceType': 'documentation'}])
        path = self.ledger([verified, row('E001', mappedTo=['G001'])])
        before = path.read_text()
        inherit.main(['--ledger', str(path)])
        self.assertEqual(path.read_text(), before)


if __name__ == '__main__':
    unittest.main()
