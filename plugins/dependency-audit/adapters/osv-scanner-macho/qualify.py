import subprocess,json,pathlib,datetime,os
import argparse
parser=argparse.ArgumentParser();parser.add_argument('--scanner',required=True,type=pathlib.Path);parser.add_argument('--work-dir',required=True,type=pathlib.Path);args=parser.parse_args()
if args.work_dir.exists():parser.error('work-dir must be new')
base=args.work_dir.resolve();base.mkdir(parents=True);root=base/'rust-fixture';(root/'src').mkdir(parents=True)
(root/'Cargo.toml').write_text('[package]\nname="macho-call-fixture"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nsmallvec="=0.6.9"\n')
(root/'Cargo.lock').write_text('version = 4\n[[package]]\nname = "macho-call-fixture"\nversion = "0.1.0"\ndependencies = ["smallvec"]\n[[package]]\nname = "smallvec"\nversion = "0.6.9"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "c4488ae950c49d403731982257768f48fada354a5203fe81f9bb6f43ca9002be"\n')
scanner=args.scanner.resolve();version=subprocess.check_output([str(scanner),'--version'],text=True)
if 'osv-scanner version: 2.7.1-openidea-macho.1' not in version.splitlines():raise SystemExit('Wrong profile version')
results=[];env=dict(os.environ)
for mode in ['called','uncalled']:
 body='let mut values=smallvec::SmallVec::<[u8; 1]>::new(); values.grow(std::hint::black_box(4)); println!("{}",values.capacity());' if mode=='called' else 'println!("fixture without SmallVec grow");'
 (root/'src/main.rs').write_text('fn main() { '+body+' }\n')
 output=base/f'rust-{mode}-result.json';args=[str(scanner),'scan','source','--call-analysis=rust','--no-call-analysis=go','--format=json','--all-vulns','--all-packages',f'--output-file={output}',str(root)]
 p=subprocess.run(args,env=env,capture_output=True,text=True,timeout=240);(base/f'rust-{mode}.log').write_text(p.stdout+'\n'+p.stderr)
 facts=[]
 if output.exists():
  v=json.loads(output.read_text())
  for source in v.get('results',[]):
   for package in source.get('packages',[]):
    for group in package.get('groups',[]):facts.append({'package':package['package'],'ids':group.get('ids'),'analysis':group.get('experimental_analysis')})
 results.append({'mode':mode,'argv':args,'status':p.returncode,'facts':facts,'output':str(output),'log':str(base/f'rust-{mode}.log')})
result={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'version':subprocess.check_output([str(scanner),'--version'],text=True),'platform':'Darwin arm64','rust':subprocess.check_output(['rustc','--version'],text=True),'results':results}
(base/'qualification.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))

for result in results:
 assert result['status']==1,result
 target=[f for f in result['facts'] if 'RUSTSEC-2019-0009' in (f['ids'] or [])]
 assert len(target)==1,target
 assert target[0]['analysis']['RUSTSEC-2019-0009']['called']==(result['mode']=='called'),target
 unknown=[f for f in result['facts'] if 'RUSTSEC-2018-0018' in (f['ids'] or [])]
 assert unknown and unknown[0]['analysis'] is None,unknown
