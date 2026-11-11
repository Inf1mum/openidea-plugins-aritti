// Original clean implementation of a bounded Ideolog JVM observation adapter.
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.text.*;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

class IdeologProbe {
  static String unhex(String value) { return new String(HexFormat.of().parseHex(value), StandardCharsets.UTF_8); }
  static String hex(String value) { return HexFormat.of().formatHex(value.getBytes(StandardCharsets.UTF_8)); }
  static int outputBytes=0;
  static void emit(String line) { outputBytes+=line.length()+1; if(outputBytes>262144) throw new IllegalArgumentException("output limit"); System.out.println(line); }
  static String cell(String value) { return value.length() > 512 ? value.substring(0,512) : value; }
  static void row(int line, String event, Pattern regex, DateFormat date, int time, int level, int category) {
    String first = event.split("\\n",2)[0];
    Matcher match = regex.matcher(first);
    ArrayList<String> values = new ArrayList<>();
    if (match.find() && match.groupCount() > 0) {
      int end = 0, lastStart = 0;
      for (int n=1; n<=match.groupCount(); n++) {
        if (match.start(n) < end || match.end(n) < 0) continue;
        values.add(match.group(n)); lastStart = match.start(n); end = match.end(n);
      }
      if (!values.isEmpty() && event.indexOf('\n') >= 0) {
        values.set(values.size()-1, event.substring(lastStart));
      }
    }
    if (values.isEmpty()) values.add(event);
    String timestamp = time >= 0 && time < values.size() ? values.get(time) : "";
    String epoch = "";
    if (date != null && !timestamp.isEmpty()) {
      try { epoch = Long.toString(date.parse(timestamp).getTime()); } catch (ParseException ignored) {}
    }
    String severity = level >= 0 && level < values.size() ? values.get(level) : "";
    String cat = category >= 0 && category < values.size() ? values.get(category) : "";
    emit(line + "\t" + epoch + "\t" + hex(cell(timestamp)) + "\t" + hex(cell(severity)) + "\t" + hex(cell(cat)) + "\t" + hex(cell(values.get(values.size()-1))));
  }
  public static void main(String[] args) {
    try {
      if (args.length != 1) throw new IllegalArgumentException("input file");
      byte[] bytes;
      try (InputStream inputFile = Files.newInputStream(Path.of(args[0]))) { bytes = inputFile.readNBytes(150001); }
      if (bytes.length > 150000) throw new IllegalArgumentException("input limit");
      String[] input = new String(bytes,StandardCharsets.UTF_8).split("\\n",-1);
      if (input.length != 10 || !input[0].equals("IDEA-JVM-1")) throw new IllegalArgumentException("protocol");
      String localeTag = unhex(input[1]), zoneName = unhex(input[2]);
      Locale locale = localeTag.isEmpty() ? Locale.getDefault(Locale.Category.FORMAT) : new Locale.Builder().setLanguageTag(localeTag).build();
      if (!localeTag.isEmpty() && locale.getLanguage().isEmpty()) throw new IllegalArgumentException("locale");
      TimeZone zone = TimeZone.getDefault();
      if (!zoneName.isEmpty()) {
        zone = Arrays.asList(TimeZone.getAvailableIDs()).contains(zoneName)
          ? TimeZone.getTimeZone(zoneName) : TimeZone.getTimeZone(java.time.ZoneId.of(zoneName));
      }
      Pattern regex = Pattern.compile(unhex(input[3]), Pattern.DOTALL), lineStart = Pattern.compile(unhex(input[4]));
      String pattern = unhex(input[5]);
      DateFormat date = pattern.isEmpty() ? null : new SimpleDateFormat(pattern, locale);
      if (date != null) date.setTimeZone(zone);
      int time = Integer.parseInt(input[6]), level = Integer.parseInt(input[7]), category = Integer.parseInt(input[8]);
      String text = unhex(input[9]);
      if (text.getBytes(StandardCharsets.UTF_8).length > 65536) throw new IllegalArgumentException("text limit");
      String[] lines = text.split("\\n",-1);
      if (lines.length > 513) throw new IllegalArgumentException("line limit");
      for (String line : lines) if (line.length() > 4096) throw new IllegalArgumentException("line width");
      emit("IDEA-JVM-1\t" + hex(System.getProperty("java.version")) + "\t" + hex(locale.toLanguageTag()) + "\t" + hex(zone.getID()));
      StringBuilder event = new StringBuilder(); int first=0, count=0;
      for (int n=0; n<lines.length; n++) {
        String line=lines[n];
        if (n==lines.length-1 && line.isEmpty()) break;
        if (event.length()>0 && lineStart.matcher(line).find()) {
          row(first,event.toString(),regex,date,time,level,category); count++;
          if (count>=512) throw new IllegalArgumentException("event limit");
          event.setLength(0); first=n;
        }
        if (event.length()==0) first=n; else event.append('\n');
        event.append(line);
      }
      if (event.length()>0) row(first,event.toString(),regex,date,time,level,category);
    } catch (Exception error) {
      System.err.println("Ideolog JVM adapter: " + error.getClass().getSimpleName()); System.exit(2);
    }
  }
}
