/* Independent explicitly reviewed JDBC/JPQL console. Separate from offline-DDL adapter. */
import java.io.*;
import java.lang.reflect.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.*;
import java.util.*;
import java.util.stream.*;

class ReviewedJpaConsole {
    static String PROFILE, VERSION, DIALECT;
    static void configure(String profile, String dialect) {
        VERSION = switch (profile) {
            case "jpql-hibernate-6.6.49-v1" -> "6.6.49.Final";
            case "jpql-hibernate-7.4.15-v1" -> "7.4.15.Final";
            default -> throw new IllegalArgumentException("unsupported console runtime");
        };
        DIALECT = "org.hibernate.dialect." + switch(dialect) {
            case "h2" -> "H2Dialect";
            case "postgresql" -> "PostgreSQLDialect";
            case "mysql" -> "MySQLDialect";
            case "mariadb" -> "MariaDBDialect";
            default -> throw new IllegalArgumentException("unsupported console dialect");
        };
        PROFILE = profile;
    }
    static Path root;
    static long bytes;
    static int files;
    static Path local(String relative) throws Exception {
        Path path = Path.of(relative);
        if (path.isAbsolute() || relative.contains("\\") || relative.contains("|") || relative.contains("\n")) throw new IllegalArgumentException("relative path");
        Path target = root.resolve(path).normalize();
        if (!target.startsWith(root) || !target.toRealPath().startsWith(root) || Files.isSymbolicLink(target)) throw new IllegalArgumentException("outside owner");
        return target;
    }
    static String hex(byte[] bytes) { return HexFormat.of().formatHex(bytes); }
    static String hash(Path path) throws Exception {
        if (!Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)) throw new IllegalArgumentException("regular file required");
        long size = Files.size(path); bytes += size; files++;
        if (size > (32L << 20) || bytes > (128L << 20) || files > 4096) throw new IllegalArgumentException("input budget");
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = Files.newInputStream(path)) { byte[] buffer = new byte[8192]; int count; long read = 0; while ((count = input.read(buffer)) != -1) { read += count; if (read > (32L << 20) || bytes - size + read > (128L << 20)) throw new IllegalArgumentException("grown input budget"); digest.update(buffer, 0, count); } bytes = bytes - size + read; }
        return hex(digest.digest());
    }
    static String quote(String value) {
        StringBuilder out = new StringBuilder("\"");
        for (char ch : value.toCharArray()) {
            switch (ch) { case '"': out.append("\\\""); break; case '\\': out.append("\\\\"); break; case '\n': out.append("\\n"); break; case '\r': out.append("\\r"); break; case '\t': out.append("\\t"); break;
                default: if (ch < 32) out.append(String.format("\\u%04x", (int) ch)); else out.append(ch); }
        }
        return out.append('"').toString();
    }
    static List<String> list(String[] args, int[] cursor, int max) {
        int count = Integer.parseInt(args[cursor[0]++]); if (count < 0 || count > max) throw new IllegalArgumentException("argument budget");
        List<String> result = new ArrayList<>(); for (int i = 0; i < count; i++) result.add(args[cursor[0]++]); return result;
    }
    static void explicitJar(Path path) throws Exception {
        try (java.util.jar.JarFile jar = new java.util.jar.JarFile(path.toFile(), false)) {
            java.util.jar.JarEntry entry = jar.getJarEntry("META-INF/MANIFEST.MF");
            if (entry == null) return;
            try (InputStream input = jar.getInputStream(entry)) {
                byte[] manifest = input.readNBytes((1 << 20) + 1);
                if (manifest.length > (1 << 20)) throw new IllegalArgumentException("manifest budget");
                String implicit = new java.util.jar.Manifest(new ByteArrayInputStream(manifest)).getMainAttributes().getValue("Class-Path");
                if (implicit != null && !implicit.isBlank()) throw new IllegalArgumentException("Manifest Class-Path is unsupported: declare only explicit runtime/project JARs without implicit classpaths");
            }
        }
    }
    static String inspect(List<String> jars, List<String> roots, List<String> sources, List<String> entities, List<String> converters) throws Exception {
        bytes = 0; files = 0;
        List<String> facts = new ArrayList<>();
        for (String jar : jars) { Path path = local(jar); if (!jar.endsWith(".jar")) throw new IllegalArgumentException("jar required"); explicitJar(path); facts.add("jar:" + jar + ":" + hash(path)); }
        for (String directory : roots) {
            Path path = local(directory); if (!Files.isDirectory(path)) throw new IllegalArgumentException("class directory required");
            List<Path> paths;
            try (Stream<Path> stream = Files.walk(path)) { paths = stream.limit(4098).sorted().toList(); }
            if (paths.size() > 4096) throw new IllegalArgumentException("class directory budget");
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            for (Path item : paths) {
                if (Files.isSymbolicLink(item) || !item.toRealPath().startsWith(root)) throw new IllegalArgumentException("class directory link");
                if (Files.isRegularFile(item, LinkOption.NOFOLLOW_LINKS)) { digest.update(path.relativize(item).toString().getBytes(StandardCharsets.UTF_8)); digest.update((byte) 0); digest.update(hash(item).getBytes(StandardCharsets.US_ASCII)); digest.update((byte) 0); }
            }
            facts.add("classes:" + directory + ":" + hex(digest.digest()));
        }
        for (String source : sources) facts.add("source:" + source + ":" + hash(local(source)));
        for (String entity : entities) { String[] pair = entity.split("\\|", -1); if (pair.length != 2 || !roots.contains(pair[1])) throw new IllegalArgumentException("entity root"); local(pair[1] + "/" + pair[0].replace('.', '/') + ".class"); facts.add("entity:" + entity); }
        for (String converter : converters) { String[] pair = converter.split("\\|", -1); if (pair.length != 2 || !roots.contains(pair[1])) throw new IllegalArgumentException("converter root"); local(pair[1] + "/" + pair[0].replace('.', '/') + ".class"); facts.add("converter:" + converter); }
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        for (String fact : facts) { digest.update(fact.getBytes(StandardCharsets.UTF_8)); digest.update((byte) 0); }
        return "{\"profile\":" + quote(PROFILE) + ",\"version\":" + quote(VERSION) + ",\"dialect\":" + quote(DIALECT) + ",\"java\":" + quote(System.getProperty("java.runtime.version")) + ",\"fingerprint\":" + quote(hex(digest.digest())) + ",\"facts\":[" + facts.stream().map(ReviewedJpaConsole::quote).collect(Collectors.joining(",")) + "],\"files\":" + files + ",\"bytes\":" + bytes + "}";
    }
    static Class<?> type(ClassLoader loader, String name) throws Exception { return Class.forName(name, true, loader); }
    static Object call(Object receiver, String name, Class<?>[] types, Object... args) throws Exception { return receiver.getClass().getMethod(name, types).invoke(receiver, args); }
    static List<String> input() throws Exception {
        byte[] bytes = System.in.readNBytes((64 << 10) + 1);
        if (bytes.length > (64 << 10)) throw new IllegalArgumentException("input budget");
        List<String> result = new ArrayList<>();
        for (String line : new String(bytes, StandardCharsets.US_ASCII).split("\n")) result.add(new String(HexFormat.of().parseHex(line), StandardCharsets.UTF_8));
        return result;
    }
    static Object parameter(String type, String value) {
        return switch (type) {
            case "string" -> value;
            case "long" -> Long.valueOf(value);
            case "decimal" -> new java.math.BigDecimal(value);
            case "boolean" -> { if (!Set.of("true", "false").contains(value)) throw new IllegalArgumentException("boolean parameter"); yield Boolean.valueOf(value); }
            case "date" -> java.time.LocalDate.parse(value);
            case "timestamp" -> java.time.LocalDateTime.parse(value);
            case "uuid" -> UUID.fromString(value);
            default -> throw new IllegalArgumentException("unsupported scalar parameter");
        };
    }
    static String cell(Object value) {
        if (value == null) return "{\"type\":\"null\",\"value\":null}";
        String text;
        if (value instanceof String s) text = s;
        else if (value instanceof java.math.BigDecimal n) text = n.toPlainString();
        else if (value instanceof Number || value instanceof Boolean || value instanceof UUID || value instanceof java.time.temporal.TemporalAccessor || value instanceof java.sql.Date || value instanceof java.sql.Timestamp) text = value.toString();
        else throw new IllegalArgumentException("Select scalar projections; entity/custom result rendering is unsupported");
        boolean truncated = text.length() > 2048;
        if (truncated) text = text.substring(0, 2048);
        return "{\"type\":" + quote(value.getClass().getName()) + ",\"value\":" + quote(text) + ",\"truncated\":" + truncated + "}";
    }
    static String query(List<String> jars, List<String> roots, List<String> entities, List<String> converters, List<String> request) throws Exception {
        if (request.size() < 7 || (request.size() - 7) % 3 != 0 || request.size() > 55) throw new IllegalArgumentException("request shape");
        String url = request.get(1), username = request.get(2), password = request.get(3), jpql = request.get(4), driverName = request.get(5);
        int maxRows = Integer.parseInt(request.get(6));
        if (maxRows < 1 || maxRows > 100 || jpql.length() > 8192 || !jpql.stripLeading().toLowerCase(Locale.ROOT).startsWith("select ") || jpql.contains(";") || url.contains(";") || url.contains("?") || url.contains("\n")) throw new IllegalArgumentException("read-only request contract");
        String[] target = switch(DIALECT) {
            case "org.hibernate.dialect.H2Dialect" -> new String[]{"jdbc:h2:","org.h2.Driver"};
            case "org.hibernate.dialect.PostgreSQLDialect" -> new String[]{"jdbc:postgresql://","org.postgresql.Driver"};
            case "org.hibernate.dialect.MySQLDialect" -> new String[]{"jdbc:mysql://","com.mysql.cj.jdbc.Driver"};
            case "org.hibernate.dialect.MariaDBDialect" -> new String[]{"jdbc:mariadb://","org.mariadb.jdbc.Driver"};
            default -> throw new IllegalArgumentException("console target dialect");
        };
        if(!url.startsWith(target[0]) || !driverName.equals(target[1]) || url.contains("@") || url.contains("#") || url.contains("\\") || url.chars().anyMatch(Character::isISOControl)) throw new IllegalArgumentException("explicit matching target/driver required");
        List<URL> urls = new ArrayList<>(); for (String jar : jars) urls.add(local(jar).toUri().toURL()); for (String directory : roots) urls.add(local(directory).toUri().toURL());
        ClassLoader previous = Thread.currentThread().getContextClassLoader();
        try (URLClassLoader loader = new URLClassLoader(urls.toArray(URL[]::new), ClassLoader.getPlatformClassLoader())) {
            Thread.currentThread().setContextClassLoader(loader);
            if (!VERSION.equals(type(loader, "org.hibernate.Version").getMethod("getVersionString").invoke(null))) throw new IllegalArgumentException("Hibernate version");
            java.sql.Driver driver = (java.sql.Driver) type(loader, driverName).getConstructor().newInstance();
            Properties credentials = new Properties(); credentials.setProperty("user", username); credentials.setProperty("password", password);
            try (java.sql.Connection connection = driver.connect(url, credentials)) {
                if (connection == null) throw new IllegalArgumentException("driver rejected target");
                connection.setReadOnly(true); connection.setAutoCommit(false);
                try {
                    Class<?> provider = type(loader, "org.hibernate.engine.jdbc.connections.spi.ConnectionProvider");
                    Object connections = java.lang.reflect.Proxy.newProxyInstance(loader, new Class<?>[]{provider}, (proxy, method, args) -> switch (method.getName()) {
                        case "getConnection" -> connection;
                        case "closeConnection", "stop" -> null;
                        case "supportsAggressiveRelease", "isUnwrappableAs" -> false;
                        case "unwrap" -> throw new IllegalArgumentException("no provider unwrap");
                        case "toString" -> "ReviewedJpaConsoleConnection";
                        case "hashCode" -> System.identityHashCode(proxy);
                        case "equals" -> proxy == args[0];
                        default -> { if (method.isDefault()) yield InvocationHandler.invokeDefault(proxy, method, args == null ? new Object[]{} : args); throw new UnsupportedOperationException(method.getName()); }
                    });
                    Map<String,Object> settings = new HashMap<>();
                    settings.put("hibernate.boot.allow_jdbc_metadata_access", false); settings.put("hibernate.dialect", DIALECT);
                    settings.put("hibernate.connection.provider_class", connections); settings.put("hibernate.connection.autocommit", false);
                    settings.put("hibernate.hbm2ddl.auto", "none"); settings.put("jakarta.persistence.schema-generation.database.action", "none"); settings.put("jakarta.persistence.schema-generation.scripts.action", "none");
                    settings.put("hibernate.cache.use_second_level_cache", false); settings.put("hibernate.archive.autodetection", "none"); settings.put("hibernate.show_sql", false);
                    settings.put("hibernate.implicit_naming_strategy", "org.hibernate.boot.model.naming.ImplicitNamingStrategyJpaCompliantImpl"); settings.put("hibernate.physical_naming_strategy", "org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl");
                    Object builder = type(loader, "org.hibernate.boot.registry.StandardServiceRegistryBuilder").getConstructor().newInstance();
                    call(builder, "clearSettings", new Class<?>[]{}); call(builder, "applySettings", new Class<?>[]{Map.class}, settings);
                    Object registry = call(builder, "build", new Class<?>[]{});
                    try {
                        Class<?> registryType = type(loader, "org.hibernate.service.ServiceRegistry");
                        if (registryType.getMethod("getService", Class.class).invoke(registry, provider) != connections) throw new IllegalStateException("provider replaced");
                        Object sources = type(loader, "org.hibernate.boot.MetadataSources").getConstructor(registryType).newInstance(registry);
                        for (String entity : entities) { String[] pair = entity.split("\\|", -1); Class<?> entityType = Class.forName(pair[0], false, loader); if (!Path.of(entityType.getProtectionDomain().getCodeSource().getLocation().toURI()).toRealPath().equals(local(pair[1]).toRealPath())) throw new IllegalArgumentException("entity origin"); call(sources,"addAnnotatedClass",new Class<?>[]{Class.class},entityType); }
                        Object metadataBuilder = call(sources,"getMetadataBuilder",new Class<?>[]{});
                        for (String converter : converters) { String[] pair = converter.split("\\|",-1); Class<?> converterType=Class.forName(pair[0],false,loader); if (!Path.of(converterType.getProtectionDomain().getCodeSource().getLocation().toURI()).toRealPath().equals(local(pair[1]).toRealPath())) throw new IllegalArgumentException("converter origin"); call(metadataBuilder,"applyAttributeConverter",new Class<?>[]{Class.class},converterType); }
                        Object metadata = call(metadataBuilder,"build",new Class<?>[]{});
                        Object factory = type(loader,"org.hibernate.boot.Metadata").getMethod("buildSessionFactory").invoke(metadata);
                        try {
                            Object session = type(loader,"org.hibernate.SessionFactory").getMethod("openSession").invoke(factory);
                            Class<?> sessionType = type(loader,"org.hibernate.Session");
                            try {
                                sessionType.getMethod("setDefaultReadOnly",boolean.class).invoke(session,true);
                                sessionType.getMethod("setHibernateFlushMode",type(loader,"org.hibernate.FlushMode")).invoke(session,Enum.valueOf((Class)type(loader,"org.hibernate.FlushMode"),"MANUAL"));
                                Object selection = sessionType.getMethod("createSelectionQuery",String.class).invoke(session,jpql);
                                Class<?> selectionType=type(loader,"org.hibernate.query.SelectionQuery");
                                selectionType.getMethod("setMaxResults",int.class).invoke(selection,maxRows+1); selectionType.getMethod("setTimeout",int.class).invoke(selection,15); selectionType.getMethod("setReadOnly",boolean.class).invoke(selection,true);
                                for(int i=7;i<request.size();i+=3) { String name=request.get(i); if(!name.matches("[A-Za-z_][A-Za-z0-9_]{0,63}"))throw new IllegalArgumentException("parameter name"); Object value=parameter(request.get(i+1),request.get(i+2)); selectionType.getMethod("setParameter",String.class,Object.class).invoke(selection,name,value); }
                                List<?> rows = (List<?>)selectionType.getMethod("getResultList").invoke(selection);
                                List<String> output=new ArrayList<>(); int columns=-1;
                                for(Object row:rows.subList(0,Math.min(maxRows,rows.size()))) { Object[] cells=row instanceof Object[] array?array:new Object[]{row}; if(cells.length>32 || (columns!=-1 && columns!=cells.length))throw new IllegalArgumentException("column budget"); columns=cells.length; List<String> values=new ArrayList<>(); for(Object value:cells)values.add(cell(value)); output.add("["+String.join(",",values)+"]"); }
                                String result="{\"rows\":["+String.join(",",output)+"],\"truncated\":"+(rows.size()>maxRows)+",\"rollbackRequested\":true}";
                                if(result.length()>(128<<10))throw new IllegalArgumentException("result budget"); return result;
                            } finally { sessionType.getMethod("close").invoke(session); }
                        } finally { type(loader,"org.hibernate.SessionFactory").getMethod("close").invoke(factory); }
                    } finally { type(loader,"org.hibernate.boot.registry.StandardServiceRegistryBuilder").getMethod("destroy",type(loader,"org.hibernate.service.ServiceRegistry")).invoke(null,registry); }
                } finally { connection.rollback(); }
            }
        } finally { Thread.currentThread().setContextClassLoader(previous); }
    }
    static void execute(String[] args) throws Exception {
        if(args.length<9 || !Set.of(17,21,25,26).contains(Runtime.version().feature()))throw new IllegalArgumentException("Java/profile arguments");
        String mode=args[0]; root=Path.of(args[1]).toRealPath(); configure(args[2],args[3]); int[] cursor={4};
        List<String> jars=list(args,cursor,64),roots=list(args,cursor,4),sources=list(args,cursor,128),entities=list(args,cursor,128),converters=list(args,cursor,64);
        if(cursor[0]!=args.length)throw new IllegalArgumentException("arguments");
        String proof=inspect(jars,roots,sources,entities,converters);
        if(mode.equals("inspect")){System.out.println("OPENIDEA_JPQL_CONSOLE/1\n"+proof);return;}
        List<String> request=input(); if(!mode.equals("query")||request.isEmpty()||!proof.equals(request.get(0)))throw new IllegalArgumentException("changed provenance");
        String result=query(jars,roots,entities,converters,request);
        if(!proof.equals(inspect(jars,roots,sources,entities,converters)))throw new IllegalArgumentException("changed provenance");
        System.out.println("OPENIDEA_JPQL_CONSOLE/1\n{\"proof\":"+quote(proof)+",\"result\":"+result+"}");
    }
    public static void main(String[] args) {
        try { execute(args); } catch(Throwable error) { System.err.println("JPQL console failed; target/query/model execution did not meet the reviewed contract. Driver/provider details suppressed."); System.exit(1); }
    }
}
