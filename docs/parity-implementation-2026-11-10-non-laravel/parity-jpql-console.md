# Explicit reviewed JPQL/JDBC console — 2026-11-11

Implemented separate optional adapter inside JPA Assist, coordinated native integration with spring_core. No host/API changes, no real user database, no automatic application bootstrap, and no mutation mode. The separate Flyway offline-DDL adapter's zero-Hibernate-JDBC helper is unchanged (SHA25647bf051a292e9d27ce6601bc3a959e3ef72cbb74cd1a632531e92b585b4933c8).

## Concrete behavior

- New `src/query_console.rs`, independent pure-JDK `adapters/ReviewedJpaConsole.java`, pinned6.6.49 and7.4.15 runtime locks, explicit JSON profile examples and portable actual JDBC fixture.
- Native JPQL Console view under Persistence with Run SELECT, Refresh Status and Cancel Owned Query; project/instance-owned child and bounded native Table result. No processes in rendering/providers.
- New→Reviewed JPQL Console Profile (Hibernate6/7) templates, commands and optional process:java grant integrated by spring_core.
- Explicit source/classpath/runtime/driver/provenance inspection before trusted-code approval, again after approval and after execution. Exact selected URL, JPQL, typed parameters, row limit and runtime are reviewed. Driver artifacts must match declared SHA256; Hibernate artifacts match official pinned locks. Unsaved source/profile differences, traversal/foreign ownership, symlink escapes and implicit manifest classpaths are refused. Source hashes do not prove bytecode equivalence.
- Manual target and separate credentials; credentials/query go child stdin and are absent from process arguments. Complete raw driver/provider logs are suppressed. URL credentials/properties/init scripts refused. No IDE datasource identity inferred.
- Hibernate public metadata/SessionFactory/Session/SelectionQuery APIs used; no project Application.main, Spring or persistence-unit discovery. Standard naming only, explicitly selected annotated roots. No effective arbitrary application-model claim.
- SELECT scalar projections only. Named string/long/decimal/boolean/date/timestamp/UUID values; max16params,100rows(default25),32columns,2048Java chars/cell and128KiBresult. One extra row detects truncation. Null/list/entity params and custom/entity result rendering are outside subset.
- JDBC read-only, manual flush, read-only query,15sec query timeout and final rollback requested. Trusted project code/driver/SELECT functions can have side effects; read-only is not a security boundary. Concrete approval discloses this, recommends a read-only DB account, and explains cancellation/server limits. Local60sec deadline is polled, not a durable watchdog. Instance shutdown closes owned handles. Results expire15min and source/profile changes hide/discard them.
- Common PostgreSQL/MySQL/MariaDB manual class/dialect targets are admitted with explicit supplied checksummed drivers, but only H2 2.2.224 is integration-qualified. Other server/driver versions are explicitly labelled unverified in approval/docs.

## Final exact validation

Console helper SHA256 `b986d745efd211edd9dba5c72b9592e238bee8ec8911074d8929a580aa9f833f`.

- Full315 JPA tests: `/tmp/parity-jpql-jpa-full-test.log`.
- Locked release wasm32-wasip2 build: `/tmp/parity-jpql-jpa-build.log`.
- Actual Hibernate6.6.49 + H2 2.2.224: `/tmp/parity-jpql6-accepted-20261111/summary.json`.
- Actual Hibernate7.4.15 + H2 2.2.224: `/tmp/parity-jpql7-accepted-20261111/summary.json`.
- Each actual fixture compiles disposable annotated model with -proc:none; runs real SELECT and all7 typed bindings; checks actual scalar values and row truncation; proves trusted converter construction and pure inspect/no app bootstrap; rejects source/profile drift before model construction, mutation and entity rendering; kills an actual executing slow JDBC SELECT, then reconnects to verify unchanged seeded data. Fresh temporary local H2 only. Fixture source/stdout/stderr/results retained per case.
- H2 artifact independently matches official Maven Central SHA256: `/tmp/parity-jpql-driver-verification.json`.
-7 module Rust tests cover cancelled approval/no query launch, no credentials/query in argv, busy/foreign/cancel/deactivate, source/profile drift at poll/render, proof drift after review, driver lock enforcement, duplicate JSON/URL option/mutation refusal, typed/bounded results, and drained post-run result protocol/proof acceptance before showing Table rows.
- Machine-readable delivery receipt: `/tmp/parity-jpql-console-receipt.json`.

## Residual contract

This closes the feasible manually configured version-qualified SELECT console alternative. Native IDE datasource selection/query APIs remain unavailable; arbitrary persistence-unit/application configuration, other ORM/driver/server versions, mutation consoles, entity graph expansion/editing and universal effective-model semantics are not claimed. Existing offline-DDL provider stays separate and no-JDBC. No paid Flyway license/runtime was invented.

## Official APIs

- https://docs.hibernate.org/orm/7.4/javadocs/org/hibernate/query/SelectionQuery.html
- https://docs.hibernate.org/orm/6.6/javadocs/org/hibernate/Session.html
- https://hibernate.org/orm/releases/7.4/
- https://repo.maven.apache.org/maven2/com/h2database/h2/2.2.224/h2-2.2.224.jar.sha256
