# envers-flyway-liquibase

Maven, Spring Boot 3.3.5, Java 17, jakarta.persistence, `hibernate-envers` and `spring-data-envers` written literally in the pom, `flyway-core` + `flyway-database-postgresql`, PostgreSQL driver. A "billing" domain (Client, Invoice, InvoiceLine) with existing Flyway migrations and a deliberate drift between the entities and the migrations.

**Open:** open this folder as the project root. **Toolchain:** JDK 17+, Maven 3.9+.

## Layout (package `com.example.billing`)

| Path | Purpose |
|---|---|
| `domain/Client`, `domain/Invoice` | `@Audited` entities (`Invoice.lines` is `@NotAudited`) |
| `domain/InvoiceLine` | NOT audited: target of "Toggle audited (Envers)" and "Add auditing fields…" |
| `repository/InvoiceRepository` | extends `RevisionRepository<Invoice, Long, Integer>`; `BillingApplication` enables `EnversRevisionRepositoryFactoryBean` |
| `repository/ClientRepository`, `InvoiceLineRepository` | plain repositories ("Create revision repository…") |
| `src/main/resources/db/migration/V1__init.sql`, `V2__add_invoice_status.sql` | existing versioned SQL migrations (default Flyway location, `V<n>__<desc>.sql`) |
| `src/main/java/db/migration/V3__BackfillInvoiceStatus.java` | existing Java migration (`BaseJavaMigration`), proves the Java location |
| `sql-import/postgresql-shop.sql` | `CREATE TYPE ... AS ENUM`, tables with PK/FK/unique/index, composite key, shared-PK child, comments, one plain-column view and one aggregate view (refused by design) |
| `sql-import/mysql-shop.sql` | backtick identifiers, `AUTO_INCREMENT`, inline `ENUM(...)`, unsigned ids, plain view and aggregate view |
| `liquibase-variant/` | Liquibase overlay (see below) |
| `GOLDEN.md` | exact expected diagnostics (the two files under `defects/`) |

## Schema baseline / migration-generation scenario (entities vs V1+V2)

| Drift | Where |
|---|---|
| new column | `Client.phone` has no column in V1 |
| NOT NULL change | `Invoice.total` is `nullable = false`; V1 declares `total numeric(12,2)` nullable |
| dropped column | `client.legacy_code` exists in V1, `Client` has no such field |
| rename candidate | V1 column `client.full_name` vs entity `Client.displayName` (`display_name`) |

Use "Create schema baseline…" (needs the existing migrations as the V1 snapshot) and "Generate Flyway migration from model changes…" on the `domain/` entities. "Create Flyway Java migration…", "Create Flyway Java callback…", "Create Flyway SQL callback…" work in this project because both `db/migration` folders exist.

## "Generate entities from SQL file…"

Open `sql-import/postgresql-shop.sql` or `sql-import/mysql-shop.sql` (or `V1__init.sql`) and invoke the action; target package `com.example.billing.domain` is the existing source package.

## Liquibase variant

`liquibase-variant/` holds `pom.xml` (liquibase-core instead of flyway), `overlay/application.properties`, and two layouts under `overlay/resources/db/`: `changelog/` (explicit `<include>` master + `changes/001..002`) and `changelog-includeall/` (`includeAll path="db/changelog/changes/"`). The changelog files are deliberately NOT under a `src/main/resources/db/changelog` path so the Flyway project stays unambiguous. To swap: run `liquibase-variant/apply.sh` (or `apply.sh includeall`), which replaces the pom, removes the Flyway migrations and copies the changelog into `src/main/resources/db/changelog`. Undo with git (`git checkout -- . && git clean -fd .` inside this folder). Then use "Generate Liquibase changelog from model changes…".

## Compile check

`src/main/java` compiled with `javac --release 17 -proc:none` against jakarta.persistence-api 3.2.0, spring-data-jpa/commons 4.1.2, spring-data-envers 4.1.2, flyway-core 10.22.0 and a two-annotation stub for `org.hibernate.envers.Audited/NotAudited` (hibernate-envers is not in `~/.m2`; stub kept outside the repo). Both SQL files were parsed with the plugin's own `persistence_core::ddl_parse` (PostgreSQL and MySQL dialects detected, tables/enum/plain view accepted, aggregate views refused as intended). `mvn -o validate` passed for `pom.xml` and `liquibase-variant/pom.xml` after temporarily swapping the parent to the cached Boot 3.3.12. Changelog XML parsed with `xml.etree`.
