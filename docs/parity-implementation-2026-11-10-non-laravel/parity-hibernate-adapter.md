# Optional compiled Hibernate metadata→DDL adapter — 2026-11-11

## Implemented contract

Flyway now exposes **Generate Reviewed Offline Hibernate Schema SQL…** and an optional `process:java` grant. A declared project-relative JSON profile selects checksum-pinned Hibernate6.6.49.Final runtime JARs, optional explicit project JARs, compiled roots and annotated entity/converter roots with declared UTF-8 source files. No provider invokes Java or discovers/runs a project build.

The bundled independent pure-JDK helper first inspects exact project-owned JARs, every compiled-root resource and declared sources without loading model classes. The UI then displays the artifact/source fingerprints, approved helper SHA256 and an explicit trusted-project-code warning. Known JVM injection/classpath environment keys are reserved empty and inherited collisions fail in API1.4. The17 supported runtime artifact hashes are pinned and independently matched against official Maven Central checksum/content endpoints. Implicit manifest Class-Path, symlink escapes, unknown options and ambiguous entity class origins are refused.

After approval, the helper uses supported Hibernate MetadataSources/MetadataBuilder and SchemaManagementToolCoordinator APIs through version-bound reflection, standard JPA/physical naming and H2Dialect. It clears implicit Hibernate settings, does not call app main, Spring, persistence-unit bootstrapping or SessionFactory, disables JDBC metadata/database actions, supplies a provider rejecting Hibernate JDBC requests and verifies that provider was not replaced. Creation SQL is captured in a bounded in-memory writer, never applied. Source/classpath fingerprints must remain unchanged afterward.

**Converters/custom types/generators/contributors may execute trusted project code**, including constructors and static initialization with arbitrary side effects or their own I/O/connections. The approval text explicitly states this; the adapter is not represented as a sandbox or complete effective application model. Compiled bytecode is authoritative; source fingerprints do not prove source/bytecode correspondence.

Generated SQL remains an instance-owned, root-scoped,15-minute preview. The existing FileActions pipeline offers **Save Hibernate schema SQL as migration…**, repeats pure inspection, prompts for version/description/directory, previews the exact H2 SQL/provenance and returns one atomic non-overwriting workspace Create. No SQL execution is requested by the adapter. Delivered relevant file changes clear the candidate; old/changed provenance cannot save.

## Files

- `plugins/flyway/src/hibernate_schema.rs`: profile/lock/provenance/runtime orchestration, candidate ownership and three Rust regression scenarios.
- `plugins/flyway/adapters/OfflineHibernateSchema.java`: bundled pure-JDK standalone helper;17 pinned runtime hashes in adjacent JSON.
- Existing Flyway command/FileActions/event integration; manifest optional Java capability and JSON profile template; direct sha2 dependency/lock additions only (SDK/API unchanged).
- `plugins/flyway/adapters/README.md`: precise setup, trust boundary, supported profile and official API sources.
- `plugins/flyway/tests/hibernate-adapter/`: explicit portable integration runner, dependency-only Maven POM and independently written annotated entity/converter/application fixtures. No runtime JARs/classes redistributed.

## Verification

- **Flyway130 Rust tests pass**, zero failed: `/tmp/parity-hibernate-adapter/cargo-tests.log`.
- Fresh release **Wasm build passes**: `/tmp/parity-hibernate-adapter/wasm-build.log`.
- `git diff --check -- plugins/flyway` passes.
- Exact delivery receipt: `/tmp/parity-hibernate-adapter/delivery-receipt.json`.
- All17 runtime artifacts independently matched official Maven Central SHA256 sidecars where present, otherwise directly fetched official artifact content SHA256: `/tmp/parity-hibernate-adapter/maven-central-verification.json`.
- Actual Java26.0.1/Hibernate6.6.49 fixture: `/private/tmp/parity-hibernate-fixture-accepted-20261111/`. Uses the SAME hashed private helper filename and standalone launch shape as production.
- Real annotations produce H2 sequence, two entity tables, varchar length/nullability, unique and foreign-key DDL. `hibernateJdbcRequests=0`.
- Converter constructor demonstrably executes a local marker write; this validates why trust review is mandatory rather than claiming metadata inspection is inert.
- An unselected Application class throws if initialized; successful generation proves no app bootstrap.
- Changed declared source rejects BEFORE converter/model execution.
- Added application hibernate.properties with a forbidden JDBC URL, persistence.xml and import.sql do not alter admitted creation SQL or request JDBC.
- An implicit remote manifest Class-Path is refused during inspection before metadata; no URL is fetched.
- No GUI acceptance, package publication or real database mutation was performed.

## Residuals

This closes a **supported optional creation-DDL adapter profile**, not all DB/model parity. It does not compare live database schemas, infer effective Spring/runtime persistence units, adopt arbitrary ORM versions/dialects/module paths or prove a source build corresponds to bytecode. Those need separately admitted profiles/adapters and their acceptance. Existing lexical JPA Assist drafts remain lexical and were not relabeled. No JPA plugin/shared source or host/API changed.

Flyway Enterprise13.12.1 comparison/snapshot/migration-SQL generation remains dependent on an unavailable installed licensed runtime; no license or proprietary implementation was fabricated. Liquibase offline snapshot/diff/grouping deliveries remain as separately documented in `/tmp/parity-database.md`.

Official sources: https://hibernate.org/orm/releases/6.6/ ; https://docs.hibernate.org/orm/6.6/introduction/html_single/ ; https://docs.hibernate.org/orm/6.6/javadocs/org/hibernate/tool/schema/spi/SchemaManagementToolCoordinator.html .
