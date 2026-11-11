#!/usr/bin/env python3
"""Validate JPA Assist acceptance receipts and report per-row coverage. Never writes requirements.json."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
PARITY = ROOT / 'plugins/jpa-assist/parity'
RESULTS = {'pass', 'fail', 'blocked'}
CHECK_VALUES = {'pass', 'fail', 'n/a'}
CHECK_KEYS = ['preview_matches', 'undo_exact', 'cancel_clean', 'stale_refused', 'crlf_ok']
REQUIRED = ['scenario', 'rows', 'host', 'fixture', 'result', 'observations', 'artifacts', 'checks', 'tester', 'date']
KNOWN = set(REQUIRED) | {'example'}
SHA = re.compile(r'^[0-9a-f]{64}$')
ROW_ID = re.compile(r'^[A-Z][0-9]{3}$')
SCENARIO_LINE = re.compile(r'^\|\s*(AC-\d{3})\s*\|([^|]*)\|', re.M)
FIXTURE_REF = re.compile(r'`(fixtures/[^`\s]+)`')
LEGACY_LINE = re.compile(r'^These codes.*$', re.M)


def parse_scenarios(text):
    """Return ({scenario: [row ids]}, [duplicate ids])."""
    scenarios, duplicates = {}, []
    for scenario, cell in SCENARIO_LINE.findall(text):
        if scenario in scenarios:
            duplicates.append(scenario)
        scenarios[scenario] = re.findall(r'\b[A-Z]\d{3}\b', cell)
    return scenarios, duplicates


def legacy_only_codes(parity):
    """Rule codes the fixtures' GOLDEN.md list as never emitted by the live provider."""
    codes = set()
    for golden in sorted((parity / 'fixtures').glob('*/GOLDEN.md')):
        for line in LEGACY_LINE.findall(golden.read_text()):
            codes.update(c for c in re.findall(r'`([^`]+)`', line) if re.fullmatch(r'jpa-[a-z-]+', c))
    return codes


def check_fixtures(parity):
    """Errors for fixture paths that do not exist and for legacy-only rule codes used as AC expectations."""
    errors = []
    acceptance = parity / 'ACCEPTANCE.md'
    if acceptance.is_file():
        text = acceptance.read_text()
        for ref in sorted(set(FIXTURE_REF.findall(text))):
            if not (parity / ref.rstrip('/')).exists():
                errors.append(f'ACCEPTANCE.md: missing fixture path {ref}')
        codes = legacy_only_codes(parity)
        for line in text.splitlines():
            scenario = re.match(r'^\|\s*(AC-\d{3})\s*\|', line)
            if not scenario:
                continue
            for code in sorted(c for c in codes if re.search(re.escape(c) + r'(?![a-z-])', line)):
                errors.append(f'ACCEPTANCE.md: {scenario.group(1)} expects {code}, which the live provider never emits')
    for path in sorted((parity / 'differential' / 'scenarios').glob('*.json')):
        try:
            data = json.loads(path.read_text())
        except ValueError as exc:
            errors.append(f'{path.name}: invalid JSON ({exc})')
            continue
        fixture = data.get('fixture')
        root = parity / 'fixtures' / str(fixture)
        if not is_str(fixture) or not root.is_dir():
            errors.append(f'{path.name}: missing fixture {fixture}')
            continue
        refs = [data.get('inputFile'), data.get('goldenRef')] + list(data.get('inputFiles') or [])
        refs += [swap.get('from') for swap in data.get('swap') or [] if isinstance(swap, dict)]
        for ref in refs:
            if ref is None:
                continue
            target = parity / ref if isinstance(ref, str) and ref.startswith('fixtures/') else root / str(ref)
            if not isinstance(ref, str) or not target.exists():
                errors.append(f'{path.name}: missing fixture path {ref}')
        if not is_str(data.get('inputFile')):
            errors.append(f'{path.name}: inputFile must be an existing path')
    return errors


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def is_str(value):
    return isinstance(value, str) and value.strip() != ''


def validate_receipt(data, receipts_dir, scenarios, row_ids):
    errors = []
    if not isinstance(data, dict):
        return ['receipt must be a JSON object']
    for key in REQUIRED:
        if key not in data:
            errors.append(f'missing field {key}')
    for key in data:
        if key not in KNOWN:
            errors.append(f'unknown field {key}')
    if 'example' in data and not isinstance(data['example'], bool):
        errors.append('example must be a boolean')
    scenario = data.get('scenario')
    if 'scenario' in data and not (isinstance(scenario, str) and re.fullmatch(r'AC-\d{3}', scenario)):
        errors.append('scenario must look like AC-001')
    elif isinstance(scenario, str) and scenario not in scenarios:
        errors.append(f'unknown scenario {scenario}')
    rows = data.get('rows')
    if 'rows' in data:
        if not isinstance(rows, list) or not all(isinstance(r, str) and ROW_ID.match(r) for r in rows) or len(set(rows)) != len(rows):
            errors.append('rows must be a list of unique ledger ids')
        else:
            for row in rows:
                if row not in row_ids:
                    errors.append(f'unknown ledger row {row}')
                elif scenario in scenarios and row not in scenarios[scenario]:
                    errors.append(f'row {row} is not covered by scenario {scenario}')
    host = data.get('host')
    if 'host' in data:
        if not isinstance(host, dict) or set(host) != {'version', 'os', 'pluginBuild'}:
            errors.append('host needs exactly version, os, pluginBuild')
        else:
            if not is_str(host['version']) or not is_str(host['os']):
                errors.append('host.version and host.os must be non-empty strings')
            if not (isinstance(host['pluginBuild'], str) and SHA.match(host['pluginBuild'])):
                errors.append('host.pluginBuild must be a lowercase sha256')
    if 'fixture' in data and not is_str(data['fixture']):
        errors.append('fixture must be a non-empty string')
    if 'result' in data and data['result'] not in RESULTS:
        errors.append('result must be pass, fail or blocked')
    if 'observations' in data and not isinstance(data['observations'], str):
        errors.append('observations must be a string')
    if 'tester' in data and not is_str(data['tester']):
        errors.append('tester must be a non-empty string')
    if 'date' in data and not (isinstance(data['date'], str) and re.fullmatch(r'\d{4}-\d{2}-\d{2}', data['date'])):
        errors.append('date must be YYYY-MM-DD')
    checks = data.get('checks')
    if 'checks' in data:
        if not isinstance(checks, dict) or set(checks) != set(CHECK_KEYS):
            errors.append(f'checks needs exactly {", ".join(CHECK_KEYS)}')
        else:
            for key, value in checks.items():
                if value not in CHECK_VALUES:
                    errors.append(f'checks.{key} must be pass, fail or n/a')
            if data.get('result') == 'pass' and 'fail' in checks.values():
                errors.append('result pass contradicts a failed check')
    artifacts = data.get('artifacts')
    if 'artifacts' in data:
        if not isinstance(artifacts, list):
            errors.append('artifacts must be a list')
        else:
            base = receipts_dir.resolve()
            for item in artifacts:
                if not isinstance(item, dict) or set(item) != {'path', 'sha256'} or not is_str(item['path']) or not (isinstance(item['sha256'], str) and SHA.match(item['sha256'])):
                    errors.append('artifact needs path and lowercase sha256')
                    continue
                target = (base / item['path']).resolve()
                if not target.is_relative_to(base):
                    errors.append(f"artifact {item['path']} escapes the receipts directory")
                elif not target.is_file():
                    errors.append(f"artifact {item['path']} does not exist")
                elif sha256(target) != item['sha256']:
                    errors.append(f"artifact {item['path']} hash mismatch")
    return errors


def run(parity):
    requirements = json.loads((parity / 'requirements.json').read_text())['requirements']
    row_ids = {r['id'] for r in requirements}
    partial = [r['id'] for r in requirements if r.get('status') == 'partial']
    scenarios, duplicates = parse_scenarios((parity / 'ACCEPTANCE.md').read_text())
    errors = [f'ACCEPTANCE.md: duplicate scenario {s}' for s in duplicates]
    for scenario, rows in scenarios.items():
        errors += [f'ACCEPTANCE.md: {scenario} cites unknown row {row}' for row in rows if row not in row_ids]
    fixture_errors = check_fixtures(parity)
    errors += fixture_errors
    receipts_dir = parity / 'receipts'
    receipts = []
    for path in sorted(receipts_dir.glob('*.json')):
        if path.name in ('receipt.schema.json', 'TEMPLATE.json'):
            continue
        try:
            data = json.loads(path.read_text())
        except ValueError as exc:
            errors.append(f'{path.name}: invalid JSON ({exc})')
            continue
        problems = validate_receipt(data, receipts_dir, scenarios, row_ids)
        errors += [f'{path.name}: {p}' for p in problems]
        if not problems:
            receipts.append(data)
    live = [r for r in receipts if r.get('example') is not True]
    coverage = {}
    for row in sorted(row_ids):
        defined = sorted(s for s, rows in scenarios.items() if row in rows)
        passed = sorted({r['scenario'] for r in live if row in r['rows'] and r['result'] == 'pass'})
        failed = sorted({r['scenario'] for r in live if row in r['rows'] and r['result'] == 'fail'})
        coverage[row] = {'partial': row in partial, 'defined': defined, 'passed': passed, 'failed': failed}
    return {
        'scenarios': len(scenarios),
        'receipts': len(receipts),
        'exampleReceipts': len(receipts) - len(live),
        'errors': errors,
        'missingFixturePaths': len([e for e in fixture_errors if 'missing fixture' in e]),
        'coverage': coverage,
        'partialWithoutScenario': [r for r in partial if not coverage[r]['defined']],
        'partialWithoutPass': [r for r in partial if not coverage[r]['passed']],
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true', help='machine-readable report')
    parser.add_argument('--require-pass', action='store_true', help='exit 1 unless every partial row has a passing non-example receipt')
    parser.add_argument('--check-fixtures', action='store_true', help='only check fixture references (ACCEPTANCE.md and DF scenarios), exit 1 on any error; also part of the default run')
    parser.add_argument('--parity-dir', type=Path, default=PARITY)
    args = parser.parse_args(argv)
    if args.check_fixtures:
        errors = check_fixtures(args.parity_dir)
        for error in errors:
            print(f'ERROR {error}')
        print(f'fixture references: {len(errors)} errors')
        return 1 if errors else 0
    report = run(args.parity_dir)
    failed = bool(report['errors']) or (args.require_pass and bool(report['partialWithoutPass']))
    if args.json:
        print(json.dumps(report, indent=2))
        return 1 if failed else 0
    for error in report['errors']:
        print(f'ERROR {error}')
    print(f"{report['scenarios']} scenarios, {report['receipts']} valid receipts ({report['exampleReceipts']} examples ignored)")
    for row, c in report['coverage'].items():
        if c['partial']:
            print(f"{row}: defined {len(c['defined'])} passed {len(c['passed'])} failed {len(c['failed'])}")
    print(f"missing fixture paths: {report['missingFixturePaths']}")
    print(f"partial rows with no scenario: {len(report['partialWithoutScenario'])}", *report['partialWithoutScenario'])
    print(f"partial rows with no passing receipt: {len(report['partialWithoutPass'])}")
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
