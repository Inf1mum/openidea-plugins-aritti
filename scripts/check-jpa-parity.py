#!/usr/bin/env python3
"""Validate the parity ledger. --release refuses every unclosed parity gap."""
import argparse
import collections
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
STATUSES = {'unverified', 'missing', 'partial', 'blocked', 'verified-parity'}
REQUIRED_IDS = {f'{prefix}{i:03}' for prefix, count in [('G', 134), ('A', 44), ('I', 12), ('S', 7), ('F', 19), ('E', 139)] for i in range(1, count + 1)}


def validate(data):
    errors = []
    if data.get('schemaVersion') != 1:
        errors.append('Unsupported ledger schema')
    reference = data.get('reference', {})
    if reference.get('plugin') != 'com.haulmont.jpab' or not reference.get('version') or not reference.get('archiveSha256'):
        errors.append('A pinned original plugin/version/hash is required')
    requirements = data.get('requirements')
    if not isinstance(requirements, list) or not requirements:
        return ['A nonempty requirements inventory is required']
    ids = set()
    for entry in requirements:
        key = entry.get('id')
        if not isinstance(key, str) or not key or key in ids:
            errors.append(f'Duplicate or missing requirement ID: {key}')
        ids.add(key)
        if entry.get('status') not in STATUSES:
            errors.append(f'{key}: invalid status')
        if not entry.get('capability') or not entry.get('family'):
            errors.append(f'{key}: capability/family required')
        mapped, na = entry.get('mappedTo'), entry.get('notApplicable')
        if mapped is not None and na is not None:
            errors.append(f'{key}: mappedTo and notApplicable are mutually exclusive')
        if mapped is not None and (not isinstance(mapped, list) or not mapped or not all(isinstance(m, str) and m[:1] in 'GSF' for m in mapped)):
            errors.append(f'{key}: mappedTo must be a nonempty list of G/S/F ids')
        if na is not None and (not isinstance(na, str) or not na.strip()):
            errors.append(f'{key}: notApplicable needs a non-empty reason')
        if entry.get('status') == 'blocked' and not entry.get('blocker'):
            errors.append(f'{key}: blocked requirements need an exact blocker')
        if entry.get('status') == 'verified-parity':
            acceptance = entry.get('acceptance')
            if not entry.get('implementation') or not entry.get('evidence') or not acceptance:
                errors.append(f'{key}: verified parity needs implementation, evidence and acceptance cases')
            for case in acceptance or []:
                if not isinstance(case, dict) or not all(case.get(k) for k in ('scenario', 'referenceEvidence', 'implementationEvidence', 'test', 'scope')) or case.get('result') != 'pass':
                    errors.append(f'{key}: incomplete differential acceptance case')
            for evidence in entry.get('evidence', []):
                if not isinstance(evidence, str):
                    errors.append(f'{key}: invalid evidence path')
                    continue
                path = (ROOT / evidence).resolve()
                if not path.is_relative_to(ROOT) or not path.is_file():
                    errors.append(f'{key}: missing local evidence {evidence}')
    for entry in requirements:
        for target in entry.get('mappedTo') or []:
            if isinstance(target, str) and target not in ids:
                errors.append(f"{entry.get('id')}: mappedTo references unknown id {target}")
    if REQUIRED_IDS - ids:
        errors.append('Missing pinned parity requirements: ' + ', '.join(sorted(REQUIRED_IDS - ids)))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ledger', type=Path, default=ROOT / 'plugins/jpa-assist/parity/requirements.json')
    parser.add_argument('--release', action='store_true')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    try:
        data = json.loads(args.ledger.read_text())
        errors = validate(data)
    except (OSError, ValueError, TypeError) as error:
        errors = [str(error)]
        data = {}
    entries = data.get('requirements', [])
    counts = dict(collections.Counter(e.get('status', 'invalid') for e in entries))
    gaps = [e.get('id') for e in entries if e.get('status') != 'verified-parity']
    derived = [e for e in entries if str(e.get('id', ''))[:1] in 'EAI']
    mapping = {
        'mapped': sum(1 for e in derived if e.get('mappedTo')),
        'notApplicable': sum(1 for e in derived if e.get('notApplicable')),
        'unmapped': sum(1 for e in derived if not e.get('mappedTo') and not e.get('notApplicable')),
    }
    passed = not errors and (not args.release or not gaps)
    summary = {'valid': not errors, 'fullParity': not errors and not gaps and bool(entries), 'requirements': len(entries), 'statuses': counts, 'openGaps': gaps, 'mapping': mapping, 'errors': errors}
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print(f'{len(entries)} requirements; {len(gaps)} open gaps; full parity: {summary["fullParity"]}')
        print('E/A/I mapping: {mapped} mapped, {notApplicable} not-applicable, {unmapped} unmapped (all still open gaps)'.format(**mapping))
        for error in errors:
            print(error, file=sys.stderr)
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
