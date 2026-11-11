#!/usr/bin/env python3
"""Explicit offline integration test; requires pinned runtime JARs, never downloads or runs a project build."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, zipfile
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--runtime',type=pathlib.Path,required=True)
parser.add_argument('--output',type=pathlib.Path)
parser.add_argument('--profile',choices=['hibernate-6.6.49-h2-v1','hibernate-6.6.49-offline-v2','hibernate-7.4.15-offline-v2'],default='hibernate-6.6.49-h2-v1')
parser.add_argument('--dialect',default='h2')
parser.add_argument('--matrix',action='store_true',help='Run every qualified preset for this ORM version, without a DB connection')
parser.add_argument('--jpa32',action='store_true',help='Include actual JPA3.2 annotation fixture (Hibernate7 only)')
args=parser.parse_args();plugin=pathlib.Path(__file__).resolve().parents[2]
version='7.4.15' if args.profile.startswith('hibernate-7.') else '6.6.49'
if args.profile.endswith('h2-v1') and args.dialect!='h2':parser.error('The legacy profile is H2 only; choose offline-v2 for other dialects')
if args.jpa32 and version!='7.4.15':parser.error('JPA3.2 requires the pinned Hibernate7 profile')
lock=json.loads((plugin/f'adapters/hibernate-{version}-runtime.json').read_text())
if args.output:base=args.output.resolve();base.mkdir(parents=True,exist_ok=False)
else:base=pathlib.Path(tempfile.mkdtemp(prefix='openidea-hibernate-fixture-'))
for directory in ['runtime','classes','private']:(base/directory).mkdir()
for name,expected in lock.items():
 source=args.runtime/name;assert hashlib.sha256(source.read_bytes()).hexdigest()==expected,name;shutil.copy2(source,base/'runtime'/name)
shutil.copytree(pathlib.Path(__file__).parent/'src',base/'src')
if args.jpa32:
 for path in (pathlib.Path(__file__).parent/'src7/example').glob('*.java'):shutil.copy2(path,base/'src/example'/path.name)
env={key:value for key,value in os.environ.items() if key not in ['JAVA_TOOL_OPTIONS','JDK_JAVA_OPTIONS','_JAVA_OPTIONS','JAVA_OPTS','CLASSPATH']}
records=[]
def run(name,argv,expected=0):
 result=subprocess.run(argv,cwd=base,env=env,capture_output=True,text=True,timeout=90)
 (base/(name+'.stdout')).write_text(result.stdout);(base/(name+'.stderr')).write_text(result.stderr);records.append({'name':name,'argv':argv,'exit':result.returncode});(base/'results.json').write_text(json.dumps(records,indent=2))
 assert (result.returncode==0)==(expected==0),(name,result.stderr)
 return result
classpath=os.pathsep.join(str(base/'runtime'/name) for name in lock)
run('compile',['javac','--release','17','-proc:none','-cp',classpath,'-d',str(base/'classes')]+[str(path) for path in sorted((base/'src/example').glob('*.java'))])
jars=['runtime/'+name for name in lock];roots=['classes'];sources=['src/example/Customer.java','src/example/Department.java','src/example/EmailConverter.java'];entities=['example.Customer|classes','example.Department|classes'];converters=['example.EmailConverter|classes']
if args.jpa32:sources.append('src/example/Jpa32Entity.java');entities.append('example.Jpa32Entity|classes')
def parameters(jars=jars):
 result=[]
 for entries in [jars,roots,sources,entities,converters]:result += [str(len(entries))]+entries
 return result
helper_source=(plugin/'adapters/OfflineHibernateSchema.java').read_bytes();helper=base/'private'/('HibernateSchema_'+hashlib.sha256(helper_source).hexdigest()[:20]+'.java');helper.write_bytes(helper_source)
common=['java','-Dfile.encoding=UTF-8','--class-path',str(base/'private'),str(helper)]
selection=[args.profile,args.dialect];marker=base/'converter-instantiated.txt'
def payload(result):return result.stdout.rsplit('OPENIDEA_HIBERNATE_SCHEMA/1\n',1)[1].strip()
def generate(name,selection):
 proof=payload(run(name+'-inspect',common+['inspect',str(base)]+selection+parameters()))
 result=json.loads(payload(run(name+'-generate',common+['generate',str(base)]+selection+parameters()+[proof])))
 assert result['proof']==proof and result['hibernateJdbcRequests']==0 and result['version']==version+'.Final'
 sql=result['sql'].lower();assert all(fragment in sql for fragment in ['customers','departments','foreign key','uq_customer_email'])
 assert result['databaseVersion'] and marker.exists()
 (base/(name+'.sql')).write_text(result['sql'])
 return proof,result
proof=payload(run('inspect',common+['inspect',str(base)]+selection+parameters()))
assert not marker.exists(),'inspection must not initialize project classes'
result=json.loads(payload(run('generate',common+['generate',str(base)]+selection+parameters()+[proof])))
assert result['proof']==proof and result['hibernateJdbcRequests']==0
assert all(fragment in result['sql'].lower() for fragment in ['customers','departments','foreign key','uq_customer_email'])
if args.jpa32:assert 'nonnegative_amount' in result['sql'].lower() and 'jpa32_records' in result['sql'].lower()
assert marker.exists(),'trusted converter construction must be disclosed, not assumed inert'
(base/'generated.sql').write_text(result['sql']);marker.unlink()
source=base/'src/example/Customer.java';original=source.read_text();source.write_text(original+'\n// source drift\n')
run('source-drift-rejected',common+['generate',str(base)]+selection+parameters()+[proof],expected=1);assert not marker.exists();source.write_text(original)
(base/'classes/hibernate.properties').write_text('hibernate.hbm2ddl.auto=create\nhibernate.connection.url=jdbc:forbidden:application\n')
(base/'classes/import.sql').write_text('INSERT INTO forbidden VALUES (1);\n');(base/'classes/META-INF').mkdir()
(base/'classes/META-INF/persistence.xml').write_text('<persistence><persistence-unit name="must-not-bootstrap"><properties><property name="jakarta.persistence.jdbc.url" value="jdbc:forbidden:unit"/></properties></persistence-unit></persistence>')
_,result2=generate('ambient-config',selection);assert result2['sql']==result['sql'] and 'insert into forbidden' not in result2['sql'].lower()
marker.unlink(missing_ok=True)
with zipfile.ZipFile(base/'runtime/implicit.jar','w') as archive:archive.writestr('META-INF/MANIFEST.MF','Manifest-Version: 1.0\nClass-Path: http://127.0.0.1:1/not-requested.jar\n\n')
run('implicit-classpath-rejected',common+['inspect',str(base)]+selection+parameters(jars+['runtime/implicit.jar']),expected=1);assert not marker.exists()
summary={'profile':args.profile,'dialect':args.dialect,'databaseVersion':result['databaseVersion'],'runtime_sha256':lock,'helper_sha256':hashlib.sha256(helper_source).hexdigest(),'hibernateJdbcRequests':0,'trusted_converter_constructor_executed':True,'source_drift_rejected_before_metadata':True,'application_configuration_ignored':True,'implicit_manifest_classpath_rejected':True,'jpa32_annotations_verified':args.jpa32}
if args.matrix:
 presets=['h2','postgresql','mysql','mariadb','sqlserver','azure-sqlserver','oracle','cockroachdb','db2','db2i','db2z','hsql','hana','postgres-plus']+(['derby','tidb'] if version.startswith('6.') else ['spanner','spanner-postgresql'])
 matrix=[]
 for dialect in presets:
  _,result=generate('matrix-'+dialect,['hibernate-'+version+'-offline-v2',dialect]);
  golden=pathlib.Path(__file__).parent/'expected'/f'hibernate-{version}-{dialect}.sql'
  if golden.exists():assert result['sql']==golden.read_text(),('real DDL drifted from pinned fixture',dialect)
  matrix.append({'dialect':dialect,'dialectClass':result['dialect'],'databaseVersion':result['databaseVersion'],'hibernateJdbcRequests':0,'sqlSha256':hashlib.sha256(result['sql'].encode()).hexdigest()})
 summary['matrix']=matrix
(base/'summary.json').write_text(json.dumps(summary,indent=2));print(base)
