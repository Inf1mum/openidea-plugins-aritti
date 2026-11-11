#!/usr/bin/env python3
"""Explicit disposable H2 JDBC console acceptance; no network, app build or user database."""
import argparse,hashlib,json,os,pathlib,shutil,subprocess,time
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--runtime',required=True,type=pathlib.Path);p.add_argument('--driver',required=True,type=pathlib.Path);p.add_argument('--profile',choices=['jpql-hibernate-6.6.49-v1','jpql-hibernate-7.4.15-v1'],required=True);p.add_argument('--output',required=True,type=pathlib.Path);a=p.parse_args()
base=a.output.resolve();base.mkdir(parents=True,exist_ok=False);plugin=pathlib.Path(__file__).resolve().parents[2];version='7.4.15' if '7.4.15' in a.profile else '6.6.49';lock=json.loads((plugin/f'adapters/hibernate-{version}-runtime.json').read_text())
for d in ['runtime','classes','private','src/example']:(base/d).mkdir(parents=True,exist_ok=True)
for name,digest in lock.items():source=a.runtime/name;assert hashlib.sha256(source.read_bytes()).hexdigest()==digest;shutil.copy2(source,base/'runtime'/name)
shutil.copy2(a.driver,base/'runtime/jdbc.jar');driverhash=hashlib.sha256(a.driver.read_bytes()).hexdigest()
(base/'src/example/Customer.java').write_text('''package example; import jakarta.persistence.*; @Entity @Table(name="customers") public class Customer { @Id public Long id; @Convert(converter=NameConverter.class) public String name; public java.math.BigDecimal amount; public boolean active; public java.time.LocalDate born; public java.time.LocalDateTime stamp; public java.util.UUID reference; }''')
(base/'src/example/NameConverter.java').write_text('''package example; import jakarta.persistence.*; @Converter public class NameConverter implements AttributeConverter<String,String> { public NameConverter() { try { java.nio.file.Files.writeString(java.nio.file.Path.of("converter-initialized"),"trusted constructor"); } catch(Exception e){throw new IllegalStateException(e);} } public String convertToDatabaseColumn(String value){return value;} public String convertToEntityAttribute(String value){return value;} }''')
(base/'src/example/Application.java').write_text('''package example; public class Application { static { if(System.nanoTime()!=0)throw new IllegalStateException("Application must not initialize"); } public static void main(String[] a){throw new IllegalStateException("Application must not bootstrap");} }''')
(base/'src/example/Slow.java').write_text('''package example; public class Slow { public static long pause(long value) throws Exception { java.nio.file.Files.writeString(java.nio.file.Path.of("slow-started"),"query running");Thread.sleep(30000);return value; } }''')
(base/'src/example/Fixture.java').write_text('''package example; public class Fixture { public static void main(String[] a)throws Exception {Class.forName("org.h2.Driver"); try(var c=java.sql.DriverManager.getConnection(a[0],"sa","fixture-secret")){try(var s=c.createStatement()){if(a[1].equals("init")){s.execute("create table customers(id bigint primary key,name varchar(80),amount decimal(12,2),active boolean,born date,stamp timestamp,reference uuid)");s.execute("insert into customers values(1,'Alice',12.50,true,DATE '2000-01-02',TIMESTAMP '2000-01-02 10:20:30','00000000-0000-0000-0000-000000000001'),(2,'Bob',30.00,false,DATE '1990-02-03',TIMESTAMP '1990-02-03 11:22:33','00000000-0000-0000-0000-000000000002')");s.execute("create alias SLOW for 'example.Slow.pause'");}else{try(var r=s.executeQuery("select count(*) from customers")){r.next();if(r.getInt(1)!=2)throw new IllegalStateException("data changed");}}}}} }''')
classpath=os.pathsep.join(str(f) for f in (base/'runtime').glob('*.jar'));env={k:v for k,v in os.environ.items() if k not in ['JAVA_TOOL_OPTIONS','JDK_JAVA_OPTIONS','_JAVA_OPTIONS','JAVA_OPTS','CLASSPATH']};records=[]
def run(name,args,stdin=None,success=True):
 result=subprocess.run(args,cwd=base,env=env,input=stdin,capture_output=True,text=True,timeout=45);(base/(name+'.stdout')).write_text(result.stdout);(base/(name+'.stderr')).write_text(result.stderr);records.append({'name':name,'argv':args,'exit':result.returncode});(base/'results.json').write_text(json.dumps(records,indent=2));assert (result.returncode==0)==success,(name,result.stderr);return result
run('compile',['javac','--release','17','-proc:none','-cp',classpath,'-d',str(base/'classes')]+[str(f) for f in (base/'src/example').glob('*.java')])
url='jdbc:h2:'+str(base/'fixture');fixture=['java','-cp',classpath+os.pathsep+str(base/'classes'),'example.Fixture',url];run('init',fixture+['init'])
profile=base/'profile.json';profile.write_text('{"fixture":"manual-JDBC-target","password":"fixture-secret"}')
helper_source=(plugin/'adapters/ReviewedJpaConsole.java').read_bytes();helper=base/'private'/('JpaConsole_'+hashlib.sha256(helper_source).hexdigest()[:20]+'.java');helper.write_bytes(helper_source)
args=['java','-Dfile.encoding=UTF-8','--class-path',str(base/'private'),str(helper)]
entries=[['runtime/'+n for n in lock]+['runtime/jdbc.jar'],['classes'],['src/example/Customer.java','src/example/NameConverter.java','profile.json'],['example.Customer|classes'],['example.NameConverter|classes']];tail=[str(base),a.profile,'h2']
for entry in entries:tail += [str(len(entry))]+entry
def payload(result):return result.stdout.rsplit('OPENIDEA_JPQL_CONSOLE/1\n',1)[1].strip()
def inspect(name):return payload(run(name,args+['inspect']+tail))
proof=inspect('inspect');marker=base/'converter-initialized';assert not marker.exists(),'pure inspection initialized project code'
def stdin(proof,query,parameters):
 values=[proof,url,'sa','fixture-secret',query,'org.h2.Driver','1']
 for name,kind,value in parameters:values += [name,kind,value]
 return ''.join(v.encode().hex()+'\n' for v in values)
query='select c.name,c.amount,c.born from Customer c where c.id >= :minimum and c.active = :active order by c.id'
data=stdin(proof,query,[('minimum','long','1'),('active','boolean','true')]);result=json.loads(payload(run('select',args+['query']+tail,data)));assert result['proof']==proof;assert result['result']['rollbackRequested'];assert marker.exists(),'converter constructor must be disclosed as executable code';row=result['result']['rows'][0];assert [c['value'] for c in row]==['Alice','12.50','2000-01-02']
truncated=json.loads(payload(run('bounded',args+['query']+tail,stdin(proof,'select c.name from Customer c order by c.id',[]))))['result'];assert truncated['truncated'] and len(truncated['rows'])==1
for name,kind,value,expected in [('text','string','Alice','Alice'),('number','decimal','12.50','12.50'),('day','date','2000-01-02','2000-01-02'),('stamp','timestamp','2000-01-02T10:20:30','unused'),('reference','uuid','00000000-0000-0000-0000-000000000001','unused')]:
 field={'text':'name','number':'amount','day':'born','stamp':'stamp','reference':'reference'}[name];r=json.loads(payload(run('bind-'+kind,args+['query']+tail,stdin(proof,'select c.name from Customer c where c.'+field+' = :value',[('value',kind,value)]))));assert r['result']['rows'][0][0]['value']=='Alice'
marker.unlink();original=profile.read_text();profile.write_text(original+' ');run('profile-drift-rejected',args+['query']+tail,data,success=False);assert not marker.exists();profile.write_text(original)
source=base/'src/example/Customer.java';originalsource=source.read_text();source.write_text(originalsource+'\n// changed');run('source-drift-rejected',args+['query']+tail,data,success=False);assert not marker.exists();source.write_text(originalsource)
run('mutation-rejected',args+['query']+tail,stdin(proof,'delete from Customer',[]),success=False)
run('entity-results-rejected',args+['query']+tail,stdin(proof,'select c from Customer c',[]),success=False)
slow=stdin(proof,"select function('SLOW',c.id) from Customer c",[])
with (base/'cancel.stdout').open('w') as out,(base/'cancel.stderr').open('w') as err:
 child=subprocess.Popen(args+['query']+tail,cwd=base,env=env,stdin=subprocess.PIPE,stdout=out,stderr=err,text=True);child.stdin.write(slow);child.stdin.close()
 deadline=time.monotonic()+20
 while not (base/'slow-started').exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.05)
 assert (base/'slow-started').exists(),'actual JDBC slow query was not reached';child.kill();child.wait(timeout=5);assert child.returncode!=0
run('database-unchanged-after-cancel',fixture+['verify'])
summary={'profile':a.profile,'helperSha256':hashlib.sha256(helper_source).hexdigest(),'driverSha256':driverhash,'runtimeSha256':lock,'actualSelect':True,'trustedConverterExecuted':True,'inspectionDidNotInitializeProject':True,'noApplicationBootstrap':True,'typedBindings':['long','boolean','string','decimal','date','timestamp','uuid'],'boundedRows':True,'profileDriftRejected':True,'sourceDriftRejected':True,'mutationRejected':True,'entityRenderingRejected':True,'actualJdbcCancellation':True,'dataUnchangedAfterCancel':True,'credentialsAbsentFromArgv':all('fixture-secret' not in str(r['argv']) for r in records)}
assert summary['credentialsAbsentFromArgv'];(base/'summary.json').write_text(json.dumps(summary,indent=2));print(base)
