# legacy-javax-hibernate5

Maven, Spring Boot 2.7.18, Java 11 (`<java.version>11</java.version>`), `javax.persistence`, Hibernate 5.6.15.Final (literal `hibernate-core` version), H2. A small "library" domain (Author, Book, Publisher, Tag) with `META-INF/orm.xml`, `@NamedQuery` and `@NamedQueries` containers.

**Open:** open this folder as the project root. **Toolchain:** JDK 11 (or 17 for editing), Maven 3.8+.

## Layout (package `com.example.library`)

| Path | Purpose |
|---|---|
| `domain/Author` | `@NamedQueries({...})` container with two `@NamedQuery` entries, ManyToMany inverse |
| `domain/Book` | `@NamedQuery`, `@Version`, `@Temporal`, ManyToOne, two owning ManyToMany with `@JoinTable` |
| `domain/Publisher`, `domain/Tag` | OneToMany inverse; String identifier |
| `repository/` | derived queries, `@Query`, `Page`, a derived-looking method (`findAllSorted`) backed by a named query |
| `service/BookService` (constructor), `service/AuthorService` (no constructor) | "Inject repository…" targets |
| `src/main/resources/META-INF/orm.xml` | `<named-query>` and `<entity>` override (javax orm 2.2 namespace) |
| `src/main/resources/application.properties` | literal datasource and naming keys |
| `persistence-xml-variant/persistence-javax-clean.xml` | javax 2.2 persistence unit; swap in as `src/main/resources/META-INF/persistence.xml` |
| `persistence-xml-variant/persistence-jakarta-schema.xml` | jakarta schema vs javax entities; swap in to see `jpa-persistence-namespace` |
| `defects/` | javax variants of the Java defects |
| `GOLDEN.md` | exact expected diagnostics per scenario |

## Acceptance features this project supports

javax namespace detection (generated code must use `javax.*`), Hibernate 5 refusal paths (Hibernate-6-only actions must refuse: literal hibernate-core 5.x and Boot 2.x), named query and `@NamedQueries` handling (`domain/Author.java`), orm.xml ownership (`orm.xml`), persistence-unit coverage and namespace mismatch rules, javax live diagnostics, repository creation and "Inject repository…".

## Compile check

`src/main` compiled with `javac --release 17 -proc:none` against javax.persistence-api 2.2 plus the jakarta/Spring jars from `~/.m2` (Spring Data 4.x jars, because 2.7 jars are not cached; interfaces used are unchanged). The sources use no post-Java-11 syntax. `mvn -o validate` was NOT run for this pom (Boot 2.7.18 parent not in the local repository); XML well-formedness was checked with `xml.etree`.
