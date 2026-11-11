# P0 spike: host evidence and helper prototype for the JDBC helper (2026-11-11)

Scope: answers the "undocumented" items of `JDBC_HELPER.md` section 1 from the host source (tag `v0.1.42` of `l0ng-ai/openidea`, cloned to `/tmp/jpa-p0-spike/openidea`, WIT package `openidea:plugin@1.5.0`) and from a local prototype (`spike/Helper.java`). No parity claim. Nothing here was run inside the IDE: every "code" verdict is a reading of the source and needs the IDE scenarios in section 6 before it is treated as measured behaviour.

Citation prefixes: `host/` = `crates/ide-host/src/plugins/`, `plug/` = `crates/ide-plugin/src/`, `ext/` = `crates/ide-extension/src/`, `core/` = `crates/ide-core/src/`.

Verdict legend: confirmed (source says so), refuted (the design assumed otherwise), unknown (source silent, how to verify given).

## 1. Corrections found first

- `ui.input` and `tasks.begin` are **not manifest capabilities**. They are WIT interface paths (`wit/host.wit:92` `interface ui`, `wit/events.wit:66` `interface tasks`); the "Capability" column of reference.md 442-465 is headed `WIT`. The `Capability` enum (`ext/capability.rs:14-45`) has no variant for them and `FromStr` ends with `_ => Err("unknown capability")` (`ext/capability.rs:318`), so declaring either string in `plugin.toml` would fail to parse. Neither call does a `require()` (`plug/wit_host.rs:560-564` input, `:1196-1203` begin). D5 ("approve `ui.input` and `tasks.begin` as new capabilities") is therefore moot: nothing to declare, nothing to approve.
- The plugin already declares `optional = ["process:java"]` and `engines.openidea = "^1.5"` (`plugins/jpa-assist/plugin.toml`, used by the H2 JPQL console), so `process:java` is a precedent, not a new grant, and API 1.4 (`database.sources` needs it, `ext/manifest.rs:343-348`) is already met.

## 2. Part A: host questions

| # | Question | Answer | Evidence | Verdict |
|---|---|---|---|---|
| 1a | Capability string for a JVM | `process:java` (program name) or `process:/abs/path/java`. A relative path containing `/` is rejected. Risk High. | `ext/capability.rs:289-294` (parse), `:70-87` (risk), `:122` describe | confirmed |
| 1b | Matching rule | Exact string equality: the `program` in `run`/`run_command`/`start` must equal the declared string (`covers` falls to `self == wanted`). No basename or glob matching. | `plug/wit_host.rs:1389` `require(Capability::Process(cmd.program))`, `ext/capability.rs:129-143` | confirmed |
| 1c | Resolution and pin | Name resolved on the login-shell `PATH` at grant time (absolute entries only; relative entries skipped), parent dir canonicalized, link name kept, content hash pinned and re-verified at every start; "not found at grant time" means no pin and a Failed error until re-granted. The user can pick another file for a declared `process:java` (`choose_program`). | `ext/ledger.rs:417-457`, `host/mod.rs:3014-3021` (`resolve_binary`), `host/api.rs:1033-1050` (verify on use), `host/mod.rs:1438-1480` | confirmed |
| 1d | Arg and env limits | <= 256 args of <= 4096 B; env <= 64 entries, name <= 256, value <= 4096 B, add-only (a name already in the inherited env is an error). No other argument filtering. | `plug/wit_host.rs:190-194,1390-1411`, `host/api.rs:446-470` | confirmed |
| 1e | cwd and env inheritance | `Some(project)` runs in the project root; `None` runs in `<data_dir>/<plugin id>` (parent of `plugin_dir()`). Child inherits the IDE environment plus login-shell variables the IDE lacks (the source comment names `JAVA_HOME`). Own process group. | `host/api.rs:1228-1232`, `core/process.rs:22-41,496-513` | confirmed |
| 2a | How stdin is passed | `run`/`run_command`: piped, `write_all` from a thread, then pipe dropped (EOF). `start`: stdin always piped, `Child::write` (<= 1 MiB per call, `wait` timeout aware) and `close_stdin`. No size cap seen on the `run` stdin string itself. | `host/api.rs:2390-2425`, `:2478-2530`, `plug/wit_host.rs:198,1492` | confirmed (no cap on `run` stdin: unknown in practice, test 8 MiB) |
| 2b | Are stdin/stdout/stderr logged | No `tracing` call anywhere in the process paths (`host/api.rs` has two unrelated `warn!` at 1941 and 2314). The only stderr logging is the **plugin's own Wasm stderr** at `info` after each call. Output of children is returned to the plugin only. | `host/api.rs` (grep `tracing::`), `plug/runtime.rs:1858-1863` | confirmed for this code path; unknown for RUST_LOG=trace in dependencies, crash reports and remote hosts (`plugins/remote_host.rs`): verify (S2) |
| 3a | Output limits | 4 MiB per stream for `run`; excess is read and discarded (no `truncated` flag). `start`: 1 MiB pending per stream (older dropped), one `poll`/`wait` chunk <= 1 MiB, <= 8 live handles. | `host/api.rs:177-181,2408-2420`, `plug/wit_host.rs:196` | confirmed |
| 3b | Time budgets | `Limits::default`: 5 s call timeout, 60 s background, 300 s idle reclaim, 256 MiB. `run_process` gets `remaining()` as timeout and is killed at it. `tasks.begin` (WIT, no grant) calls `renew_background(call_started)`: deadline = call start + 60 s; `report`/`is_cancelled` renew it, but a blocked `run_process` cannot call them, so 60 s is the ceiling. Time spent in a user prompt is **excluded** from the budget. | `plug/types.rs:344-357`, `plug/wit_host.rs:1196-1203,1246-1260,1445,322-325` | confirmed |
| 4a | `ui.input` capability | Does not exist as a capability (section 1). Needs an interactive window context (command/event), except quick-pick/input/confirm from an action prompt. | `plug/wit_host.rs:309-319` | confirmed |
| 4b | Masking | **None.** `Prompt::Input` carries only `{prompt, value, multiline}`; the frontend calls `promptInput` with no password option, so the text is shown in clear. Value pre-fill is clipped at 4096 B. No secret flag in WIT. Whether the typed text is logged by the UI layer: not seen in code. | `wit/host.wit:90-92`, `plug/wit_host.rs:560-564`, `src/features/plugins/runtimeUi.tsx:395-406` | refuted (masked prompt does not exist); UI echo itself needs a screenshot (S1) |
| 4c | Any other no-echo path | `secrets` capability is write-only for the plugin and `{{secret:}}` works only in fetch headers; `database.sources.set_credential` stores a password the plugin can never read. The only masked entry is the native Database dialog, which cannot feed the helper. | `plug/wit_host.rs:1364-1385`, `wit/database.wit:33-34` | confirmed: no masked prompt, no read-back |
| 5 | Child sockets | No sandbox in the host: grep for seatbelt/sandbox-exec/landlock/unshare/seccomp/CLONE_NEWNET over `crates/` hits only spell-check dictionaries. Children are plain `std::process::Command` with piped stdio. `net:` gates `host::fetch` only. Measured outside the IDE: a JVM opened sockets (refused on 127.0.0.1:1, connect timeout to a black hole). | `host/api.rs:2390-2420,2478-2530`, `plug/wit_host.rs:1307-1352` | confirmed by code; unmeasured as a child of the IDE (S4) |
| 6 | `database.sources` | Plugin-owned sources only: `upsert-source`, `remove-source`, `sources` (ids only), `set-credential` (write-only, 64 KiB), `connect` (asks the invoking window to run the usual connect/introspection, 180 s). Needs API 1.4, a **trusted** project and an open window for `connect`; writes `.openidea/data-sources.toml` in the project. Drivers accepted: `sqlite, postgres, mysql, sqlServer, clickhouse, oracle`. **No mariadb, h2 or db2.** No password, token or secret-looking key in URL or options; no `;` in URL. No schema, query, table or credential read-back. | `wit/database.wit`, `host/database.rs:15-90,92-110`, `plug/wit_host.rs:2310-2375`, `ext/manifest.rs:343-348` | confirmed. Pure connection-management UX; it cannot serve D7's H2/MariaDB/Db2 and cannot feed the helper |
| 7a | `plugin_dir` path to a child | `plugin_dir()` = `<data_dir>/<id>/files`, created owner-only, canonicalized. `command()` checks program and sizes only, never argument contents, so the path (or a `.java` inside it) is a valid argument. The plugin can write there without any `fs.*` grant (`granted_file`/`writable_file` add `plugin_dir` implicitly). | `host/api.rs:2767-2775`, `plug/wit_host.rs:254-270,1388-1411` | confirmed |
| 7b | Child reads outside the project | Not confined: same uid, no sandbox, no argument-path check. `fs.read` scoping applies to the Wasm guest's own `fs::read_*` only. The JVM can read `~/.m2`, `~/.gradle` and any user file. | `host/api.rs:2390-2420`, `plug/wit_host.rs:1702-1715` | confirmed (matches D3 acceptance) |
| 8a | Cancel / timeout | `run`: polled every 10 ms; on cancel or deadline `kill_tree` (SIGKILL to the process group, then reap). JVM is its own group leader, so its children die too. Error is `Cancelled` or `Failed("java timed out after ...")`. | `host/api.rs:2442-2452`, `core/process.rs:31-55` | confirmed |
| 8b | Children (`start`) | Killed on `release`/drop, plugin stop or failure (`kill_processes`), idle **reclaim** (`reclaim` calls it), window close, 8-handle cap enforced. | `host/api.rs:710-716,770,845-849,2478-2489,2601-2606`, `host/mod.rs:2788-2805` | confirmed |
| 8c | Orphans if the IDE itself dies | No parent-death mechanism seen (no `PR_SET_PDEATHSIG`, no pipe watcher in the helper). A SIGKILLed IDE would leave the JVM until its own end. The helper's watchdog (`deadlineMs`) is the only backstop, so keep it. | absence in `core/process.rs` | unknown: kill -9 the IDE during a slow connect and `pgrep -f Helper.java` after the deadline (S5) |
| 9 | macOS `/usr/bin/java` | Here `/usr/bin/java` is the 135,664-byte root-owned launcher stub that execs the JDK chosen by `java_home`. It **ignored an invalid `JAVA_HOME`** and fell back to an installed JDK (`JAVA_HOME=/nonexistent /usr/bin/java -version` printed 26.0.1). Since the host pins the stub, switching JDKs does not change the pin and never prompts re-approval. Behaviour with **no** JDK installed (Apple prints "Unable to locate a Java Runtime", exit 1, from memory) was **not** reproduced: both JDKs here are installed. The plugin cannot read `JAVA_HOME` (Wasm, no env) and cannot override it (env add-only). | local runs; `ext/ledger.rs:447-457`, `host/api.rs:446-470` | partly confirmed; no-JDK stub case unknown (S3) |

### JDK detection recommendation

1. Do not look at `JAVA_HOME`. Run the helper `ping` through `process:java`; success returns `java.version` and `java.jdk`.
2. Failure mapping (measured with `java --limit-modules java.base,java.sql,java.logging Helper.java`): exit 1, stderr contains `Module jdk.compiler not in boot Layer` -> `JdkRequired`. Spawn failure or `Failed("java: ...")` with exit 127 or "not found on PATH when this permission was allowed" -> `JvmMissing`. Apple stub text containing `Unable to locate a Java Runtime` -> `JvmMissing` (verify wording in S3). Anything else non-JSON -> `HelperProtocol`.
3. To pick a specific JDK, tell the user to choose the `java` file for `process:java` in Settings > Plugins (`choose_program`), not to set `JAVA_HOME`.

## 3. Part B: helper prototype results

`spike/Helper.java` (335 lines, `java.*` only, JDK 17+; `javac --release 17` compiles clean, ran on JDK 26.0.1 and Temurin 25.0.3). Ops: `ping`, `loadDriver`, `dmProbe`, `testConnection`, `listSchemas`, `listTables` (offset/limit), `describeTable` (columns, PK, FK, indexes). One request per process; stdout is rebound to stderr at start so only the response uses the saved stdout.

Environment: Apple Silicon macOS, H2 2.3.232 from `~/.m2`, pgjdbc 42.7.13 from `~/.m2`, other drivers downloaded to `/tmp/jpa-p0-spike/jars` (about 20 MB, outside the repo). **Docker was not running** (`failed to connect to the docker API ... orbstack`), so it was not started and no PostgreSQL container was used. No real PostgreSQL/MySQL server was exercised: only driver loading and unreachable-host paths.

### Startup wall time (7 runs each, median; `java Helper.java` source launch)

| Request | default flags | `-Dfile.encoding=UTF-8 -Xmx256m -Djava.awt.headless=true -XX:TieredStopAtLevel=1` |
|---|---|---|
| `ping` (no driver) | 0.35 s | 0.29 s |
| `testConnection` (H2 jar, file db) | 0.53 s | 0.43 s |
| `listTables` (4 tables) | 0.53 s | 0.43 s |
| `describeTable` (columns+PK+FK+indexes) | 0.56 s | 0.46 s |
| `loadDriver` only: pgjdbc / MySQL / MariaDB / mssql / Oracle ojdbc11 / Db2 jcc | 0.44 / 0.35 / 0.36 / 0.43 / 0.49 / 0.36 s (single runs, default+two flags) | |

So the driver adds about 0.1-0.2 s; source compilation is a fixed cost paid on every launch (no cache) and sits well under the 1-2 s guess in section 1. A 5 s call budget (no task) is enough for a local metadata call but not for a 5 s connect timeout plus launch (see correction C3).

### Driver loading and `DriverManager`

- All six drivers load through `URLClassLoader` under source launch (pgjdbc 42.7.13, Connector/J 26.10.0, MariaDB 4.0.0, mssql-jdbc 14.1.0.jre11-preview, ojdbc11 23.26.4.0.0, Db2 jcc 12.1.5.0, plus H2). No transitive jar was needed for any of them in the load test (not a connect test).
- **DriverManager pitfall confirmed.** After `Class.forName(name, true, urlClassLoader)` the driver's static initializer registers it, but `DriverManager.getDrivers()` returned an empty list (the caller's loader cannot see the class) and `DriverManager.getConnection` failed with `No suitable driver found for jdbc:h2:...`. Workaround verified: instantiate the class and call `Driver.connect(url, props)` directly (all later results used this). Also set `DriverManager.setLoginTimeout` only as a hint.

### H2 metadata (file db, schema SHOP; 600 empty tables in schema BIG)

- Columns, PK (composite `ORDER_ID,N` in KEY_SEQ order), FK (`FK_CUST`, update rule 1 = RESTRICT, delete rule 0 = CASCADE, matching the DDL), identity (`IS_AUTOINCREMENT=YES`), default (`'anon'`), table remarks (`customers`), view listed as `VIEW`: all correct.
- Index info needs filtering: the PK index (`PRIMARY_KEY_n`) and a **synthetic FK-backing index** (`FK_CUST_INDEX_8`) are returned beside the user's `IDX_ORD_PLACED`. A unique constraint `UQ_EMAIL` appears only as unique index `UQ_EMAIL_INDEX_5` (confirms O1/section 5: constraint vs unique index is not recoverable; names do not round-trip).
- H2 2.x reports `TABLE_TYPE` `BASE TABLE` (a `["TABLE","VIEW"]` filter still matched) and `TYPE_NAME` `CHARACTER VARYING`, `INTEGER`, `DECIMAL`, so section 5 needs an H2 type table; `COLUMN_SIZE` for BIGINT is 64 (bits), not digits.
- `setReadOnly(true)` is only a hint: H2 reported `readOnly:false`. Read-only safety must come from "metadata API only, no SQL", not from the flag.
- Paging: 600 tables in one `listTables` = 49,305 B JSON, 0.4 s; `limit 200` -> 16,503 B, `truncated:true`, `next:200`; offset 400 -> 200 rows, `truncated:false`. Paging works; `describeTable` per table not timed at scale (H2 in-process numbers would not transfer to a network DB).
- An H2 URL `jdbc:h2:mem:t;INIT=CREATE TABLE PWNED(A INT)\;...` **executed SQL at connect** (both tables then appeared in `listTables`). This proves the hostile-URL risk is real for H2 (section 6 of the design). In-memory H2 is empty in a one-shot process, so it is useless for this design; only file or `tcp:` URLs matter.

### Password and failure modes

| Check | Result |
|---|---|
| Password in argv | `ps -ww -p <pid> -o args=` showed only `/usr/bin/java -Duser.language=en Helper.java`; sentinel `S3NT1NEL-pw` occurrences in args 0, in the process environment (`ps eww`) 0, in `lsof` 0. (A first grep hit 3 matches that were my own shell command line, which carried the literal; the pid-scoped re-check is the valid one.) |
| Password in stdout/stderr | 0 occurrences; stderr was 0 bytes in the wrong-password and unreachable cases. |
| Wrong password (H2) | `AuthFailed`, SQLState 28000, vendor code 28000 (H2's own code, not MySQL 1045). |
| Missing jar / missing class | `DriverNotFound` for both (`FileNotFoundException`, `ClassNotFoundException`). |
| URL for another driver | `Driver.connect` returns `null`; mapped to `UrlUnsupported` (my first mapping wrongly said NetworkUnreachable). |
| Malformed / empty / wrong-version JSON | `BadRequest` (position in message), `BadRequest`, `ProtocolVersion`; exit 0, one line each. |
| Unreachable host, 5 s connect timeout | pgjdbc to `10.255.255.1`: 5.42 s wall, SQLState 08001, message "Connection attempt timed out", **root cause is `PSQLException`, not `SocketTimeoutException`**, so timeout vs refused must be told apart by message/root class (refused has root `ConnectException`). MySQL: 5.50 s with `connectTimeout=5000` (milliseconds!); pgjdbc `connectTimeout` is seconds. MariaDB closed port: 08000, root `ConnectException`. |
| Driver messages are localized | With a French locale, H2 returned French plus English; fixed by `-Duser.language=en`. |
| Password scrubbing | Replacing the password literal in messages corrupts text when the password is short (`p` turned "Exception" into "Exce***tion"). Scrub only passwords >= 4 chars (done in the prototype) plus the `password=`/`pwd=` URL pattern. |
| JRE-only launcher | exit 1, `Module jdk.compiler not in boot Layer` (section 2 detection). |

Not exercised: stray `System.out` output from a driver, `maxBytes` enforcement, SSL, Kerberos/integrated auth, real server error codes (28P01 etc.), per-vendor catalog differences beyond H2.

## 4. Proposed edits to `JDBC_HELPER.md` (not applied)

- C1 (section 1 rows 24, section 9 D5, section 7 P3): replace "new caps `ui.input`, `tasks.begin`" by "WIT calls, no capability string exists; declare only `process:java` (optional, already present) and `database.sources` (optional)". Cite `ext/capability.rs:318`.
- C2 (section 4, D4): masking does not exist; a prompted password is shown in clear. Keep "config first, then prompt" but state it in UX copy, offer "ask every time" only, and file an upstream issue for a `secret` flag on `ui.input` (`Prompt::Input` plus `promptInput` in `runtimeUi.tsx`). `database.sources` cannot be the fallback (write-only credential, no mariadb/h2/db2).
- C3 (section 2.3, section 6 "Timeout"): every helper call must run under `begin_task`. Without it the whole call has 5 s, and 5 s connect timeout plus about 0.45 s launch exceeds it. Set `deadlineMs` = host budget left minus a margin (about 3 s) so the helper answers before the host kills it, and keep helper `connectTimeout` <= that. Prompts are excluded from the budget, so ask first and launch second.
- C4 (section 2.2): DriverManager failure is now measured; keep direct `Driver.connect`. Add `-Duser.language=en` to the flags and a note that `connectTimeout` units are per driver (seconds: pgjdbc, mssql `loginTimeout`; ms: MySQL, MariaDB).
- C5 (section 2.5 / section 6): classify timeout by message/root class; SQLState 08001 covers both refused and timed out for pgjdbc. Add `UrlUnsupported` mapping for `connect` returning `null`. Scrub only passwords >= 4 chars.
- C6 (section 2.4, 5): do not filter `getTables` by the literal `TABLE`; pass null types and map vendor type strings (H2: `BASE TABLE`, `VIEW`). Filter PK-index and FK-backing indexes in the Rust mapper. Add an H2 type row to section 5 (`CHARACTER VARYING`, `INTEGER`, `DECIMAL`, `TIMESTAMP WITH TIME ZONE`...).
- C7 (section 6 hostile URLs, D7): H2 is now in scope, but `INIT=` runs arbitrary SQL at connect. Policy: accept only `jdbc:h2:file:`/`jdbc:h2:tcp:` with no `;` property except an allowlist (suggest none), always add `ACCESS_MODE_DATA=r` and `IFEXISTS=TRUE` (documented H2 options, **not tested here**) so a read-only attach never creates a database file; reject `INIT`, `TRACE_*`, `AUTO_SERVER`, `RUNSCRIPT`. Equivalent per-vendor lists are required for SQL Server (`jdbc:sqlserver` properties such as `authenticationScheme` with JNI/Kerberos, `accessToken`), Oracle (TNS_ADMIN, wallet paths, `oracle.net.*`), Db2 (`pluginName`, `securityMechanism`); all from memory, verify against each driver doc in P1.
- C8 (section 3 B1): "highest version wins" is unsafe: Central's `release` for mssql-jdbc is `14.1.0.jre11-preview` and for Db2 is `12.1.5.0_special_89304`. Skip versions with qualifiers other than `jre*`/numeric, prefer the version the project's build declares, otherwise the highest stable. MySQL coordinates are now `com.mysql:mysql-connector-j` 26.x. Driver classes verified: `org.postgresql.Driver`, `com.mysql.cj.jdbc.Driver`, `org.mariadb.jdbc.Driver`, `com.microsoft.sqlserver.jdbc.SQLServerDriver`, `oracle.jdbc.OracleDriver`, `com.ibm.db2.jcc.DB2Driver`, `org.h2.Driver`.
- C9 (section 1 row 15, section 6 JVM row): "a JDK switch that changes the hash needs re-approval" is false for the macOS stub (pin covers the stub; JDK choice is made by `java_home`). Use `ping` for the real `java.version`, and `choose_program` for selecting a JDK.
- C10 (D2): `database.sources` supports only `sqlite/postgres/mysql/sqlServer/clickhouse/oracle`, writes `.openidea/data-sources.toml` into the project (a VCS-visible file) and never returns credentials. It is declarable (user approved, optional) but offers no help for the helper, H2, MariaDB or Db2. Keep it out of P1.
- C11 (section 2.3): `run` has no stdin cap in the host code read (child `write` is capped at 1 MiB), output over 4 MiB is silently dropped without a flag, so `maxBytes` (3 MB) is the only truncation signal: keep it, and make the plugin treat a response without a trailing newline or invalid JSON as `HelperProtocol`, not as success.
- C12 (section 4): the host logs the plugin's own Wasm stderr at info (`plug/runtime.rs:1858-1863`). Add a rule: the plugin never `eprintln!`s request/response text, and the stderr tail it surfaces is scrubbed.

## 5. Manifest capability list implied by the decisions

| String | Required/optional | Why | Notes |
|---|---|---|---|
| `process:java` | optional (already declared) | launch `java <plugin_dir>/jdbc/Helper.java` | handle `Error::Denied` as `JvmMissing`-style "grant needed" |
| `database.sources` | optional, **only when a feature uses it** (D2) | native source list/credentials/connect UX for postgres/mysql/sqlServer/oracle | needs `engines.openidea ^1.4` (already `^1.5`), trusted project; not needed by P1 |
| `storage`, `project.read` | already required | profile metadata, Spring config reading | unchanged |
| `ui.input`, `tasks.begin` | **none** (WIT calls) | prompt, 60 s budget | nothing to declare |
| `net:*`, `fs.read:*`, `fs.write:*` | none | D3, D10 | `plugin_dir` is implicit |

## 6. Real-IDE scenarios the human must run (OpenIDEA >= the build that ships API 1.5)

Use a trusted scratch project and a throwaway plugin or the local `link` flow; record each result in `audit/jpa-native/jdbc-spike/`.

- S1 Prompt echo: from a command call `host::input("Password", None)`, type `S3NT1NEL-pw`, screenshot whether characters are visible; then grep the IDE log (`RUST_LOG=trace`) and the webview devtools console for the sentinel.
- S2 Logging and limits: run `process:java` with the helper, stdin 8 MiB then 1 MiB, stdout 5 MiB; confirm no stdin/args text in the log tabs or trace log; confirm the 4 MiB output cut and that a stdin over the unknown cap fails or hangs.
- S3 JDK states: (a) with a normal JDK, grant `process:java`, run `ping`, switch JDK via `java_home`/sdkman and re-run (does the pin prompt?); (b) on a machine with no JDK, record the stub's stderr/exit code and what the host returns; (c) run with a JRE-only `java` chosen via Settings and confirm `Module jdk.compiler not in boot Layer`.
- S4 Egress from the child: with a PostgreSQL on `127.0.0.1` (docker or local), run `testConnection` through the host; repeat with `net` not granted; confirm success. Do the same from a remote project (server side).
- S5 Budgets and cancellation: (a) helper to a black-hole host with `connectTimeout=5` without `begin_task` (expect a host timeout error), then with `begin_task` and `deadlineMs=50000` (expect the helper's response); (b) cancel the task mid-connect and `pgrep -f Helper.java`; (c) `kill -9` the IDE during a slow connect and check the JVM disappears at the helper deadline; (d) idle 5 minutes mid-connect and confirm the JVM is killed by reclaim.
- S6 Dismissed prompt and window rules: dismiss the input box (expect `None`), call `input` from a background/on-schedule call (expect `Invalid`), check the wait does not consume the 5 s budget.
- S7 Real servers: PostgreSQL 16, MySQL 8, MariaDB 11 containers (127.0.0.1 only) with the Helper, to pin type names, `TABLE_TYPE`, wrong-password SQLState/vendor codes and the connect-timeout classification on real drivers. Not done here (Docker was down). SQL Server, Oracle and Db2 need their own images and licences.

## 7. Recommended P1 increment

P1 proceeds, with the section 4 corrections folded in. It is one agent-sized unit with no new manifest strings:

1. Promote `spike/Helper.java` to `plugins/jpa-assist/helper/Helper.java`: add `maxBytes`, watchdog bounded by the host budget, URL allowlist/denylist per section 4 C7 (PostgreSQL, MySQL, MariaDB first, H2 `file:`/`tcp:` with the INIT/`;` policy; SQL Server/Oracle/Db2 allowlists as stubs that reject unknown properties), `describeTables` for up to 25 tables, `-Duser.language=en`, error taxonomy from the spike.
2. Rust protocol types and a fake-runner client in `persistence-core` (`introspect.rs`) with golden recorded JSON from the H2 fixtures of this spike (`spike/h2-init.sql`, `spike/h2-big.sql`, both loadable with `org.h2.tools.RunScript`); `cargo test` only, no JVM in CI by default, JVM tests behind `JPA_JDBC_JAVA=1`.
3. Wire nothing into the UI yet: a hidden `JPA: Test JDBC Helper` command (begin_task, `process:java`, `ping`) is enough to run S2, S3, S5 in the IDE. Run S1-S7 before P3 and update `JDBC_HELPER.md` section 1 from the receipts.

Exit gate for P1: protocol tests for every error code plus the 600-table paging case, and the S2/S3/S5 receipts. Parity statuses stay `unverified`.
