# Optional reviewed JDBC JPQL console

This separate adapter executes an explicitly reviewed **SELECT scalar projection**
against a manually supplied JDBC target, using selected compiled annotated classes
and checksum-pinned Hibernate6.6.49 or7.4.15. The Flyway offline schema adapter
retains its separate no-JDBC contract. Neither adapter boots a project application,
Spring or a persistence unit. The console builds its own Hibernate SessionFactory
from the selected classes and standard JPA/physical naming strategies.

## Prepare your explicit profile

1. Save/build the desired compiled project model yourself. The plugin does not run
   Maven, Gradle, annotation processors or an application build automatically.
2. Place every runtime dependency from the selected `hibernate-*-runtime.json`
   checksum lock inside the owning project. The dependency-only Flyway fixture
   POM can explicitly prepare6.6.49 or, with `-Dhibernate.version=7.4.15.Final`,7.4.15.
   No binaries are redistributed or automatically downloaded.
3. Copy `jpql-6.6.49-profile.example.json` or `jpql-7.4.15-profile.example.json` to
   `.openidea/jpql-console.json`. Supply explicit runtime, JDBC driver and optional
   project JAR paths, compiled roots, annotated entity/converter class/source/root
   declarations, target URL, separate username/password, SELECT, typed parameters
   and row limit. Select all association entity roots. Sources do not prove that
   bytecode was compiled from the current source. All artifacts must be owned by
   this project; symlink escapes and implicit JAR manifest classpaths are refused.
4. `driverJars` maps each explicit JDBC artifact to its SHA256. Obtain drivers from
   an official source and verify their checksums yourself before trusting them.
   Example H2 is2.2.224 (MPL2.0/EPL1.0), independently matched to Maven Central.
   The default runtime's core provenance is pinned by the bundled lock; arbitrary
   project/driver artifacts are presented as explicitly trusted supplied inputs.
5. Choose **JPA Assist: Run Reviewed JPQL SELECT…**, or **Run SELECT…** in the native
   **JPQL Console** view. Review the exact target URL, SELECT, typed values, profile
   and source/classpath fingerprints before connecting. Credentials are separate
   fields and travel through child stdin, never process arguments. Raw driver and
   provider logs are suppressed; results are shown in a native read-only table.

The manually supplied URL is not native IDE datasource identity. Bare `jdbc:h2:`,
`jdbc:postgresql://`, `jdbc:mysql://` and `jdbc:mariadb://` targets are admitted with
matching standard driver/dialect classes; URL properties, credentials and init
scripts are refused. Actual integration acceptance covers H2 on both runtime
versions. Other drivers/servers remain supplied, unverified profiles; there is no
claim of database-wide or arbitrary application-model compatibility.

Profiles contain credentials. Store/manage their files according to your project
policy and exclude them from source control when appropriate. The adapter does
not write credential files or import application datasource configuration.

## Execution and boundaries

Named scalar parameter types: `string`, `long`, `decimal`, `boolean`, `date`
(ISO local date), `timestamp` (ISO local datetime), and `uuid`; values are explicit
strings. Null, collection and entity parameters are refused. SELECT must start
with `select `; multiple statements and mutation commands are refused. Hibernate's
`createSelectionQuery` additionally checks query semantics. Result rows contain
scalar projections, not editable/expanded entity graphs; unsupported result
objects are refused without rendering their getters or entity `toString`.

The adapter requests JDBC read-only, disabled automatic flushing, read-only
queries, a15sec query timeout and rollback in `finally`. **Read-only is a driver
hint, not a security boundary.** Use a database account with read-only privileges.
SELECT functions, project converters/types/generators/contributors and JDBC driver
initialization can execute arbitrary trusted code or cause side effects. There is
no process/network sandbox, effective application configuration or permission
proof. There is no mutation mode.

Rows are capped at100 (default25), columns at32, scalar text at2048 Java characters,
output at128KiB, and named parameters at16. An extra row detects truncation.
One managed child belongs to this plugin instance and project. **Refresh Status**
polls bounded output; its local60sec guard is checked when polled. Driver query
timeouts can be advisory. **Cancel Owned Query** kills that child; server-side
cancellation/rollback is not proved. Plugin shutdown closes the handle; this is
not a durable background job. Results expire after15minutes and are hidden when
source/profile ownership changes. Source/classpath inspection repeats after
approval and after execution; observed drift discards results. Filesystem races
and arbitrary behavior of trusted project code are not excluded.

## Actual disposable acceptance

With explicitly prepared runtime and H2 driver, run:

```sh
python3 plugins/jpa-assist/tests/query-console/run.py --runtime /path/to/pinned/runtime --driver /path/to/h2-2.2.224.jar --profile jpql-hibernate-7.4.15-v1 --output /tmp/new-disposable-jpql-fixture
```

The fixture compiles its own annotated entity with `-proc:none`, creates and seeds
only a new temporary H2 database, checks all7 typed bindings and scalar results,
row truncation, drift refusal, mutation/entity-render rejection, and kills an
actual slow JDBC SELECT before verifying unchanged data. It never uses user DBs.
Rust tests separately cover concrete approval cancellation, instance/project
ownership, busy/cancel/deactivate, drift around approval/poll/render, pinned driver
proof and bounded protocol rendering. Both runtimes were accepted with Java26;
Java17/21/25/26 are the declared compatible runtime profiles.

Official API references:

- [Hibernate6.6 Session](https://docs.hibernate.org/orm/6.6/javadocs/org/hibernate/Session.html)
- [Hibernate7.4 SelectionQuery](https://docs.hibernate.org/orm/7.4/javadocs/org/hibernate/query/SelectionQuery.html)
- [Hibernate7.4 release compatibility](https://hibernate.org/orm/releases/7.4/)
- [H2 source and licensing](https://github.com/h2database/h2database)
