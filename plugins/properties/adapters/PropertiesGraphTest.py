"""Real JDK compiler tests; no project classes are loaded or executed.
Run: python3 plugins/properties/adapters/PropertiesGraphTest.py
Protocol: java --source 17 PropertiesGraph.java REQUEST OUTPUT. Hex fields encode
UTF8; EDGE start/end are Java UTF16 offsets around the *whole* string literal,
including quotes. SOURCE hash is SHA256 over effective UTF8 source snapshots.
A `complete` result covers only the declared physical Java roots and admitted
JDK ResourceBundle / canonical PropertyKey contracts, never reflection/framework
runtime behavior. Compiler errors, dynamic relevant calls, Kotlin, unsupported
input, symlinks, count/byte/depth/time/output bounds force `incomplete`.
SOURCE_ROOT accepts src/main/java, src/test/java, and module-prefixed variants.
SOURCE overrides must name physically inventoried files (including ignored ones).
Optional BUNDLE hex-relative-focused-properties inventories all sibling .properties
files (BUNDLE hex-relative-path output records); native filters exact bundle family.
Global limits: 400 Java files,128KiB each,4MiB Java+resource total,512 directories,
8192 entries,4096 entries/directory,200 resource siblings,depth24,eight seconds,
1024 edges and256KiB output. All inventory bounds count ignored/hidden entries.
Compiler diagnostics, class-backed bundles (including package-private declarations
in differently named files), multi-argument getBundle and inherited PropertyKey
contracts without explicit override annotations force incomplete. Dynamic variable
keys, method-reference arguments and wrapper-inferred contracts are not edited.
Optional CLASSPATH_PROFILE is a bounded PROPERTIES_CLASSPATH_V1 plain-text file
listing exact absolute JARs. Profile/JAR SHA256s are returned and rechecked;
implicit manifest Class-Path, archive indices/bombs and competing bundle owners
are refused. No compiler plugins, annotation processors or implicit source loading
are permitted. Atomic output terminates with END count count. The caller
must revalidate inventory and effective snapshots before applying any edits.
"""
import pathlib, subprocess, tempfile, unittest, hashlib, time, zipfile
HELPER=pathlib.Path(__file__).with_name('PropertiesGraph.java')
def h(s):return s.encode().hex()
ANNOTATION='package org.jetbrains.annotations; public @interface PropertyKey {String resourceBundle();}'
class GraphTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.classes=tempfile.TemporaryDirectory()
        subprocess.run(['javac','--release','17','-d',cls.classes.name,str(HELPER)],check=True,capture_output=True)
    @classmethod
    def tearDownClass(cls):cls.classes.cleanup()
    def graph(self,files,overrides=None,extra=None):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();base=root/'src/main/java';base.mkdir(parents=True)
            for path,text in files.items():
                target=root/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text)
            records=['PROPERTIES_GRAPH_V1','ROOT\t'+h(str(root)),'SOURCE_ROOT\t'+h('src/main/java')]
            records += ['SOURCE\t'+h(p)+'\t'+h(t) for p,t in (overrides or {}).items()]
            records += extra or []
            request=root/'request';output=root/'output';request.write_text('\n'.join(records)+'\n')
            subprocess.run(['java','-cp',self.classes.name,'PropertiesGraph',str(request),str(output)],check=True,capture_output=True,timeout=15)
            self.assertFalse(list(root.rglob('*.class')), 'adapter must not generate project class files')
            self.assertFalse((root/'request.classpath').exists(), 'private dependency copies must retire')
            lines=output.read_text().splitlines();self.assertTrue(lines[-1].startswith('END\t'))
            edges=[];sources={};unknown=[];self.bundle_files=[];self.classpath={};self.profile=None
            for line in lines[1:-1]:
                f=line.split('\t')
                if f[0]=='EDGE':edges.append((bytes.fromhex(f[1]).decode(),int(f[2]),int(f[3]),bytes.fromhex(f[4]).decode(),bytes.fromhex(f[5]).decode()))
                elif f[0]=='SOURCE':sources[bytes.fromhex(f[1]).decode()]=f[2]
                elif f[0]=='PROFILE':self.profile=(bytes.fromhex(f[1]).decode(),f[2])
                elif f[0]=='CLASSPATH':self.classpath[bytes.fromhex(f[1]).decode()]=f[2]
                elif f[0]=='BUNDLE':self.bundle_files.append(bytes.fromhex(f[1]).decode())
                elif f[0]=='UNKNOWN':unknown.append(bytes.fromhex(f[1]).decode())
            return lines[0].split('\t')[1],edges,sources,unknown
    def test_real_jdk_binding_alias_escaped_literal_utf16_and_no_execution(self):
        source=r'''import java.util.ResourceBundle;
class A {static {if(true) throw new Error("must never run");}
static final ResourceBundle B=ResourceBundle.getBundle("messages");
void f(){ final ResourceBundle alias=B; /* 😀 */ alias.containsKey("he\u006clo"); alias.getObject("obj"); alias.getStringArray("array");}}
'''
        status,edges,_,unknown=self.graph({'src/main/java/A.java':source})
        self.assertEqual((status,unknown),('complete',[]));self.assertEqual({e[4] for e in edges},{'hello','obj','array'})
        for _,start,end,bundle,key in edges:
            literal=source.encode('utf-16-le')[start*2:end*2].decode('utf-16-le')
            self.assertTrue(literal.startswith('"') and literal.endswith('"'));self.assertEqual(bundle,'messages')
    def test_canonical_annotations_constants_overloaded_crossfile_binding(self):
        files={'src/main/java/org/jetbrains/annotations/PropertyKey.java':ANNOTATION,
        'src/main/java/Names.java':'class Names {static final String B="mess"+"ages";}',
        'src/main/java/Api.java':'''import org.jetbrains.annotations.PropertyKey; class Api {
static void use(@PropertyKey(resourceBundle=Names.B) String key,int n){}
static void use(int n,String ordinary){}
@PropertyKey(resourceBundle=Names.B) String field="initial";
void f(){field="assigned";}}''',
        'src/main/java/Client.java':'class Client {void f(){Api.use("real",1);Api.use(1,"not-a-key");}}'}
        status,edges,_,unknown=self.graph(files)
        self.assertEqual((status,unknown),('complete',[]));self.assertEqual({e[4] for e in edges},{'initial','assigned','real'})
        self.assertTrue(all(e[3]=='messages' for e in edges))
    def test_lookalikes_do_not_create_edges(self):
        source='''class ResourceBundle {static ResourceBundle getBundle(String s){return null;} String getString(String s){return null;}}
@interface PropertyKey {String resourceBundle();}
class A {@PropertyKey(resourceBundle="fake") String key="not-a-key";void f(){ResourceBundle.getBundle("fake").getString("not-a-key");}}'''
        status,edges,_,unknown=self.graph({'src/main/java/A.java':source})
        self.assertEqual((status,edges,unknown),('complete',[],[]))
    def test_unsaved_snapshot_replaces_physical_and_hashes_exact_text(self):
        saved='class A {Object f(){return java.util.ResourceBundle.getBundle("messages").getString("old");}}'
        unsaved=saved.replace('"old"','"new"')
        status,edges,sources,_=self.graph({'src/main/java/A.java':saved},{'src/main/java/A.java':unsaved})
        self.assertEqual(status,'complete');self.assertEqual(edges[0][4],'new');self.assertEqual(sources['src/main/java/A.java'],hashlib.sha256(unsaved.encode()).hexdigest())
    def test_dynamic_unknown_receiver_nonfinal_and_compiler_diagnostics_incomplete(self):
        for source in ['class A {Object f(String name){return java.util.ResourceBundle.getBundle(name).getString("key");}}',
        'class A {Object f(java.util.ResourceBundle r){return r.getString("key");}}',
        'class A {Object f(){java.util.ResourceBundle r=java.util.ResourceBundle.getBundle("messages");r=java.util.ResourceBundle.getBundle("other");return r.getString("key");}}',
        'class A { Missing type; }']:
            status,_,_,unknown=self.graph({'src/main/java/A.java':source});self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_physical_inventory_includes_hidden_and_ignored_sources(self):
        files={'src/main/java/.hidden/Ignored.java':'class Ignored {Object f(){return java.util.ResourceBundle.getBundle("messages").getString("ignored");}}',
               'src/main/java/.gitignore':'*\n'}
        status,edges,sources,_=self.graph(files)
        self.assertEqual(status,'complete');self.assertIn('src/main/java/.hidden/Ignored.java',sources);self.assertEqual(edges[0][4],'ignored')
    def test_kotlin_companion_and_uninventoried_snapshot_and_classpath_refused(self):
        for files,overrides,extra in [({'src/main/kotlin/A.kt':'class A'},None,None),({}, {'src/main/java/Missing.java':'class Missing{}'},None),({},None,['CLASSPATH\t'+h('/tmp')])]:
            status,_,_,unknown=self.graph(files,overrides,extra);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_return_annotation_and_type_use_contract(self):
        files={'src/main/java/org/jetbrains/annotations/PropertyKey.java':
               'package org.jetbrains.annotations; import java.lang.annotation.*; @Target({ElementType.TYPE_USE,ElementType.METHOD}) public @interface PropertyKey {String resourceBundle();}',
               'src/main/java/A.java':'import org.jetbrains.annotations.PropertyKey; class A { @PropertyKey(resourceBundle="messages") static String key(){return "returned";} }'}
        status,edges,_,unknown=self.graph(files)
        self.assertEqual((status,unknown),('complete',[]));self.assertEqual([e[4] for e in edges],['returned'])
    def test_bundle_overloads_and_annotated_constant_key_reads_are_incomplete(self):
        source='class A {Object f(){return java.util.ResourceBundle.getBundle("messages", java.util.Locale.ROOT).getString("key");}}'
        status,edges,_,unknown=self.graph({'src/main/java/A.java':source})
        self.assertEqual(status,'incomplete');self.assertFalse(edges);self.assertTrue(unknown)
        files={'src/main/java/org/jetbrains/annotations/PropertyKey.java':ANNOTATION,
               'src/main/java/A.java':'import org.jetbrains.annotations.PropertyKey; class A { @PropertyKey(resourceBundle="messages") static final String KEY="field"; Object f(){return java.util.ResourceBundle.getBundle("messages").getString(KEY);} }'}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_bundle_sibling_inventory_includes_ignored_translations(self):
        files={'src/main/resources/messages.properties':'hello=World',
               'src/main/resources/messages_fr.properties':'hello=Bonjour',
               'src/main/resources/.gitignore':'messages_fr.properties',
               'src/main/resources/other.properties':'other=Other'}
        status,_,_,unknown=self.graph(files,extra=['BUNDLE\t'+h('src/main/resources/messages.properties')])
        self.assertEqual((status,unknown),('complete',[]))
        self.assertEqual(set(self.bundle_files),{'src/main/resources/messages.properties','src/main/resources/messages_fr.properties','src/main/resources/other.properties'})
    def test_varargs_contract_covers_every_literal_and_refuses_array_argument(self):
        files={'src/main/java/org/jetbrains/annotations/PropertyKey.java':ANNOTATION,
               'src/main/java/A.java':'import org.jetbrains.annotations.PropertyKey; class A {static void keys(@PropertyKey(resourceBundle="messages") String... keys){} void f(){keys("one","two");}}'}
        status,edges,_,unknown=self.graph(files);self.assertEqual((status,unknown),('complete',[]));self.assertEqual({e[4] for e in edges},{'one','two'})
        files['src/main/java/A.java']=files['src/main/java/A.java'].replace('keys("one","two")','keys(new String[]{"one","two"})')
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_class_resource_bundle_in_differently_named_file_is_incomplete(self):
        source='class messages extends java.util.ListResourceBundle {protected Object[][] getContents(){return new Object[][]{{"key","value"}};}}'
        files={'src/main/java/Other.java':source,
               'src/main/java/A.java':'class A {Object f(){return java.util.ResourceBundle.getBundle("messages").getString("key");}}'}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(any('class-based' in reason for reason in unknown))
    def test_inherited_parameter_contract_without_explicit_override_is_unknown(self):
        files={'src/main/java/org/jetbrains/annotations/PropertyKey.java':ANNOTATION,
               'src/main/java/A.java':'import org.jetbrains.annotations.PropertyKey; class Base {void use(@PropertyKey(resourceBundle="messages") String key){}} class A extends Base {void use(String key){} void f(){use("key");}}'}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(any('inherited PropertyKey parameter' in reason for reason in unknown))
    def test_inventory_directory_entry_and_request_record_limits(self):
        files={f'src/main/java/d{i}/note.txt':'' for i in range(513)}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(any('directory inventory' in reason for reason in unknown))
        files={f'src/main/java/n{i}.txt':'' for i in range(4097)}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
        status,_,_,unknown=self.graph({},extra=['SOURCE_ROOT\t'+h('src/main/java')]*513)
        self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_source_class_bundle_collision_is_unknown(self):
        files={'src/main/java/messages_fr.java':'class messages_fr {}',
               'src/main/java/A.java':'class A {Object f(){return java.util.ResourceBundle.getBundle("messages").getString("key");}}'}
        status,_,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertTrue(any('shadow' in reason for reason in unknown))
    def test_size_and_count_bounds_are_refusals(self):
        for files in [{'src/main/java/A.java':' '* (128*1024+1)}, {f'src/main/java/A{i}.java':f'class A{i} {{}}' for i in range(401)}]:
            status,edges,_,unknown=self.graph(files);self.assertEqual(status,'incomplete');self.assertFalse(edges);self.assertTrue(unknown)
    def test_malformed_utf8_protocol_and_snapshot_refused(self):
        status,_,_,unknown=self.graph({},extra=['UNKNOWN\t00'])
        self.assertEqual(status,'incomplete');self.assertTrue(unknown)
        status,_,_,unknown=self.graph({},extra=['SOURCE\t'+h('src/main/java/A.java')+'\tff'])
        self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_physical_source_drift_during_compilation_is_incomplete(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();base=root/'src/main/java';base.mkdir(parents=True)
            source=base/'A.java';source.write_text('class A {} //before')
            request=root/'request';output=root/'output'
            request.write_text('PROPERTIES_GRAPH_V1\nROOT\t'+h(str(root))+'\nSOURCE_ROOT\t'+h('src/main/java')+'\n')
            process=subprocess.Popen(['java','-cp',self.classes.name,'PropertiesGraph',str(request),str(output)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            until=time.monotonic()+12;generation=0
            while process.poll() is None and time.monotonic()<until:
                source.write_text('class A {} //'+str(generation));generation+=1;time.sleep(.001)
            if process.poll() is None:process.kill()
            stdout,stderr=process.communicate(timeout=5)
            self.assertEqual(process.returncode,0,stderr.decode());self.assertGreater(generation,1)
            self.assertIn('\tincomplete\n',output.read_text());self.assertNotIn('EDGE\t',output.read_text())
    def test_symlink_inventory_refused(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();base=root/'src/main/java';base.mkdir(parents=True);(root/'outside').write_text('class A{}');(base/'A.java').symlink_to(root/'outside')
            request=root/'request';out=root/'out';request.write_text('PROPERTIES_GRAPH_V1\nROOT\t'+h(str(root))+'\nSOURCE_ROOT\t'+h('src/main/java')+'\n')
            subprocess.run(['java','-cp',self.classes.name,'PropertiesGraph',str(request),str(out)],check=True,capture_output=True,timeout=15)
            self.assertIn('\tincomplete\n',out.read_text());self.assertNotIn('EDGE\t',out.read_text())
    def dependency_jar(self,root):
        src=root/'deps';classes=root/'classes';classes.mkdir();annotation=src/'org/jetbrains/annotations/PropertyKey.java';annotation.parent.mkdir(parents=True);annotation.write_text(ANNOTATION)
        api=src/'external/Api.java';api.parent.mkdir();api.write_text('package external; import org.jetbrains.annotations.PropertyKey; public class Api {static {System.exit(91);} public static void use(@PropertyKey(resourceBundle="messages") String key){}}')
        subprocess.run(['javac','--release','17','-proc:none','-d',str(classes),str(annotation),str(api)],check=True,capture_output=True)
        jar=root/'explicit.jar'
        with zipfile.ZipFile(jar,'w',zipfile.ZIP_DEFLATED) as out:
            for path in classes.rglob('*.class'):out.write(path,path.relative_to(classes).as_posix())
        return jar
    def test_approved_jar_profile_resolves_binary_property_contract_without_bootstrap(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();jar=self.dependency_jar(root);profile=root/'classpath.txt';profile.write_text('PROPERTIES_CLASSPATH_V1\n'+str(jar)+'\n')
            files={'src/main/java/A.java':'class A {void f(){external.Api.use("dep.key");}}'}
            status,_,_,_=self.graph(files);self.assertEqual(status,'incomplete')
            status,edges,_,unknown=self.graph(files,extra=['CLASSPATH_PROFILE\t'+h(str(profile))]);self.assertEqual((status,unknown),('complete',[]));self.assertEqual([e[4] for e in edges],['dep.key']);self.assertEqual(self.classpath,{str(jar):hashlib.sha256(jar.read_bytes()).hexdigest()});self.assertEqual(self.profile,(str(profile),hashlib.sha256(profile.read_bytes()).hexdigest()))
    def test_classpath_profile_refuses_symlink_malformed_implicit_and_expanding_jars(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();jar=self.dependency_jar(root);profile=root/'classpath.txt'
            for bad in [root/'link.jar',root/'bad.jar',root/'implicit.jar',root/'bomb.jar']:
                if bad.name=='link.jar':bad.symlink_to(jar)
                elif bad.name=='bad.jar':bad.write_bytes(b'not a jar')
                elif bad.name=='implicit.jar':
                    with zipfile.ZipFile(bad,'w') as out:out.writestr('META-INF/MANIFEST.MF','Manifest-Version: 1.0\r\nClass-Path: /outside/unapproved.jar\r\n\r\n')
                else:
                    with zipfile.ZipFile(bad,'w',zipfile.ZIP_DEFLATED) as out:out.writestr('huge.txt',b'x'*(2*1024*1024))
                profile.write_text('PROPERTIES_CLASSPATH_V1\n'+str(bad)+'\n');status,_,_,unknown=self.graph({},extra=['CLASSPATH_PROFILE\t'+h(str(profile))]);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
            profile.write_text('unknown profile\n');status,_,_,unknown=self.graph({},extra=['CLASSPATH_PROFILE\t'+h(str(profile))]);self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_effectively_final_local_aliases_are_proved_by_exact_binding_and_all_writes(self):
        source='class A {Object f(){var bundle=java.util.ResourceBundle.getBundle("messages"); var alias=bundle;return alias.getString("key");}}'
        status,edges,_,unknown=self.graph({'src/main/java/A.java':source});self.assertEqual((status,unknown),('complete',[]));self.assertEqual([e[4] for e in edges],['key'])
        changed=source.replace('return alias.getString', 'bundle=java.util.ResourceBundle.getBundle("other");return alias.getString')
        status,_,_,unknown=self.graph({'src/main/java/A.java':changed});self.assertEqual(status,'incomplete');self.assertTrue(unknown)
    def test_dependency_bundle_owner_collision_refuses_physical_properties_rename(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d).resolve();jar=self.dependency_jar(root)
            with zipfile.ZipFile(jar,'a') as out:out.writestr('messages_fr.properties','key=dependency')
            profile=root/'classpath.txt';profile.write_text('PROPERTIES_CLASSPATH_V1\n'+str(jar)+'\n')
            source='class A {Object f(){return java.util.ResourceBundle.getBundle("messages").getString("key");}}'
            status,_,_,unknown=self.graph({'src/main/java/A.java':source},extra=['CLASSPATH_PROFILE\t'+h(str(profile))]);self.assertEqual(status,'incomplete');self.assertTrue(any('classpath resource/class' in why for why in unknown))
if __name__=='__main__':unittest.main()
