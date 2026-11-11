# boot-jpa-kotlin-jakarta

Gradle Kotlin DSL, Kotlin 2.0.21 with `kotlin("plugin.jpa")` and `kotlin("plugin.spring")` plus an `allOpen` block for `jakarta.persistence.Entity|MappedSuperclass|Embeddable`, Spring Boot 3.3.5, Java 17 toolchain, jakarta.persistence, Hibernate 6.x (via Boot 3), PostgreSQL driver. The same shop domain as `boot-jpa-java-jakarta`, written in Kotlin.

**Open:** open this folder as the project root. **Toolchain:** JDK 17+, Gradle 8.10+ (no wrapper is committed; run `gradle wrapper` or use a local Gradle). Editing needs no database.

## Layout (package `com.example.shop`, `src/main/kotlin`)

| Path | Purpose |
|---|---|
| `domain/Customer`, `CustomerOrder`, `CustomerProfile`, `OrderLine`, `Product`, `Category` | entities as `class` (not `data class`), `MutableList`/`MutableSet` associations, constructor-parameter columns, `@Embedded`, `@MapsId`, `@NamedEntityGraph`, `@NamedQuery` |
| `domain/OrderLineId` | `data class` embeddable id (legal: embeddables are not entities) |
| `domain/Payment` (SINGLE_TABLE), `CardPayment`, `Vehicle` (JOINED), `Auditable` (`@MappedSuperclass`), `Address` (`@Embeddable`) | inheritance and embeddables |
| `repository/` | derived queries, `@Query`, `Page`/`Slice`, `@EntityGraph`, interface and data-class projections |
| `dto/` | interface projection, `data class` DTOs with `@field:` validation |
| `service/CustomerService` | primary-constructor injection |
| `service/OrderService`, `PricingComponent` | no primary constructor (target of "Inject repository…") |
| `defects/` | `KotlinDataClassDefect`, `KotlinValDefect`, `KotlinMappingDefects`, `KotlinNoArgDefect` |
| `build.gradle.kts` | literal `kotlin("plugin.jpa")`, `allOpen`, `spring-boot-starter-data-jpa`, Boot 3.3.5 plugin version |
| `GOLDEN.md` | exact expected diagnostics |

## Acceptance features this project supports

Kotlin-side live diagnostics (`jpa-data-class`, `jpa-val-attribute`, `jpa-immutable-collection`, plus the shared Java/Kotlin mapping rules: generated value, column length/precision, unidirectional one-to-many, many-to-many list), Kotlin repository creation/query actions (`repository/CustomerRepository.kt`), Kotlin DTO/projection generation, Kotlin `entityLanguage` detection from `build.gradle.kts` and `src/main/kotlin`, "Inject repository" with primary constructor (`service/OrderService.kt`), datasource configuration, Persistence Designer on Kotlin entities. To see the no-arg-plugin family manually, delete the `kotlin("plugin.jpa")` line (`jpa-kotlin-plugin-missing` is a legacy/save-event rule, not part of the golden list; see `GOLDEN.md`).

## Compile check

All `src/main/kotlin` sources (defects included) compiled with the Kotlin 2.2.21 embedded compiler (`K2JVMCompiler`, no compiler plugins) against the same jars as the Java project. Gradle itself was not run (not installed); `build.gradle.kts`/`settings.gradle.kts` were only reviewed (brace balance checked). `src/test` not compiled.
