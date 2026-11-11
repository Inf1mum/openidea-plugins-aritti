#!/usr/bin/env python3
"""Update the JPA parity ledger from released-host runner receipts.

Dry-run by default: prints, per partial row, the documented claims covered by passing
scenarios and which rows qualify to close. `--apply` copies the receipts into
parity/receipts/L1 and records `evidenceLevel.L1` on rows with scenarios. `--close`
(requires `--apply`) additionally sets `verified-parity` with documentation-derived
acceptance cases for rows where EVERY explicit documented claim is covered by a passing
scenario and no explicit claim needs a desktop. Documentation-derived verification is
weaker than an observed comparison with the original plugin (see PLAN.md amendment);
this script never closes a row on anything but fully passing, fully covered claims.
"""
import argparse
import json
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PARITY = ROOT / 'plugins/jpa-assist/parity'
HARNESS = 'scripts/runtime-smoke/src/bin/jpa_assist.rs'
SHA = re.compile(r'[0-9a-f]{64}')


def load_json(path):
    return json.loads(Path(path).read_text())


def explicit_claims(parity, row):
    path = parity / 'claims' / f'{row}.json'
    if not path.is_file():
        return {}
    return {c['id']: c for c in load_json(path).get('claims', []) if c.get('strength') == 'explicit'}


def read_receipts(directory):
    receipts = {}
    for path in sorted(Path(directory).glob('RS-*.json')):
        receipt = load_json(path)
        receipts[receipt['scenario']] = (path, receipt)
    return receipts


def merge_runs(directories, minimum=3):
    """Combine several runner outputs. A scenario counts as passing only if it passed in EVERY run and its
    receipts are byte-identical: the host drops provider results over its time budget, so a run on a loaded
    machine can fail spuriously, and one lucky pass proves nothing."""
    runs = [read_receipts(d) for d in directories]
    if len(runs) < minimum:
        raise SystemExit(f'need at least {minimum} runner outputs (got {len(runs)}): single runs are not reliable evidence')
    merged = {}
    for scenario in sorted(set().union(*[set(r) for r in runs])):
        present = [r[scenario] for r in runs if scenario in r]
        path, receipt = present[0]
        receipt = dict(receipt)
        texts = {Path(p).read_text() for p, _ in present}
        if len(present) != len(runs):
            receipt['status'] = 'fail'
            receipt['flaky'] = f'missing from {len(runs) - len(present)} of {len(runs)} runs'
        elif any(rc.get('status') != 'pass' for _, rc in present):
            receipt['status'] = 'fail'
            receipt['flaky'] = 'failed in at least one run' if any(rc.get('status') == 'pass' for _, rc in present) else 'failed in every run'
        elif len(texts) != 1:
            receipt['status'] = 'fail'
            receipt['flaky'] = 'receipts differ between runs (non-deterministic)'
        merged[scenario] = (path, receipt)
    return merged


def analyse(parity, ledger, receipts):
    """Return per-row analysis for rows that have at least one scenario."""
    rows = {entry['id']: entry for entry in ledger['requirements']}
    analysis = {}
    for scenario, (path, receipt) in receipts.items():
        for row in receipt.get('rows', []):
            item = analysis.setdefault(row, {'scenarios': [], 'passing': set(), 'failing': set(), 'covered': set()})
            item['scenarios'].append(scenario)
            if receipt.get('status') == 'pass':
                item['passing'].add(scenario)
                item['covered'].update(c for c in receipt.get('claimIds', []) if c.startswith(row + '-C'))
            else:
                item['failing'].add(scenario)
    for row, item in analysis.items():
        entry = rows.get(row)
        explicit = explicit_claims(parity, row)
        item['explicit'] = set(explicit)
        item['notHeadless'] = {i for i, c in explicit.items() if not c.get('verifiableHeadless')}
        item['status'] = entry['status'] if entry else None
        item['closable'] = bool(
            entry and entry['status'] == 'partial' and entry.get('implementation') and explicit
            and not item['failing'] and not item['notHeadless'] and item['explicit'] <= item['covered']
        )
    return analysis


def wasm_hashes(receipts, scenarios):
    return {receipts[s][1].get('wasmSha256') for s in scenarios}


def rel(path):
    return Path(path).resolve().relative_to(Path(ROOT).resolve()).as_posix()


def apply_level1(parity, ledger, receipts, analysis, destination):
    destination.mkdir(parents=True, exist_ok=True)
    for scenario, (path, receipt) in receipts.items():
        if 'flaky' in receipt:
            (destination / f'{scenario}.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
        else:
            shutil.copyfile(path, destination / f'{scenario}.json')
    rows = {entry['id']: entry for entry in ledger['requirements']}
    for row, item in analysis.items():
        entry = rows.get(row)
        if not entry:
            continue
        hashes = wasm_hashes(receipts, item['scenarios'])
        if len(hashes) != 1 or not SHA.fullmatch(next(iter(hashes)) or ''):
            raise SystemExit(f'{row}: scenarios were run against different or invalid wasm hashes: {hashes}')
        level = entry.setdefault('evidenceLevel', {})
        files = [rel(destination / f'{s}.json') for s in sorted(item['scenarios'])]
        level['L1'] = {
            'status': 'fail' if item['failing'] else 'pass',
            'harness': HARNESS,
            'receipt': files[0],
            'receipts': files,
            'wasmSha256': next(iter(hashes)),
            'claimsCovered': sorted(item['covered']),
            'claimsExplicit': sorted(item['explicit']),
        }


def close_rows(parity, ledger, receipts, analysis, destination, docs_date):
    rows = {entry['id']: entry for entry in ledger['requirements']}
    closed = []
    for row, item in sorted(analysis.items()):
        if not item['closable']:
            continue
        entry = rows[row]
        explicit = explicit_claims(parity, row)
        cases = []
        for scenario in sorted(item['passing']):
            receipt = receipts[scenario][1]
            ids = sorted(c for c in receipt.get('claimIds', []) if c in explicit)
            if not ids:
                continue
            sources = sorted({explicit[i]['source'] for i in ids})
            cases.append({
                'scenario': scenario,
                'referenceType': 'documentation',
                'referenceSources': sources,
                'referenceClaims': [f'{i}: {explicit[i]["statement"]}' for i in ids],
                'claimIds': ids,
                'docsRetrievedOn': docs_date,
                'referenceEvidence': sources[0],
                'implementationEvidence': rel(destination / f'{scenario}.json'),
                'test': f'jpa_assist --only {scenario}',
                'scope': f'released host v0.1.42, headless, fixture {receipt.get("fixture")}; documented claims only (documentation-derived, not an observed comparison)',
                'result': 'pass',
            })
        entry['status'] = 'verified-parity'
        entry['evidence'] = sorted({c['implementationEvidence'] for c in cases})
        entry['acceptance'] = cases
        closed.append(row)
    return closed


def report(analysis):
    lines = ['row  status    scenarios  covered/explicit  notHeadless  failing  closable']
    # failing includes scenarios that were flaky across runs
    for row, item in sorted(analysis.items()):
        lines.append(
            f'{row:<5}{str(item["status"]):<10}{len(item["scenarios"]):<11}'
            f'{len(item["covered"])}/{len(item["explicit"]):<15}{len(item["notHeadless"]):<13}{len(item["failing"]):<9}{item["closable"]}'
        )
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--runner-out', required=True, action='append', help='runner OUT directory (contains receipts/RS-*.json); repeat at least 3 times (separate runs): a scenario passes only if it passed identically in every run')
    parser.add_argument('--min-runs', type=int, default=3, help='minimum number of runner outputs (default 3)')
    parser.add_argument('--apply', action='store_true', help='copy receipts and record evidenceLevel.L1')
    parser.add_argument('--close', action='store_true', help='with --apply: close fully covered rows as verified-parity (documentation-derived)')
    parser.add_argument('--docs-date', default='2026-11-11', help='retrieval date recorded on documentation cases')
    parser.add_argument('--parity-dir', default=str(PARITY))
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args(argv)
    if args.close and not args.apply:
        parser.error('--close requires --apply')
    parity = Path(args.parity_dir)
    ledger_path = parity / 'requirements.json'
    ledger = load_json(ledger_path)
    receipts = merge_runs([Path(d) / 'receipts' for d in args.runner_out], args.min_runs)
    if not receipts:
        print('no RS-*.json receipts found', file=sys.stderr)
        return 2
    analysis = analyse(parity, ledger, receipts)
    closable = sorted(r for r, i in analysis.items() if i['closable'])
    closed = []
    if args.apply:
        destination = parity / 'receipts' / 'L1'
        apply_level1(parity, ledger, receipts, analysis, destination)
        if args.close:
            closed = close_rows(parity, ledger, receipts, analysis, destination, args.docs_date)
        ledger_path.write_text(json.dumps(ledger, indent=2, ensure_ascii=False) + '\n')
    if args.json:
        print(json.dumps({'rows': {r: {k: sorted(v) if isinstance(v, set) else v for k, v in i.items()} for r, i in analysis.items()},
                          'closable': closable, 'closed': closed, 'applied': args.apply}, indent=2))
    else:
        print(report(analysis))
        print(f'closable rows (every explicit documented claim covered and passing, none needs a desktop): {closable}')
        print(f'applied: {args.apply}; closed: {closed}')
        print('Documentation-derived closure is weaker than an observed comparison with the original plugin.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
