#!/usr/bin/env python3
"""Close extension-point rows (E/A/I/F) that map to G/S/F rows which are all verified-parity by their own evidence.

Dry-run by default; `--apply` writes the ledger. An inherited row repeats no test: it records `referenceType: inherited`
and the rows it inherits from, and the checker counts it apart. Rows with notApplicable, a knownGap (a capability of the original we lack), no mappedTo, or a target that is
open or itself inherited stay open.
"""
import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / 'plugins/jpa-assist/parity/requirements.json'


def inheritable(entries):
    by_id = {e['id']: e for e in entries}
    result = []
    for entry in entries:
        targets = entry.get('mappedTo')
        if entry['status'] == 'verified-parity' or entry.get('notApplicable') or entry.get('knownGap') or not targets or entry['id'][:1] not in 'EAIF':
            continue
        rows = [by_id.get(t) for t in targets]
        if all(r and r['status'] == 'verified-parity' and not any(
                c.get('referenceType') == 'inherited' for c in r.get('acceptance') or []) for r in rows):
            result.append((entry, rows))
    return result


def apply(entry, rows):
    implementation = sorted({i for r in rows for i in r.get('implementation') or []})
    evidence = sorted({i for r in rows for i in r.get('evidence') or []})
    entry['status'] = 'verified-parity'
    entry['implementation'] = implementation
    entry['evidence'] = evidence
    entry['acceptance'] = [{
        'scenario': 'inherited from ' + ', '.join(r['id'] for r in rows),
        'referenceType': 'inherited',
        'inheritedFrom': [r['id'] for r in rows],
        'scope': 'registered extension point of the original, implemented through the mapped user-visible rows; no separate test',
        'result': 'pass',
    }]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--ledger', type=Path, default=LEDGER)
    args = parser.parse_args(argv)
    data = json.loads(args.ledger.read_text())
    found = inheritable(data['requirements'])
    print(f'inheritable rows: {len(found)}')
    print(', '.join(e['id'] for e, _ in found))
    if args.apply:
        for entry, rows in found:
            apply(entry, rows)
        args.ledger.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n')
        print('applied')
    return 0


if __name__ == '__main__':
    sys.exit(main())
