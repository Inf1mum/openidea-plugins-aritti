# multimodule-gradle

Gradle Kotlin DSL multi-module build: `entities` (Java, `java-library`, jakarta.persistence, Spring Data JPA) and `app` (Kotlin, Spring Boot 3.3.5, `kotlin("plugin.jpa")`/`plugin.spring`/`allOpen`, depends on `project(":entities")`). Exercises module-aware language and build-fact detection: Java entities in one module, Kotlin entities and Kotlin repositories/services in another.

**Open:** open this folder (the repository root) as the project root; to see rules whose gate requires the conventional `<root>/src/main/java` layout, ALSO open `entities/` or `app/` directly as its own project. **Toolchain:** JDK 17+, Gradle 8.10+ (no wrapper committed).

## Layout

| Path | Purpose |
|---|---|
| `settings.gradle.kts`, `build.gradle.kts` | root plugin versions (`apply false`), `include("entities", "app")` |
| `entities/build.gradle.kts` | Java module, literal `jakarta.persistence-api` and `spring-data-jpa` |
| `entities/src/main/java/com/example/mm/domain/Warehouse`, `Item` | Java entities, OneToMany/ManyToOne |
| `entities/src/main/java/com/example/mm/defects/` | Java defects (`ModuleNoArgDefect`, `ModuleMappingDefects`) |
| `app/build.gradle.kts` | Kotlin module, literal `kotlin("plugin.jpa")`, Boot plugin, `spring-boot-starter-data-jpa` |
| `app/src/main/kotlin/com/example/mm/app/WarehouseRepository` | Kotlin repository for a Java entity from the other module |
| `app/src/main/kotlin/com/example/mm/app/WarehouseService` (primary constructor), `ShipmentService` (none) | injection targets |
| `app/src/main/kotlin/com/example/mm/app/Shipment` | Kotlin entity |
| `app/src/main/kotlin/com/example/mm/app/defects/` | Kotlin defects (`KotlinDataDefect`, `KotlinValModuleDefect`) |
| `GOLDEN.md` | expected diagnostics when opening the repository root |

Suggested checks: open a file in `entities/` (language and facts resolve to Java) and in `app/` (Kotlin), and confirm the actions offered for each follow its own module.

## Compile check

`entities/src` compiled with `javac --release 17 -proc:none`; `app/src/main/kotlin` compiled with the Kotlin 2.2.21 embedded compiler against `entities` output, the Spring jars from `~/.m2` and a one-annotation stub for Boot 3's `@EntityScan` (the cached Boot is 4.x, where it moved). Gradle was not run; the `.kts` files were only reviewed.
