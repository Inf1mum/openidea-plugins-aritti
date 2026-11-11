import java.io.*;
import java.nio.*;
import java.nio.charset.*;
import java.util.regex.*;

/** Bounded Java 17 Pattern worker; no application bootstrap or network. */
class GenericRegex {
    static String read(DataInputStream in, int max) throws Exception {
        int n=in.readInt();
        if(n<0 || n>max) throw new IOException();
        byte[] b=in.readNBytes(n);
        if(b.length!=n) throw new IOException();
        return StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(b)).toString();
    }
    static String quote(String s) throws Exception {
        if(s==null || s.getBytes(StandardCharsets.UTF_8).length>16384) throw new IOException();
        StringBuilder b=new StringBuilder("\"");
        for(int i=0;i<s.length();i++) {
            char c=s.charAt(i);
            if(c=='"' || c=='\\') b.append('\\').append(c);
            else if(c<32 || Character.isSurrogate(c)) b.append(String.format("\\u%04x",(int)c));
            else b.append(c);
        }
        return b.append('"').toString();
    }
    public static void main(String[] args) {
        try {
            DataInputStream in=new DataInputStream(System.in);
            String pattern=read(in,4096); int flags=Integer.parseInt(read(in,16));
            String[] indexes=read(in,128).split(",");
            if(indexes.length!=5) throw new IOException();
            String body=read(in,262144); if(in.read()!=-1) throw new IOException();
            Matcher m=Pattern.compile(pattern,flags).matcher(body);
            StringBuilder out=new StringBuilder("["); int count=0;
            while(m.find()) {
                if(++count>50) throw new IOException();
                if(count>1) out.append(','); out.append('[');
                for(int i=0;i<indexes.length;i++) {
                    if(i>0) out.append(','); out.append(quote(m.group(Integer.parseInt(indexes[i]))));
                }
                out.append(']'); if(out.length()>1048576) throw new IOException();
            }
            out.append(']'); System.out.print(out);
        } catch(Throwable error) { System.exit(2); }
    }
}
