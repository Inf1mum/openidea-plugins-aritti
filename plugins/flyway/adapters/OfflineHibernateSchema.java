/* Independent, scripts-only Hibernate metadata adapter. No application or persistence-unit bootstrap. */
import java.io.*;
import java.lang.reflect.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.*;
import java.util.*;
import java.util.stream.*;

class OfflineHibernateSchema {
    static String PROFILE = "hibernate-6.6.49-h2-v1";
    static String VERSION = "6.6.49.Final";
    static String DIALECT = "org.hibernate.dialect.H2Dialect";
    static String TARGET_VERSION = "unreported";
    static void configure(String profile, String dialect) {
        VERSION = switch (profile) {
            case "hibernate-6.6.49-h2-v1", "hibernate-6.6.49-offline-v2" -> "6.6.49.Final";
            case "hibernate-7.4.15-offline-v2" -> "7.4.15.Final";
            default -> throw new IllegalArgumentException("unsupported version profile");
        };
        if (profile.equals("hibernate-6.6.49-h2-v1") && !dialect.equals("h2")) throw new IllegalArgumentException("legacy H2 profile");
        String type = switch (dialect) {
            case "h2" -> "H2Dialect";
            case "postgresql" -> "PostgreSQLDialect";
            case "mysql" -> "MySQLDialect";
            case "mariadb" -> "MariaDBDialect";
            case "sqlserver" -> "SQLServerDialect";
            case "azure-sqlserver" -> "AzureSQLServerDialect";
            case "oracle" -> "OracleDialect";
            case "cockroachdb" -> "CockroachDialect";
            case "db2" -> "DB2Dialect";
            case "db2i" -> "DB2iDialect";
            case "db2z" -> "DB2zDialect";
            case "hsql" -> "HSQLDialect";
            case "hana" -> "HANADialect";
            case "spanner" -> { if (!VERSION.startsWith("7.")) throw new IllegalArgumentException("Pinned6 Spanner export omits mapped constraints; use qualified7 profile"); yield "SpannerDialect"; }
            case "sybase-ase" -> throw new IllegalArgumentException("Stock SybaseASE export omits nullable unique mappings; no qualified preset");
            case "postgres-plus" -> "PostgresPlusDialect";
            case "derby" -> { if (!VERSION.startsWith("6.")) throw new IllegalArgumentException("Derby is not in the pinned7 core"); yield "DerbyDialect"; }
            case "tidb" -> { if (!VERSION.startsWith("6.")) throw new IllegalArgumentException("TiDB is not in the pinned7 core"); yield "TiDBDialect"; }
            case "spanner-postgresql" -> { if (!VERSION.startsWith("7.")) throw new IllegalArgumentException("Spanner PostgreSQL is not in the pinned6 core"); yield "SpannerPostgreSQLDialect"; }
            default -> throw new IllegalArgumentException("unsupported explicit dialect");
        };
        PROFILE = profile; DIALECT = "org.hibernate.dialect." + type;
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
        return "{\"profile\":" + quote(PROFILE) + ",\"version\":" + quote(VERSION) + ",\"dialect\":" + quote(DIALECT) + ",\"java\":" + quote(System.getProperty("java.runtime.version")) + ",\"fingerprint\":" + quote(hex(digest.digest())) + ",\"facts\":[" + facts.stream().map(OfflineHibernateSchema::quote).collect(Collectors.joining(",")) + "],\"files\":" + files + ",\"bytes\":" + bytes + "}";
    }
    static Class<?> type(ClassLoader loader, String name) throws Exception { return Class.forName(name, true, loader); }
    static Object call(Object receiver, String name, Class<?>[] types, Object... args) throws Exception { return receiver.getClass().getMethod(name, types).invoke(receiver, args); }
    static class BoundedWriter extends StringWriter {
        void check(int length) { if (getBuffer().length() + length > (128 << 10)) throw new IllegalArgumentException("SQL output budget"); }
        @Override public void write(String value) { check(value.length()); super.write(value); }
        @Override public void write(String value, int offset, int length) { check(length); super.write(value, offset, length); }
        @Override public void write(char[] value, int offset, int length) { check(length); super.write(value, offset, length); }
        @Override public void write(int value) { check(1); super.write(value); }
    }
    static String generate(List<String> jars, List<String> roots, List<String> entities, List<String> converters) throws Exception {
        List<URL> urls = new ArrayList<>(); for (String jar : jars) urls.add(local(jar).toUri().toURL()); for (String directory : roots) urls.add(local(directory).toUri().toURL());
        ClassLoader previous = Thread.currentThread().getContextClassLoader();
        try (URLClassLoader loader = new URLClassLoader(urls.toArray(URL[]::new), ClassLoader.getPlatformClassLoader())) {
            Thread.currentThread().setContextClassLoader(loader);
            if (!VERSION.equals(type(loader, "org.hibernate.Version").getMethod("getVersionString").invoke(null))) throw new IllegalArgumentException("unsupported Hibernate version");
            Class<?> provider = type(loader, "org.hibernate.engine.jdbc.connections.spi.ConnectionProvider");
            final int[] connections = {0};
            Object noConnections = java.lang.reflect.Proxy.newProxyInstance(loader, new Class<?>[]{provider}, (proxy, method, args) -> {
                return switch (method.getName()) {
                    case "getConnection" -> { connections[0]++; throw new java.sql.SQLException("Offline adapter refuses Hibernate JDBC connections"); }
                    case "closeConnection", "stop" -> null;
                    case "supportsAggressiveRelease", "isUnwrappableAs" -> false;
                    case "unwrap" -> throw new IllegalArgumentException("No offline JDBC provider unwrap");
                    case "toString" -> "OfflineHibernateNoConnections";
                    case "hashCode" -> System.identityHashCode(proxy);
                    case "equals" -> proxy == args[0];
                    default -> { if (method.isDefault()) yield InvocationHandler.invokeDefault(proxy, method, args == null ? new Object[]{} : args); throw new UnsupportedOperationException(method.getName()); }
                };
            });
            StringWriter sql = new BoundedWriter();
            Map<String, Object> settings = new HashMap<>();
            settings.put("hibernate.boot.allow_jdbc_metadata_access", false);
            settings.put("hibernate.dialect", DIALECT);
            settings.put("hibernate.archive.autodetection", "none");
            settings.put("hibernate.implicit_naming_strategy", "org.hibernate.boot.model.naming.ImplicitNamingStrategyJpaCompliantImpl");
            settings.put("hibernate.physical_naming_strategy", "org.hibernate.boot.model.naming.PhysicalNamingStrategyStandardImpl");
            settings.put("hibernate.connection.provider_class", noConnections);
            settings.put("hibernate.hbm2ddl.auto", "none");
            settings.put("jakarta.persistence.schema-generation.database.action", "none");
            settings.put("jakarta.persistence.schema-generation.scripts.action", "create");
            settings.put("jakarta.persistence.schema-generation.create-source", "metadata");
            settings.put("jakarta.persistence.schema-generation.scripts.create-target", sql);
            settings.put("hibernate.hbm2ddl.delimiter", ";");
            settings.put("hibernate.hbm2ddl.import_files", "");
            settings.put("hibernate.hbm2ddl.skip_default_import_file", true);
            settings.put("hibernate.hbm2ddl.halt_on_error", true);
            Object builder = type(loader, "org.hibernate.boot.registry.StandardServiceRegistryBuilder").getConstructor().newInstance();
            call(builder, "clearSettings", new Class<?>[]{});
            call(builder, "applySettings", new Class<?>[]{Map.class}, settings);
            Object registry = call(builder, "build", new Class<?>[]{});
            try {
                Class<?> registryType = type(loader, "org.hibernate.service.ServiceRegistry");
                if (registryType.getMethod("getService", Class.class).invoke(registry, provider) != noConnections) throw new IllegalStateException("Hibernate connection provider replaced");
                Class<?> environmentType = type(loader, "org.hibernate.engine.jdbc.env.spi.JdbcEnvironment");
                Object environment = registryType.getMethod("getService", Class.class).invoke(registry, environmentType);
                Object actualDialect = environmentType.getMethod("getDialect").invoke(environment);
                if (!actualDialect.getClass().getName().equals(DIALECT)) throw new IllegalStateException("configured dialect replaced");
                TARGET_VERSION = type(loader, "org.hibernate.dialect.Dialect").getMethod("getVersion").invoke(actualDialect).toString();
                Object sources = type(loader, "org.hibernate.boot.MetadataSources").getConstructor(registryType).newInstance(registry);
                for (String entity : entities) {
                    String[] pair = entity.split("\\|", -1); Class<?> entityType = Class.forName(pair[0], false, loader);
                    if (!Path.of(entityType.getProtectionDomain().getCodeSource().getLocation().toURI()).toRealPath().equals(local(pair[1]).toRealPath())) throw new IllegalArgumentException("entity classpath shadow");
                    call(sources, "addAnnotatedClass", new Class<?>[]{Class.class}, entityType);
                }
                Object metadataBuilder = call(sources, "getMetadataBuilder", new Class<?>[]{});
                Class<?> converter = type(loader, "jakarta.persistence.AttributeConverter");
                for (String entry : converters) {
                    String[] pair = entry.split("\\|", -1); Class<?> converterType = Class.forName(pair[0], false, loader);
                    if (!converter.isAssignableFrom(converterType) || !Path.of(converterType.getProtectionDomain().getCodeSource().getLocation().toURI()).toRealPath().equals(local(pair[1]).toRealPath())) throw new IllegalArgumentException("converter classpath shadow");
                    call(metadataBuilder, "applyAttributeConverter", new Class<?>[]{Class.class}, converterType);
                }
                Object metadata = call(metadataBuilder, "build", new Class<?>[]{});
                Class<?> delayed = type(loader, "org.hibernate.tool.schema.spi.DelayedDropRegistry");
                Object noDrop = java.lang.reflect.Proxy.newProxyInstance(loader, new Class<?>[]{delayed}, (proxy, method, args) -> { throw new IllegalStateException("No delayed DB action permitted"); });
                type(loader, "org.hibernate.tool.schema.spi.SchemaManagementToolCoordinator").getMethod("process", type(loader, "org.hibernate.boot.Metadata"), registryType, Map.class, delayed).invoke(null, metadata, registry, settings, noDrop);
                if (registryType.getMethod("getService", Class.class).invoke(registry, provider) != noConnections) throw new IllegalStateException("Hibernate connection provider replaced");
                if (connections[0] != 0 || sql.getBuffer().length() > (128 << 10) || sql.toString().isBlank()) throw new IllegalStateException("offline output contract");
                return sql.toString();
            } finally { type(loader, "org.hibernate.boot.registry.StandardServiceRegistryBuilder").getMethod("destroy", type(loader, "org.hibernate.service.ServiceRegistry")).invoke(null, registry); }
        } finally { Thread.currentThread().setContextClassLoader(previous); }
    }
    public static void main(String[] args) throws Exception {
        if (args.length < 7) throw new IllegalArgumentException("adapter arguments");
        if (!Set.of(17, 21, 25, 26).contains(Runtime.version().feature())) throw new IllegalArgumentException("supported Java profiles:17/21/25/26");
        String mode = args[0]; root = Path.of(args[1]).toRealPath(); int[] cursor = {2};
        if (args[2].startsWith("hibernate-")) { configure(args[2], args[3]); cursor[0] = 4; } else { configure("hibernate-6.6.49-h2-v1", "h2"); }
        List<String> jars = list(args, cursor, 64), roots = list(args, cursor, 4), sources = list(args, cursor, 128), entities = list(args, cursor, 128), converters = list(args, cursor, 64);
        String proof = inspect(jars, roots, sources, entities, converters);
        if (mode.equals("inspect") && cursor[0] == args.length) { System.out.println("OPENIDEA_HIBERNATE_SCHEMA/1"); System.out.println(proof); return; }
        if (!mode.equals("generate") || cursor[0] + 1 != args.length || !proof.equals(args[cursor[0]])) throw new IllegalArgumentException("changed input provenance");
        String sql = generate(jars, roots, entities, converters);
        if (!proof.equals(inspect(jars, roots, sources, entities, converters))) throw new IllegalArgumentException("inputs changed during metadata generation");
        System.out.println("OPENIDEA_HIBERNATE_SCHEMA/1");
        System.out.println("{\"profile\":" + quote(PROFILE) + ",\"version\":" + quote(VERSION) + ",\"dialect\":" + quote(DIALECT) + ",\"databaseVersion\":" + quote(TARGET_VERSION) + ",\"proof\":" + quote(proof) + ",\"hibernateJdbcRequests\":0,\"sql\":" + quote(sql) + "}");
    }
}
