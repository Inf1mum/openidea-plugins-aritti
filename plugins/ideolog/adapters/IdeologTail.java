// Owned UTF8 file-tail profile. No project/application code is loaded.
import java.io.*;
import java.nio.*;
import java.nio.channels.*;
import java.nio.file.*;
import java.nio.file.attribute.*;
import java.util.*;
class IdeologTail {
  static String hex(byte[] bytes) { return HexFormat.of().formatHex(bytes); }
  static int output=0;
  static void emit(String line) { output+=line.length()+1; if(output>2097152) throw new IllegalStateException("output budget"); System.out.println(line); }
  @SuppressWarnings("unchecked")
  static SeekableByteChannel open(Path root, Path file, ArrayList<DirectoryStream<Path>> owners) throws Exception {
    DirectoryStream<Path> stream=Files.newDirectoryStream(root); owners.add(stream);
    if(!(stream instanceof SecureDirectoryStream)) throw new IOException("secure-directory-stream unavailable");
    SecureDirectoryStream<Path> secure=(SecureDirectoryStream<Path>)stream;
    Path relative=root.relativize(file);
    for(int n=0;n<relative.getNameCount()-1;n++) {
      secure=secure.newDirectoryStream(relative.getName(n),LinkOption.NOFOLLOW_LINKS);owners.add(secure);
    }
    return secure.newByteChannel(relative.getFileName(),Set.of(StandardOpenOption.READ,LinkOption.NOFOLLOW_LINKS));
  }
  public static void main(String[] args) {
    try {
      if(args.length!=3) throw new IllegalArgumentException("args");
      Path root=Path.of(args[0]).toRealPath(), file=Path.of(args[1]).toAbsolutePath().normalize();
      if(!file.startsWith(root) || file.equals(root) || !file.toRealPath().startsWith(root)) throw new IOException("source owner");
      long lifetime=Long.parseLong(args[2]); if(lifetime<1000 || lifetime>300000) throw new IllegalArgumentException("lease");
      long end=System.nanoTime()+lifetime*1000000L, position=-1, generation=0, readTotal=0;
      Object identity=null; byte[] prefix=new byte[0]; long missingSince=-1;
      boolean announced=false;
      while(System.nanoTime()<end) {
        try {
        if(!root.equals(Path.of(args[0]).toRealPath()) || !file.toRealPath().startsWith(root) || Files.isSymbolicLink(file)) throw new IOException("source changed owner");
        BasicFileAttributes attr=Files.readAttributes(file,BasicFileAttributes.class,LinkOption.NOFOLLOW_LINKS);
        if(attr.fileKey()==null) throw new IOException("file identity unavailable");
        if(!attr.isRegularFile()) throw new IOException("not regular");
        ArrayList<DirectoryStream<Path>> owners=new ArrayList<>();
        try(SeekableByteChannel channel=open(root,file,owners)) {
          if(!announced) {emit("TAIL1\tREADY\t"+hex(root.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8))+"\t"+hex(file.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8)));announced=true;}
          long size=channel.size();
          boolean changed=identity!=null && !Objects.equals(identity,attr.fileKey());
          if(!changed && prefix.length>0) {ByteBuffer verify=ByteBuffer.allocate(prefix.length);channel.read(verify);changed=!Arrays.equals(prefix,Arrays.copyOf(verify.array(),verify.position()));}
          if(position<0) {
            position=Math.max(0,size-65536);
            if(position>0) {channel.position(position);ByteBuffer head=ByteBuffer.allocate((int)Math.min(4097,size-position));channel.read(head);int n=0;while(n<head.position() && head.array()[n]!='\n')n++;if(n==head.position())throw new IOException("initial line bound");position+=n+1;}
          } else if(changed || size<position) {generation++;position=0;prefix=new byte[0];emit("TAIL1\tRESET\t"+generation);}
          identity=attr.fileKey();
          if(prefix.length==0 && size>0) {channel.position(0);ByteBuffer first=ByteBuffer.allocate((int)Math.min(64,size));channel.read(first);prefix=Arrays.copyOf(first.array(),first.position());}
          channel.position(position);ByteBuffer chunk=ByteBuffer.allocate(4096);int got=channel.read(chunk);
          if(got>0) {long at=position;position+=got;readTotal+=got;if(readTotal>1048576)throw new IOException("input budget");emit("TAIL1\tDATA\t"+generation+"\t"+at+"\t"+hex(Arrays.copyOf(chunk.array(),got)));}
        } finally {for(int n=owners.size()-1;n>=0;n--)owners.get(n).close();}
        missingSince=-1;
        Thread.sleep(250);
        } catch(NoSuchFileException missing) {
          if(missingSince<0) {missingSince=System.nanoTime();emit("TAIL1\tWAIT\tmissing");}
          if(System.nanoTime()-missingSince>2000000000L) throw new IOException("source missing beyond rotation grace");
          Thread.sleep(250);
        }
      }
      emit("TAIL1\tEND\tlease expired");
    } catch(Exception error) {System.err.println("Ideolog tail stopped: "+error.getClass().getSimpleName()+": "+error.getMessage());System.exit(2);}
  }
}
