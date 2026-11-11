#!/usr/bin/env python3
"""Real exact-profile checks in fresh disposable roots. Downloads Maven dependencies.
Never loads user project settings, datasource configuration or classpath entries.
"""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile,time,zipfile
HERE=Path(__file__).resolve().parent
PROBE=HERE.parents[1]/'runtime/BatchRuntimeProbe.java'
QUERY=HERE.parents[1]/'runtime/BatchRuntimeQuery.java'
WORK=Path(tempfile.mkdtemp(prefix='openidea-batch-cli-',dir='/tmp'))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def run(args,root,label,timeout=45):
    result=subprocess.run(args,cwd=root,capture_output=True,text=True,timeout=timeout)
    (root/f'{label}.log').write_text(result.stdout+result.stderr)
    return result
def setup(name,version,template,control=False):
    root=WORK/name;(root/'src/main/java/fixture').mkdir(parents=True)
    (root/'src/main/java/fixture/BatchContext.java').write_text((HERE/template).read_text())
    if version=='5.2.6':
        resources=root/'src/main/resources';resources.mkdir();(resources/'jobs.xml').write_text('<beans xmlns="http://www.springframework.org/schema/beans" xmlns:c="http://www.springframework.org/schema/context" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.springframework.org/schema/beans https://www.springframework.org/schema/beans/spring-beans.xsd http://www.springframework.org/schema/context https://www.springframework.org/schema/context/spring-context.xsd"><c:annotation-config/><bean class="fixture.BatchContext"/></beans>')
    h2='<dependency><groupId>com.h2database</groupId><artifactId>h2</artifactId><version>2.3.232</version></dependency>'if control else''
    (root/'pom.xml').write_text(f'<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId><artifactId>batch-cli</artifactId><version>1</version><properties><maven.compiler.release>17</maven.compiler.release></properties><dependencies><dependency><groupId>org.springframework.batch</groupId><artifactId>spring-batch-core</artifactId><version>{version}</version></dependency>{h2}</dependencies><build><plugins><plugin><groupId>org.apache.maven.plugins</groupId><artifactId>maven-compiler-plugin</artifactId><version>3.14.1</version></plugin><plugin><groupId>org.apache.maven.plugins</groupId><artifactId>maven-dependency-plugin</artifactId><version>3.8.1</version></plugin></plugins></build></project>')
    result=run(['mvn','-q','compile','dependency:copy-dependencies'],root,'maven',180)
    assert result.returncode==0,(root/'maven.log').read_text()
    jars=sorted((root/'target/dependency').glob('*.jar'));return root,[root/'target/classes',*jars]
def helper(root,asset):
    private=root.parent/'private-helpers';private.mkdir(exist_ok=True);path=private/f'{asset.stem}-{sha(asset)}.java';path.write_bytes(asset.read_bytes());return str(path)
def cp(paths):return __import__('os').pathsep.join(str(p)for p in paths)
def probe(root,profile,paths,label='probe'):
    return run(['java','--source','17',helper(root,PROBE),str(root),profile,'fixture.BatchContext',str(root/'src/main/java/fixture/BatchContext.java'),'java',cp(paths)],root,label)
def fields(output):return dict(line.split('=',1)for line in output.splitlines()[1:])
def check_probe(root,profile,paths):
    result=probe(root,profile,paths);assert result.returncode==0,result.stderr
    data=fields(result.stdout);assert data['profile']==profile
    assert data['source-sha256']==sha(root/'src/main/java/fixture/BatchContext.java')
    assert data['context-sha256']==sha(root/'target/classes/fixture/BatchContext.class')
    return result.stdout
def start(root,profile,paths,label='start'):
    main='org.springframework.batch.core.launch.support.'+('CommandLineJobRunner'if profile.startswith('runner')else'CommandLineJobOperator')
    args=['java','-cp',cp(paths),main,'fixture.BatchContext']+(['fixture']if profile.startswith('runner')else['start','fixture'])
    result=run(args,root,label);assert result.returncode==0,result.stdout+result.stderr
    assert 'OPENIDEA_BATCH_FIXTURE_COMPLETED'in result.stdout,result.stdout+result.stderr
    return args
def probe_cases(root,profile,paths):
    if profile.startswith('runner'):
        xml_source=root/'src/main/resources/jobs.xml';args=['java','--source','17',helper(root,PROBE),str(root),profile,'jobs.xml',str(xml_source),'xml',cp(paths)]
        result=run(args,root,'xml-probe');assert result.returncode==0,result.stderr;assert fields(result.stdout)['context-sha256']==sha(xml_source)
        args=['java','-cp',cp(paths),'org.springframework.batch.core.launch.support.CommandLineJobRunner','jobs.xml','fixture'];result=run(args,root,'xml-start');assert result.returncode==0 and 'OPENIDEA_BATCH_FIXTURE_COMPLETED'in result.stdout,result.stdout+result.stderr
    if profile.startswith('operator'):
        inspection=run(['java','--class-path',cp(paths),'--source','17',helper(root,QUERY),profile,'fixture.BatchContext','java','999999'],root,'inspect-empty-repository');assert inspection.returncode==0 and 'found=false'in inspection.stdout,inspection.stdout+inspection.stderr
    fake=root/'target/implicit.jar'
    with zipfile.ZipFile(fake,'w')as jar:jar.writestr('META-INF/MANIFEST.MF','Manifest-Version: 1.0\nClass-Path: outside.jar\n\n')
    result=probe(root,profile,[*paths,fake],'reject-manifest-classpath');assert result.returncode!=0 and'Manifest Class-Path'in result.stderr
    fake.unlink()
    core=next(p for p in paths if p.name.startswith('spring-batch-core-'));changed=root/'target/modified'/core.name;changed.parent.mkdir();changed.write_bytes(core.read_bytes())
    with zipfile.ZipFile(changed,'a')as jar:jar.writestr('changed-proof-marker.txt','Same manifest/version, different artifact')
    altered=[changed if p==core else p for p in paths];result=probe(root,profile,altered,'reject-core-hash');assert result.returncode!=0 and 'Core artifact SHA256'in result.stderr
    wrong=root/'target/classes/fixture/Wrong.class';wrong.write_bytes((root/'target/classes/fixture/BatchContext.class').read_bytes())
    args=['java','--source','17',helper(root,PROBE),str(root),profile,'fixture.Wrong',str(root/'src/main/java/fixture/BatchContext.java'),'java',cp(paths)]
    result=run(args,root,'reject-binary-name');assert result.returncode!=0 and'binary name'in result.stderr;wrong.unlink()
    mr=root/'target/multi-release-context.jar';data=(root/'target/classes/fixture/BatchContext.class').read_bytes()
    with zipfile.ZipFile(mr,'w')as jar:
        jar.writestr('META-INF/MANIFEST.MF','Manifest-Version: 1.0\nMulti-Release: true\n\n')
        jar.writestr('fixture/BatchContext.class',b'invalid base entry: runtime must select version17')
        jar.writestr('META-INF/versions/17/fixture/BatchContext.class',data)
    result=probe(root,profile,[mr,*paths[1:]],'multi-release-probe');assert result.returncode==0,result.stderr
    assert fields(result.stdout)['context-sha256']==hashlib.sha256(data).hexdigest()
    start(root,profile,[mr,*paths[1:]],'multi-release-start')
def controls():
    root,paths=setup('runner-control','5.2.6','ControlContext.java',True);(root/'state').mkdir()
    before=check_probe(root,'runner-5.2.6',paths)
    base=['java','-cp',cp(paths),'org.springframework.batch.core.launch.support.CommandLineJobRunner','fixture.BatchContext']
    query=['java','-cp',cp(paths),'org.h2.tools.Shell','-url','jdbc:h2:file:./state/batch;AUTO_SERVER=TRUE','-user','sa','-password','','-sql','SELECT JOB_EXECUTION_ID, STATUS FROM BATCH_JOB_EXECUTION ORDER BY JOB_EXECUTION_ID']
    log=open(root/'start.log','w');child=subprocess.Popen(base+['fixture'],cwd=root,stdout=log,stderr=subprocess.STDOUT)
    try:
        for _ in range(100):
            if'OPENIDEA_CONTROL_TICK='in(root/'start.log').read_text():break
            assert child.poll()is None,(root/'start.log').read_text()
            time.sleep(.1)
        result=run(query,root,'query-started');matches=re.findall(r'(\d+)\s*\|\s*(STARTED|STOPPING|STOPPED|COMPLETED|FAILED)',result.stdout);assert matches,result.stdout
        execution=matches[-1][0];stop=run(base+['-stop',execution],root,'stop');assert stop.returncode==0,stop.stdout+stop.stderr
        child.wait(timeout=30);log.close()
        stopped=run(query,root,'query-stopped');assert re.search(rf'{execution}\s*\|\s*STOPPED',stopped.stdout),stopped.stdout
        inspection=run(['java','--class-path',cp(paths),'--source','17',helper(root,QUERY),'runner-5.2.6','fixture.BatchContext','java',execution],root,'inspect-stopped');assert inspection.returncode==0 and 'status=STOPPED'in inspection.stdout,inspection.stdout+inspection.stderr
        xml_inspection=run(['java','--class-path',cp(paths),'--source','17',helper(root,QUERY),'runner-5.2.6','jobs.xml','xml',execution],root,'inspect-stopped-xml');assert xml_inspection.returncode==0 and 'status=STOPPED'in xml_inspection.stdout,xml_inspection.stdout+xml_inspection.stderr
        restart=run(base+['-restart',execution],root,'restart');assert restart.returncode==0,restart.stdout+restart.stderr
        completed=run(query,root,'query-completed');assert'COMPLETED'in completed.stdout,completed.stdout
        completed_id=re.findall(r'(\d+)\s*\|\s*COMPLETED',completed.stdout)[-1]
        inspection=run(['java','--class-path',cp(paths),'--source','17',helper(root,QUERY),'runner-5.2.6','fixture.BatchContext','java',completed_id],root,'inspect-completed');assert inspection.returncode==0 and 'status=COMPLETED'in inspection.stdout,inspection.stdout+inspection.stderr
        missing=run(['java','--class-path',cp(paths),'--source','17',helper(root,QUERY),'runner-5.2.6','fixture.BatchContext','java','999999'],root,'inspect-missing');assert missing.returncode==0 and 'found=false'in missing.stdout,missing.stdout+missing.stderr
        after=probe(root,'runner-5.2.6',paths,'probe-after');assert after.returncode==0 and after.stdout==before,'Classpath/source proof changed during state-only job operations'
        return {'profile':'runner-5.2.6','first_execution':execution,'stop_cli_exit':stop.returncode,'restart_cli_exit':restart.returncode,'stopped_observed':True,'completed_observed':True,'artifact_proof_unchanged':True,'typed_inspection_stopped':True,'typed_inspection_completed':True,'typed_inspection_missing':True,'logs':str(root)}
    finally:
        if child.poll()is None:child.kill();child.wait()
        log.close()
def main():
    print(f'Disposable evidence: {WORK}',flush=True);receipts=[]
    for name,version,profile,template in [('runner','5.2.6','runner-5.2.6','RunnerContext.java'),('operator','6.0.6','operator-6.0.6','OperatorContext.java')]:
        root,paths=setup(name,version,template);proof=check_probe(root,profile,paths);args=start(root,profile,paths);probe_cases(root,profile,paths)
        receipts.append({'profile':profile,'probe':fields(proof),'xml_context_start':profile.startswith('runner'),'typed_empty_repository':profile.startswith('operator'),'argv':args,'jars':{p.name:sha(p)for p in paths if p.is_file()},'logs':str(root)});print(profile,'PASS start, pinned-core hash/binary-name/implicit-classpath rejection and actual multi-release context',flush=True)
    receipts.append(controls());(WORK/'receipt.json').write_text(json.dumps({'probe_sha256':sha(PROBE),'query_sha256':sha(QUERY),'checks':receipts},indent=2)+'\n');print('PASS controls: SQL and typed inspection STOPPED/COMPLETED, typed missing, artifact proof unchanged',flush=True);print(WORK/'receipt.json',flush=True)
if __name__=='__main__':main()
