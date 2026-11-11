#!/usr/bin/env python3
"""Freeze current public Marketplace metadata, excluding Laravel originals.

Uses the repository's original-counterpart ID inventory. This does not download
plugin binaries or establish feature parity. Preserve compatibility branches:
the latest publication may target an older IDE build than another current release.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', type=Path, default=ROOT/'docs/parity-2026-11-08/evidence.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--additional-id', action='append', type=int, default=[], help='Additional original counterpart such as DDEV')
    args=parser.parse_args()
    original=json.loads(args.inventory.read_text())
    excluded={7532,13441}
    names={int(o['marketplace_id']):o.get('name') for o in original['originals'] if int(o['marketplace_id']) not in excluded}
    for ident in args.additional_id:
        if ident in excluded:parser.error('Laravel counterparts are excluded')
        names.setdefault(ident,None)

    def get(url):
        request=urllib.request.Request(url,headers={'User-Agent':'OpenIDEA non-Laravel parity audit'})
        with urllib.request.urlopen(request,timeout=45) as response:raw=response.read()
        return json.loads(raw),hashlib.sha256(raw).hexdigest()

    def fetch(ident):
        base=f'https://plugins.jetbrains.com/api/plugins/{ident}'
        row={'marketplace_id':ident,'name':names[ident], 'metadata_url':base,'updates_url':base+'/updates?size=50'}
        try:
            metadata,mhash=get(base);updates,uhash=get(row['updates_url'])
            row.update(status='fetched',metadata_sha256=mhash,updates_sha256=uhash,metadata=metadata,updates=updates)
        except Exception as error:row.update(status='error',error=str(error))
        print(f'{ident}: {row["status"]}',flush=True)
        return row

    with ThreadPoolExecutor(max_workers=6) as pool:rows=list(pool.map(fetch,sorted(names)))
    output={'fetched_at':datetime.now(timezone.utc).isoformat(),'excluded_original_ids':sorted(excluded),'scope':'Public metadata/version/compatibility freeze, not source or feature acceptance','originals':rows}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(output,indent=2)+'\n')
    return 0 if all(r['status']=='fetched' for r in rows) else 1


if __name__=='__main__':raise SystemExit(main())
