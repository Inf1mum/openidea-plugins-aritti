// Original OpenIDEA adapter probe. Source-file mode intentionally does not load
// application classes: it inspects bounded files/JAR metadata only.
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.jar.JarFile;

class BatchRuntimeProbe {
 static final long MAX_FILE=16L*1024*1024,MAX_TOTAL=128L*1024*1024;
 static long total=0;
 static byte[] read(Path path) throws Exception {
  long size=Files.size(path);if(size>MAX_FILE||size<0||(total+=size)>MAX_TOTAL)throw new IllegalArgumentException("Classpath input exceeds probe limit");
  byte[] bytes=Files.readAllBytes(path);if(bytes.length!=size)throw new IllegalArgumentException("Input changed during probe");return bytes;
 }
 static void bind(MessageDigest digest,String key,byte[] value){
  byte[] name=key.getBytes(StandardCharsets.UTF_8);digest.update(java.nio.ByteBuffer.allocate(4).putInt(name.length).array());digest.update(name);digest.update(java.nio.ByteBuffer.allocate(8).putLong(value.length).array());digest.update(value);
 }
 static String hash(byte[] value)throws Exception{return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(value));}
 static Path within(Path root,String value)throws Exception {
  Path path=Path.of(value).toRealPath();if(!path.startsWith(root))throw new IllegalArgumentException("Classpath/source escapes the configured project");return path;
 }
 static void verifyClass(byte[] bytes,String context)throws Exception {
  try(var in=new java.io.DataInputStream(new java.io.ByteArrayInputStream(bytes))){
   if(in.readInt()!=0xCAFEBABE)throw new IllegalArgumentException("Context is not a compiled class");in.readUnsignedShort();int major=in.readUnsignedShort();if(major>Runtime.version().feature()+44)throw new IllegalArgumentException("Context class needs a newer Java runtime");
   int count=in.readUnsignedShort();String[]strings=new String[count];int[]classes=new int[count];
   for(int i=1;i<count;i++){int tag=in.readUnsignedByte();switch(tag){
    case 1:strings[i]=in.readUTF();break;
    case 3:case 4:in.skipNBytes(4);break;
    case 5:case 6:in.skipNBytes(8);i++;break;
    case 7:classes[i]=in.readUnsignedShort();break;
    case 8:case 16:case 19:case 20:in.skipNBytes(2);break;
    case 9:case 10:case 11:case 12:case 17:case 18:in.skipNBytes(4);break;
    case 15:in.skipNBytes(3);break;
    default:throw new IllegalArgumentException("Unsupported context class constant pool");
   }}
   in.readUnsignedShort();int current=in.readUnsignedShort();if(current<=0||current>=count||classes[current]<=0||classes[current]>=count||!context.replace('.','/').equals(strings[classes[current]]))throw new IllegalArgumentException("Compiled class binary name does not match the exact configured context");
  }
 }
 public static void main(String[] args)throws Exception {
  for(String key:new String[]{"JAVA_TOOL_OPTIONS","_JAVA_OPTIONS","JDK_JAVA_OPTIONS"})if(System.getenv(key)!=null&&!System.getenv(key).isBlank())throw new IllegalArgumentException("Inherited JVM injection options must be removed before this explicit profile can run: "+key);
  if(args.length!=6)throw new IllegalArgumentException("Expected project, profile, context, source, kind and classpath");
  Path root=Path.of(args[0]).toRealPath();String profile=args[1],context=args[2],kind=args[4];
  // SHA256 of org.springframework.batch:spring-batch-core from Maven Central,
  // exact release artifacts qualified by tests/runtime-cli/verify.py.
  String version,launcher,coreHash;
  if(profile.equals("runner-5.2.6")){version="5.2.6";coreHash="283db480ba39f1808c766efd314e070090de055915f219523995bf1aa3c58d7c";launcher="org/springframework/batch/core/launch/support/CommandLineJobRunner.class";}
  else if(profile.equals("operator-6.0.6")){version="6.0.6";coreHash="2b043159df359d4ef29952131db58fc2bc6c74b333c4202c97d3a8d80a21a19f";launcher="org/springframework/batch/core/launch/support/CommandLineJobOperator.class";}
  else throw new IllegalArgumentException("Unsupported exact Batch profile");
  Path source=within(root,args[3]);byte[] sourceBytes=read(source);
  String target=kind.equals("java")?context.replace('.','/')+".class":context;
  String[] entries=args[5].split(java.util.regex.Pattern.quote(File.pathSeparator),-1);
  if(entries.length==0||entries.length>32)throw new IllegalArgumentException("Explicit classpath needs 1-32 entries");
  int core=0,launchers=0,contexts=0;String contextHash="";List<Path>seen=new ArrayList<>();MessageDigest digest=MessageDigest.getInstance("SHA-256");
  for(String entry:entries){
   Path path=within(root,entry);if(seen.contains(path))throw new IllegalArgumentException("Duplicate physical classpath entry");seen.add(path);
   bind(digest,"entry",path.toString().getBytes(StandardCharsets.UTF_8));
   if(Files.isDirectory(path)){
    if(Files.exists(path.resolve(launcher)))throw new IllegalArgumentException("Launcher shadowed by application classes");
    Path resource=path.resolve(target).normalize();if(!resource.startsWith(path))throw new IllegalArgumentException("Context escapes classpath directory");
    if(Files.isRegularFile(resource)){byte[]bytes=read(within(root,resource.toString()));if(kind.equals("java"))verifyClass(bytes,context);contextHash=hash(bytes);bind(digest,"context",bytes);contexts++;}
    // Bind every ordinary file in the application classes/resource directory,
    // including application configuration, not just the selected class.
    List<Path>files;
    try(var walk=Files.walk(path)){files=walk.filter(Files::isRegularFile).limit(2049).sorted().toList();}
    if(files.size()>2048)throw new IllegalArgumentException("Classes directory exceeds 2048 files");
    for(Path file:files){Path real=within(root,file.toString());bind(digest,"file:"+path.relativize(file),read(real));}
   }else{
    if(!path.toString().endsWith(".jar"))throw new IllegalArgumentException("Classpath file must be a JAR");
    byte[]bytes=read(path);bind(digest,"jar",bytes);
    try(JarFile jar=new JarFile(path.toFile(),true,JarFile.OPEN_READ,Runtime.version())){
     var manifest=jar.getManifest();String implicit=manifest==null?null:manifest.getMainAttributes().getValue("Class-Path");if(implicit!=null&&!implicit.isBlank())throw new IllegalArgumentException("Manifest Class-Path adds unreviewed dependencies");
     if(jar.getJarEntry(launcher)!=null){
      launchers++;if(!hash(bytes).equals(coreHash))throw new IllegalArgumentException("Core artifact SHA256 differs from the pinned official Maven profile");String actual=manifest==null?null:manifest.getMainAttributes().getValue("Implementation-Version");
      if(!version.equals(actual)||!path.getFileName().toString().equals("spring-batch-core-"+version+".jar"))throw new IllegalArgumentException("Launcher does not match the exact declared Batch core version");core++;
     }
     var item=jar.getJarEntry(target);if(item!=null){if(item.getSize()<0||item.getSize()>MAX_FILE)throw new IllegalArgumentException("Context resource is too large");try(var stream=jar.getInputStream(item)){byte[]content=stream.readNBytes((int)MAX_FILE+1);if(content.length>MAX_FILE)throw new IllegalArgumentException("Context resource is too large");if(kind.equals("java"))verifyClass(content,context);contextHash=hash(content);contexts++;}}
    }
   }
   digest.update((byte)0xff);
  }
  if(core!=1||launchers!=1||contexts!=1)throw new IllegalArgumentException("Classpath needs exactly one supported launcher and one selected context");
  if(kind.equals("xml")&&!contextHash.equals(hash(sourceBytes)))throw new IllegalArgumentException("Compiled XML resource differs from the reviewed source");
  System.out.println("OPENIDEA_BATCH_PROFILE_V1");System.out.println("profile="+profile);System.out.println("source-sha256="+hash(sourceBytes));System.out.println("context-sha256="+contextHash);System.out.println("classpath-sha256="+HexFormat.of().formatHex(digest.digest()));System.out.println("java="+Runtime.version().feature());
 }
}
