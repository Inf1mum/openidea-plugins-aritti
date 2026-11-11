#!/usr/bin/env python3
"""Explicit local build of the separately versioned experimental MachO profile."""
import argparse,hashlib,json,pathlib,platform,subprocess,tarfile,urllib.request,os
p=argparse.ArgumentParser();p.add_argument('--work-dir',required=True,type=pathlib.Path);p.add_argument('--go',required=True,type=pathlib.Path);a=p.parse_args()
a.go=a.go.resolve();a.work_dir=a.work_dir.resolve()
here=pathlib.Path(__file__).resolve().parent;m=json.loads((here/'source.json').read_text())
if platform.system()!='Darwin' or platform.machine()!='arm64':p.error('This qualified profile supports Darwin arm64 only.')
if a.work_dir.exists():p.error('work-dir must not exist; existing files are never reused or overwritten.')
probe_env=dict(os.environ);probe_env.update(GOTOOLCHAIN='local',GOWORK='off',GOENV='off',GOFLAGS='')
version=subprocess.check_output([str(a.go),'version'],text=True,env=probe_env,cwd=here).split()[2]
if version!=m['go']:p.error('Expected '+m['go']+', received '+version)
patch=here/'source.patch'
if hashlib.sha256(patch.read_bytes()).hexdigest()!=m['patch_sha256']:p.error('Patch digest mismatch')
a.work_dir.mkdir(parents=True);archive=a.work_dir/'source.tar.gz'
with urllib.request.urlopen(m['source_url'],timeout=60) as r:archive.write_bytes(r.read(64<<20))
if hashlib.sha256(archive.read_bytes()).hexdigest()!=m['source_sha256']:raise SystemExit('Official source archive digest mismatch')
with tarfile.open(archive) as tar:
 for member in tar.getmembers():
  path=pathlib.PurePosixPath(member.name)
  if path.is_absolute() or '..' in path.parts or member.issym() or member.islnk():raise SystemExit('Unsafe archive entry')
 tar.extractall(a.work_dir,filter='data')
source=a.work_dir/('osv-scanner-'+m['commit']);subprocess.run(['patch','-p1','--batch','--forward','-i',str(patch)],cwd=source,check=True)
env=dict(os.environ);env.update(GOFLAGS='',GOWORK='off',GOTOOLCHAIN='local',GOENV='off',GO111MODULE='on',CGO_ENABLED='0',GOOS='darwin',GOARCH='arm64',GOPATH=str(a.work_dir/'go-work'),GOMODCACHE=str(a.work_dir/'go-modules'),GOCACHE=str(a.work_dir/'go-build-cache'))
for name,digest in m['patched_sha256'].items():
 if hashlib.sha256((source/name).read_bytes()).hexdigest()!=digest:raise SystemExit('Patched source digest mismatch: '+name)
def go(*args,log):
 with (a.work_dir/log).open('w') as output:subprocess.run([str(a.go),*args],cwd=source,stdout=output,stderr=subprocess.STDOUT,check=True,env=env)
go('mod','verify',log='modules.log')
go('test','-mod=readonly','./internal/sourceanalysis','-run','TestOpenIDEA|Test_functionsFromDWARF|Test_extractRlibArchive','-v',log='reader-tests.log')
binary=a.work_dir/'osv-scanner-macho'
go('build','-mod=readonly','-trimpath','-buildvcs=false','-ldflags','-X github.com/google/osv-scanner/v2/internal/version.OSVVersion='+m['profile'],'-o',str(binary),'./cmd/osv-scanner',log='build.log')
go('mod','verify',log='modules-final.log')
receipt={**m,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'version':subprocess.check_output([str(binary),'--version'],text=True),'go_version':subprocess.check_output([str(a.go),'version'],text=True,env=env,cwd=source)}
(a.work_dir/'build-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(binary)
