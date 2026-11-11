# JDBC helper P0 prototype (throwaway)

`Helper.java` is the spike of the managed JDBC helper described in `../JDBC_HELPER.md` (protocol v1, one JSON line in on stdin, one JSON line out on stdout, JDK 17+ and `java.*` only). Findings are in `../P0_SPIKE.md`. It is not shipped and not wired into the plugin.

Ops: `ping`, `loadDriver`, `dmProbe` (shows why `DriverManager` cannot be used), `testConnection`, `listSchemas`, `listTables` (offset/limit paging), `describeTable` (columns, PK, FK, indexes).

Run it (any JDK 17+; `-Duser.language=en` keeps driver messages in English):

```sh
H2=~/.m2/repository/com/h2database/h2/2.3.232/h2-2.3.232.jar
echo '{"v":1,"id":"1","op":"testConnection","deadlineMs":50000,
 "conn":{"url":"jdbc:h2:/tmp/demo","user":"sa","driver":{"jars":["'$H2'"],"class":"org.h2.Driver"}},
 "secrets":{"password":""},"args":{}}' | java -Duser.language=en Helper.java
```

Known limits of the prototype: timeouts are set per URL prefix (PostgreSQL, MySQL, MariaDB, SQL Server only), the URL allowlist/denylist and `maxBytes` cap are not implemented, error scrubbing skips passwords shorter than 4 characters, and `setReadOnly(true)` is only a driver hint (H2 ignores it).
