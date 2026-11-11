import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.sql.*;
import java.util.*;

/**
 * P0 spike of the JDBC metadata helper, protocol v1. One request: a JSON line on stdin.
 * One response: a JSON line on stdout. JDK only (java.*), JDK 17+, run as `java Helper.java`.
 * The password is read only from the stdin request (secrets.password), never argv/env.
 */
public class Helper {
    static final int V = 1;
    static final String HELPER_VERSION = "0.0-spike";
    static PrintStream realOut;
    static String password = "";
    static volatile boolean responded;

    public static void main(String[] args) throws Exception {
        realOut = new PrintStream(new FileOutputStream(FileDescriptor.out), false, StandardCharsets.UTF_8);
        System.setOut(System.err); // stray driver output must never corrupt the protocol channel
        String line = new String(System.in.readAllBytes(), StandardCharsets.UTF_8).trim();
        Map<String, Object> req;
        try {
            Object parsed = new Json(line).parse();
            if (!(parsed instanceof Map)) throw new IllegalArgumentException("request is not an object");
            req = cast(parsed);
        } catch (RuntimeException e) {
            emit(error(null, "BadRequest", "malformed JSON: " + e.getMessage(), null, 0, false));
            return;
        }
        Object id = req.get("id");
        try {
            if (!Objects.equals(num(req.get("v")), (long) V)) {
                emit(error(id, "ProtocolVersion", "helper speaks v" + V, null, 0, false));
                return;
            }
            Map<String, Object> secrets = req.get("secrets") == null ? Map.of() : cast(req.get("secrets"));
            password = secrets.get("password") == null ? "" : String.valueOf(secrets.get("password"));
            long deadline = req.get("deadlineMs") == null ? 50_000 : num(req.get("deadlineMs"));
            Thread wd = new Thread(() -> {
                try { Thread.sleep(deadline); } catch (InterruptedException e) { return; }
                emit(error(id, "DeadlineExceeded", "helper deadline " + deadline + " ms", null, 0, true));
                Runtime.getRuntime().halt(0);
            }, "watchdog");
            wd.setDaemon(true);
            wd.start();
            long t0 = System.nanoTime();
            Map<String, Object> result = dispatch(req);
            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("v", V); resp.put("id", id); resp.put("ok", true);
            resp.put("truncated", Boolean.TRUE.equals(result.remove("_truncated")));
            resp.put("next", result.remove("_next"));
            resp.put("result", result);
            resp.put("timingMs", Map.of("total", (System.nanoTime() - t0) / 1_000_000));
            emit(resp);
        } catch (Throwable t) {
            emit(classify(id, t));
        }
        System.exit(0);
    }

    static Map<String, Object> dispatch(Map<String, Object> req) throws Exception {
        String op = String.valueOf(req.get("op"));
        Map<String, Object> args = req.get("args") == null ? Map.of() : cast(req.get("args"));
        switch (op) {
            case "ping": return ping();
            case "loadDriver": {
                Driver d = loadDriver(Helper.<Map<String, Object>>cast(Helper.<Map<String, Object>>cast(req.get("conn")).get("driver")));
                return obj("driverClass", d.getClass().getName(), "major", d.getMajorVersion(), "minor", d.getMinorVersion(),
                        "loader", String.valueOf(d.getClass().getClassLoader().getClass().getSimpleName()));
            }
            case "dmProbe": return dmProbe(cast(req.get("conn")));
            default: break;
        }
        try (Connection c = connect(cast(req.get("conn")))) {
            DatabaseMetaData md = c.getMetaData();
            switch (op) {
                case "testConnection":
                    return obj("productName", md.getDatabaseProductName(), "productVersion", md.getDatabaseProductVersion(),
                            "driverName", md.getDriverName(), "driverVersion", md.getDriverVersion(),
                            "catalog", c.getCatalog(), "schema", schemaOf(c), "readOnly", c.isReadOnly());
                case "listSchemas": return listSchemas(md, c);
                case "listTables": return listTables(md, args);
                case "describeTable": return describeTable(md, args);
                default: throw new IllegalArgumentException("unknown op " + op);
            }
        }
    }

    static Map<String, Object> ping() {
        boolean jdk = false;
        try { Class.forName("javax.tools.ToolProvider"); jdk = javax.tools.ToolProvider.getSystemJavaCompiler() != null; } catch (Throwable ignored) { }
        return obj("helperVersion", HELPER_VERSION, "java", obj("version", System.getProperty("java.version"),
                "vendor", System.getProperty("java.vendor"), "jdk", jdk), "os", System.getProperty("os.name"));
    }

    static Driver loadDriver(Map<String, Object> spec) throws Exception {
        List<URL> urls = new ArrayList<>();
        for (Object j : list(spec.get("jars"))) {
            Path p = Paths.get(String.valueOf(j));
            if (!Files.isRegularFile(p)) throw new FileNotFoundException(p.toString());
            urls.add(p.toUri().toURL());
        }
        URLClassLoader cl = new URLClassLoader(urls.toArray(new URL[0]), Helper.class.getClassLoader());
        Class<?> k = Class.forName(String.valueOf(spec.get("class")), true, cl);
        return (Driver) k.getDeclaredConstructor().newInstance();
    }

    static Connection connect(Map<String, Object> conn) throws Exception {
        Driver d = loadDriver(cast(conn.get("driver")));
        String url = String.valueOf(conn.get("url"));
        Properties p = new Properties();
        if (conn.get("user") != null) p.setProperty("user", String.valueOf(conn.get("user")));
        if (!password.isEmpty()) p.setProperty("password", password);
        // connectTimeout units differ: seconds for pgjdbc and mssql (loginTimeout), milliseconds for MySQL/MariaDB.
        if (url.startsWith("jdbc:postgresql:")) { p.setProperty("connectTimeout", "5"); p.setProperty("loginTimeout", "5"); }
        else if (url.startsWith("jdbc:mysql:") || url.startsWith("jdbc:mariadb:")) p.setProperty("connectTimeout", "5000");
        else if (url.startsWith("jdbc:sqlserver:")) p.setProperty("loginTimeout", "5");
        if (conn.get("props") != null) for (Map.Entry<String, Object> e : Helper.<Map<String, Object>>cast(conn.get("props")).entrySet()) p.setProperty(e.getKey(), String.valueOf(e.getValue()));
        DriverManager.setLoginTimeout(5);
        Connection c = d.connect(url, p);
        if (c == null) throw new IllegalStateException("UrlUnsupported");
        try { c.setReadOnly(true); } catch (SQLException ignored) { }
        return c;
    }

    // Documents the DriverManager pitfall: a driver loaded by a private URLClassLoader is not visible to DriverManager.
    static Map<String, Object> dmProbe(Map<String, Object> conn) throws Exception {
        loadDriver(cast(conn.get("driver")));
        List<String> registered = new ArrayList<>();
        for (Enumeration<Driver> e = DriverManager.getDrivers(); e.hasMoreElements(); ) registered.add(e.nextElement().getClass().getName());
        String viaDm;
        try (Connection c = DriverManager.getConnection(String.valueOf(conn.get("url")), String.valueOf(conn.get("user")), password)) {
            viaDm = "connected";
        } catch (SQLException e) { viaDm = "SQLException: " + e.getMessage(); }
        return obj("visibleToDriverManager", registered, "getConnectionViaDriverManager", viaDm);
    }

    static String schemaOf(Connection c) { try { return c.getSchema(); } catch (Throwable t) { return null; } }

    static Map<String, Object> listSchemas(DatabaseMetaData md, Connection c) throws SQLException {
        List<Object> cats = new ArrayList<>(), schemas = new ArrayList<>();
        try (ResultSet rs = md.getCatalogs()) { while (rs.next()) cats.add(rs.getString(1)); }
        try (ResultSet rs = md.getSchemas()) { while (rs.next()) schemas.add(obj("catalog", rs.getString("TABLE_CATALOG"), "name", rs.getString("TABLE_SCHEM"))); }
        return obj("catalogs", cats, "schemas", schemas, "current", obj("catalog", c.getCatalog(), "schema", schemaOf(c)));
    }

    static Map<String, Object> listTables(DatabaseMetaData md, Map<String, Object> a) throws SQLException {
        String schema = (String) a.get("schema");
        String pattern = a.get("pattern") == null ? "%" : String.valueOf(a.get("pattern"));
        int offset = a.get("offset") == null ? 0 : num(a.get("offset")).intValue();
        int limit = a.get("limit") == null ? 200 : num(a.get("limit")).intValue();
        List<Object> out = new ArrayList<>();
        int seen = 0; boolean more = false;
        try (ResultSet rs = md.getTables(null, schema, pattern, new String[]{"TABLE", "VIEW"})) {
            while (rs.next()) {
                if (seen++ < offset) continue;
                if (out.size() >= limit) { more = true; break; }
                out.add(obj("catalog", rs.getString("TABLE_CAT"), "schema", rs.getString("TABLE_SCHEM"), "name", rs.getString("TABLE_NAME"),
                        "type", rs.getString("TABLE_TYPE"), "remarks", rs.getString("REMARKS")));
            }
        }
        Map<String, Object> r = obj("tables", out);
        r.put("_truncated", more);
        r.put("_next", more ? (Object) (offset + out.size()) : null);
        return r;
    }

    static Map<String, Object> describeTable(DatabaseMetaData md, Map<String, Object> a) throws SQLException {
        String schema = (String) a.get("schema"), table = String.valueOf(a.get("table"));
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("table", table);
        r.put("columns", facet(() -> {
            List<Object> cols = new ArrayList<>();
            try (ResultSet rs = md.getColumns(null, schema, table, "%")) {
                while (rs.next()) cols.add(obj("name", rs.getString("COLUMN_NAME"), "typeName", rs.getString("TYPE_NAME"), "dataType", rs.getInt("DATA_TYPE"),
                        "size", rs.getInt("COLUMN_SIZE"), "scale", rs.getInt("DECIMAL_DIGITS"), "nullable", rs.getInt("NULLABLE"),
                        "default", rs.getString("COLUMN_DEF"), "autoIncrement", rs.getString("IS_AUTOINCREMENT"),
                        "generated", rs.getString("IS_GENERATEDCOLUMN"), "remarks", rs.getString("REMARKS"), "position", rs.getInt("ORDINAL_POSITION")));
            }
            return cols;
        }));
        r.put("primaryKey", facet(() -> {
            TreeMap<Integer, String> cols = new TreeMap<>(); String name = null;
            try (ResultSet rs = md.getPrimaryKeys(null, schema, table)) { while (rs.next()) { cols.put(rs.getInt("KEY_SEQ"), rs.getString("COLUMN_NAME")); name = rs.getString("PK_NAME"); } }
            return obj("name", name, "columns", new ArrayList<>(cols.values()));
        }));
        r.put("foreignKeys", facet(() -> {
            Map<String, Map<String, Object>> fks = new LinkedHashMap<>();
            try (ResultSet rs = md.getImportedKeys(null, schema, table)) {
                while (rs.next()) {
                    Map<String, Object> fk = fks.computeIfAbsent(rs.getString("FK_NAME") + "|" + rs.getString("PKTABLE_NAME"), k -> obj("name", null, "refSchema", null, "refTable", null,
                            "columns", new ArrayList<>(), "refColumns", new ArrayList<>(), "updateRule", 0, "deleteRule", 0));
                    fk.put("name", rs.getString("FK_NAME")); fk.put("refSchema", rs.getString("PKTABLE_SCHEM")); fk.put("refTable", rs.getString("PKTABLE_NAME"));
                    fk.put("updateRule", rs.getInt("UPDATE_RULE")); fk.put("deleteRule", rs.getInt("DELETE_RULE"));
                    list(fk.get("columns")).add(rs.getString("FKCOLUMN_NAME")); list(fk.get("refColumns")).add(rs.getString("PKCOLUMN_NAME"));
                }
            }
            return new ArrayList<>(fks.values());
        }));
        r.put("indexes", facet(() -> {
            Map<String, Map<String, Object>> idx = new LinkedHashMap<>();
            try (ResultSet rs = md.getIndexInfo(null, schema, table, false, true)) {
                while (rs.next()) {
                    String n = rs.getString("INDEX_NAME"); if (n == null) continue; // tableIndexStatistic rows
                    boolean uniq = !rs.getBoolean("NON_UNIQUE");
                    Map<String, Object> ix = idx.computeIfAbsent(n, k -> obj("name", k, "unique", uniq, "columns", new ArrayList<>(), "filter", null));
                    list(ix.get("columns")).add(rs.getString("COLUMN_NAME")); ix.put("filter", rs.getString("FILTER_CONDITION"));
                }
            }
            return new ArrayList<>(idx.values());
        }));
        return r;
    }

    interface Facet { Object get() throws Exception; }

    static Object facet(Facet f) {
        try { return f.get(); } catch (Exception e) { return obj("error", scrub(String.valueOf(e.getMessage()))); }
    }

    static Map<String, Object> classify(Object id, Throwable t) {
        Throwable root = t; while (root.getCause() != null && root.getCause() != root) root = root.getCause();
        String state = t instanceof SQLException ? ((SQLException) t).getSQLState() : null;
        int vendor = t instanceof SQLException ? ((SQLException) t).getErrorCode() : 0;
        String code = "Internal"; boolean retry = false;
        if (t instanceof FileNotFoundException) code = "DriverNotFound";
        else if (t instanceof ClassNotFoundException) code = "DriverNotFound";
        else if (t instanceof NoClassDefFoundError || t instanceof LinkageError) code = "DriverLoadFailed";
        else if (t instanceof IllegalStateException && "UrlUnsupported".equals(t.getMessage())) code = "UrlUnsupported";
        else if (t instanceof IllegalArgumentException) code = "BadRequest";
        else if (root instanceof SocketTimeoutException) { code = "ConnectTimeout"; retry = true; }
        else if (root instanceof ConnectException || root instanceof UnknownHostException || root instanceof NoRouteToHostException) { code = "NetworkUnreachable"; retry = true; }
        else if (state != null && (state.equals("28P01") || state.startsWith("28") || vendor == 1045 || vendor == 28000)) code = "AuthFailed";
        else if (state != null && (state.equals("42501") || vendor == 1142 || vendor == 1044)) code = "PermissionDenied";
        else if (state != null && state.startsWith("08")) { code = "NetworkUnreachable"; retry = true; }
        else if (t instanceof SQLException) code = "SqlError";
        String msg = t.getClass().getSimpleName() + ": " + t.getMessage() + " [root=" + root.getClass().getName() + "]";
        return error(id, code, msg, state, vendor, retry);
    }

    static Map<String, Object> error(Object id, String code, String msg, String state, int vendor, boolean retry) {
        Map<String, Object> e = obj("code", code, "message", scrub(msg), "sqlState", state, "vendorCode", vendor, "retryable", retry);
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("v", V); r.put("id", id); r.put("ok", false); r.put("error", e);
        return r;
    }

    static String scrub(String s) {
        if (s == null) return null;
        if (password.length() >= 4) s = s.replace(password, "***"); // short passwords would mangle ordinary words: see P0_SPIKE.md
        return s.replaceAll("(?i)(password|pwd)=[^&;\\s]*", "$1=***");
    }

    static synchronized void emit(Map<String, Object> m) {
        if (responded) return;
        responded = true;
        StringBuilder sb = new StringBuilder();
        Json.write(sb, m);
        realOut.println(sb);
        realOut.flush();
    }

    // ---- helpers ----
    @SuppressWarnings("unchecked") static <T> T cast(Object o) { return (T) o; }
    @SuppressWarnings("unchecked") static List<Object> list(Object o) { return o == null ? new ArrayList<>() : (List<Object>) o; }
    static Long num(Object o) { return o instanceof Number ? ((Number) o).longValue() : null; }
    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i < kv.length; i += 2) m.put((String) kv[i], kv[i + 1]);
        return m;
    }

    // ---- minimal JSON ----
    static final class Json {
        final String s; int i;
        Json(String s) { this.s = s; }
        Object parse() { Object v = value(); ws(); if (i != s.length()) throw new IllegalArgumentException("trailing data at " + i); return v; }
        void ws() { while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++; }
        Object value() {
            ws();
            if (i >= s.length()) throw new IllegalArgumentException("unexpected end");
            char c = s.charAt(i);
            if (c == '{') {
                i++; Map<String, Object> m = new LinkedHashMap<>(); ws();
                if (s.charAt(i) == '}') { i++; return m; }
                while (true) { ws(); String k = str(); ws(); expect(':'); m.put(k, value()); ws();
                    if (s.charAt(i) == ',') { i++; continue; } expect('}'); return m; }
            }
            if (c == '[') {
                i++; List<Object> l = new ArrayList<>(); ws();
                if (s.charAt(i) == ']') { i++; return l; }
                while (true) { l.add(value()); ws(); if (s.charAt(i) == ',') { i++; continue; } expect(']'); return l; }
            }
            if (c == '"') return str();
            if (s.startsWith("true", i)) { i += 4; return Boolean.TRUE; }
            if (s.startsWith("false", i)) { i += 5; return Boolean.FALSE; }
            if (s.startsWith("null", i)) { i += 4; return null; }
            int st = i; while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            if (st == i) throw new IllegalArgumentException("unexpected '" + c + "' at " + i);
            String n = s.substring(st, i);
            return n.contains(".") || n.contains("e") || n.contains("E") ? (Object) Double.parseDouble(n) : (Object) Long.parseLong(n);
        }
        void expect(char c) { if (i >= s.length() || s.charAt(i) != c) throw new IllegalArgumentException("expected '" + c + "' at " + i); i++; }
        String str() {
            expect('"'); StringBuilder b = new StringBuilder();
            while (true) {
                if (i >= s.length()) throw new IllegalArgumentException("unterminated string");
                char c = s.charAt(i++);
                if (c == '"') return b.toString();
                if (c != '\\') { b.append(c); continue; }
                char e = s.charAt(i++);
                switch (e) {
                    case 'n': b.append('\n'); break; case 't': b.append('\t'); break; case 'r': b.append('\r'); break;
                    case 'b': b.append('\b'); break; case 'f': b.append('\f'); break;
                    case 'u': b.append((char) Integer.parseInt(s.substring(i, i + 4), 16)); i += 4; break;
                    default: b.append(e);
                }
            }
        }
        static void write(StringBuilder b, Object o) {
            if (o == null) b.append("null");
            else if (o instanceof String) { b.append('"'); for (char c : ((String) o).toCharArray()) {
                switch (c) { case '"': b.append("\\\""); break; case '\\': b.append("\\\\"); break; case '\n': b.append("\\n"); break;
                    case '\r': b.append("\\r"); break; case '\t': b.append("\\t"); break;
                    default: if (c < 0x20) b.append(String.format("\\u%04x", (int) c)); else b.append(c); } } b.append('"'); }
            else if (o instanceof Map) { b.append('{'); boolean f = true; for (Map.Entry<?, ?> e : ((Map<?, ?>) o).entrySet()) { if (!f) b.append(','); f = false; write(b, String.valueOf(e.getKey())); b.append(':'); write(b, e.getValue()); } b.append('}'); }
            else if (o instanceof Collection) { b.append('['); boolean f = true; for (Object x : (Collection<?>) o) { if (!f) b.append(','); f = false; write(b, x); } b.append(']'); }
            else if (o instanceof Number || o instanceof Boolean) b.append(o);
            else write(b, o.toString());
        }
    }
}
