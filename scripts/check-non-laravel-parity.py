#!/usr/bin/env python3
"""Build/test selected non-Laravel packages and validate the exact new components.

The API 1.4 checker is built separately with:
  cargo build --manifest-path scripts/api14-package-checker/Cargo.toml
No packages are installed, published, or copied into dist. JSON/logs go to --output.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tempfile
import tomllib
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def fingerprint(directory):
    digest = hashlib.sha256()
    for base, directories, filenames in os.walk(directory):
        directories[:] = sorted(d for d in directories if d not in ('target','dist','.git','node_modules','__pycache__'))
        for filename in sorted(filenames):
            path = Path(base)/filename
            if path.is_symlink():
                content = os.readlink(path).encode()
            else:
                content = path.read_bytes()
            digest.update(path.relative_to(directory).as_posix().encode()+b'\0'+content+b'\0')
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plugin', action='append', default=[], help='Package name, repeatable; defaults to all except Laravel')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--reuse', type=Path, action='append', default=[], help='Reuse a passing results.json only when source, shared crates, tools and exact artifact still match')
    parser.add_argument('--checker', type=Path, default=ROOT/'scripts/api14-package-checker/target/debug/openidea-parity-package-checker')
    parser.add_argument('--cli', type=Path, default=ROOT/'scripts/api14-package-checker/target/debug/openidea-plugin', help='Released API 1.4 CLI for full installer validation in a temporary package')
    parser.add_argument('--timeout', type=int, default=900)
    args = parser.parse_args()
    known = {p.parent.name: p.parent for p in (ROOT/'plugins').glob('*/plugin.toml') if p.parent.name != 'laravel'}
    if set(args.plugin)-known.keys(): parser.error('Unknown or excluded plugin: '+', '.join(sorted(set(args.plugin)-known.keys())))
    if args.jobs < 1 or args.timeout < 1: parser.error('jobs and timeout must be positive')
    output = args.output.resolve()
    if output == ROOT or ROOT.is_relative_to(output) or output.is_relative_to(ROOT/'plugins') or output.is_relative_to(ROOT/'crates') or output.is_relative_to(ROOT/'scripts'):
        parser.error('output must not overlap source directories')
    output.mkdir(parents=True, exist_ok=True)
    checker = args.checker.resolve()
    cli = args.cli.resolve()
    tools = {'checker':hashlib.sha256(checker.read_bytes()).hexdigest() if checker.is_file() else None,
             'cli':hashlib.sha256(cli.read_bytes()).hexdigest() if cli.is_file() else None,
             'runner':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             'rustc':subprocess.run(['rustc','--version'],capture_output=True,text=True).stdout.strip()}
    reusable = {}
    for previous in args.reuse:
        receipt = previous/'results.json' if previous.is_dir() else previous
        for record in json.loads(receipt.read_text()).get('packages',[]):
            if record.get('pass'):reusable[record['plugin']] = (record,receipt.parent)


    def run(package):
        directory = known[package]
        record = {'plugin': package, 'steps': {}}
        record['source_before'] = fingerprint(directory)
        record['shared_before'] = fingerprint(ROOT/'crates')
        record['tools'] = tools
        if package in reusable:
            old,previous = reusable[package]
            old_artifact = old.get('artifact',{})
            old_path = Path(old_artifact.get('path',''))
            if old.get('tools')==tools and old.get('source_after')==record['source_before'] and old.get('shared_after')==record['shared_before'] and old_path.is_file() and hashlib.sha256(old_path.read_bytes()).hexdigest()==old_artifact.get('sha256'):
                reused = {**old,'reused_from':str(previous/'results.json')}
                for log in previous.glob(package+'-*.log'):
                    destination = output/log.name
                    if log.resolve()!=destination.resolve():shutil.copy2(log,destination)
                (output/f'{package}.json').write_text(json.dumps(reused,indent=2)+'\n')
                print(f'{package}: PASS (unchanged verified source/artifact/tools)',flush=True)
                return reused
        artifact = None
        commands = [('test', ['cargo','test','--locked']), ('wasm', ['cargo','build','--locked','--release','--target','wasm32-wasip2','--message-format=json-render-diagnostics'])]
        for label, command in commands:
            start = time.monotonic()
            try:
                result = subprocess.run(command, cwd=directory, capture_output=True, text=True, timeout=args.timeout)
                text = result.stdout + result.stderr
                (output/f'{package}-{label}.log').write_text(text)
                record['steps'][label] = {'exit':result.returncode, 'seconds':round(time.monotonic()-start,2)}
                if label == 'test':
                    counts = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed', text)
                    record['tests'] = {'passed':sum(int(a) for a,b in counts), 'failed':sum(int(b) for a,b in counts)}
                if label == 'wasm' and result.returncode == 0:
                    for line in result.stdout.splitlines():
                        try: item=json.loads(line)
                        except json.JSONDecodeError: continue
                        if item.get('reason') == 'compiler-artifact':
                            for filename in item.get('filenames',[]):
                                if filename.endswith('.wasm'): artifact=Path(filename)
            except subprocess.TimeoutExpired:
                record['steps'][label] = {'exit':'timeout'}
            except OSError as error:
                record['steps'][label] = {'exit':'error','reason':str(error)}
        if artifact:
            record['artifact'] = {'path':str(artifact), 'sha256':hashlib.sha256(artifact.read_bytes()).hexdigest()}
            if checker.is_file():
                try:
                    result = subprocess.run([str(checker),str(directory),str(artifact)],capture_output=True,text=True,timeout=args.timeout)
                    (output/f'{package}-component.log').write_text(result.stdout+result.stderr)
                    record['steps']['component'] = {'exit':result.returncode}
                except subprocess.TimeoutExpired: record['steps']['component'] = {'exit':'timeout'}
                except OSError as error: record['steps']['component'] = {'exit':'error','reason':str(error)}
            else: record['steps']['component'] = {'exit':'blocked','reason':f'Build released API 1.4 checker at {checker}'}
        else: record['steps']['component'] = {'exit':'blocked','reason':'no fresh Wasm artifact'}
        if artifact and cli.is_file():
            try:
                with tempfile.TemporaryDirectory(prefix='openidea-parity-package-') as temporary:
                    staged = Path(temporary)/package
                    shutil.copytree(directory, staged, ignore=shutil.ignore_patterns('target','dist','.git'))
                    manifest = tomllib.loads((staged/'plugin.toml').read_text())
                    relative_wasm = Path(manifest['wasm'])
                    if relative_wasm.is_absolute() or '..' in relative_wasm.parts:
                        raise ValueError('Wasm path escapes staged package')
                    destination = staged/relative_wasm
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(artifact, destination)
                    result = subprocess.run([str(cli),'check',str(staged)],capture_output=True,text=True,timeout=args.timeout)
                    (output/f'{package}-package.log').write_text(result.stdout+result.stderr)
                    record['steps']['package'] = {'exit':result.returncode}
            except subprocess.TimeoutExpired: record['steps']['package'] = {'exit':'timeout'}
            except (OSError, ValueError) as error: record['steps']['package'] = {'exit':'error','reason':str(error)}
        else: record['steps']['package'] = {'exit':'blocked','reason':'fresh artifact or released CLI missing'}
        record['source_after'] = fingerprint(directory)
        record['shared_after'] = fingerprint(ROOT/'crates')
        record['steps']['source_stability'] = {'exit':0 if record['source_before']==record['source_after'] and record['shared_before']==record['shared_after'] else 'changed', 'reason':'Source fingerprints must remain stable during verification'}
        record['pass'] = all(s['exit']==0 for s in record['steps'].values())
        (output/f'{package}.json').write_text(json.dumps(record,indent=2)+'\n')
        print(f'{package}: '+('PASS' if record['pass'] else 'FAIL/BLOCKED'),flush=True)
        return record

    names = sorted(set(args.plugin) or known.keys())
    with ThreadPoolExecutor(max_workers=args.jobs) as pool: records=list(pool.map(run,names))
    summary = {'scope':'Rust tests, fresh Wasm builds, released-host component checks and staged installer validation; not desktop parity', 'excluded':['laravel'], 'packages':records}
    (output/'results.json').write_text(json.dumps(summary,indent=2)+'\n')
    return 0 if all(r['pass'] for r in records) else 1


if __name__ == '__main__': raise SystemExit(main())
