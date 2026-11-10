#!/usr/bin/env python3
"""Validate the Laravel leaf acceptance contract, not its implementation claims.

No network, build, or repository mutation. --self-test exercises rejection rules.
Verified leaves need individually scoped passing test/native/live receipts for all
required gates. This checker checks receipt shape/coverage; acceptance reviewers
must authenticate the actual commands, artifacts and results.
"""
from __future__ import annotations

import argparse
import copy
import json
import re
import sys
import unittest
from collections import Counter
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT = ROOT / 'docs/laravel-parity-contract.json'
DEFAULT_LEDGER = ROOT / 'docs/laravel-parity-acceptance.md'
STATUSES = {'implemented_unverified', 'partial', 'missing', 'unqualified',
            'api_excluded', 'accepted_difference', 'verified'}
ROLES = {'confirmed_baseline', 'implementation_safety', 'accepted_difference', 'unqualified'}
KINDS = {'baseline', 'source', 'test', 'native', 'live', 'api', 'user_decision'}
GATES = {'test', 'native', 'live'}


def ledger_ids(path: Path) -> set[str]:
    text = path.read_text()
    start = text.index('## Historical checkpoint status and evidence rules')
    end = text.index('## Current-host capability qualification', start)
    ids = re.findall(r'^\| ([GEMRVBT]\d{2}) \|', text[start:end], re.M)
    if len(ids) != len(set(ids)) or len(ids) != 67:
        raise ValueError('Historical ledger must retain 67 unique parent IDs')
    return set(ids)


def validate(contract: dict, expected_parents: set[str]) -> list[str]:
    errors = []
    def need(condition, message):
        if not condition:
            errors.append(message)
    def text(value):
        return isinstance(value, str) and bool(value.strip())
    if not isinstance(contract, dict):
        return ['Contract must be an object']
    need(contract.get('contractVersion') == 1, 'Unsupported contractVersion')
    baseline = contract.get('baseline', {})
    need(isinstance(baseline, dict) and baseline.get('version') == '13.4', 'Baseline must remain 13.4')
    need(isinstance(baseline, dict) and baseline.get('hostAPI') == '1.4', 'Host API must remain 1.4')
    need(set(contract.get('statusEnum', [])) == STATUSES, 'statusEnum must match checker statuses')
    parents = contract.get('parentIds', [])
    need(isinstance(parents, list) and len(parents) == len(set(parents)) and set(parents) == expected_parents,
         'parentIds must match all 67 historical parents exactly')
    owners = contract.get('owners', {})
    need(isinstance(owners, dict) and bool(owners), 'Declare owner roles')
    differences = contract.get('acceptedDifferences', [])
    need(isinstance(differences, list), 'acceptedDifferences must be an array')
    difference_ids = {d.get('id') for d in differences if isinstance(d, dict)} if isinstance(differences, list) else set()
    for difference in differences if isinstance(differences, list) else []:
        if not isinstance(difference, dict):
            errors.append('Difference must be an object'); continue
        proof = difference.get('evidence', {})
        need(isinstance(proof, dict) and proof.get('kind') == 'user_decision' and proof.get('outcome') == 'accepted'
             and text(proof.get('reference')), 'Accepted difference needs explicit user-decision evidence')
    facts = contract.get('qualifiedAPIFacts', [])
    need(isinstance(facts, list), 'qualifiedAPIFacts must be an array')
    api_facts = {f.get('id'): f for f in facts if isinstance(f, dict)} if isinstance(facts, list) else {}
    leaves = contract.get('leaves')
    if not isinstance(leaves, list) or not leaves:
        return errors + ['Declare leaf records']
    seen, covered, tasks = set(), set(), set()
    excluded = {}
    for leaf in leaves:
        if not isinstance(leaf, dict):
            errors.append('Leaf must be an object'); continue
        leaf_id = leaf.get('id')
        parent = leaf.get('parentId')
        prefix = str(leaf_id or '<missing id>')
        need(isinstance(leaf_id, str) and re.fullmatch(r'[GEMRVBT]\d{2}\.[A-Za-z][A-Za-z0-9_]*', leaf_id), f'{prefix}: invalid leaf id')
        if isinstance(leaf_id, str):
            need(leaf_id not in seen, f'{prefix}: duplicate id'); seen.add(leaf_id)
        need(parent in expected_parents, f'{prefix}: unknown parent')
        if isinstance(parent, str):
            covered.add(parent)
            need(isinstance(leaf_id, str) and leaf_id.startswith(parent + '.'), f'{prefix}: id/parent mismatch')
        status = leaf.get('currentStatus')
        need(status in STATUSES, f'{prefix}: unknown status')
        role = leaf.get('baselineRole')
        need(role in ROLES, f'{prefix}: unknown baselineRole')
        for key in ['behavior', 'versionBoundary', 'remaining', 'task']:
            need(text(leaf.get(key)), f'{prefix}: missing {key}')
        need('13.4' in str(leaf.get('versionBoundary', '')), f'{prefix}: missing version boundary')
        task = leaf.get('task')
        if isinstance(task, str):
            need(task not in tasks, f'{prefix}: duplicate task'); tasks.add(task)
        need(leaf.get('owner') in owners, f'{prefix}: unknown owner')
        url = urlsplit(str(leaf.get('baselineURL', '')))
        need(url.scheme == 'https' and url.netloc == 'laravel-idea.com' and url.path.startswith('/docs/'),
             f'{prefix}: baselineURL must be official public documentation')
        gates = leaf.get('gates', {})
        need(isinstance(gates, dict) and set(gates) == GATES, f'{prefix}: test/native/live gates required')
        if not isinstance(gates, dict): gates = {}
        for name in GATES:
            gate = gates.get(name, {})
            need(isinstance(gate, dict) and isinstance(gate.get('required'), bool) and text(gate.get('scenario')),
                 f'{prefix}: invalid {name} gate')
        evidence = leaf.get('evidence', [])
        need(isinstance(evidence, list) and bool(evidence), f'{prefix}: evidence required')
        if not isinstance(evidence, list): evidence = []
        for proof in evidence:
            need(isinstance(proof, dict) and proof.get('kind') in KINDS and text(proof.get('reference'))
                 and text(proof.get('outcome')) and text(proof.get('artifactOrRevision')), f'{prefix}: malformed evidence')
        need(any(isinstance(e, dict) and e.get('kind') == 'baseline' and e.get('reference') == leaf.get('baselineURL') for e in evidence),
             f'{prefix}: baseline evidence must match URL')
        if status in {'implemented_unverified', 'partial'}:
            sources = [e for e in evidence if isinstance(e, dict) and e.get('kind') == 'source']
            need(bool(sources), f'{prefix}: implementation status needs source evidence')
            for source in sources:
                path = source.get('reference', '').split(':', 1)[0]
                need(bool(path) and not Path(path).is_absolute() and '..' not in Path(path).parts
                     and (ROOT / path).is_file(), f'{prefix}: source file does not exist in workspace')
        if role in {'confirmed_baseline', 'implementation_safety'} and status != 'api_excluded':
            need(all(isinstance(gates.get(g), dict) and gates[g].get('required') is True for g in ['test', 'native']),
                 f'{prefix}: executable baseline/safety leaves require test and native gates')
        if status == 'verified':
            need(any(isinstance(e, dict) and e.get('kind') == 'source' for e in evidence),
                 f'{prefix}: verified implementation needs source evidence')
            need(role not in {'unqualified', 'accepted_difference'}, f'{prefix}: unqualified/difference cannot be verified')
            for gate in GATES:
                if isinstance(gates.get(gate), dict) and gates[gate].get('required'):
                    receipts = [e for e in evidence if isinstance(e, dict) and e.get('kind') == gate
                                and e.get('outcome') == 'passed' and e.get('scope') == 'leaf'
                                and leaf_id in e.get('covers', [])
                                and re.fullmatch(r'(?:[a-f0-9]{7,40}|[a-f0-9]{64})', str(e.get('artifactOrRevision', '')))]
                    need(bool(receipts), f'{prefix}: verified requires passing leaf-scoped {gate} receipt')
        if role == 'unqualified' or status == 'unqualified':
            need(role == 'unqualified' and status == 'unqualified', f'{prefix}: unqualified role/status mismatch')
            need(not any(isinstance(g, dict) and g.get('required') for g in gates.values()),
                 f'{prefix}: do not invent executable gates before baseline qualification')
        difference = leaf.get('acceptedDifference')
        if status == 'accepted_difference' or role == 'accepted_difference':
            need(status == role == 'accepted_difference' and difference in difference_ids,
                 f'{prefix}: difference requires accepted decision and matching role/status')
        else:
            need(difference is None, f'{prefix}: only accepted-difference records may waive format compatibility')
        need('apiExclusion' in leaf, f'{prefix}: explicit apiExclusion field required')
        exclusion = leaf.get('apiExclusion')
        if status == 'api_excluded':
            need(isinstance(exclusion, dict), f'{prefix}: API exclusion needs exact proof')
            if isinstance(exclusion, dict):
                fact_id = exclusion.get('factId')
                fact = api_facts.get(fact_id, {})
                need(bool(fact) and leaf_id in fact.get('appliedToLeaves', []), f'{prefix}: exclusion needs catalogued exact fact')
                for key in ['blockedBehavior', 'supportedAlternative', 'sdkReference']:
                    need(text(exclusion.get(key)), f'{prefix}: API exclusion missing {key}')
                need(any(isinstance(e, dict) and e.get('kind') == 'api' and e.get('outcome') == 'confirmed' for e in evidence),
                     f'{prefix}: API exclusion needs confirmed API receipt')
                excluded[leaf_id] = fact_id
        else:
            need(exclusion is None, f'{prefix}: nonexcluded leaf must retain null apiExclusion')
    need(covered == expected_parents, 'Every historical parent needs at least one leaf')
    for fact_id, fact in api_facts.items():
        for leaf_id in fact.get('appliedToLeaves', []):
            need(excluded.get(leaf_id) == fact_id, f'{fact_id}: exclusion assignment not matched by leaf')
    return errors


class RejectionTests(unittest.TestCase):
    contract = None
    parents = None
    def test_current_contract(self):
        self.assertEqual(validate(self.contract, self.parents), [])
    def invalid(self, edit, needle):
        value = copy.deepcopy(self.contract); edit(value)
        self.assertTrue(any(needle in error for error in validate(value, self.parents)), needle)
    def test_duplicate_id(self):
        self.invalid(lambda c: c['leaves'].append(copy.deepcopy(c['leaves'][0])), 'duplicate id')
    def test_missing_parent(self):
        self.invalid(lambda c: c.update(leaves=[l for l in c['leaves'] if l['parentId'] != 'G01']), 'Every historical parent')
    def test_unknown_status(self):
        self.invalid(lambda c: c['leaves'][0].update(currentStatus='done'), 'unknown status')
    def test_verified_without_receipts(self):
        self.invalid(lambda c: c['leaves'][0].update(currentStatus='verified'), 'verified requires')
    def test_aggregate_receipt_does_not_verify_leaf(self):
        def edit(c):
            leaf = c['leaves'][0]; leaf['currentStatus'] = 'verified'
            for gate in GATES:
                leaf['evidence'].append({'kind':gate, 'reference':'aggregate test command', 'outcome':'passed',
                                         'artifactOrRevision':'077677c', 'scope':'aggregate', 'covers':[]})
        self.invalid(edit, 'verified requires')
    def test_verified_cannot_disable_gates(self):
        def edit(c):
            leaf = c['leaves'][0]; leaf['currentStatus'] = 'verified'
            for gate in GATES: leaf['gates'][gate]['required'] = False
        self.invalid(edit, 'require test and native gates')
    def test_api_exclusion_without_proof(self):
        self.invalid(lambda c: c['leaves'][0].update(currentStatus='api_excluded'), 'exact proof')
    def test_unapproved_difference(self):
        self.invalid(lambda c: c['leaves'][0].update(currentStatus='accepted_difference', baselineRole='accepted_difference', acceptedDifference='invented'), 'difference requires')
    def test_unqualified_workflow_cannot_gain_requirements(self):
        def edit(c):
            leaf = next(l for l in c['leaves'] if l['currentStatus'] == 'unqualified')
            leaf['gates']['live']['required'] = True
        self.invalid(edit, 'do not invent')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--contract', type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument('--ledger', type=Path, default=DEFAULT_LEDGER)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    try:
        contract = json.loads(args.contract.read_text())
        parents = ledger_ids(args.ledger)
        errors = validate(contract, parents)
    except (ValueError, TypeError, OSError) as error:
        print(f'FAIL: {error}', file=sys.stderr); return 1
    if errors:
        print('\n'.join('FAIL: ' + e for e in errors), file=sys.stderr); return 1
    if args.self_test:
        RejectionTests.contract, RejectionTests.parents = contract, parents
        result = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromTestCase(RejectionTests))
        if not result.wasSuccessful(): return 1
    counts = Counter(l['currentStatus'] for l in contract['leaves'])
    print(f'PASS: {len(contract["leaves"])} leaves / {len(parents)} parents; ' + ', '.join(f'{k}={v}' for k,v in sorted(counts.items())))
    print('This is contract/evidence-shape validation, not implementation or full-parity acceptance.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
