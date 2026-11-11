# mapstruct-lombok-dto

Maven, Spring Boot 3.3.5, Java 17, MapStruct 1.6.3 (`mapstruct`, `mapstruct-processor`) and Lombok written literally in `pom.xml`, with `lombok-mapstruct-binding` in `annotationProcessorPaths`. A small CRM domain with an existing `@Mapper`, DTOs in three styles, and Lombok entities with deliberate defects.

**Open:** open this folder as the project root. **Toolchain:** JDK 17+, Maven 3.9+.

## Layout (package `com.example.crm`)

| Path | Purpose |
|---|---|
| `domain/Account`, `domain/Contact` | clean Lombok entities (`@Getter @Setter @NoArgsConstructor`) |
| `repository/` | Spring Data repositories |
| `dto/AccountRecordDto` | record DTO |
| `dto/AccountPojoDto` | hand-written POJO DTO |
| `dto/ContactLombokDto` | Lombok `@Data` DTO (fine on a DTO, flagged only on entities) |
| `mapper/AccountMapper` | existing `@Mapper(componentModel = "spring")` (targets for "Add mapping method to mapper…", "Create generic base mapper…", "Inject mapper…") |
| `service/AccountService` (no constructor), `service/ContactService` (constructor) | "Inject repository…"/"Inject mapper…" targets |
| `defects/LombokDataDefect` | `@Data` on an entity with associations and a generated id |
| `defects/LombokToStringDefect` | `@ToString` including associations |
| `defects/LombokEqualsDefect` | `@EqualsAndHashCode` including associations and generated id |
| `defects/LombokBuilderDefect` | `@Builder` without a no-arg constructor |
| `pom-modelmapper.xml` | ModelMapper variant (`mvn -f pom-modelmapper.xml`; mapstruct removed, `org.modelmapper:modelmapper` added) for "Create ModelMapper configuration…" / "Add ModelMapper conversion methods…" |
| `pom-blaze.xml` | Blaze-Persistence variant (blaze spring-data integration + entity-view api/processor) for "Create Blaze entity view…" and "Create entity view repository…" |
| `GOLDEN.md` | exact expected diagnostics |

The plugin reads literal build text above a source file: only `pom.xml` is the default build file. To test the variant actions, temporarily copy the variant over `pom.xml` (or open the variant as `pom.xml` in a scratch copy); the plugin does not read `pom-modelmapper.xml`/`pom-blaze.xml` by name.

## Acceptance features this project supports

Lombok inspections (`jpa-lombok-data`, `-to-string`, `-equals`, `-equals-field`, `-constructors`) and the "Edit Lombok annotations…" editor, DTO generation with Lombok styles (the Lombok literal proof in `pom.xml` is required by the DTO action), record/class conversion (`dto/AccountRecordDto`, `dto/AccountPojoDto`), MapStruct mapper creation from entity+DTO, mapper extension actions, ModelMapper and Blaze variants.

## Compile check

`src/main` compiled with `javac --release 17 -proc:none` against jakarta.persistence-api, Spring jars and mapstruct 1.6.3 from `~/.m2`, plus a stub for the Lombok annotations (lombok is not in `~/.m2`; annotations only, no code generation, stub kept outside the repo). MapStruct and Lombok annotation processing were NOT run (it needs real Lombok). All three poms pass `xml.etree` parsing; `mvn -o validate` passed for all three after temporarily swapping the parent to the cached Boot 3.3.12.
