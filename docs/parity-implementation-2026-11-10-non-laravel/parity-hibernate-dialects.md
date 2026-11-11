# Qualified offline Hibernate dialect/runtime extension — 2026-11-11

Implemented entirely within Flyway, retaining the explicit trusted-code review and preview-only atomic migration workflow. No host API changes, JPA source changes, DB/server use, app bootstrap or licensed Flyway capability fabricated.

## Current supported contract

- Pinned `hibernate-6.6.49-offline-v2` and current stable `hibernate-7.4.15-offline-v2` runtime profiles; legacy H2 v1 remains compatible.
- Optional explicit `dialect` profile property or concrete chooser before artifact review. Same public MetadataSources/MetadataBuilder/SchemaManagementToolCoordinator SPI works on both actual runtimes.
- 16 qualified dialect presets per version: H2/PostgreSQL/MySQL/MariaDB/SQL Server/Azure SQL Server/Oracle/CockroachDB/DB2/DB2i/DB2z/HSQLDB/HANA/Postgres Plus plus 6.x Derby/TiDB or 7.x Spanner/Spanner PostgreSQL.
- Selection is included in exact inspect/generation proof and source/classpath provenance. Actual registry dialect class is verified; its stock target database version is captured and disclosed as a runtime default, not detected server identity.
- Separate Hibernate7 profile template with 15 exact checksum-pinned JAR names. POM can prepare either runtime explicitly; no provider or automatic dependency download.
- Official latest 7.4.15 release supports JPA3.2 and Java17/21/25/26. Fifteen selected runtime artifacts independently match official Maven Central checksum/artifact content: `/tmp/parity-hibernate-7-adapter/maven-central-verification.json`.

## Exact final acceptance

Final helper SHA256: `47bf051a292e9d27ce6601bc3a959e3ef72cbb74cd1a632531e92b585b4933c8`.

- 132 Flyway Rust tests pass: `/tmp/parity-hibernate-adapter/v2-full-tests.log`.
- Locked wasm32-wasip2 release build passes: `/tmp/parity-hibernate-adapter/v2-wasm-build.log`.
- Actual runtime 6.6.49, 16/16 presets: `/tmp/parity-hibernate6-qualified-20261111/summary.json` (each generated SQL and stdout/stderr retained alongside).
- Actual runtime 7.4.15, 16/16 presets: `/tmp/parity-hibernate7-qualified-20261111/summary.json`.
- Every matrix generation retained mapped tables, foreign key and named nullable unique constraint and reported zero Hibernate JDBC requests. These are exporter output acceptance checks, not executed server SQL acceptance.
- Actual compiled Jakarta Persistence3.2 annotations `@CheckConstraint` and `@Column(secondPrecision=3)` exported with Hibernate7/PostgreSQL: `/tmp/parity-hibernate7-jpa32-qualified-20261111/summary.json`, `generated.sql`. Zero Hibernate JDBC requests.
- All three final fixtures also prove converter constructor execution, pure inspect without model initialization, source drift refusal before converter initialization, ignored application configs/default import scripts and rejected implicit remote JAR manifest classpaths.
- Combined machine-readable receipt: `/tmp/parity-hibernate-adapter/v2-delivery-receipt.json`.

## Evidence-driven exclusions

Stock Sybase ASE output omits the fixture's nullable unique constraint (both versions); stock Hibernate6 Spanner omits mapped foreign-key/unique semantics. These presets are refused rather than presenting partial creation SQL as a qualified mapping contract. Exploratory evidence retained in `/tmp/parity-hibernate-dialect-matrix-20261111/results.json` and the earlier final-matrix failure directories `/tmp/parity-hibernate6-matrix-final-20261111` / `/tmp/parity-hibernate7-matrix-final-20261111`. Hibernate7 Spanner and SpannerPostgreSQL pass the final mapping contract. Other untested mapping features and dialect/server-version combinations are not implied.

Trusted project converters, types, generators and service contributors may perform arbitrary code/I/O independently; no sandbox claimed. The adapter itself does not request application bootstrap or SQL execution. Effective app configuration, native datasource selection, live incremental diff, and licensed Flyway comparison remain separate residuals.

## Official references

- https://hibernate.org/orm/releases/7.4/ (7.4.15 stable, JPA3.2, Java versions, Boot4.1)
- https://docs.hibernate.org/orm/7.4/javadocs/org/hibernate/tool/schema/spi/SchemaManagementToolCoordinator.html
- https://hibernate.org/orm/releases/6.6/
