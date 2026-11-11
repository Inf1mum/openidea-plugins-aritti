/* Standalone JDK17+ semantic adapter. Compiles explicit snapshots; never loads project classes. */
import java.io.*;
import java.net.URI;
import java.nio.charset.*;
import java.nio.file.*;
import java.nio.file.attribute.*;
import java.security.*;
import java.util.*;
import java.util.zip.*;
import javax.lang.model.element.*;
import javax.lang.model.util.*;
import javax.lang.model.type.*;
import javax.tools.*;
import com.sun.source.tree.*;
import com.sun.source.util.*;

public final class PropertiesGraph {
    static final int MAX_SOURCE=128*1024, MAX_TOTAL=4*1024*1024, MAX_FILES=400, MAX_EDGES=1024, MAX_OUTPUT=256*1024;
    static final String HEADER="PROPERTIES_GRAPH_V1";
    static final HexFormat HEX=HexFormat.of();
    final long deadline=System.nanoTime()+8_000_000_000L;
    final SortedSet<String> unknown=new TreeSet<>();
    final SortedMap<String,String> sources=new TreeMap<>();
    final Map<String,String> overrides=new HashMap<>();
    final List<String> roots=new ArrayList<>();
    final List<String> edges=new ArrayList<>();
    final Set<String> usedBundles=new HashSet<>(), sourceClasses=new HashSet<>(), dependencyBundleOwners=new HashSet<>();
    Path root;
    String bundleRequest;
    String classpathProfile;
    String profileHash;
    final Map<Path,String> jarHashes=new LinkedHashMap<>();
    final Map<Path,Path> jarSnapshots=new LinkedHashMap<>();
    Path classpathWorkspace;
    boolean workspaceCreated;
    final Map<Path,BasicFileAttributes> jarAttributes=new HashMap<>();
    final Set<Element> assignedVariables=new HashSet<>();
    final SortedSet<String> bundleFiles=new TreeSet<>();
    Trees trees;
    Types types;
    Elements elements;
    final Set<Path> seenDirectories=new HashSet<>(),seenEntries=new HashSet<>();
    final Set<Element> checkedInheritance=new HashSet<>();
    final Map<URI,Source> compilationSources=new HashMap<>();
    int total;
    final Map<Path,BasicFileAttributes> attributes=new HashMap<>();
    final Map<Path,List<Path>> directories=new HashMap<>();
    final Map<Path,byte[]> physicalHashes=new HashMap<>();
    void tick() throws IOException {if(System.nanoTime()>deadline)throw new IOException("analysis time exceeds eight seconds");}
    void directoryBound(Path path)throws IOException{tick();if(seenDirectories.add(path)&&seenDirectories.size()>512)throw new IOException("directory inventory exceeds512");}
    void entryBound(Path path)throws IOException{tick();if(seenEntries.add(path)&&seenEntries.size()>8192)throw new IOException("physical inventory exceeds8192 entries");}
    static byte[] boundedBytes(Path path,int max)throws IOException{
        try(InputStream stream=Files.newInputStream(path,LinkOption.NOFOLLOW_LINKS)){
            byte[] bytes=stream.readNBytes(max+1);if(bytes.length>max)throw new IOException("input exceeds byte bound");return bytes;
        }
    }
    static String hex(String s){return HEX.formatHex(s.getBytes(StandardCharsets.UTF_8));}
    static String unhex(String s) throws IOException {
        try {byte[] b=HEX.parseHex(s);return StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(java.nio.ByteBuffer.wrap(b)).toString();}
        catch(Exception e){throw new IOException("invalid hex UTF8 field");}
    }
    String relative(Path path){return root.relativize(path).toString().replace(File.separatorChar,'/');}
    Path inside(String name) throws IOException {
        Path p=Path.of(name);
        if(p.isAbsolute()||name.contains("\\")||name.isEmpty()||name.indexOf('\0')>=0)throw new IOException("invalid project-relative path");
        for(Path part:p)if(part.toString().equals("..")||part.toString().equals("."))throw new IOException("traversal in source path");
        p=root.resolve(p).normalize();
        if(!p.startsWith(root))throw new IOException("source outside root");
        return p;
    }
    void noLinks(Path path) throws IOException {
        Path at=path.getRoot();
        for(Path part:path){at=at.resolve(part);if(Files.isSymbolicLink(at))throw new IOException("symlink in admitted path");}
    }
    void request(Path request) throws Exception {
        classpathWorkspace=Path.of(request.toAbsolutePath().toString()+".classpath");
        if(Files.size(request)>2*MAX_TOTAL+65536)throw new IOException("request exceeds bound");
        byte[] requestBytes=boundedBytes(request,2*MAX_TOTAL+65536);
        List<String> lines=StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(java.nio.ByteBuffer.wrap(requestBytes)).toString().lines().toList();
        if(lines.size()>512)throw new IOException("request record count exceeds512");
        if(lines.isEmpty()||!lines.get(0).equals(HEADER))throw new IOException("invalid request header");
        for(String line:lines.subList(1,lines.size())) {
            tick();String[] f=line.split("\\t",-1);
            if(f.length==2&&f[0].equals("ROOT")){
                if(root!=null)throw new IOException("duplicate root");root=Path.of(unhex(f[1]));
                if(!root.isAbsolute()||!root.normalize().equals(root))throw new IOException("root must be normalized absolute path");
                noLinks(root);if(!Files.isDirectory(root,LinkOption.NOFOLLOW_LINKS))throw new IOException("root is not a directory");
            }else if(f.length==2&&f[0].equals("CLASSPATH_PROFILE")){
                if(classpathProfile!=null)throw new IOException("duplicate classpath profile");classpathProfile=unhex(f[1]);
            }else if(f.length==2&&f[0].equals("BUNDLE")){
                if(bundleRequest!=null)throw new IOException("duplicate bundle inventory request");bundleRequest=unhex(f[1]);
            }else if(f.length==2&&f[0].equals("SOURCE_ROOT")){roots.add(unhex(f[1]));}
            else if(f.length==3&&f[0].equals("SOURCE")){
                String path=unhex(f[1]),text=unhex(f[2]);
                if(text.getBytes(StandardCharsets.UTF_8).length>MAX_SOURCE||overrides.putIfAbsent(path,text)!=null)throw new IOException("invalid snapshot size or duplicate source");
            }else throw new IOException("unsupported request record (classpath is unavailable in v1)");
        }
        if(root==null||roots.isEmpty()||roots.size()>16)throw new IOException("root and one to sixteen source roots required");
        for(String name:roots){
            // Explicit conventional Java roots only; no generated/bootstrap sources.
            if(!(name.equals("src/main/java")||name.equals("src/test/java")||name.endsWith("/src/main/java")||name.endsWith("/src/test/java")))throw new IOException("non-conventional Java source root");
            Path source=inside(name);noLinks(source);
            if(!Files.isDirectory(source,LinkOption.NOFOLLOW_LINKS))throw new IOException("declared source root missing");
            walk(source,0);
            Path kotlin=source.resolveSibling("kotlin");
            if(Files.exists(kotlin,LinkOption.NOFOLLOW_LINKS)){
                noLinks(kotlin);if(hasKotlin(kotlin,0))unknown.add("Kotlin companion sources are outside the Java semantic graph");
            }
        }
        if(bundleRequest!=null)inventoryBundles();
        if(classpathProfile!=null)classpath();
        if(!sources.keySet().containsAll(overrides.keySet()))throw new IOException("snapshot outside physical Java inventory");
    }
    Path absoluteFile(String name)throws IOException{
        Path path=Path.of(name);if(!path.isAbsolute()||!path.normalize().equals(path)||name.indexOf('\0')>=0)throw new IOException("classpath path must be normalized absolute");
        noLinks(path);return path;
    }
    String fileHash(Path path,long max)throws Exception{
        MessageDigest hash=MessageDigest.getInstance("SHA-256");long count=0;
        try(InputStream in=Files.newInputStream(path,LinkOption.NOFOLLOW_LINKS)){
            byte[] buffer=new byte[65536];for(int n;(n=in.read(buffer))!=-1;){tick();count+=n;if(count>max)throw new IOException("classpath byte bound");hash.update(buffer,0,n);}
        }return HEX.formatHex(hash.digest());
    }
    void classpath()throws Exception{
        Path profile=absoluteFile(classpathProfile);BasicFileAttributes before=Files.readAttributes(profile,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        if(!before.isRegularFile()||before.size()>65536)throw new IOException("classpath profile bound/kind");
        byte[] bytes=boundedBytes(profile,65536);String text=StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(java.nio.ByteBuffer.wrap(bytes)).toString();
        String[] lines=text.split("\\R",-1);if(lines.length<2||!lines[0].equals("PROPERTIES_CLASSPATH_V1"))throw new IOException("classpath profile version");
        profileHash=HEX.formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));jarAttributes.put(profile,before);
        Files.createDirectory(classpathWorkspace);workspaceCreated=true;
        if(Files.getFileStore(classpathWorkspace).supportsFileAttributeView("posix"))Files.setPosixFilePermissions(classpathWorkspace,PosixFilePermissions.fromString("rwx------"));
        long compressed=0,expanded=0;int entries=0;
        for(int i=1;i<lines.length;i++){
            if(lines[i].isEmpty()&&i==lines.length-1)continue;
            if(jarHashes.size()>=128)throw new IOException("classpath exceeds128 JARs");Path jar=absoluteFile(lines[i]);
            if(!jar.toString().endsWith(".jar")||jarHashes.containsKey(jar))throw new IOException("classpath requires distinct .jar files");
            BasicFileAttributes attrs=Files.readAttributes(jar,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);if(!attrs.isRegularFile()||attrs.size()>128L*1024*1024)throw new IOException("classpath JAR bound/kind");
            compressed+=attrs.size();if(compressed>512L*1024*1024)throw new IOException("classpath exceeds512MiB");
            Path snapshot=Files.createTempFile(classpathWorkspace,"dependency-",".jar");jarSnapshots.put(jar,snapshot);
            MessageDigest copyHash=MessageDigest.getInstance("SHA-256");long copied=0;
            try(InputStream in=Files.newInputStream(jar,LinkOption.NOFOLLOW_LINKS);OutputStream out=Files.newOutputStream(snapshot,StandardOpenOption.TRUNCATE_EXISTING)){
                byte[] buffer=new byte[65536];for(int n;(n=in.read(buffer))!=-1;){tick();copied+=n;if(copied>128L*1024*1024)throw new IOException("classpath copy exceeds128MiB");copyHash.update(buffer,0,n);out.write(buffer,0,n);}
            }
            String copiedHash=HEX.formatHex(copyHash.digest());
            if(!stable(attrs,Files.readAttributes(jar,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS))||!copiedHash.equals(fileHash(jar,128L*1024*1024)))throw new IOException("classpath changed during snapshot");
            try(ZipFile zip=new ZipFile(snapshot.toFile())){
                var all=zip.entries();while(all.hasMoreElements()){
                    tick();ZipEntry entry=all.nextElement();if(++entries>100000)throw new IOException("classpath exceeds100000 entries");
                    String name=entry.getName();if(name.length()>2048)throw new IOException("classpath entry name bound");
                    String owned=name; if(owned.startsWith("META-INF/versions/")){String[] parts=owned.split("/",4);if(parts.length==4)owned=parts[3];}
                    if(owned.endsWith(".class"))dependencyBundleOwners.add(owned.substring(0,owned.length()-6).replace('/','.'));
                    if(owned.endsWith(".properties"))dependencyBundleOwners.add(owned.substring(0,owned.length()-11).replace('/','.'));
                    if(name.equalsIgnoreCase("META-INF/INDEX.LIST"))throw new IOException("implicit indexed classpath is unsupported");
                    if(name.equalsIgnoreCase("META-INF/MANIFEST.MF")){
                        byte[] manifest;try(InputStream in=zip.getInputStream(entry)){manifest=in.readNBytes(65537);}if(manifest.length>65536)throw new IOException("classpath manifest bound");
                        java.util.jar.Manifest parsed=new java.util.jar.Manifest(new ByteArrayInputStream(manifest));String implicit=parsed.getMainAttributes().getValue(java.util.jar.Attributes.Name.CLASS_PATH);
                        if(implicit!=null&&!implicit.isBlank())throw new IOException("implicit manifest Class-Path is unsupported");
                    }
                    if(name.startsWith("/")||name.contains("\\")||Arrays.asList(name.split("/")).contains(".."))throw new IOException("classpath archive path");
                    long size=entry.getSize(),packed=entry.getCompressedSize();if(size<0||packed<0||size>64L*1024*1024||size>packed*200+1024*1024)throw new IOException("classpath archive expansion bound");expanded+=size;if(expanded>1024L*1024*1024)throw new IOException("classpath expands beyond1GiB");
                }
            }
            jarHashes.put(jar,copiedHash);jarAttributes.put(jar,attrs);
        }
        verifyClasspath();
    }
    void cleanupClasspath(){
        for(Path snapshot:jarSnapshots.values())try{Files.deleteIfExists(snapshot);}catch(IOException ignored){}
        if(workspaceCreated)try{Files.deleteIfExists(classpathWorkspace);}catch(IOException ignored){}
    }
    void verifyClasspath()throws Exception{
        if(classpathProfile==null)return;
        Path profile=absoluteFile(classpathProfile);
        if(!profileHash.equals(fileHash(profile,65536)))throw new IOException("classpath profile changed");
        for(var item:jarAttributes.entrySet())if(!stable(item.getValue(),Files.readAttributes(item.getKey(),BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS)))throw new IOException("classpath metadata changed");
        for(var item:jarHashes.entrySet())if(!item.getValue().equals(fileHash(item.getKey(),128L*1024*1024)))throw new IOException("classpath content changed");
    }
    void inventoryBundles()throws Exception{
        Path focused=inside(bundleRequest);noLinks(focused);
        if(!bundleRequest.endsWith(".properties"))throw new IOException("bundle inventory requires properties path");
        Path dir=focused.getParent();directoryBound(dir);BasicFileAttributes before=Files.readAttributes(dir,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        List<Path> children=new ArrayList<>();
        try(DirectoryStream<Path> ds=Files.newDirectoryStream(dir)){for(Path path:ds){entryBound(path);if(children.size()>=200)throw new IOException("bundle sibling inventory exceeds200");children.add(path);}}
        children.sort(Comparator.comparing(Path::toString));
        directories.put(dir,List.copyOf(children));attributes.put(dir,before);
        for(Path path:children){
            tick();BasicFileAttributes a=Files.readAttributes(path,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
            if(a.isSymbolicLink())throw new IOException("symlink in bundle sibling inventory");
            if(!path.toString().endsWith(".properties"))continue;
            if(!a.isRegularFile()||a.size()>MAX_SOURCE)throw new IOException("bundle source kind/size exceeds bound");
            total+=a.size();if(total>MAX_TOTAL)throw new IOException("Java and bundle inventory total exceeds4MiB");
            bundleFiles.add(relative(path));attributes.put(path,a);
        }
        if(!bundleFiles.contains(bundleRequest))throw new IOException("focused properties file absent from physical inventory");
    }
    boolean hasKotlin(Path path,int depth) throws IOException {
        tick();if(depth>24)throw new IOException("source depth exceeds24");
        BasicFileAttributes before=Files.readAttributes(path,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        if(before.isSymbolicLink())throw new IOException("symlink in Kotlin inventory");
        if(before.isDirectory()){
            directoryBound(path);
            List<Path> children=new ArrayList<>();
            try(DirectoryStream<Path> ds=Files.newDirectoryStream(path)){for(Path p:ds){entryBound(p);if(children.size()>=4096)throw new IOException("Kotlin directory inventory bound");children.add(p);}}
            children.sort(Comparator.comparing(Path::toString));attributes.put(path,before);directories.put(path,List.copyOf(children));
            for(Path p:children)if(hasKotlin(p,depth+1))return true;
        }
        return path.toString().endsWith(".kt")||path.toString().endsWith(".kts");
    }
    void walk(Path dir,int depth)throws Exception{
        tick();if(depth>24)throw new IOException("source depth exceeds24");
        directoryBound(dir);
        BasicFileAttributes before=Files.readAttributes(dir,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        if(!before.isDirectory()||before.isSymbolicLink())throw new IOException("invalid source directory");
        List<Path> children=new ArrayList<>();
        try(DirectoryStream<Path> ds=Files.newDirectoryStream(dir)){for(Path p:ds){entryBound(p);if(children.size()>=4096)throw new IOException("directory inventory exceeds bound");children.add(p);}}
        children.sort(Comparator.comparing(Path::toString));
        attributes.put(dir,before);directories.put(dir,List.copyOf(children));
        for(Path path:children){
            tick();BasicFileAttributes a=Files.readAttributes(path,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
            if(a.isSymbolicLink())throw new IOException("symlink in physical source inventory");
            if(a.isDirectory()){walk(path,depth+1);continue;}
            if(path.toString().endsWith(".kt")||path.toString().endsWith(".kts")){unknown.add("Kotlin source is outside the Java semantic graph");continue;}
            if(!path.toString().endsWith(".java"))continue;
            if(!a.isRegularFile()||a.size()>MAX_SOURCE)throw new IOException("Java source kind/size bound exceeded");
            String key=relative(path);
            if(sources.containsKey(key))continue;
            if(sources.size()>=MAX_FILES)throw new IOException("Java source count exceeds400");
            noLinks(path);byte[] bytes=boundedBytes(path,MAX_SOURCE);
            BasicFileAttributes after=Files.readAttributes(path,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
            if(bytes.length>MAX_SOURCE||!stable(a,after))throw new IOException("Java source changed during read");
            attributes.put(path,after);physicalHashes.put(path,MessageDigest.getInstance("SHA-256").digest(bytes));
            String text=StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(java.nio.ByteBuffer.wrap(bytes)).toString();
            text=overrides.getOrDefault(key,text);total+=text.getBytes(StandardCharsets.UTF_8).length;
            if(total>MAX_TOTAL)throw new IOException("Java source total exceeds4MiB");sources.put(key,text);
        }
        BasicFileAttributes after=Files.readAttributes(dir,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        if(!stable(before,after))throw new IOException("directory changed during inventory");
    }
    void verifyInventory()throws Exception{
        verifyClasspath();
        for(var item:attributes.entrySet()){
            tick();BasicFileAttributes now=Files.readAttributes(item.getKey(),BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
            if(!stable(item.getValue(),now)||now.isSymbolicLink())throw new IOException("physical source inventory changed during compilation");
        }
        for(var item:directories.entrySet()){
            List<Path> current=new ArrayList<>();
            try(DirectoryStream<Path> ds=Files.newDirectoryStream(item.getKey())){for(Path p:ds){entryBound(p);if(current.size()>=4096)throw new IOException("directory inventory exceeds bound");current.add(p);}}
            current.sort(Comparator.comparing(Path::toString));
            if(!current.equals(item.getValue()))throw new IOException("physical source inventory membership changed");
        }
        for(var item:physicalHashes.entrySet()){
            tick();if(Files.size(item.getKey())>MAX_SOURCE)throw new IOException("physical source grew during compilation");
            noLinks(item.getKey());byte[] bytes=boundedBytes(item.getKey(),MAX_SOURCE);
            if(!MessageDigest.isEqual(item.getValue(),MessageDigest.getInstance("SHA-256").digest(bytes)))throw new IOException("physical source text changed during compilation");
        }
    }
    static boolean stable(BasicFileAttributes a,BasicFileAttributes b){return a.isDirectory()==b.isDirectory()&&a.isRegularFile()==b.isRegularFile()&&a.size()==b.size()&&a.lastModifiedTime().equals(b.lastModifiedTime())&&Objects.equals(a.fileKey(),b.fileKey());}
    static final class Source extends SimpleJavaFileObject {
        final String text,path;
        Source(String path,String text){super(URI.create("mem:///"+hex(path)+"/"+Path.of(path).getFileName()),Kind.SOURCE);this.path=path;this.text=text;}
        public CharSequence getCharContent(boolean ignore){return text;}
    }
    TreePath path(TreePath parent,Tree tree){return tree==null?null:new TreePath(parent,tree);}
    String constant(TreePath path){
        if(path==null)return null;Tree t=path.getLeaf();
        if(t instanceof LiteralTree l&&l.getValue() instanceof String s)return s;
        if(t instanceof ParenthesizedTree p)return constant(path(path,p.getExpression()));
        Element e=trees.getElement(path);
        if(e instanceof VariableElement v&&v.getConstantValue() instanceof String s)return s;
        if(t instanceof BinaryTree b&&b.getKind()==Tree.Kind.PLUS){String l=constant(path(path,b.getLeftOperand())),r=constant(path(path,b.getRightOperand()));if(l!=null&&r!=null)return l+r;}
        return null;
    }
    String bundle(TreePath p,Set<Element> visiting){
        if(p==null)return null;Tree t=p.getLeaf();
        if(t instanceof ParenthesizedTree par)return bundle(path(p,par.getExpression()),visiting);
        if(t instanceof MethodInvocationTree call){
            Element e=trees.getElement(p);
            if(e instanceof ExecutableElement m&&owner(m).equals("java.util.ResourceBundle")&&m.getSimpleName().contentEquals("getBundle")&&call.getArguments().size()==1)return constant(path(p,call.getArguments().get(0)));
        }
        Element e=trees.getElement(p);
        if(e instanceof VariableElement v&&(v.getModifiers().contains(Modifier.FINAL)||v.getKind()==ElementKind.LOCAL_VARIABLE&&!assignedVariables.contains(v))&&visiting.add(e)){
            TreePath declaration=trees.getPath(v);
            if(declaration!=null&&declaration.getLeaf() instanceof VariableTree variable){String result=bundle(path(declaration,variable.getInitializer()),visiting);visiting.remove(e);return result;}
        }
        return null;
    }
    static String owner(Element e){Element enclosing=e.getEnclosingElement();return enclosing instanceof TypeElement t?t.getQualifiedName().toString():"";}
    String annotationBundle(Element element){
        List<AnnotationMirror> annotations=new ArrayList<>(element.getAnnotationMirrors());
        annotations.addAll(element.asType().getAnnotationMirrors());
        if(element instanceof ExecutableElement method)annotations.addAll(method.getReturnType().getAnnotationMirrors());
        for(AnnotationMirror a:annotations)if(a.getAnnotationType().asElement() instanceof TypeElement type&&type.getQualifiedName().contentEquals("org.jetbrains.annotations.PropertyKey")){
            TypeMirror contractType=element instanceof ExecutableElement method?method.getReturnType():element.asType();
            if(contractType instanceof ArrayType array)contractType=array.getComponentType();
            Element valueType=types.asElement(contractType);
            if(!(valueType instanceof TypeElement declared&&declared.getQualifiedName().contentEquals("java.lang.String"))){unknown.add("PropertyKey contract is not a String value");return null;}
            for(var item:a.getElementValues().entrySet())if(item.getKey().getSimpleName().contentEquals("resourceBundle")&&item.getValue().getValue() instanceof String s)return s;
            unknown.add("PropertyKey annotation has no constant resourceBundle contract");
        }
        return null;
    }
    void inheritedContracts(ExecutableElement method){
        if(!checkedInheritance.add(method)||!(method.getEnclosingElement() instanceof TypeElement owner))return;
        List<TypeMirror> pending=new ArrayList<>(types.directSupertypes(owner.asType()));Set<String> seen=new HashSet<>();
        while(!pending.isEmpty()){
            TypeMirror parent=pending.remove(pending.size()-1);if(!seen.add(parent.toString()))continue;
            if(seen.size()>64){unknown.add("inherited contract hierarchy exceeds64 types");return;}
            Element e=types.asElement(parent);if(!(e instanceof TypeElement type))continue;
            for(Element member:type.getEnclosedElements())if(member instanceof ExecutableElement base&&elements.overrides(method,base,owner)){
                if(annotationBundle(method)==null&&annotationBundle(base)!=null)unknown.add("inherited PropertyKey return contract requires explicit override annotation");
                for(int i=0;i<Math.min(method.getParameters().size(),base.getParameters().size());i++)
                    if(annotationBundle(method.getParameters().get(i))==null&&annotationBundle(base.getParameters().get(i))!=null)unknown.add("inherited PropertyKey parameter contract requires explicit override annotation");
            }
            pending.addAll(types.directSupertypes(parent));
        }
    }
    void edge(TreePath p,String bundle,String contract){
        if(bundle==null||bundle.isEmpty()||bundle.length()>512){unknown.add(contract+": dynamic or invalid bundle identity");return;}
        usedBundles.add(bundle);
        Tree t=p.getLeaf();
        if(!(t instanceof LiteralTree literal&&literal.getValue() instanceof String key)){
            unknown.add(contract+": key is not a physical string literal");return;
        }
        if(key.length()>512){unknown.add(contract+": key exceeds512 UTF16 units");return;}
        CompilationUnitTree unit=p.getCompilationUnit();long start=trees.getSourcePositions().getStartPosition(unit,t),end=trees.getSourcePositions().getEndPosition(unit,t);
        Source source=compilationSources.get(unit.getSourceFile().toUri());
        if(source==null){unknown.add("compiler source identity unavailable");return;}
        if(start<0||end<=start||end>source.text.length()){unknown.add("compiler source span unavailable");return;}
        if(edges.size()>=MAX_EDGES){unknown.add("more than1024 semantic edges");return;}
        String record="EDGE\t"+hex(source.path)+"\t"+start+"\t"+end+"\t"+hex(bundle)+"\t"+hex(key);
        if(!edges.contains(record))edges.add(record);
    }
    void compile() throws Exception {
        JavaCompiler compiler=ToolProvider.getSystemJavaCompiler();if(compiler==null)throw new IOException("JDK compiler unavailable");
        DiagnosticCollector<JavaFileObject> diagnostics=new DiagnosticCollector<>();
        try(StandardJavaFileManager files=compiler.getStandardFileManager(diagnostics,Locale.ROOT,StandardCharsets.UTF_8)){
            files.setLocationFromPaths(StandardLocation.SOURCE_PATH,List.of());files.setLocationFromPaths(StandardLocation.CLASS_PATH,jarSnapshots.values());
            List<Source> units=new ArrayList<>();sources.forEach((p,t)->{Source source=new Source(p,t);units.add(source);compilationSources.put(source.toUri(),source);});
            if(units.isEmpty())return;
            JavacTask task=(JavacTask)compiler.getTask(null,files,diagnostics,List.of("-proc:none","--release","17","-implicit:none","-Xlint:none"),null,units);
            List<CompilationUnitTree> parsed=new ArrayList<>();for(var unit:task.parse()){tick();parsed.add(unit);}task.analyze();tick();trees=Trees.instance(task);types=task.getTypes();elements=task.getElements();
            for(var diagnostic:diagnostics.getDiagnostics())if(diagnostic.getKind()==Diagnostic.Kind.ERROR)unknown.add("compiler diagnostics prevent complete semantic binding");
            for(var unit:parsed)new TreePathScanner<Void,Void>(){
                public Void scan(Tree tree,Void unused){if(System.nanoTime()>deadline)throw new IllegalStateException("write traversal time exceeded");return super.scan(tree,unused);}
                void write(ExpressionTree target){Element element=trees.getElement(path(getCurrentPath(),target));if(element!=null)assignedVariables.add(element);}
                public Void visitAssignment(AssignmentTree tree,Void unused){write(tree.getVariable());return super.visitAssignment(tree,unused);}
                public Void visitCompoundAssignment(CompoundAssignmentTree tree,Void unused){write(tree.getVariable());return super.visitCompoundAssignment(tree,unused);}
                public Void visitUnary(UnaryTree tree,Void unused){if(Set.of(Tree.Kind.PREFIX_INCREMENT,Tree.Kind.PREFIX_DECREMENT,Tree.Kind.POSTFIX_INCREMENT,Tree.Kind.POSTFIX_DECREMENT).contains(tree.getKind()))write(tree.getExpression());return super.visitUnary(tree,unused);}
            }.scan(unit,null);
            for(var unit:parsed)new TreePathScanner<Void,Void>(){
                public Void scan(Tree tree,Void unused){if(System.nanoTime()>deadline)throw new IllegalStateException("semantic traversal time exceeded");return super.scan(tree,unused);}
                void arguments(ExecutableElement method,List<? extends ExpressionTree> args){
                    List<? extends VariableElement> params=method.getParameters();
                    for(int i=0;i<args.size();i++){
                        int parameter=i<params.size()?i:method.isVarArgs()?params.size()-1:-1;
                        if(parameter<0)continue;
                        String contract=annotationBundle(params.get(parameter));
                        if(contract!=null)edge(path(getCurrentPath(),args.get(i)),contract,"PropertyKey parameter");
                    }
                }
                public Void visitClass(ClassTree declaration,Void unused){
                    Element element=trees.getElement(getCurrentPath());
                    if(element instanceof TypeElement type){
                        sourceClasses.add(type.getQualifiedName().toString());
                        TypeElement resourceBundle=elements.getTypeElement("java.util.ResourceBundle");
                        if(resourceBundle!=null&&types.isSubtype(types.erasure(type.asType()),types.erasure(resourceBundle.asType())))
                            unknown.add("declared class-based ResourceBundle is outside physical properties ownership");
                    }
                    return super.visitClass(declaration,unused);
                }
                public Void visitMethodInvocation(MethodInvocationTree call,Void unused){
                    TreePath p=getCurrentPath();Element resolved=trees.getElement(p);
                    if(resolved instanceof ExecutableElement method){
                        inheritedContracts(method);
                        arguments(method,call.getArguments());
                        String name=method.getSimpleName().toString();
                        if(owner(method).equals("java.util.ResourceBundle")){
                            if(name.equals("getBundle")){
                                if(call.getArguments().size()!=1)unknown.add("ResourceBundle.getBundle custom loader/control/module/locale overload is outside physical ownership");
                                else if(constant(path(p,call.getArguments().get(0)))==null)unknown.add("ResourceBundle.getBundle has dynamic bundle name");
                            }
                            if(Set.of("getString","getObject","getStringArray","containsKey").contains(name)&&call.getArguments().size()==1){
                                Tree select=call.getMethodSelect();String identity=select instanceof MemberSelectTree m?bundle(path(p,m.getExpression()),new HashSet<>()):null;
                                edge(path(p,call.getArguments().get(0)),identity,"ResourceBundle key access");
                            }
                        }
                    }
                    return super.visitMethodInvocation(call,unused);
                }
                public Void visitNewClass(NewClassTree call,Void unused){Element e=trees.getElement(getCurrentPath());if(e instanceof ExecutableElement m)arguments(m,call.getArguments());return super.visitNewClass(call,unused);}
                public Void visitVariable(VariableTree var,Void unused){Element e=trees.getElement(getCurrentPath());if(e!=null&&var.getInitializer()!=null){String b=annotationBundle(e);if(b!=null)edge(path(getCurrentPath(),var.getInitializer()),b,"PropertyKey variable");}return super.visitVariable(var,unused);}
                public Void visitAssignment(AssignmentTree assign,Void unused){Element e=trees.getElement(path(getCurrentPath(),assign.getVariable()));if(e!=null){String b=annotationBundle(e);if(b!=null)edge(path(getCurrentPath(),assign.getExpression()),b,"PropertyKey assignment");}return super.visitAssignment(assign,unused);}
                public Void visitReturn(ReturnTree returned,Void unused){
                    if(returned.getExpression()!=null){
                        TreePath enclosing=getCurrentPath().getParentPath();
                        while(enclosing!=null&&!(enclosing.getLeaf() instanceof MethodTree)&&!(enclosing.getLeaf() instanceof LambdaExpressionTree))enclosing=enclosing.getParentPath();
                        if(enclosing!=null&&enclosing.getLeaf() instanceof MethodTree){Element method=trees.getElement(enclosing);if(method!=null){String contract=annotationBundle(method);if(contract!=null)edge(path(getCurrentPath(),returned.getExpression()),contract,"PropertyKey return");}}
                    }
                    return super.visitReturn(returned,unused);
                }
                public Void visitMemberReference(MemberReferenceTree reference,Void unused){Element e=trees.getElement(getCurrentPath());if(e instanceof ExecutableElement m&&(owner(m).equals("java.util.ResourceBundle")||m.getParameters().stream().anyMatch(p->annotationBundle(p)!=null)))unknown.add("method reference has unbound runtime key arguments");return super.visitMemberReference(reference,unused);}
            }.scan(unit,null);
            for(String bundle:usedBundles){
                for(String type:sourceClasses)if(type.equals(bundle)||type.startsWith(bundle+"_"))unknown.add("source class may shadow a properties ResourceBundle: "+type);
                for(String type:dependencyBundleOwners)if(type.equals(bundle)||type.startsWith(bundle+"_"))unknown.add("classpath resource/class may shadow the physical properties ResourceBundle: "+type);
            }
        }
    }
    String output()throws Exception{
        StringBuilder body=new StringBuilder();
        if(classpathProfile!=null){body.append("PROFILE\t").append(hex(classpathProfile)).append('\t').append(profileHash).append('\n');for(var item:jarHashes.entrySet())body.append("CLASSPATH\t").append(hex(item.getKey().toString())).append('\t').append(item.getValue()).append('\n');}
        for(var entry:sources.entrySet())body.append("SOURCE\t").append(hex(entry.getKey())).append('\t').append(HEX.formatHex(MessageDigest.getInstance("SHA-256").digest(entry.getValue().getBytes(StandardCharsets.UTF_8)))).append('\n');
        for(String path:bundleFiles)body.append("BUNDLE\t").append(hex(path)).append('\n');
        Collections.sort(edges);for(String edge:edges)body.append(edge).append('\n');
        for(String why:unknown)body.append("UNKNOWN\t").append(hex(why)).append('\n');
        String out=HEADER+"\t"+(unknown.isEmpty()?"complete":"incomplete")+"\n"+body+"END\t"+sources.size()+"\t"+edges.size()+"\n";
        if(out.getBytes(StandardCharsets.UTF_8).length>MAX_OUTPUT)throw new IOException("graph output exceeds256KiB");return out;
    }
    static void writeAtomic(Path target,String content)throws Exception{
        Path absolute=target.toAbsolutePath();Path temp=Files.createTempFile(absolute.getParent(),".properties-graph-",".tmp");
        try{Files.writeString(temp,content,StandardCharsets.UTF_8);Files.move(temp,absolute,StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);}finally{Files.deleteIfExists(temp);}
    }
    public static void main(String[] args)throws Exception{
        if(args.length!=2)throw new IllegalArgumentException("PropertiesGraph REQUEST OUTPUT");
        PropertiesGraph graph=new PropertiesGraph();
        try{graph.request(Path.of(args[0]));graph.compile();graph.verifyInventory();graph.tick();}
        catch(Exception error){graph.unknown.add("adapter refused incomplete input: "+error.getClass().getSimpleName()+": "+String.valueOf(error.getMessage()));graph.edges.clear();}
        finally{graph.cleanupClasspath();}
        String result;
        try{result=graph.output();}catch(Exception error){result=HEADER+"\tincomplete\nUNKNOWN\t"+hex("adapter output bound exceeded")+"\nEND\t0\t0\n";}
        writeAtomic(Path.of(args[1]),result);
    }
}
