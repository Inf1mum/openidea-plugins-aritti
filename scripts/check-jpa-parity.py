#!/usr/bin/env python3
"""Validate the parity ledger. --release refuses every unclosed parity gap."""
import argparse
import collections
import json
import re
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
STATUSES = {'unverified', 'missing', 'partial', 'blocked', 'verified-parity'}
CLAIMS_DIR = ROOT / 'plugins/jpa-assist/parity/claims'
LEVELS = ('L0', 'L1', 'L2', 'L3')
LEVEL_STATES = {'pass', 'fail', 'none'}
REFERENCE_TYPES = {'observed', 'documentation', 'source-reading', 'inherited'}
REQUIRED_IDS = {f'{prefix}{i:03}' for prefix, count in [('G', 134), ('A', 44), ('I', 12), ('S', 7), ('F', 19), ('E', 139)] for i in range(1, count + 1)}


def repo_file(value):
    if not isinstance(value, str) or not value:
        return False
    path = (ROOT / value).resolve()
    return path.is_relative_to(ROOT) and path.is_file()


def strings(value):
    return isinstance(value, list) and bool(value) and all(isinstance(v, str) and v.strip() for v in value)


def validate_evidence_level(key, level):
    """Optional per-row progress record; never affects status rules."""
    if not isinstance(level, dict) or not set(level) <= set(LEVELS):
        return [f'{key}: evidenceLevel must be an object with keys among L0..L3']
    errors = []
    for name, item in level.items():
        if not isinstance(item, dict) or item.get('status') not in LEVEL_STATES:
            errors.append(f'{key}: evidenceLevel.{name}.status must be pass, fail or none')
            continue
        passing = item['status'] == 'pass'
        files = []
        if name == 'L0':
            if 'tests' in item and not strings(item['tests']):
                errors.append(f'{key}: evidenceLevel.L0.tests must be a nonempty list of names')
            if passing and (not strings(item.get('tests')) or not item.get('commit')):
                errors.append(f'{key}: evidenceLevel.L0 pass needs tests and commit')
        elif name == 'L1':
            files = [item.get('harness'), item.get('receipt')]
            if passing and (not all(files) or not re.fullmatch(r'[0-9a-f]{64}', str(item.get('wasmSha256', '')))):
                errors.append(f'{key}: evidenceLevel.L1 pass needs harness, receipt and 64-hex wasmSha256')
            if 'wasmSha256' in item and not re.fullmatch(r'[0-9a-f]{64}', str(item['wasmSha256'])):
                errors.append(f'{key}: evidenceLevel.L1.wasmSha256 must be 64 hex characters')
            files = [f for f in files if f is not None]
        else:
            refs = item.get('receipts')
            if (passing or refs is not None) and not strings(refs):
                errors.append(f'{key}: evidenceLevel.{name}.receipts must be a nonempty list of paths')
            files = refs if isinstance(refs, list) else []
        for ref in files:
            if not repo_file(ref):
                errors.append(f'{key}: evidenceLevel.{name} missing file {ref}')
    return errors


def claim_ids(key, claims_dir):
    path = Path(claims_dir) / f'{key}.json'
    if not path.is_file():
        return None
    doc = json.loads(path.read_text())
    items = doc.get('claims', []) if isinstance(doc, dict) else doc
    return {c.get('id') if isinstance(c, dict) else c for c in items if not isinstance(c, dict) or c.get('strength', 'explicit') == 'explicit'}


def validate_documentation_case(key, case):
    errors = []
    if not all(case.get(k) for k in ('scenario', 'implementationEvidence', 'test', 'scope')) or case.get('result') != 'pass':
        errors.append(f'{key}: incomplete documentation acceptance case')
    if not repo_file(case.get('implementationEvidence')):
        errors.append(f'{key}: documentation case implementationEvidence must be an existing file')
    sources = case.get('referenceSources')
    if not strings(sources) or not all(u.startswith('https://') for u in sources):
        errors.append(f'{key}: documentation case needs referenceSources as nonempty https URLs')
    if not strings(case.get('referenceClaims')):
        errors.append(f'{key}: documentation case needs nonempty referenceClaims')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', str(case.get('docsRetrievedOn', ''))):
        errors.append(f'{key}: documentation case needs docsRetrievedOn (YYYY-MM-DD)')
    return errors


def validate_source_reading_case(key, case):
    errors = []
    if not all(case.get(k) for k in ('scenario', 'implementationEvidence', 'test', 'scope')) or case.get('result') != 'pass':
        errors.append(f'{key}: incomplete source-reading acceptance case')
    if not repo_file(case.get('implementationEvidence')):
        errors.append(f'{key}: source-reading case implementationEvidence must be an existing file')
    files = case.get('referenceFiles')
    if not strings(files) or not all(f.startswith('audit/jpa-buddy-') and '..' not in f for f in files):
        errors.append(f'{key}: source-reading case needs referenceFiles under audit/jpa-buddy-<version>/ (read-only, never copied)')
    if not strings(case.get('referenceClaims')):
        errors.append(f'{key}: source-reading case needs nonempty referenceClaims (the behaviour read, in our own words)')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', str(case.get('sourceReadOn', ''))):
        errors.append(f'{key}: source-reading case needs sourceReadOn (YYYY-MM-DD)')
    return errors


def validate_inherited_case(key, case, entries):
    errors = []
    sources = case.get('inheritedFrom')
    if not strings(sources):
        return [f'{key}: inherited case needs inheritedFrom (mapped G/S/F row ids)']
    if case.get('result') != 'pass' or not case.get('scope'):
        errors.append(f'{key}: incomplete inherited acceptance case')
    for source in sources:
        target = entries.get(source)
        if target is None or source == key:
            errors.append(f'{key}: inheritedFrom {source} is unknown or self')
        elif target.get('status') != 'verified-parity' or any(
                isinstance(c, dict) and c.get('referenceType') == 'inherited' for c in target.get('acceptance') or []):
            errors.append(f'{key}: inheritedFrom {source} must itself be verified-parity by its own evidence')
    return errors


def validate(data, claims_dir=CLAIMS_DIR):
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
    by_id = {e.get('id'): e for e in requirements if isinstance(e, dict)}
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
        if 'evidenceLevel' in entry:
            errors.extend(validate_evidence_level(key, entry['evidenceLevel']))
        if entry.get('status') == 'blocked' and not entry.get('blocker'):
            errors.append(f'{key}: blocked requirements need an exact blocker')
        if entry.get('status') == 'verified-parity':
            acceptance = entry.get('acceptance')
            if not entry.get('implementation') or not entry.get('evidence') or not acceptance:
                errors.append(f'{key}: verified parity needs implementation, evidence and acceptance cases')
            for case in acceptance or []:
                if isinstance(case, dict) and case.get('referenceType', 'observed') not in REFERENCE_TYPES:
                    errors.append(f'{key}: unknown referenceType {case.get("referenceType")}')
                elif isinstance(case, dict) and case.get('referenceType') == 'documentation':
                    errors.extend(validate_documentation_case(key, case))
                elif isinstance(case, dict) and case.get('referenceType') == 'source-reading':
                    errors.extend(validate_source_reading_case(key, case))
                elif isinstance(case, dict) and case.get('referenceType') == 'inherited':
                    errors.extend(validate_inherited_case(key, case, by_id))
                elif not isinstance(case, dict) or not all(case.get(k) for k in ('scenario', 'referenceEvidence', 'implementationEvidence', 'test', 'scope')) or case.get('result') != 'pass':
                    errors.append(f'{key}: incomplete differential acceptance case')
            try:
                required = claim_ids(key, claims_dir)
            except (OSError, ValueError, AttributeError):
                required = None
                errors.append(f'{key}: unreadable claims file')
            if required:
                covered = set()
                for case in acceptance or []:
                    if isinstance(case, dict) and isinstance(case.get('claimIds'), list):
                        covered.update(case['claimIds'])
                if required - covered:
                    errors.append(f'{key}: documented claims without acceptance case: ' + ', '.join(sorted(map(str, required - covered))))
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


def coverage(entries):
    table = {name: 0 for name in (*LEVELS, 'none')}
    for e in entries:
        level = e.get('evidenceLevel') if isinstance(e.get('evidenceLevel'), dict) else {}
        passing = [n for n in LEVELS if isinstance(level.get(n), dict) and level[n].get('status') == 'pass']
        table[passing[-1] if passing else 'none'] += 1
    return table


def verified_split(entries):
    """(observed, documentation) rows. A row is as weak as its weakest case: documentation < source-reading < observed.
    Rows resting on source-reading but no documentation case are listed by `verified_source_reading`, not here."""
    observed, documentation = [], []
    for e in entries:
        if e.get('status') != 'verified-parity':
            continue
        types = {c.get('referenceType', 'observed') for c in e.get('acceptance') or [] if isinstance(c, dict)}
        if 'documentation' in types:
            documentation.append(e.get('id'))
        elif not types & {'source-reading', 'inherited'}:
            observed.append(e.get('id'))
    return observed, documentation


def verified_inherited(entries):
    return [e.get('id') for e in entries if e.get('status') == 'verified-parity'
            and any(isinstance(c, dict) and c.get('referenceType') == 'inherited' for c in e.get('acceptance') or [])]


def verified_source_reading(entries):
    result = []
    for e in entries:
        if e.get('status') != 'verified-parity':
            continue
        types = {c.get('referenceType', 'observed') for c in e.get('acceptance') or [] if isinstance(c, dict)}
        if 'source-reading' in types and not types & {'documentation', 'inherited'}:
            result.append(e.get('id'))
    return result


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
    observed, documentation = verified_split(entries)
    source_reading = verified_source_reading(entries)
    inherited = verified_inherited(entries)
    levels = coverage(entries)
    passed = not errors and (not args.release or not gaps)
    summary = {'valid': not errors, 'fullParity': not errors and not gaps and bool(entries), 'requirements': len(entries), 'statuses': counts, 'openGaps': gaps, 'mapping': mapping, 'evidenceCoverage': levels, 'verifiedObservedReference': observed, 'verifiedDocumentationReference': documentation, 'verifiedSourceReadingReference': source_reading, 'verifiedInherited': inherited, 'errors': errors}
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print(f'{len(entries)} requirements; {len(gaps)} open gaps; full parity: {summary["fullParity"]}')
        print('E/A/I mapping: {mapped} mapped, {notApplicable} not-applicable, {unmapped} unmapped (all still open gaps)'.format(**mapping))
        print('Evidence coverage (rows by highest passing level): ' + ', '.join(f'{k}={v}' for k, v in levels.items()))
        print(f'Verified with observed reference: {len(observed)}; verified with documentation-derived reference (weaker): {len(documentation)}')
        print(f'Verified from reading the original source (read-only, between documentation and observed): {len(source_reading)}')
        print(f'Verified by inheritance from verified G/S/F rows (no independent test): {len(inherited)}')
        if documentation:
            print('Documentation-derived: ' + ', '.join(map(str, documentation)))
        for error in errors:
            print(error, file=sys.stderr)
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
