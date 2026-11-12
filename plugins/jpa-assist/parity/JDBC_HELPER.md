# Managed JDBC helper: design for live database access (Wave 10, G080-G103)

Status: design only, 2026-11-11. Decision log source: `ROADMAP.md` line 16 (managed JDBC helper route chosen; optional `process:*` pre-approved; `net` and extra `fs` must be asked).
"ref" below is `.claude/skills/writing-openidea-plugins/reference.md` (line numbers of this worktree). "undocumented" means ref does not say; section 7 lists how to verify.

Corrections to the brief, found while reading:
- `docs/api-1.3-gap-reassessment-2026-11-08.md` does not exist in this worktree; the equivalent content is the delta table at ref 1976-2012. `docs/upstream-issues.md` has nothing on JDBC/process.
- The "Gradle cache precedent" is false: `plugins/spring-boot/plugin.toml:26` declares only `fs.read:~/.m2/repository` (optional). No plugin declares `fs.read:~/.gradle/...` (grep over `plugins/*/plugin.toml`).
- `database.sources` exists (ref 2001) and was not in the brief; see D2.

## 1. Host facts that constrain the design

| Fact | Source |
|---|---|
| Manifest grant is `process:<program>` (high risk); example `optional = ["process:kubectl"]`. The program is resolved to an absolute path, its content hash recorded, re-checked before every start; a replaced binary is refused until re-approved. Remote project: resolved on the server. | ref 305, 310-317, 1014 |
| Launch APIs: `Project::run_process(program,args,stdin)` and `host::run_command(cmd,project,stdin)` return `{code,stdout,stderr}`; 4 MiB output; "killed when the call is cancelled or times out"; run in the project root of a **trusted** project. `host::start(cmd,project)` returns `Child` (`poll(max_bytes)`, `write`, `close_stdin`, `wait(timeout_ms)`, `kill`), <= 8 handles, dropping kills. | ref 473-475 |
| Env: add-only (a key already in the prepared env, including login-shell `PATH`, is an error), <= 64 entries, values <= 4 KiB. The pinned file decides what runs (so `java` comes from the login-shell `PATH`, not `JAVA_HOME`). | ref 497-501 |
| Unpolled Child output keeps the last 1 MiB per stream; one read <= 1 MiB. Children end when the plugin is disabled, unloaded, **reclaimed**, stopped, or the window closes. | ref 503-505 |
| Idle instance reclaimed after 5 min (`deactivate` runs); keep nothing in memory you cannot rebuild. `on-schedule` does not keep it alive. | ref 280-284, 2009-2012 |
| Budgets: commands 5 s wall (60 s while a `host::begin_task` is open; needs `tasks.begin`), 256 MiB memory, calls serialized per instance so a long command delays providers. `Task` has `is_cancelled`. | ref 456, 1401-1405; SKILL.md 172 |
| `host::fs::plugin_dir()` (API 1.3): private owner-only dir, no `fs.*` grant, survives reclaim/restart, "A returned path works ... as an argument to an approved process". | ref 337-352 |
| `host::storage` 64 MiB, plaintext key-value (capability `storage`, already required). | ref 296, SKILL.md 171 |
| `{{secret:name}}` works only in `host::fetch` request headers; secrets are never readable by the plugin. | ref 803-806, SKILL.md 179 |
| `host::input(prompt,value)` (`ui.input`): one-line box, `None` on dismiss, command/event of a window only. Masking: undocumented. | ref 464 |
| Web-frame PTY and `open_terminal` are interactive terminals bound to the frame/tab; PTY output acks one chunk at a time, 256 Ki UTF-16 cap, killed on unmount. Not a data channel; not used. | ref 1106-1138, 479 |
| Owned runs/tests are "separate from disposable Child". | ref 1999 |
| Database sources: capability `database.sources` allows plugin-owned source create/list/remove/credentials/connect through native UI; "No global existing-source enumeration, schema/query/table/model API". Methods per `docs/laravel-parity-acceptance.md:481`: `upsert_data_source, remove_data_source, data_sources, set_data_source_credential, connect_data_source`. | ref 2001 |
| `net:<host>` gates `host::fetch` only. Whether a spawned JVM's own sockets are restricted: undocumented. The Wasm sandbox has "no network" (ref 1395-1397) but that is the plugin store, not children. | ref 297-307 |

Consequences:
1. **A helper cannot survive reclaim.** Child ends with the instance, and reclaim happens after 5 idle minutes with no keepalive. Design is therefore **one-shot request/response, state in `plugin_dir`/`storage`**. A warm JVM held in instance memory between commands is a pure optimization that must tolerate loss (D9: not in v1).
2. **Hard ceiling of 60 s per operation** (5 s without a task). Large schemas must be paged across calls; every op runs inside a cancellable `begin_task`.
3. Calls are serialized per instance: one request in flight; no concurrency inside the helper protocol.
4. **`process:java` is a strong grant**: the JVM runs arbitrary code as the user and can read any file the user can. fs/net manifest capabilities do not confine it (undocumented otherwise). This is why options in section 3 need no `fs.read`: the JVM does the file access. The user must understand they approve that, not "just a database client".
5. Undocumented, must be spiked (section 7, P0): whether `Child.poll`/`wait` block inside the call budget; size cap of `stdin`/`write`; args count/length for `run_process` (PTY has 256 x 4096 B, ref 1128-1130); whether stdin content is logged by the host; masking of `ui.input`; whether `process:` accepts an absolute path; cwd of `host::start`.

Local measurement (scratch, not in repo): trivial `java Helper.java` (JDK 26, source launch, read one stdin line, print one JSON line) takes 0.20-0.52 s wall. A real metadata helper (~600 lines) will compile longer; assume 1-2 s, verify.

## 2. Helper architecture

### 2.1 Form: single-file source launch, not a jar
- Ship `plugins/jpa-assist/helper/Helper.java` (single file, JDK only, no dependencies, Java 17 language level). Embed it with `include_str!` in the Wasm; at first use (and when its hash differs) the plugin writes it to `plugin_dir()/jdbc/Helper.java` (owner-only, no grant) and runs `java [flags] <plugin_dir>/jdbc/Helper.java` (JEP 330). Requires a **JDK** (module `jdk.compiler`), 17+ recommended minimum (JEP 330 itself is 11+). A JRE-only or jlink-stripped runtime fails to launch: map to `JdkRequired`.
- Prebuilt jar: technically feasible locally (a JDK is installed here) and could be shipped as `include_bytes!` + `fs::write_bytes` into `plugin_dir`, but the repo CI/release flow (cargo + release-please, `CLAUDE.md`) has no Java build step, a committed binary jar is an audit/supply-chain burden, and whether the package accepts extra resources is undocumented. Verdict: source launch now; jar only if startup measures badly (P6 optional).
- Flags: `-Dfile.encoding=UTF-8 -Dstdout.encoding=UTF-8 -Xmx256m -Djava.awt.headless=true -XX:TieredStopAtLevel=1`. The helper rebinds `System.out` to stderr at startup and writes protocol only to the saved original stdout (JDBC drivers and the launcher print stray text).
- No JSON library exists in the JDK: Helper.java carries a ~120-line minimal JSON reader/writer (objects, arrays, strings, numbers, booleans, null). Acceptable: the schema is flat and tested by the same fixtures.

### 2.2 Driver loading (inside the helper)
Do not rely on `DriverManager` with source launch (classloader visibility of drivers loaded by a custom `URLClassLoader` is a known trap). Helper receives `driver: {jars:[...], class:"org.postgresql.Driver"}`, builds a `URLClassLoader`, instantiates the class, calls `Driver.connect(url, props)`. Verify in P0 that this works under source launch for pgjdbc, Connector/J and MariaDB.

### 2.3 Protocol: JSON lines, version 1
One request per process (stdin, single line, then EOF); one response line on stdout; exit code 0 if a response was written (errors are in-band), nonzero only for launcher failures.

Request:
```json
{"v":1,"id":"<uuid>","op":"listTables","deadlineMs":50000,"maxBytes":3000000,
 "conn":{"url":"jdbc:postgresql://h:5432/db","user":"u","driver":{"jars":["/abs/postgresql-42.7.4.jar"],"class":"org.postgresql.Driver"},"props":{"sslmode":"require"}},
 "secrets":{"password":"..."},
 "args":{"schema":"public","types":["TABLE","VIEW"],"pattern":null,"offset":0,"limit":200}}
```
Response (exactly one line):
```json
{"v":1,"id":"<uuid>","ok":true,"truncated":false,"next":null,"result":{...},"timingMs":{"connect":120,"query":80}}
{"v":1,"id":"<uuid>","ok":false,"error":{"code":"AuthFailed","message":"...redacted...","sqlState":"28P01","vendorCode":0,"retryable":false}}
```
Rules:
- `v` mismatch -> `ProtocolVersion` (helper replies with its own `v`; plugin and helper ship in one package, so mismatch only signals a stale `plugin_dir` copy; the plugin rewrites the file by hash).
- Password only in `secrets.password` of the stdin line; never argv, never env. The helper never echoes `secrets`/`conn.url` userinfo; it scrubs the password string and `password=` URL params from every error message and stack trace before output.
- `deadlineMs`: helper arms a daemon watchdog; at the deadline it calls `Statement.cancel()`/closes the connection, emits `DeadlineExceeded`, exits. Also sets `DriverManager`-style login timeout and `socketTimeout` equivalents via props (PG `connectTimeout`, `socketTimeout`; MySQL `connectTimeout`, `socketTimeout`).
- Cancellation: there is no in-band cancel in a one-shot design. The plugin cancels by killing the process (`run_process` is killed on cancel per ref 473; or `Child::kill`). Result is `Cancelled` on the plugin side; nothing to clean (read-only metadata).
- Output bound: helper stops adding items at `maxBytes` (default 3 MB, under the 4 MiB `run_process` cap), sets `truncated:true` and `next` cursor/offset. Result paging, never a bigger buffer.
- Phase 1 uses `run_command`/`run_process` with stdin. `Child` is only needed for progress lines (`{"progress":...}` on stdout before the final line, last-line-wins) in P4 for big schemas; adopting it changes nothing in the protocol.

### 2.4 Operations (v1 is metadata-only, read-only)
The helper stays dumb: `java.sql.DatabaseMetaData` only, `connection.setReadOnly(true)`, no SQL text accepted from the plugin. All dialect knowledge lives in Rust, testable with recorded JSON and no JVM.

| op | args | result |
|---|---|---|
| `ping` | none | `{helperVersion, java:{version,vendor,jdk:bool}, os}`; no DB; used for JVM/JDK probe and protocol check |
| `listDrivers` | `roots:[...]`, `coords:[ "org.postgresql:postgresql:42.7.4" ]` | drivers found (section 3 B) with jar path, class, version from `Driver.getMajorVersion` |
| `testConnection` | conn | `{productName, productVersion, driverName, driverVersion, catalog, schema, readOnly}` (this also answers `getVersion`) |
| `listSchemas` | conn | `{catalogs:[...], schemas:[{catalog,name,isDefault}], current:{catalog,schema}}` |
| `listTables` | conn, schema, types, pattern, offset, limit | `[{catalog,schema,name,type,remarks}]` (TABLE, VIEW, MATERIALIZED VIEW where reported, no SYSTEM*) |
| `describeTables` | conn, schema, tables[<=25] | per table: `columns` (raw `COLUMN_NAME, TYPE_NAME, DATA_TYPE, COLUMN_SIZE, DECIMAL_DIGITS, NULLABLE, COLUMN_DEF, IS_AUTOINCREMENT, IS_GENERATEDCOLUMN, REMARKS, ORDINAL_POSITION`), `primaryKey` (`getPrimaryKeys`, KEY_SEQ order, PK name), `foreignKeys` (`getImportedKeys` grouped by FK_NAME, KEY_SEQ, UPDATE/DELETE rule, PK table schema), `indexes` (`getIndexInfo`: name, NON_UNIQUE, ordered columns, ASC_OR_DESC, FILTER_CONDITION), per-facet `error` object when one call fails (permission) so the Rust side yields `Fact::Unknown` |
| `describeExtras` (P5, G092/G100/G103) | conn, schema, tables | vendor read-only catalog queries hard-coded in the helper (PG `pg_enum`, MySQL `INFORMATION_SCHEMA.COLUMNS.COLUMN_TYPE`), allowlisted, no user SQL |

`listForeignKeys`/`listIndexes` are folded into `describeTables` (separate ops would cost an extra JVM launch each, 1-2 s, for no gain). One launch can also carry `"batch":[op,...]` later if measurements warrant.

### 2.5 Error taxonomy (`error.code`)
Plugin-detected before/around launch: `JvmMissing` (spawn failure, code 127, or `Error::Denied("process:java")`), `JdkRequired` (launcher message about missing compiler module / source-launch unsupported), `JvmTooOld` (`ping` java.version < 17), `HelperProtocol` (empty/non-JSON stdout, nonzero exit with stderr tail <= 2 KiB kept), `Cancelled`, `HostBudget` (call killed at 5/60 s with no response).
Helper-reported: `ProtocolVersion`, `BadRequest`, `DriverNotFound` (no jar/class), `DriverLoadFailed` (`ClassNotFound`, `NoClassDefFoundError`: transitive dep missing), `UrlUnsupported` (driver rejects URL or URL not in allowlist), `UrlRejected` (denied property, section 6), `ConnectTimeout`, `NetworkUnreachable` (SQLState `08xxx`, `ConnectException`), `TlsFailed` (SSL handshake/trust), `AuthFailed` (PG `28P01`/`28000`, MySQL vendor 1045), `PermissionDenied` (`42501`, MySQL 1142/1044), `SchemaNotFound`, `ResultTooLarge`/`truncated`, `DeadlineExceeded`, `Internal`. Each carries `sqlState`, `vendorCode`, a redacted `message`, `retryable`. UI maps each code to one actionable sentence (e.g. `JvmMissing`: "Install a JDK 17+ and put `java` on PATH, then approve process:java").

## 3. JDBC driver acquisition: options and exact capability cost

All options assume the user already granted `process:java`. "Plugin-side" means the Wasm plugin itself touches files.

| Option | How | Capabilities beyond `process:java` | Notes |
|---|---|---|---|
| A. User-provided jar paths | Setting `jpa-assist.jdbc.driverJars` (string list, absolute paths) passed in `conn.driver.jars`; the JVM opens them | **None.** Plugin never reads the path. (If the plugin wanted to `stat` the path itself it would need `fs.read:<path>`, and a manifest cannot name arbitrary user paths, so do not.) | Simplest and most robust; user picks the exact driver. Errors from the helper (`DriverNotFound`). |
| B1. Helper-side cache discovery | `listDrivers` walks `~/.m2/repository` and `~/.gradle/caches/modules-2/files-2.1` for known coordinates (below), preferring the version the project's own build declares (plugin reads `pom.xml`/`build.gradle*` through existing `project.read`) | **None** (JVM reads). | Layouts: Maven `~/.m2/repository/<g/path>/<a>/<v>/<a>-<v>.jar`; Gradle `files-2.1/<g>/<a>/<v>/<sha1>/<a>-<v>.jar`. Transitive jars (e.g. pgjdbc `checker-qual`, MySQL protobuf optional) must be verified per driver: P0. Gradle may relocate (`GRADLE_USER_HOME`): also accept it as a setting. |
| B2. Plugin-side cache discovery | plugin walks caches via `Project`/`host::fs` | `fs.read:~/.m2/repository` (exists on spring-boot, `plugin.toml:26`, optional) and a new `fs.read:~/.gradle/caches/modules-2/files-2.1` (does not exist anywhere yet) | Gives nothing B1 lacks; costs two high-risk grants. Not recommended. |
| C. Download on demand | fetch jar from Maven Central into `plugin_dir` | `net:repo.maven.apache.org` (or `repo1.maven.org`) + `net` approval; binary body handling through `host::fetch` (8 MiB cap, ref 794) is undocumented; or the JVM downloads itself, bypassing `net` gating (undocumented), which would be a policy dodge: do not | Needs user approval of `net`. Phase 7 at earliest. Must verify SHA-1/SHA-256 against Central. |
| D. Bundle in package | `include_bytes!` + write to `plugin_dir` | None; package/Wasm size limit undocumented | Licences (verify before any use): pgjdbc BSD-2-Clause (~1 MB); MariaDB Connector/J LGPL-2.1; MySQL Connector/J GPLv2 with FOSS exception (do not bundle into an MIT plugin); H2 MPL/EPL; xerial sqlite Apache-2.0. Version staleness and CVE upkeep fall on us. Not recommended in v1. |

Recommendation: A + B1, bundle nothing, no fs/net manifest additions. Known coordinates seed list: `org.postgresql:postgresql` (`org.postgresql.Driver`), `com.mysql:mysql-connector-j` and legacy `mysql:mysql-connector-java` (`com.mysql.cj.jdbc.Driver`), `org.mariadb.jdbc:mariadb-java-client` (`org.mariadb.jdbc.Driver`). The project's build files tell which one the app uses; the helper picks it by coordinate match; if several versions exist, the highest unless the build pins one.

## 4. Credentials

Sources, in order: (1) Spring config via existing `project.read` and the parser in `crates/spring-text/src/config.rs` (keys `spring.datasource.url|username|password|driver-class-name`, profile files `application-<profile>.(properties|yml)`; discovery G081 needs no helper); (2) interactive `host::input` for anything missing or `${ENV}`-placeholdered (the Wasm plugin has no environment, ref 1395, so env placeholders cannot be resolved by the plugin).

Rules:
- Transport: the password travels only in the stdin request line, `secrets.password`. Never in args (visible in process listings) and not in env (visible to same-user inspection; `run_command` env is add-only, 4 KiB values, ref 497-501, but argv/env leakage is the avoidable risk). Whether the host logs stdin: undocumented, verify in P0; if it does, switch to a one-time mode where the plugin writes nothing and the helper prompts nothing (no alternative exists: then escalate upstream).
- At rest: plugin may store in `storage`/`plugin_dir`: connection profile id, display name, URL **with userinfo and password-like query parameters stripped**, username, driver coordinate/jar path, chosen schemas, filters, helper-independent snapshot cache (section 5). It must never store the password (storage is plaintext). `{{secret:}}` cannot help (fetch headers only). `database.sources` + `set_data_source_credential` would put a password in the keychain for native use, but read-back to the plugin is undocumented (secrets are write-only, ref 1997), so it cannot feed the helper without a prompt: see D2.
- In memory: the password is re-read from project config at call time when it is a literal; a prompted password is kept in instance memory only, so the 5-minute reclaim bounds its life; user can also choose "ask every time".
- Display: never render the password; masked placeholder in all views. `ui.input` masking undocumented; if it echoes, show a warning-free prompt but prefer config-sourced passwords and say so in UX copy.
- Untrusted repo: a cloned project's `application.yml` can point the helper at any host. Require explicit per-connection confirmation showing host, port, database, user; never auto-connect on open/save; only in trusted projects (platform rule for process, ref 473).

## 5. Mapping to `persistence-core::schema` (`crates/persistence-core/src/schema.rs`)

Pipeline: helper raw JSON -> pure Rust `introspect` module (new, in `persistence-core`, serde types for the protocol plus `fn to_snapshot(dialect, raw) -> SchemaSnapshot`) -> `SchemaSnapshot::normalize()/validate()` -> existing diff. The reverse-engineering logic (G087-G103) must consume `SchemaSnapshot` regardless of origin, so the DDL-text fallback (`ROADMAP.md` line 59) shares it.

Field rules (`schema.rs` types: `Table`, `Column`, `Fact`, `LogicalType`, `SqlType{logical,raw}`, `ForeignKey`, `UniqueConstraint`, `Index`, `Provenance`):
- `Table.name`: bare name when the schema is the connection default or the only schema selected, else `schema.table` (must match what the entity-side snapshot builder emits from `@Table(schema=)`: verify the builder before P2; `by_name` lowercases and rejects duplicates, schema.rs `by_name`). `provenance: Literal`.
- `sql_type`: `Known{logical, raw}` with `raw` = `TYPE_NAME` plus size (`varchar(255)`); `Unknown("unmapped SQL type 'jsonb' (java.sql.Types 1111)")` otherwise. `length` for Char/Varchar from `COLUMN_SIZE`; `precision/scale` for Decimal; others `None`.
- `nullable`: NULLABLE 0 -> `Known(false)`, 1 -> `Known(true)`, 2 (unknown) -> `Unknown`.
- `default`: `Known(Some(text))` / `Known(None)` from `COLUMN_DEF`; a per-facet helper error -> `Unknown(reason)`.
- `generated`: `IS_AUTOINCREMENT=YES` or `IS_GENERATEDCOLUMN=YES` -> `Known(true)`, `NO` -> `Known(false)`, empty -> `Unknown`.
- `primary_key`/`foreign_keys`/`indexes`: `Known` or `Unknown(reason)` per helper facet error. FK `ref_table` follows the table-name rule; `ref_columns` `Known` from imported keys.
- Unique: a non-PK index with `NON_UNIQUE=false` becomes a `UniqueConstraint` and is **not** repeated in `indexes` (metadata cannot tell a unique constraint from a unique index; double-reporting would make the diff emit both). Confirm how the entity side models `@Index(unique=true)` vs `@UniqueConstraint` before P2 (open item O1). Expression/partial indexes (null column names, `FILTER_CONDITION`) -> table `indexes = Unknown("expression or partial index")`.
- Bounds: `MAX_TABLES = 500`, `MAX_COLUMNS = 500` (schema.rs 11-12). Above that `validate()` fails (`TooManyTables`); UI must require a schema/pattern filter, and a filtered snapshot sets `complete: false` (absence unproven, schema.rs `complete`). Views are separate (G095): not put in `tables` unless the user picks them, with `Unknown` PK until an id is chosen.
- Renames never inferred (schema.rs header): unchanged.

Dialect physical -> `LogicalType` (first targets; MySQL rows also serve MariaDB). Anything not listed is `Unknown`.

| PostgreSQL `TYPE_NAME` | Logical | | MySQL `TYPE_NAME` | Logical |
|---|---|---|---|---|
| int2, smallserial | Smallint | | TINYINT | Tinyint |
| int4, serial | Integer | | SMALLINT, MEDIUMINT, INT | Smallint, Integer, Integer |
| int8, bigserial | Bigint | | BIGINT | Bigint |
| bool | Boolean | | BIT(1) (Connector/J default for tinyint(1)) | Boolean |
| float4 / float8 | Real / Double | | FLOAT / DOUBLE | Real / Double |
| numeric | Decimal | | DECIMAL | Decimal |
| bpchar / varchar | Char / Varchar | | CHAR / VARCHAR | Char / Varchar |
| text | Text | | TINYTEXT, TEXT, MEDIUMTEXT, LONGTEXT | Text |
| date / time | Date / Time | | DATE / TIME | Date / Time |
| timestamp / timestamptz | Timestamp / TimestampTz | | DATETIME, TIMESTAMP | Timestamp |
| uuid | Uuid | | BINARY, VARBINARY, *BLOB | Binary |
| bytea | Binary | | UNSIGNED widening: TINYINT->Smallint, SMALLINT->Integer, INT->Bigint, BIGINT->Decimal(20,0), `raw` keeps `UNSIGNED` | |
| Unknown: timetz, json, jsonb, xml, inet/cidr, arrays (`_x`), interval, money, hstore, geometry, user enums/domains, ranges | | | Unknown: ENUM, SET, JSON, YEAR, GEOMETRY family, BIT(n>1) | |

Unknown types preserve `raw` in the reason so G093 can offer "deferred mapping" (G094) instead of dropping the column. Enum values (G092) and vendor specifics (G103) need `describeExtras` (P5). The table is a proposal to be pinned by golden fixtures from real databases (section 7), not an asserted vendor fact.

## 6. Security and failure modes

| Case | Behavior |
|---|---|
| JVM missing / not on login-shell PATH | `JvmMissing`; guidance text; feature stays inert, rest of plugin unaffected (grant is `optional`, handle `Error::Denied`). Document that the pinned `java` is whatever the login shell finds, and a JDK switch that changes the binary hash needs re-approval (ref 310-317). On macOS `/usr/bin/java` is a launcher stub; behavior with it: P0. |
| JRE only / JDK < 17 | `JdkRequired` / `JvmTooOld` from `ping`. |
| Driver missing / transitive jar missing | `DriverNotFound` / `DriverLoadFailed` with the coordinates searched and the setting to change. |
| Wrong credentials | `AuthFailed`; clears the in-memory password; never retries automatically (lockout risk). |
| TLS | pass-through URL params only; no auto-trust; `TlsFailed` states "server certificate not trusted"; user must supply `sslmode=verify-full` + root cert path or accept plain. Never silently downgrade. |
| Timeout / unreachable | helper deadline -> `ConnectTimeout`/`DeadlineExceeded`; host kill at budget -> `HostBudget`. 60 s cap (5 s without a task). |
| Huge schema | list with `limit`, describe <= 25 tables per call, `next` cursor, filter required above 500 tables, `truncated` flag; per-launch 3 MB cap. |
| Concurrent requests | impossible inside one instance (serialized); plugin keeps a single-flight guard per profile so a second click reports "already running"; two windows = two instances, each its own process (no shared mutable state, cache writes are atomic whole-file). |
| Hostile URLs (repo-supplied) | allowlist prefixes `jdbc:postgresql:`, `jdbc:mysql:`, `jdbc:mariadb:` in v1; reject URL/properties that load classes or files: PG `socketFactory`, `sslfactory`, `sslhostnameverifier`, `sslpasswordcallback`, `authenticationPluginClassName`, `loggerFile/loggerLevel`; MySQL `allowLoadLocalInfile`, `allowUrlInLocalInfile`, `autoDeserialize`, `queryInterceptors`, `statementInterceptors`, `exceptionInterceptors`, `connectionLifecycleInterceptors`, `socketFactory`, `propertiesTransform`; force `autoDeserialize=false`, `allowLoadLocalInfile=false`. No `jdbc:h2:` (INIT scripts, file access) in v1. Superseded 2026-11-12: per-vendor key allowlists plus a shared denylist, verified against vendor docs in `JDBC_URL_RULES.md`; Oracle and Db2 are disabled until verified; H2 is allowed with the caveat recorded there. |
| Data safety | read-only connection, metadata API only, no SQL accepted; helper never writes files; stderr tail logged only after password scrub. |
| Remote projects | `java` resolves and runs on the server (ref 1386); DB must be reachable from there. |
| Helper tampering | file lives in owner-only `plugin_dir`; plugin compares hash on every launch and rewrites if different. The pin covers `java`, not the helper source, so this check is ours. |

## 7. Phased plan (agent-sized increments) and acceptance

Docker is installed (`docker --version` -> 29.4.0); nothing was started. Local JDK 26 is present for spikes.

| Phase | Scope | Acceptance |
|---|---|---|
| P0 Spike (throwaway plugin or a scratch branch, not jpa-assist) | Verify every "undocumented" in section 1: `process:java` manifest string and hash pin (incl. macOS stub); `run_process` with stdin ~2 KiB and 3 MB stdout; `begin_task` raising to 60 s; arg path inside `plugin_dir`; cancel kills JVM; whether stdin/args are logged anywhere (Output/Problems/logs); `ui.input` masking; `Child.poll/wait` blocking semantics; process network egress allowed; JEP 330 + `URLClassLoader` driver load for pgjdbc, Connector/J, MariaDB. | One receipt file per fact under `audit/jpa-native/jdbc-spike/`; section 1 updated from "undocumented" to measured. Gate for everything below. |
| P1 Helper + protocol | `helper/Helper.java` with `ping`, `testConnection`, `listSchemas`, `listTables`, `describeTables`, JSON reader/writer, watchdog, scrub, URL allowlist/denylist, size cap; Rust protocol types + client in `persistence-core` (`introspect.rs`), with a trait so tests use a fake runner. | `cargo test` fake-runner protocol tests (all error codes, truncation, version mismatch, scrub); `java Helper.java` golden tests driven from a Rust/shell test behind `JPA_JDBC_JAVA=1`; `check-jpa-parity.py` unchanged. |
| P2 Mapping + fixtures | `to_snapshot` for PostgreSQL and MySQL (section 5); recorded raw JSON from real servers committed as fixtures; golden `SchemaSnapshot` JSON. | Unit tests per type row; Unknown-with-reason cases; `validate()` with 501 tables fails and yields `complete:false` path. |
| P3 Connection UX (G080-G085 slice) | Profile model, Spring config discovery (G081, no JVM needed), password flow, driver discovery A+B1, Test Connection command with the error taxonomy, schema picker (G084/G086). New caps requested: `process:java` (optional), `tasks.begin`, `ui.input`. | Native acceptance in the IDE: JVM missing, driver missing, wrong password, cancel mid-connect, reclaim after 5 min (state survives, helper re-launches). Receipts under `audit/jpa-native/wave10/`. |
| P4 Introspection + cache (G098) | Paged snapshot build with progress, cache in `plugin_dir` keyed by fingerprint (url sans secrets, user, schemas, helperVersion), explicit refresh, staleness badge. | Cache survives restart; no secret appears in cache or storage (test greps for the password sentinel). |
| P5 Reverse engineering (G087-G095, G097, G099-G103) | Consume `SchemaSnapshot` (DB or DDL-text source): inventory vs existing entities, field selection, indexes, enums via `describeExtras`, unknown/deferred types, view ids, cardinality inference, naming cleanup. Output via reviewed FileActions (existing pattern). | Compile-test harness (opt-in) on generated entities for the fixture DB; differential cases against JPA Buddy per `ROADMAP.md` verification program. |
| P6 Model vs DB diff (G096 and G104-129 live paths) | Build entity `SchemaSnapshot`, diff against the DB snapshot with existing `diff`, `Danger` classification; read-only report first. | Snapshot equality for round trip (entity -> DDL -> docker DB -> snapshot -> diff = no changes). |
| P7 Optional | Downloader (needs `net`), jar-based helper, warm helper, extra dialects. | Only after user decisions D3/D9/D7. |

Docker integration (opt-in, `JPA_JDBC_IT=1`, never default): `docker run --rm -d -p 0:5432 postgres:16` and `mysql:8` (+ `mariadb:11`), init script creating a fixture schema covering every row of the section 5 table, composite keys, FK with ON DELETE rule, unique constraint vs unique index, partial index, enum, view, comment, unsigned columns; driver jars resolved from `~/.m2`/Gradle; assertions on golden JSON. Script lives in `scripts/`, outside this design.

## 8. Open decisions (each with recommendation) and row verdicts

| # | Decision | Recommendation |
|---|---|---|
| D1 | Helper form and JDK floor | Single-file source launch, JDK 17+; jar only if startup measures badly. |
| D2 | Declare `database.sources` (native datasource list/credentials/connect, ref 2001)? New capability, so needs your approval. | Defer. It cannot give the plugin schema access or read credentials back (undocumented); revisit after P0 if read-back works, for G080/G082 UX only. |
| D3 | Driver acquisition | A + B1 (user path + helper-side cache discovery). No bundling, no downloader, no `fs.read`, no `net`. You must accept that `process:java` lets the JVM read user files. |
| D4 | Passwords | Config-sourced at call time, else prompt; never stored; stdin only. Requires `ui.input` (new capability). |
| D5 | New capabilities beyond pre-approved `process:java` | Approve `tasks.begin` (60 s budget, cancel) and `ui.input`. |
| D6 | Scope of live access | Read-only metadata; DDL/SQL execution (the execute half of G083) out of scope; revisit separately. |
| D7 | First dialects | PostgreSQL, MySQL, MariaDB; H2 in-memory later; others on request. |
| D8 | Connect confirmation for repo-sourced URLs | Mandatory, every new URL, trusted projects only. |
| D9 | Warm helper reuse inside an instance | No in v1; one-shot only. Measure after P3. |
| D10 | Network | Declare nothing (`net` only gates `host::fetch`); P0 verifies child egress. If the host ever gates child sockets, ask then. |
| O1 | Entity-side snapshot: table naming with schema, `@Index(unique)` vs `@UniqueConstraint` | Inspect the builder before P2; align, do not change `schema.rs`. |

Row verdicts (G080-G103; all currently `unverified` in `requirements.json`):

| Rows | Blocked on | Notes |
|---|---|---|
| G081 datasource discovery (from properties/YAML/profiles) | nothing | Pure source parsing (`crates/spring-text/src/config.rs`); can start now. Only "validate by connecting" needs P1-P3. |
| G082 datasource generation | nothing for text generation; D2 only if "from a native source" | Generates config text from a profile. |
| G083 DDL/SQL options | D6 (execution); text options unblocked | Execute half out of scope under recommended D6. |
| G080 connection management, G084 schema selection, G086 database selection | P0, D1, D3, D4, D5, D8 | Need the helper end to end. G080 may additionally use D2. |
| G085 database coverage | D7 (+ D3 per vendor) | PG/MySQL/MariaDB first; others need driver coordinates and fixtures. |
| G087-G091, G095, G097 (inventory, field selection, indexes/constraints, schema annotation, parent mappings, view ids, cardinality) | P0..P2 (helper + snapshot); logic itself unblocked | Also runnable on the DDL-text path today. |
| G092 enum creation, G100 comments (remarks come from `describeTables`; enum values need `describeExtras`), G103 vendor mappings | P5 `describeExtras`, D7 | Vendor catalog queries, hard-coded allowlist. |
| G093, G094 unknown/deferred types | P2 | Mostly pure over `Fact::Unknown`; helper only supplies raw types. |
| G096 existing entity updates / sync | P2 + P6 | Needs entity snapshot (O1) and diff. |
| G098 schema cache | P4, D4 (secret-free cache rule) | `plugin_dir`, no extra capability. |
| G099 generation defaults, G101 name cleanup, G102 naming algorithms | nothing | Settings/pure naming; share work with G031/G032. |

Upstream alternative stays open (ROADMAP line 14): a host schema API would replace the helper wholesale; the `SchemaSnapshot` mapping layer and the reverse-engineering logic are the parts that survive either way.

## 9. Decision log (2026-11-11, answered by the user)

| # | Decision | Answer | Note |
|---|---|---|---|
| D1 | Helper form / JDK floor | Accepted: single-file source launch, JDK 17+ | |
| D2 | `database.sources` capability | **Approved now** (differs from the recommendation to defer) | Declare as an OPTIONAL capability only when a feature uses it; still no schema API or credential read-back documented, so it only serves connection-management UX. |
| D3 | Driver acquisition | User-provided jar path + helper-side Maven/Gradle cache discovery | No bundling, no downloader, no `fs.read`, no `net`. The user accepts that `process:java` lets the JVM read files the user can read. |
| D4 | Passwords | From config at call time, else prompt; never stored; helper stdin only | |
| D5 | New capabilities | `ui.input` and `tasks.begin` approved (new, beyond pre-approved optional `process:*`) | Declare as optional where the host allows. |
| D6 | Scope | Read-only metadata | DDL/SQL execution out of scope (G083 execute half). |
| D7 | First dialects | **All six at once** (differs from the recommendation: PostgreSQL, MySQL, MariaDB, then others) | PostgreSQL, MySQL, MariaDB, H2, SQL Server, Oracle, Db2. Driver coordinates and fixtures are needed for each; only PostgreSQL/MySQL have SQL/changelog emitters today. |
| D8 | Connect confirmation | Mandatory confirmation for every new URL | |
| D9 | Warm helper | No: one-shot per request | |
| D10 | Network | No `net` capability declared; spike verifies child egress | |
| O1 | Entity-side snapshot alignment | Open (inspect the builder before P2) | |

Next: P0 host spike (see parity/P0_SPIKE.md once written).

## 10. P0 spike corrections (from parity/P0_SPIKE.md; apply during P1)

Source: host source at v0.1.42 (cited in P0_SPIKE.md) plus local measurements with an H2 prototype (parity/spike/Helper.java). Docker was not running, so PostgreSQL/MySQL/MariaDB behavior is still unmeasured.

- **D5 is moot.** `ui.input` and `tasks.begin` are WIT calls, not manifest capability strings; the manifest parser rejects unknown capability strings. Declare only `process:java` (optional, already present) and optionally `database.sources`.
- **D4 needs reconfirmation.** There is no masked prompt: `host::input` shows what the user types in clear, and passwords cannot be read back from secrets or `database.sources`. The decision was "config first, else prompt"; the prompt echoes. Options: keep the prompt with explicit UX copy, or config/env placeholders only until the host adds a secret flag (upstream request for `l0ng-ai/openidea`).
- **D2 recap.** `database.sources` accepts only sqlite, postgres, mysql, sqlServer, clickhouse and oracle (no MariaDB, H2 or Db2), writes `.openidea/data-sources.toml` into the project and never returns credentials. Approved as optional, but keep it out of P1.
- **D7 consequence.** All six dialects need the helper's own driver handling; H2 is in scope only with a URL policy (file/tcp only, no `;` properties, read-only options that were NOT tested).
- **Budget.** Every helper call must run under `begin_task` (otherwise 5 s total, less than a 5 s connect timeout plus about 0.45 s JVM launch). Set the helper `deadlineMs` below the host budget; connect timeout units differ per driver (seconds: pgjdbc, mssql; milliseconds: MySQL, MariaDB).
- **Driver loading.** `DriverManager.getConnection` fails with "No suitable driver" under URLClassLoader source launch; call `Driver.connect` directly (measured). All six drivers loaded.
- **Driver versions.** Do not pick the highest Maven Central version: mssql-jdbc and Db2 "release" resolve to preview/special builds. Prefer the version the project's build declares, otherwise the highest stable.
- **Metadata.** Pass null table types and map vendor strings (H2 reports `BASE TABLE`); filter PK-backing and FK-backing indexes in the Rust mapper.
- **Hygiene.** Never `eprintln!` request or response text (the host logs the plugin's Wasm stderr at info); scrub only passwords of 4+ characters; treat a response without a trailing newline or with invalid JSON as a protocol error. Measured: the password never appeared in argv, env, stdout or stderr.
- **Platform.** The host applies no sandbox to children and does not log child stdin/stdout/stderr; cancel/timeout/reclaim kill the process group. Whether an orphaned JVM survives a `kill -9` of the IDE is unknown.
- **Still unknown (need a real IDE session):** prompt echo and log capture, macOS `/usr/bin/java` stub with no JDK, stdin size cap, JVM network egress as a child of the IDE, real PostgreSQL/MySQL/MariaDB metadata and error codes.

**Reconfirmed by the user after the spike (2026-11-11):** D4 stays "config first, else prompt" even though the prompt shows the typed password in clear (no masked prompt in the host). The UX copy must say so before the prompt, the password is still sent only on the helper's stdin and never stored or logged by the plugin, and an upstream request for a `secret` flag on `host::input` stays on the list.

### 2026-11-12 decisions (answered by the user)

| # | Decision | Answer | Note |
|---|---|---|---|
| N1 | Table names | Bare by default; a cross-schema foreign key keeps `schema.table`; same-named tables in different schemas are an error unless qualification is on | Implemented as `SchemaFilter.qualify_schema` (profile setting, toggled in Manage) and `MapError::DuplicateTable`, whose message names the table and the setting. |
| N2 | More than 500 tables | Never truncate; the connection flow requires a schema or table filter before introspecting | `connection::needs_filter` (> `MAX_TABLES`, tables only, views excluded); `list_with_filter` asks for a `LIKE` pattern up to 3 times, fails with the count if it still matches too many, and stops cleanly when the prompt is dismissed. |
| N3 | Enabled databases | Only H2, PostgreSQL and MySQL/MariaDB; SQL Server, Oracle and Db2 stay disabled until the `jpa-w4-urls` agent verifies their URL rules | `Dbms::ENABLED`; `Profile::validate` and Spring discovery refuse the others with a reason. `Helper.java`'s URL policy was not edited; the plugin only calls it. H2 in P3 is limited to `file:`, `tcp://` and path URLs without `;`/`?` settings (an in-memory database is empty in a one-shot process). |

### P3 outcome (connection UX, 2026-11-12)

Built, all read-only: connection profiles (name, dbms, credential-free URL, user, driver jar path, schema filter, optional Spring source) in plugin `storage` (key `jdbc.profiles`, `deny_unknown_fields` so a stored `password` is rejected); a per-family driver jar cache in `plugin_dir()/jdbc-drivers.json`; Spring/Quarkus/persistence.xml discovery reusing `datasource_facts.rs` (now also keeps the literal user name; `literal_password` re-reads a literal password at connect time and never stores it); the password flow (literal Spring password, else a clear-text prompt whose copy says so; empty allowed; dismiss cancels before any launch); confirmation of every new URL/user/jar (`jdbc.confirmed` in storage, no secrets); `HostRunner` (the real `Runner`, `host::run_command("java", [flags, plugin_dir/JdbcHelper.java], stdin)`, helper rewritten by byte comparison); commands `jdbc-manage`, `jdbc-test` (ping + testConnection under `begin_task`, cancel polled between launches) and `jdbc-browse` (schema picker, 500-table filter rule, table picker, chunked `describeTables` with progress and cancel, `to_snapshot` summary); one actionable message per error code (`connection::error_message`) plus two plugin-side additions to the taxonomy: `LaunchError::Denied` / `ErrorCode::ProcessDenied`.

Capabilities: none added. `process:java` was already optional; `storage` and `project.read` were already required; `ui.input` and `tasks.begin` are WIT calls (section 10). Maven/Gradle cache discovery (B1) is not built: the helper has no `listDrivers` op and the plugin has no `fs.read` for `~/.m2`, so only the user-supplied jar path plus the cache above exist. Open for a later increment: a `listDrivers` op in `Helper.java`, or a project-local jar search through `project.find_files`.

Not verified in the IDE (see ACCEPTANCE.md section 9, AC-099 to AC-106): prompt echo, log capture, macOS stub with no JDK, child egress, real PostgreSQL/MySQL/MariaDB error codes and type names, cancel and reclaim behaviour, orphaned JVM after `kill -9`. Verified here: fake-runner and MockHost tests for every error path, and the real helper against real H2 files through the real command flows (`JPA_JDBC_JAVA=1 cargo test real_h2` in `plugins/jpa-assist`).
