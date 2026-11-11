#!/usr/bin/env python3
"""Prepare type-preserving localization of reviewed direct Rust UI literals.

Requires a fresh audit manifest and explicit --file ownership list. Format macro
literals and dynamic templates are never rewritten. Outputs an English key payload
for human translation; it never fills missing translations with English.
"""
import argparse
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--manifest',type=Path,required=True)
parser.add_argument('--file',action='append',required=True)
parser.add_argument('--keys',type=Path,required=True)
parser.add_argument('--apply',action='store_true')
args=parser.parse_args()
report=json.loads(args.manifest.read_text())
catalog=json.loads((ROOT/'crates/ui-kit/i18n/catalog.json').read_text())
payload={};total=0
for relative in args.file:
    path=(ROOT/relative).resolve()
    if not path.is_relative_to(ROOT/'plugins/laravel/src') or path.suffix!='.rs':raise SystemExit('Only owned Laravel Rust source files are accepted')
    name=str(path.relative_to(ROOT));source=path.read_text()
    if hashlib.sha256(path.read_bytes()).hexdigest()!=report['sourceDigests'].get(name):raise SystemExit(f'Stale source audit: {name}')
    sites=[site for site in report['sites'] if site['file']==name and site['status']=='unwrapped' and site['confidence']=='confirmed' and not site['formatLiteral'] and not site.get('constantContext') and site['sourceStart'] is not None]
    spans={}
    for site in sites:
        start,end=site['sourceStart'],site['sourceEnd'];literal=source[start:end]
        if not (literal.startswith('"') or literal.startswith('r')):continue
        english=site['text'];existing=next((key for key,value in catalog['en'].items() if value==english),None)
        key=existing or f'native.{path.stem}.{hashlib.sha256(english.encode()).hexdigest()[:10]}'
        if not existing:payload[key]=english
        spans[(start,end)]=f'crate::ui_text::text({json.dumps(key)}, {literal})'
    for (start,end),replacement in sorted(spans.items(),reverse=True):source=source[:start]+replacement+source[end:]
    if args.apply:path.write_text(source)
    total+=len(spans)
args.keys.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'sites':total,'newKeys':len(payload),'applied':args.apply}))
