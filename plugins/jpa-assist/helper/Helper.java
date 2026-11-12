import java.io.FileDescriptor;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintStream;
import java.net.ConnectException;
import java.net.NoRouteToHostException;
import java.net.SocketTimeoutException;
import java.net.URL;
import java.net.URLClassLoader;
import java.net.URLDecoder;
import java.net.UnknownHostException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.cert.CertificateException;
import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Properties;
import java.util.Set;
import java.util.TreeMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import javax.net.ssl.SSLException;

/**
 * Managed JDBC metadata helper, protocol v1 (see plugins/jpa-assist/parity/JDBC_HELPER.md).
 *
 * One request per process: a single JSON line on stdin, then EOF. One response: a single JSON line on the
 * original stdout. JDK only (java.*), Java 17 language level, started as `java Helper.java`.
 * Metadata only: no SQL text is accepted and none is executed.
 * The password is read only from the request (secrets.password); never from argv or the environment.
 */
public class Helper {
    static final int PROTOCOL = 1;
    static final String HELPER_VERSION = "1.0.0";
    static final int MAX_REQUEST_BYTES = 1 << 20;
    static final long DEFAULT_DEADLINE_MS = 50_000;
    static final long MAX_DEADLINE_MS = 55_000;
    static final int DEFAULT_MAX_BYTES = 3_000_000;
    static final int MAX_MAX_BYTES = 3_500_000;
    static final int MIN_MAX_BYTES = 4096;
    static final int ENVELOPE_RESERVE = 2048;
    static final int MAX_DESCRIBE_TABLES = 25;
    static final int MAX_LIST_LIMIT = 1000;
    static final int MAX_JARS = 16;
    static final int MAX_MESSAGE = 2000;
    static final int CONNECT_TIMEOUT_SECONDS = 5;

    static final Set<String> DRIVER_CLASSES = Set.of(
            "org.postgresql.Driver",
            "com.mysql.cj.jdbc.Driver",
            "com.mysql.jdbc.Driver",
            "org.mariadb.jdbc.Driver",
            "org.h2.Driver",
            "com.microsoft.sqlserver.jdbc.SQLServerDriver",
            "oracle.jdbc.OracleDriver",
            "oracle.jdbc.driver.OracleDriver",
            "com.ibm.db2.jcc.DB2Driver");

    static final Set<String> TABLE_TYPES = Set.of("TABLE", "BASE TABLE", "PARTITIONED TABLE", "VIEW", "MATERIALIZED VIEW");
    static final Set<String> SYSTEM_SCHEMAS = Set.of("information_schema", "pg_catalog", "pg_toast", "mysql",
            "performance_schema", "sys", "sysibm", "syscat", "sysstat", "sysfun", "sysproc", "syspublic");

    // Properties that make a driver load classes, read files, run SQL or leak secrets. Lower case.
    // Written from knowledge of known driver CVE classes; re-check against current driver docs.
    static final Set<String> DENIED_KEYS = Set.of(
            "password", "pwd", "passwd",
            "socketfactory", "socketfactoryarg", "sslfactory", "sslfactoryarg", "sslhostnameverifier",
            "sslpasswordcallback", "authenticationpluginclassname", "loggerfile", "loggerlevel",
            "jaaslogin", "jaasapplicationname", "replication",
            "allowloadlocalinfile", "allowloadlocalinfileinpath", "allowlocalinfile", "allowurlinlocalinfile",
            "autodeserialize", "queryinterceptors", "statementinterceptors", "exceptioninterceptors",
            "connectionlifecycleinterceptors", "propertiestransform", "authenticationplugins",
            "defaultauthenticationplugin", "clientinfoprovider", "loadbalancestrategy", "logger",
            "profilersql", "credentialtype", "tlssockettype", "pamusecleartext", "restrictedauth",
            "init", "runscript", "trace_level_file", "trace_level_system_out", "auto_server");

    static final Map<String, Set<String>> STRICT_KEYS = Map.of(
            "sqlserver", Set.of("databasename", "database", "encrypt", "trustservercertificate", "hostnameincertificate",
                    "applicationintent", "instancename", "logintimeout", "applicationname", "user"),
            "oracle", Set.of("user"),
            "db2", Set.of("sslconnection", "user"),
            "h2", Set.of("user"));

    static final Pattern KEY_PATTERN = Pattern.compile("(?:^|[?&;(,:@/])\\s*([a-z0-9_.\\-]+)\\s*=");
    static final Pattern SQLSERVER_URL = Pattern.compile("^jdbc:sqlserver://[\\w.\\-\\\\:\\[\\]]+(;.*)?$", Pattern.CASE_INSENSITIVE);
    static final Pattern ORACLE_URL = Pattern.compile("^jdbc:oracle:thin:@(//)?[\\w.\\-]+(:\\d+)?([:/][\\w.\\-]+)?$", Pattern.CASE_INSENSITIVE);
    static final Pattern DB2_URL = Pattern.compile("^jdbc:db2://[\\w.\\-]+(:\\d+)?/[\\w.\\-]+(:.*)?$", Pattern.CASE_INSENSITIVE);
    static final Pattern H2_URL = Pattern.compile("^jdbc:h2:(file:[^;?]+|tcp://[^;?]+|[/~][^;?]*)$");

    static final List<String> secrets = new CopyOnWriteArrayList<>();
    static String password = "";
    static PrintStream realOut;
    static int maxBytes = DEFAULT_MAX_BYTES;
    static volatile boolean responded;

    static final class HelperException extends Exception {
        private static final long serialVersionUID = 1L;

        final String code;

        HelperException(String code, String message) {
            super(message);
            this.code = code;
        }
    }

    static final class Reply {
        final Map<String, Object> result;
        boolean truncated;
        Long next;

        Reply(Map<String, Object> result) {
            this.result = result;
        }
    }

    public static void main(String[] args) {
        realOut = new PrintStream(new FileOutputStream(FileDescriptor.out), false, StandardCharsets.UTF_8);
        System.setOut(System.err);
        Object id = null;
        try {
            Map<String, Object> req = parseRequest(readStdin());
            id = req.get("id") == null ? null : clip(String.valueOf(req.get("id")), 128);
            if (!Objects.equals(num(req.get("v")), (long) PROTOCOL)) {
                emit(error(id, "ProtocolVersion", "helper speaks v" + PROTOCOL, null, 0, false));
                System.exit(0);
            }
            Object secret = objOrEmpty(req.get("secrets")).get("password");
            password = secret == null ? "" : String.valueOf(secret);
            registerSecret(password);
            for (Map.Entry<String, Object> prop : objOrEmpty(objOrEmpty(req.get("conn")).get("props")).entrySet()) {
                if (looksSecret(prop.getKey())) {
                    registerSecret(prop.getValue());
                }
            }
            long deadline = clamp(req.get("deadlineMs") == null ? DEFAULT_DEADLINE_MS : num(req.get("deadlineMs")), 1, MAX_DEADLINE_MS);
            maxBytes = (int) clamp(req.get("maxBytes") == null ? DEFAULT_MAX_BYTES : num(req.get("maxBytes")), MIN_MAX_BYTES, MAX_MAX_BYTES);
            startWatchdog(id, deadline);

            long started = System.nanoTime();
            Map<String, Long> timing = new LinkedHashMap<>();
            Reply reply = dispatch(req, deadline, timing);
            timing.put("total", (System.nanoTime() - started) / 1_000_000);

            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("v", PROTOCOL);
            resp.put("id", id);
            resp.put("ok", true);
            resp.put("truncated", reply.truncated);
            resp.put("next", reply.next);
            resp.put("result", reply.result);
            resp.put("timingMs", timing);
            String line = toJson(resp);
            if (line.getBytes(StandardCharsets.UTF_8).length > maxBytes) {
                emit(error(id, "ResultTooLarge", "response exceeds maxBytes " + maxBytes, null, 0, false));
            } else {
                emitLine(line);
            }
        } catch (Throwable t) {
            emit(classify(id, t));
        }
        System.exit(0);
    }

    // The deadline is the only backstop if the host disappears, so it also covers a blocked connect.
    // It answers and halts instead of closing the connection: the process dies with it.
    static void startWatchdog(Object id, long deadlineMs) {
        Thread watchdog = new Thread(() -> {
            try {
                Thread.sleep(deadlineMs);
            } catch (InterruptedException e) {
                return;
            }
            if (emit(error(id, "DeadlineExceeded", "helper deadline " + deadlineMs + " ms", null, 0, true))) {
                Runtime.getRuntime().halt(0);
            }
        }, "watchdog");
        watchdog.setDaemon(true);
        watchdog.start();
    }

    static String readStdin() throws HelperException, IOException {
        InputStream in = System.in;
        byte[] bytes = in.readNBytes(MAX_REQUEST_BYTES + 1);
        if (bytes.length > MAX_REQUEST_BYTES) {
            throw new HelperException("BadRequest", "request larger than " + MAX_REQUEST_BYTES + " bytes");
        }
        return new String(bytes, StandardCharsets.UTF_8).trim();
    }

    static Map<String, Object> parseRequest(String line) throws HelperException {
        try {
            Object parsed = new Json(line).parse();
            if (!(parsed instanceof Map)) {
                throw new IllegalArgumentException("request is not an object");
            }
            return cast(parsed);
        } catch (RuntimeException e) {
            throw new HelperException("BadRequest", "malformed JSON: " + e.getMessage());
        }
    }

    static Reply dispatch(Map<String, Object> req, long deadlineMs, Map<String, Long> timing) throws Exception {
        String op = String.valueOf(req.get("op"));
        Map<String, Object> args = objOrEmpty(req.get("args"));
        if (op.equals("ping")) {
            return new Reply(ping());
        }
        if (!Set.of("testConnection", "listSchemas", "listTables", "describeTables").contains(op)) {
            throw new HelperException("BadRequest", "unknown op " + op);
        }
        Map<String, Object> conn = objOrEmpty(req.get("conn"));
        long connectStart = System.nanoTime();
        try (Connection c = connect(conn, deadlineMs)) {
            timing.put("connect", (System.nanoTime() - connectStart) / 1_000_000);
            long queryStart = System.nanoTime();
            DatabaseMetaData md = c.getMetaData();
            Reply reply;
            switch (op) {
                case "testConnection":
                    reply = new Reply(obj("productName", md.getDatabaseProductName(), "productVersion", md.getDatabaseProductVersion(),
                            "driverName", md.getDriverName(), "driverVersion", md.getDriverVersion(),
                            "catalog", c.getCatalog(), "schema", schemaOf(c), "readOnly", c.isReadOnly()));
                    break;
                case "listSchemas":
                    reply = listSchemas(c, md);
                    break;
                case "listTables":
                    reply = listTables(c, md, args);
                    break;
                default:
                    reply = describeTables(c, md, args);
                    break;
            }
            timing.put("query", (System.nanoTime() - queryStart) / 1_000_000);
            return reply;
        }
    }

    static Map<String, Object> ping() {
        boolean jdk = false;
        try {
            jdk = javax.tools.ToolProvider.getSystemJavaCompiler() != null;
        } catch (Throwable ignored) {
        }
        return obj("helperVersion", HELPER_VERSION, "protocol", PROTOCOL,
                "java", obj("version", System.getProperty("java.version"), "vendor", System.getProperty("java.vendor"), "jdk", jdk),
                "os", System.getProperty("os.name"));
    }

    // ---- connection ----

    static Driver loadDriver(Map<String, Object> spec) throws Exception {
        String className = spec.get("class") == null ? "" : String.valueOf(spec.get("class"));
        if (!DRIVER_CLASSES.contains(className)) {
            throw new HelperException("BadRequest", "driver class not on the allowlist: " + clip(className, 120));
        }
        List<Object> jars = list(spec.get("jars"));
        if (jars.isEmpty() || jars.size() > MAX_JARS) {
            throw new HelperException("BadRequest", "driver.jars must hold 1.." + MAX_JARS + " entries");
        }
        List<URL> urls = new ArrayList<>();
        for (Object jar : jars) {
            Path path = Paths.get(String.valueOf(jar));
            if (!path.isAbsolute() || !path.toString().endsWith(".jar")) {
                throw new HelperException("BadRequest", "driver jar must be an absolute .jar path: " + clip(path.toString(), 200));
            }
            if (!Files.isRegularFile(path)) {
                throw new FileNotFoundException(path.toString());
            }
            urls.add(path.toUri().toURL());
        }
        // DriverManager cannot see a driver from a private loader ("No suitable driver"), so connect through the instance.
        URLClassLoader loader = new URLClassLoader(urls.toArray(new URL[0]), ClassLoader.getPlatformClassLoader());
        Class<?> type = Class.forName(className, true, loader);
        if (!Driver.class.isAssignableFrom(type)) {
            throw new HelperException("BadRequest", "class is not a java.sql.Driver");
        }
        return (Driver) type.getDeclaredConstructor().newInstance();
    }

    static Connection connect(Map<String, Object> conn, long deadlineMs) throws Exception {
        if (!(conn.get("url") instanceof String)) {
            throw new HelperException("BadRequest", "conn.url is required");
        }
        String url = (String) conn.get("url");
        Map<String, Object> props = objOrEmpty(conn.get("props"));
        String vendor = checkUrl(url, props);
        Driver driver = loadDriver(objOrEmpty(conn.get("driver")));

        Properties info = new Properties();
        if (conn.get("user") != null) {
            info.setProperty("user", String.valueOf(conn.get("user")));
        }
        for (Map.Entry<String, Object> e : props.entrySet()) {
            info.setProperty(e.getKey(), String.valueOf(e.getValue()));
        }
        applyForcedProperties(vendor, url, info, deadlineMs);
        DriverManager.setLoginTimeout(CONNECT_TIMEOUT_SECONDS);
        Connection connection = driver.connect(url, info);
        if (connection == null) {
            throw new HelperException("UrlUnsupported", "driver does not accept this URL");
        }
        try {
            connection.setReadOnly(true);
        } catch (SQLException ignored) {
            // Only a hint (H2 ignores it): safety comes from using the metadata API only.
        }
        return connection;
    }

    static void applyForcedProperties(String vendor, String url, Properties info, long deadlineMs) {
        if (!password.isEmpty()) {
            info.setProperty("password", password);
        }
        long socketMs = Math.max(1000, deadlineMs);
        switch (vendor) {
            case "postgresql":
                // connectTimeout and loginTimeout are seconds for pgjdbc.
                info.setProperty("connectTimeout", String.valueOf(CONNECT_TIMEOUT_SECONDS));
                info.setProperty("loginTimeout", String.valueOf(CONNECT_TIMEOUT_SECONDS));
                info.setProperty("socketTimeout", String.valueOf((socketMs + 999) / 1000));
                break;
            case "mysql":
            case "mariadb":
                // milliseconds for MySQL and MariaDB.
                info.setProperty("connectTimeout", String.valueOf(CONNECT_TIMEOUT_SECONDS * 1000));
                info.setProperty("socketTimeout", String.valueOf(socketMs));
                info.setProperty("autoDeserialize", "false");
                info.setProperty("allowLoadLocalInfile", "false");
                info.setProperty("allowLocalInfile", "false");
                info.setProperty("allowUrlInLocalInfile", "false");
                break;
            case "sqlserver":
                info.setProperty("loginTimeout", String.valueOf(CONNECT_TIMEOUT_SECONDS));
                info.setProperty("socketTimeout", String.valueOf(socketMs));
                break;
            case "oracle":
                info.setProperty("oracle.net.CONNECT_TIMEOUT", String.valueOf(CONNECT_TIMEOUT_SECONDS * 1000));
                info.setProperty("oracle.jdbc.ReadTimeout", String.valueOf(socketMs));
                break;
            case "db2":
                info.setProperty("loginTimeout", String.valueOf(CONNECT_TIMEOUT_SECONDS));
                break;
            case "h2":
                if (!url.toLowerCase(Locale.ROOT).startsWith("jdbc:h2:tcp:")) {
                    info.setProperty("ACCESS_MODE_DATA", "r");
                    info.setProperty("IFEXISTS", "TRUE");
                }
                break;
            default:
                break;
        }
    }

    /** Returns the vendor key. UrlUnsupported when the scheme is not allowed, UrlRejected when a property is. */
    static String checkUrl(String url, Map<String, Object> props) throws HelperException {
        if (url.length() > 2048 || url.chars().anyMatch(ch -> ch < 0x20)) {
            throw new HelperException("BadRequest", "conn.url is malformed");
        }
        String lower = url.toLowerCase(Locale.ROOT);
        String vendor;
        if (lower.startsWith("jdbc:postgresql:")) {
            vendor = "postgresql";
        } else if (lower.startsWith("jdbc:mysql:")) {
            vendor = "mysql";
        } else if (lower.startsWith("jdbc:mariadb:")) {
            vendor = "mariadb";
        } else if (lower.startsWith("jdbc:h2:")) {
            vendor = "h2";
        } else if (lower.startsWith("jdbc:sqlserver:")) {
            vendor = "sqlserver";
        } else if (lower.startsWith("jdbc:oracle:thin:")) {
            vendor = "oracle";
        } else if (lower.startsWith("jdbc:db2:")) {
            vendor = "db2";
        } else {
            throw new HelperException("UrlUnsupported", "URL scheme is not supported");
        }

        String decoded;
        try {
            decoded = URLDecoder.decode(url, StandardCharsets.UTF_8).toLowerCase(Locale.ROOT);
        } catch (IllegalArgumentException e) {
            throw new HelperException("UrlRejected", "URL has invalid percent-encoding");
        }
        if (decoded.indexOf('\0') >= 0) {
            throw new HelperException("UrlRejected", "URL contains a NUL character");
        }
        boolean shapeOk = true;
        switch (vendor) {
            case "sqlserver":
                shapeOk = SQLSERVER_URL.matcher(url).matches();
                break;
            case "oracle":
                shapeOk = ORACLE_URL.matcher(url).matches();
                break;
            case "db2":
                shapeOk = DB2_URL.matcher(url).matches();
                break;
            case "h2":
                shapeOk = H2_URL.matcher(url).matches();
                break;
            default:
                break;
        }
        if (!shapeOk) {
            throw new HelperException("UrlRejected", "URL shape is not allowed for " + vendor);
        }

        Set<String> keys = new java.util.LinkedHashSet<>();
        Matcher m = KEY_PATTERN.matcher(vendor.equals("h2") ? decoded : decoded.substring(decoded.indexOf(':') + 1));
        while (m.find()) {
            keys.add(m.group(1));
        }
        for (String key : props.keySet()) {
            keys.add(key.toLowerCase(Locale.ROOT));
        }
        Set<String> strict = STRICT_KEYS.get(vendor);
        for (String key : keys) {
            if (DENIED_KEYS.contains(key)) {
                throw new HelperException("UrlRejected", "property '" + key + "' is not allowed");
            }
            if (strict != null && !strict.contains(key)) {
                throw new HelperException("UrlRejected", "property '" + key + "' is not allowed for " + vendor);
            }
        }
        return vendor;
    }

    // ---- metadata ----

    static final class Scope {
        final String catalog;
        final String schema;

        Scope(String catalog, String schema) {
            this.catalog = catalog;
            this.schema = schema;
        }
    }

    static boolean catalogIsSchema(Connection c, DatabaseMetaData md) {
        try {
            String term = md.getSchemaTerm();
            if (term == null || term.isEmpty()) {
                return true;
            }
            String product = String.valueOf(md.getDatabaseProductName()).toLowerCase(Locale.ROOT);
            if (!product.contains("mysql") && !product.contains("mariadb")) {
                return false;
            }
            try (ResultSet schemas = md.getSchemas()) {
                return !schemas.next();
            }
        } catch (SQLException e) {
            return false;
        }
    }

    static String schemaOf(Connection c) {
        try {
            return c.getSchema();
        } catch (Throwable t) {
            return null;
        }
    }

    static List<String> catalogNames(DatabaseMetaData md) throws SQLException {
        List<String> names = new ArrayList<>();
        try (ResultSet rs = md.getCatalogs()) {
            while (rs.next()) {
                names.add(rs.getString(1));
            }
        }
        return names;
    }

    static List<String> schemaNames(DatabaseMetaData md) throws SQLException {
        List<String> names = new ArrayList<>();
        try (ResultSet rs = md.getSchemas()) {
            while (rs.next()) {
                names.add(rs.getString("TABLE_SCHEM"));
            }
        }
        return names;
    }

    static Scope resolveScope(Connection c, DatabaseMetaData md, Object requested, boolean useCatalog) throws Exception {
        String wanted = requested == null ? null : String.valueOf(requested);
        List<String> names;
        try {
            names = useCatalog ? catalogNames(md) : schemaNames(md);
        } catch (SQLException e) {
            names = new ArrayList<>();
        }
        String chosen = wanted != null ? wanted : (useCatalog ? c.getCatalog() : schemaOf(c));
        if (chosen == null) {
            return new Scope(useCatalog ? null : c.getCatalog(), null);
        }
        String resolved = null;
        if (names.contains(chosen)) {
            resolved = chosen;
        } else {
            List<String> insensitive = new ArrayList<>();
            for (String n : names) {
                if (n != null && n.equalsIgnoreCase(chosen)) {
                    insensitive.add(n);
                }
            }
            if (insensitive.size() == 1) {
                resolved = insensitive.get(0);
            }
        }
        if (resolved == null) {
            if (wanted != null && !names.isEmpty()) {
                throw new HelperException("SchemaNotFound", "schema not found: " + clip(wanted, 200));
            }
            resolved = chosen;
        }
        return useCatalog ? new Scope(resolved, null) : new Scope(c.getCatalog(), resolved);
    }

    static String escapeLike(DatabaseMetaData md, String s) throws SQLException {
        String esc = md.getSearchStringEscape();
        if (s == null || esc == null || esc.isEmpty()) {
            return s;
        }
        StringBuilder b = new StringBuilder();
        for (char ch : s.toCharArray()) {
            if (ch == '_' || ch == '%' || esc.indexOf(ch) >= 0) {
                b.append(esc);
            }
            b.append(ch);
        }
        return b.toString();
    }

    static Reply listSchemas(Connection c, DatabaseMetaData md) throws Exception {
        boolean useCatalog = catalogIsSchema(c, md);
        List<Object> catalogs = new ArrayList<>(catalogNames(md));
        String currentCatalog = c.getCatalog();
        String currentSchema = schemaOf(c);
        List<Object> schemas = new ArrayList<>();
        if (useCatalog) {
            for (Object name : catalogs) {
                schemas.add(obj("catalog", name, "name", name, "isDefault", Objects.equals(name, currentCatalog),
                        "system", SYSTEM_SCHEMAS.contains(String.valueOf(name).toLowerCase(Locale.ROOT))));
            }
        } else {
            try (ResultSet rs = md.getSchemas()) {
                while (rs.next()) {
                    String name = rs.getString("TABLE_SCHEM");
                    schemas.add(obj("catalog", optString(rs, "TABLE_CATALOG"), "name", name, "isDefault", Objects.equals(name, currentSchema),
                            "system", name != null && (SYSTEM_SCHEMAS.contains(name.toLowerCase(Locale.ROOT)) || name.toLowerCase(Locale.ROOT).startsWith("pg_temp") || name.toLowerCase(Locale.ROOT).startsWith("pg_toast_temp"))));
                }
            }
        }
        return new Reply(obj("catalogs", catalogs, "schemas", schemas, "current", obj("catalog", currentCatalog, "schema", currentSchema)));
    }

    static String normalizeTableType(String raw) {
        if (raw == null) {
            return null;
        }
        String t = raw.toUpperCase(Locale.ROOT);
        if (!TABLE_TYPES.contains(t)) {
            return null;
        }
        if (t.equals("BASE TABLE") || t.equals("PARTITIONED TABLE")) {
            return "TABLE";
        }
        return t;
    }

    static Reply listTables(Connection c, DatabaseMetaData md, Map<String, Object> args) throws Exception {
        boolean useCatalog = catalogIsSchema(c, md);
        Scope scope = resolveScope(c, md, args.get("schema"), useCatalog);
        String pattern = args.get("pattern") == null ? "%" : String.valueOf(args.get("pattern"));
        Set<String> wantedTypes = new java.util.HashSet<>();
        if (args.get("types") == null) {
            wantedTypes.addAll(List.of("TABLE", "VIEW", "MATERIALIZED VIEW"));
        } else {
            for (Object t : list(args.get("types"))) {
                wantedTypes.add(String.valueOf(t).toUpperCase(Locale.ROOT));
            }
        }
        long offset = Math.max(0, args.get("offset") == null ? 0 : num(args.get("offset")));
        long limit = clamp(args.get("limit") == null ? 200 : num(args.get("limit")), 1, MAX_LIST_LIMIT);

        List<Object> out = new ArrayList<>();
        long seen = 0;
        long bytes = 0;
        boolean more = false;
        try (ResultSet rs = md.getTables(scope.catalog, scope.schema == null ? null : escapeLike(md, scope.schema), pattern, null)) {
            while (rs.next()) {
                String type = normalizeTableType(rs.getString("TABLE_TYPE"));
                if (type == null || !wantedTypes.contains(type)) {
                    continue;
                }
                if (seen++ < offset) {
                    continue;
                }
                Map<String, Object> item = obj("catalog", rs.getString("TABLE_CAT"), "schema", rs.getString("TABLE_SCHEM"),
                        "name", rs.getString("TABLE_NAME"), "type", type, "remarks", optString(rs, "REMARKS"));
                int size = toJson(item).getBytes(StandardCharsets.UTF_8).length + 1;
                if (out.size() >= limit || bytes + size > maxBytes - ENVELOPE_RESERVE) {
                    if (out.isEmpty()) {
                        throw new HelperException("ResultTooLarge", "a single table entry exceeds maxBytes");
                    }
                    more = true;
                    break;
                }
                bytes += size;
                out.add(item);
            }
        }
        Reply reply = new Reply(obj("tables", out));
        reply.truncated = more;
        reply.next = more ? Long.valueOf(offset + out.size()) : null;
        return reply;
    }

    static Reply describeTables(Connection c, DatabaseMetaData md, Map<String, Object> args) throws Exception {
        List<Object> names = list(args.get("tables"));
        if (names.isEmpty() || names.size() > MAX_DESCRIBE_TABLES) {
            throw new HelperException("BadRequest", "tables must hold 1.." + MAX_DESCRIBE_TABLES + " names");
        }
        int offset = (int) Math.max(0, args.get("offset") == null ? 0 : num(args.get("offset")));
        boolean useCatalog = catalogIsSchema(c, md);
        Scope scope = resolveScope(c, md, args.get("schema"), useCatalog);

        List<Object> out = new ArrayList<>();
        long bytes = 0;
        int index = offset;
        for (; index < names.size(); index++) {
            if (!(names.get(index) instanceof String)) {
                throw new HelperException("BadRequest", "table names must be strings");
            }
            Map<String, Object> table = describeOne(md, scope, (String) names.get(index));
            int size = toJson(table).getBytes(StandardCharsets.UTF_8).length + 1;
            if (bytes + size > maxBytes - ENVELOPE_RESERVE) {
                if (out.isEmpty()) {
                    throw new HelperException("ResultTooLarge", "a single table description exceeds maxBytes");
                }
                break;
            }
            bytes += size;
            out.add(table);
        }
        Reply reply = new Reply(obj("tables", out));
        reply.truncated = index < names.size();
        reply.next = reply.truncated ? Long.valueOf(index) : null;
        return reply;
    }

    interface Facet {
        Object get() throws Exception;
    }

    static void facet(Map<String, Object> table, Map<String, Object> errors, String name, Facet f) {
        try {
            table.put(name, f.get());
        } catch (Exception e) {
            table.put(name, null);
            errors.put(name, scrub(clip(String.valueOf(e.getMessage()), 500)));
        }
    }

    static Map<String, Object> describeOne(DatabaseMetaData md, Scope scope, String table) throws SQLException {
        String escapedTable = escapeLike(md, table);
        String schemaPattern = scope.schema == null ? null : escapeLike(md, scope.schema);
        Map<String, Object> out = new LinkedHashMap<>();
        Map<String, Object> errors = new LinkedHashMap<>();
        out.put("name", table);
        out.put("schema", scope.schema != null ? scope.schema : scope.catalog);
        String type = null;
        String remarks = null;
        boolean found = false;
        try (ResultSet rs = md.getTables(scope.catalog, schemaPattern, escapedTable, null)) {
            while (rs.next()) {
                if (table.equals(rs.getString("TABLE_NAME"))) {
                    found = true;
                    type = normalizeTableType(rs.getString("TABLE_TYPE"));
                    remarks = optString(rs, "REMARKS");
                    break;
                }
            }
        } catch (SQLException e) {
            errors.put("table", scrub(clip(String.valueOf(e.getMessage()), 500)));
        }
        out.put("found", found);
        out.put("type", type);
        out.put("remarks", remarks);

        facet(out, errors, "columns", () -> {
            List<Object> cols = new ArrayList<>();
            try (ResultSet rs = md.getColumns(scope.catalog, schemaPattern, escapedTable, "%")) {
                while (rs.next()) {
                    if (!table.equals(rs.getString("TABLE_NAME"))) {
                        continue;
                    }
                    cols.add(obj("name", rs.getString("COLUMN_NAME"), "typeName", rs.getString("TYPE_NAME"), "dataType", rs.getInt("DATA_TYPE"),
                            "size", optInt(rs, "COLUMN_SIZE"), "scale", optInt(rs, "DECIMAL_DIGITS"), "nullable", optInt(rs, "NULLABLE"),
                            "default", optString(rs, "COLUMN_DEF"), "autoIncrement", optString(rs, "IS_AUTOINCREMENT"),
                            "generated", optString(rs, "IS_GENERATEDCOLUMN"), "remarks", optString(rs, "REMARKS"),
                            "position", rs.getInt("ORDINAL_POSITION")));
                }
            }
            return cols;
        });
        facet(out, errors, "primaryKey", () -> {
            TreeMap<Integer, String> cols = new TreeMap<>();
            String name = null;
            try (ResultSet rs = md.getPrimaryKeys(scope.catalog, scope.schema, table)) {
                while (rs.next()) {
                    cols.put(rs.getInt("KEY_SEQ"), rs.getString("COLUMN_NAME"));
                    name = rs.getString("PK_NAME");
                }
            }
            return obj("name", name, "columns", new ArrayList<>(cols.values()));
        });
        facet(out, errors, "foreignKeys", () -> {
            Map<String, Map<String, Object>> fks = new LinkedHashMap<>();
            Map<String, TreeMap<Integer, String[]>> parts = new LinkedHashMap<>();
            int anonymous = 0;
            try (ResultSet rs = md.getImportedKeys(scope.catalog, scope.schema, table)) {
                while (rs.next()) {
                    String name = rs.getString("FK_NAME");
                    int seq = rs.getInt("KEY_SEQ");
                    if (name == null && seq == 1) {
                        anonymous++;
                    }
                    String key = name != null ? name + "|" + rs.getString("PKTABLE_NAME") : "\0" + anonymous + "|" + rs.getString("PKTABLE_NAME");
                    Map<String, Object> fk = fks.computeIfAbsent(key, k -> obj("name", null, "refCatalog", null, "refSchema", null, "refTable", null,
                            "columns", new ArrayList<>(), "refColumns", new ArrayList<>(), "updateRule", null, "deleteRule", null));
                    fk.put("name", name);
                    fk.put("refCatalog", rs.getString("PKTABLE_CAT"));
                    fk.put("refSchema", rs.getString("PKTABLE_SCHEM"));
                    fk.put("refTable", rs.getString("PKTABLE_NAME"));
                    fk.put("updateRule", optInt(rs, "UPDATE_RULE"));
                    fk.put("deleteRule", optInt(rs, "DELETE_RULE"));
                    parts.computeIfAbsent(key, k -> new TreeMap<>()).put(seq, new String[]{rs.getString("FKCOLUMN_NAME"), rs.getString("PKCOLUMN_NAME")});
                }
            }
            for (Map.Entry<String, Map<String, Object>> e : fks.entrySet()) {
                List<Object> cols = list(e.getValue().get("columns"));
                List<Object> refCols = list(e.getValue().get("refColumns"));
                for (String[] pair : parts.get(e.getKey()).values()) {
                    cols.add(pair[0]);
                    refCols.add(pair[1]);
                }
            }
            return new ArrayList<>(fks.values());
        });
        facet(out, errors, "indexes", () -> {
            Map<String, Map<String, Object>> idx = new LinkedHashMap<>();
            Map<String, TreeMap<Integer, Object>> cols = new LinkedHashMap<>();
            try (ResultSet rs = md.getIndexInfo(scope.catalog, scope.schema, table, false, true)) {
                while (rs.next()) {
                    String name = rs.getString("INDEX_NAME");
                    if (name == null) {
                        continue;
                    }
                    boolean unique = !rs.getBoolean("NON_UNIQUE");
                    Map<String, Object> ix = idx.computeIfAbsent(name, k -> obj("name", k, "unique", unique, "columns", new ArrayList<>(), "filter", null));
                    String filter = optString(rs, "FILTER_CONDITION");
                    if (filter != null && !filter.isEmpty()) {
                        ix.put("filter", filter);
                    }
                    String order = optString(rs, "ASC_OR_DESC");
                    cols.computeIfAbsent(name, k -> new TreeMap<>()).put(rs.getInt("ORDINAL_POSITION"),
                            obj("name", rs.getString("COLUMN_NAME"), "order", order));
                }
            }
            for (Map.Entry<String, Map<String, Object>> e : idx.entrySet()) {
                list(e.getValue().get("columns")).addAll(cols.get(e.getKey()).values());
            }
            return new ArrayList<>(idx.values());
        });
        if (!errors.isEmpty()) {
            out.put("facetErrors", errors);
        }
        return out;
    }

    static String optString(ResultSet rs, String column) {
        try {
            return rs.getString(column);
        } catch (SQLException e) {
            return null;
        }
    }

    static Integer optInt(ResultSet rs, String column) {
        try {
            int v = rs.getInt(column);
            return rs.wasNull() ? null : Integer.valueOf(v);
        } catch (SQLException e) {
            return null;
        }
    }

    // ---- errors ----

    static Map<String, Object> classify(Object id, Throwable t) {
        if (t instanceof HelperException) {
            return error(id, ((HelperException) t).code, t.getMessage(), null, 0, false);
        }
        List<Throwable> chain = new ArrayList<>();
        for (Throwable x = t; x != null && !chain.contains(x) && chain.size() < 16; x = x.getCause()) {
            chain.add(x);
        }
        Throwable root = chain.get(chain.size() - 1);
        String state = null;
        int vendor = 0;
        for (Throwable x : chain) {
            if (x instanceof SQLException) {
                state = ((SQLException) x).getSQLState();
                vendor = ((SQLException) x).getErrorCode();
                break;
            }
        }
        boolean timeout = false;
        boolean network = false;
        boolean tls = false;
        for (Throwable x : chain) {
            String m = x.getMessage() == null ? "" : x.getMessage().toLowerCase(Locale.ROOT);
            tls |= x instanceof SSLException || x instanceof CertificateException;
            timeout |= x instanceof SocketTimeoutException || m.contains("timed out") || m.contains("timeout");
            network |= x instanceof ConnectException || x instanceof UnknownHostException || x instanceof NoRouteToHostException;
        }
        String code = "Internal";
        boolean retry = false;
        if (t instanceof FileNotFoundException || root instanceof ClassNotFoundException) {
            code = "DriverNotFound";
        } else if (t instanceof LinkageError || root instanceof LinkageError) {
            code = "DriverLoadFailed";
        } else if (t instanceof IllegalArgumentException) {
            code = "BadRequest";
        } else if (state != null && state.startsWith("28") || vendor == 1045 || vendor == 18456 || vendor == 1017) {
            code = "AuthFailed";
        } else if ("42501".equals(state) || vendor == 1142 || vendor == 1044 || vendor == 1031) {
            code = "PermissionDenied";
        } else if (tls) {
            code = "TlsFailed";
        } else if (timeout) {
            code = "ConnectTimeout";
            retry = true;
        } else if (network || state != null && state.startsWith("08")) {
            code = "NetworkUnreachable";
            retry = true;
        } else if (t instanceof SQLException) {
            code = "SqlError";
        }
        String message = t.getClass().getSimpleName() + ": " + t.getMessage();
        if (root != t) {
            message += " [root=" + root.getClass().getName() + "]";
        }
        return error(id, code, message, state, vendor, retry);
    }

    static Map<String, Object> error(Object id, String code, String message, String state, int vendor, boolean retry) {
        Map<String, Object> e = obj("code", code, "message", scrub(clip(message, MAX_MESSAGE)), "sqlState", state, "vendorCode", vendor, "retryable", retry);
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("v", PROTOCOL);
        r.put("id", id);
        r.put("ok", false);
        r.put("error", e);
        return r;
    }

    static boolean looksSecret(String key) {
        String k = key.toLowerCase(Locale.ROOT);
        return k.contains("pass") || k.contains("pwd") || k.contains("secret") || k.contains("token");
    }

    static void registerSecret(Object value) {
        // Short secrets would mangle ordinary words when replaced, so only 4+ characters are scrubbed literally.
        if (value != null && String.valueOf(value).length() >= 4) {
            secrets.add(String.valueOf(value));
        }
    }

    static String scrub(String s) {
        if (s == null) {
            return null;
        }
        for (String secret : secrets) {
            s = s.replace(secret, "***");
        }
        s = s.replaceAll("(?i)(password|pwd)=[^&;\\s]*", "$1=***");
        return s.replaceAll("(?i)(//[^/:@\\s]*):[^/@\\s]*@", "$1:***@");
    }

    static boolean emit(Map<String, Object> m) {
        return emitLine(toJson(m));
    }

    static synchronized boolean emitLine(String line) {
        if (responded) {
            return false;
        }
        responded = true;
        realOut.print(line);
        realOut.print('\n');
        realOut.flush();
        return true;
    }

    // ---- small helpers ----

    @SuppressWarnings("unchecked")
    static <T> T cast(Object o) {
        return (T) o;
    }

    @SuppressWarnings("unchecked")
    static List<Object> list(Object o) {
        return o == null ? new ArrayList<>() : (List<Object>) o;
    }

    static Map<String, Object> objOrEmpty(Object o) {
        return o instanceof Map ? cast(o) : new LinkedHashMap<>();
    }

    static Long num(Object o) {
        return o instanceof Number ? Long.valueOf(((Number) o).longValue()) : null;
    }

    static long clamp(Long value, long min, long max) {
        long v = value == null ? min : value;
        return Math.max(min, Math.min(max, v));
    }

    static String clip(String s, int max) {
        return s.length() <= max ? s : s.substring(0, max) + "...";
    }

    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i < kv.length; i += 2) {
            m.put((String) kv[i], kv[i + 1]);
        }
        return m;
    }

    static String toJson(Object o) {
        StringBuilder b = new StringBuilder();
        Json.write(b, o);
        return b.toString();
    }

    // ---- minimal JSON ----
    static final class Json {
        static final int MAX_DEPTH = 32;
        final String s;
        int i;
        int depth;

        Json(String s) {
            this.s = s;
        }

        Object parse() {
            Object v = value();
            ws();
            if (i != s.length()) {
                throw new IllegalArgumentException("trailing data at " + i);
            }
            return v;
        }

        void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) {
                i++;
            }
        }

        char peek() {
            if (i >= s.length()) {
                throw new IllegalArgumentException("unexpected end");
            }
            return s.charAt(i);
        }

        Object value() {
            ws();
            char c = peek();
            if (c == '{') {
                return object();
            }
            if (c == '[') {
                return array();
            }
            if (c == '"') {
                return str();
            }
            if (s.startsWith("true", i)) {
                i += 4;
                return Boolean.TRUE;
            }
            if (s.startsWith("false", i)) {
                i += 5;
                return Boolean.FALSE;
            }
            if (s.startsWith("null", i)) {
                i += 4;
                return null;
            }
            int start = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) {
                i++;
            }
            if (start == i) {
                throw new IllegalArgumentException("unexpected '" + c + "' at " + i);
            }
            String n = s.substring(start, i);
            return n.contains(".") || n.contains("e") || n.contains("E") ? (Object) Double.parseDouble(n) : (Object) Long.parseLong(n);
        }

        Map<String, Object> object() {
            if (++depth > MAX_DEPTH) {
                throw new IllegalArgumentException("nesting too deep");
            }
            i++;
            Map<String, Object> m = new LinkedHashMap<>();
            ws();
            if (peek() == '}') {
                i++;
                depth--;
                return m;
            }
            while (true) {
                ws();
                String k = str();
                ws();
                expect(':');
                m.put(k, value());
                ws();
                if (peek() == ',') {
                    i++;
                    continue;
                }
                expect('}');
                depth--;
                return m;
            }
        }

        List<Object> array() {
            if (++depth > MAX_DEPTH) {
                throw new IllegalArgumentException("nesting too deep");
            }
            i++;
            List<Object> l = new ArrayList<>();
            ws();
            if (peek() == ']') {
                i++;
                depth--;
                return l;
            }
            while (true) {
                l.add(value());
                ws();
                if (peek() == ',') {
                    i++;
                    continue;
                }
                expect(']');
                depth--;
                return l;
            }
        }

        void expect(char c) {
            if (i >= s.length() || s.charAt(i) != c) {
                throw new IllegalArgumentException("expected '" + c + "' at " + i);
            }
            i++;
        }

        String str() {
            expect('"');
            StringBuilder b = new StringBuilder();
            while (true) {
                if (i >= s.length()) {
                    throw new IllegalArgumentException("unterminated string");
                }
                char c = s.charAt(i++);
                if (c == '"') {
                    return b.toString();
                }
                if (c != '\\') {
                    b.append(c);
                    continue;
                }
                if (i >= s.length()) {
                    throw new IllegalArgumentException("unterminated escape");
                }
                char e = s.charAt(i++);
                switch (e) {
                    case 'n':
                        b.append('\n');
                        break;
                    case 't':
                        b.append('\t');
                        break;
                    case 'r':
                        b.append('\r');
                        break;
                    case 'b':
                        b.append('\b');
                        break;
                    case 'f':
                        b.append('\f');
                        break;
                    case 'u':
                        if (i + 4 > s.length()) {
                            throw new IllegalArgumentException("bad unicode escape");
                        }
                        b.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                        i += 4;
                        break;
                    case '"':
                    case '\\':
                    case '/':
                        b.append(e);
                        break;
                    default:
                        throw new IllegalArgumentException("bad escape \\" + e);
                }
            }
        }

        static void write(StringBuilder b, Object o) {
            if (o == null) {
                b.append("null");
            } else if (o instanceof String) {
                b.append('"');
                for (char c : ((String) o).toCharArray()) {
                    switch (c) {
                        case '"':
                            b.append("\\\"");
                            break;
                        case '\\':
                            b.append("\\\\");
                            break;
                        case '\n':
                            b.append("\\n");
                            break;
                        case '\r':
                            b.append("\\r");
                            break;
                        case '\t':
                            b.append("\\t");
                            break;
                        default:
                            if (c < 0x20) {
                                b.append(String.format("\\u%04x", (int) c));
                            } else {
                                b.append(c);
                            }
                    }
                }
                b.append('"');
            } else if (o instanceof Map) {
                b.append('{');
                boolean first = true;
                for (Map.Entry<?, ?> e : ((Map<?, ?>) o).entrySet()) {
                    if (!first) {
                        b.append(',');
                    }
                    first = false;
                    write(b, String.valueOf(e.getKey()));
                    b.append(':');
                    write(b, e.getValue());
                }
                b.append('}');
            } else if (o instanceof Collection) {
                b.append('[');
                boolean first = true;
                for (Object x : (Collection<?>) o) {
                    if (!first) {
                        b.append(',');
                    }
                    first = false;
                    write(b, x);
                }
                b.append(']');
            } else if (o instanceof Number || o instanceof Boolean) {
                b.append(o);
            } else {
                write(b, o.toString());
            }
        }
    }
}
