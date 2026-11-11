# boot-jpa-java-jakarta

Maven, Spring Boot 3.3.5, Java 17, `jakarta.persistence`, Hibernate 6.x (managed by Boot 3, proven by the literal parent version plus `spring-boot-starter-data-jpa`), PostgreSQL driver, no Lombok.
Realistic "shop" domain used to exercise the Java/jakarta side of JPA Assist.

**Open:** open this folder itself as the project root in the OpenIDEA desktop IDE (the plugin finds `pom.xml`, `src/main/java` and `src/main/resources` relative to the root).
**Toolchain:** JDK 17+, Maven 3.9+. No database is needed for editing (the plugin never connects); `mvn spring-boot:run` would need PostgreSQL at `localhost:5432/shop`.

## Layout (package `com.example.shop`)

| Path | Purpose |
|---|---|
| `domain/Customer`, `CustomerProfile`, `CustomerOrder`, `OrderLine`, `Product`, `Category` | ManyToOne / OneToMany / ManyToMany / OneToOne (`@MapsId`), `@Embedded Address`, `@Version`, `@NamedEntityGraph`, `@NamedQuery` |
| `domain/OrderLineId` + `OrderLine` | embedded id (`@EmbeddedId`, `@MapsId("orderId")`) |
| `domain/OrderStatus`, `StatusConverter` | enum (`@Enumerated(STRING)`) and an `AttributeConverter` |
| `domain/Auditable` | `@MappedSuperclass` |
| `domain/Payment` (SINGLE_TABLE) + `CardPayment`, `TransferPayment` | single-table inheritance with discriminator |
| `domain/Vehicle` (JOINED) + `Truck` | joined inheritance |
| `domain/OrderListener` | listener class with callbacks |
| `repository/*Repository` | derived queries, `@Query` (JPQL and native), `Page`/`Slice`/`Optional` returns, `@EntityGraph`, interface projection (`CustomerNameView`), record projection (`CustomerSummary`), `@Modifying` |
| `dto/` | interface projection, record, POJO DTO with validation |
| `service/CustomerService`, `ReportService` | services with an existing constructor |
| `service/OrderService`, `PricingComponent` | `@Service`/`@Component` WITHOUT a constructor (target of "Inject repository…") |
| `src/main/resources/application.properties`, `application-dev.properties` | literal `spring.datasource.*`, `spring.jpa.hibernate.naming.*` keys |
| `persistence-xml-variant/` | `persistence-clean.xml`, `persistence-defects.xml`: NOT active by default (kept out of `META-INF` because the plugin globs `**/persistence.xml`); swap in by copying to `src/main/resources/META-INF/persistence.xml` |
| `src/main/java/.../defects/`, `repository/UnknownParamRepository`, `repository/SourceIdMismatchRepository` | deliberate defects, each file starts with `// expect: <rule code>` |
| `GOLDEN.md` | exact expected diagnostics (rule, file, line) per scenario |
| `src/test/java` | minimal `@SpringBootTest` |

## Acceptance features this project supports

Live diagnostics (every family in `parity/INSPECTIONS.md` that the live provider can emit for Java/jakarta: see `GOLDEN.md`), Spring Data repository creation and batch creation (entities under `domain/`), query method / derived query to `@Query` / query editing (`repository/CustomerRepository.java`), named query and entity graph actions (`Customer`, `Product`), index/unique constraint, validation constraint, embedded identifier (`domain/OrderLineId`), attribute converter, DTO class/projection/flattened DTO/record-class conversion (`domain/Customer`, `domain/CustomerOrder`, `dto/*`), name constants, proxy-safe equals/hashCode/toString (`domain/CustomerOrder`), extract mapped superclass (`domain/Customer` + `domain/Product` share `Auditable`), rename source-local attribute, Hibernate user type/event listener/JSON column (Hibernate 6 is provable), inject repository (`service/OrderService`, `service/PricingComponent`), datasource configuration (`application.properties`), copy DDL / copy DTO from the structure tree, Persistence Designer, Source Diagram, JPQL console and JPQL language injection (`@Query` strings), persistence-unit coverage (swap in the XML variants), naming-strategy detection (literal keys).

## Compile check

Non-test sources (including the defects, which compile except `defects/ConverterDefects.java`, which cannot: javac rejects a non-AttributeConverter in `@Convert`, exactly the defect it demonstrates; it was excluded from the compile) were compiled with `javac --release 17 -proc:none` against jakarta.persistence-api 3.2.0, spring-data-jpa/commons 4.1.2, spring-context/tx/beans/core 7.0.10 and jakarta.validation-api 3.1.1 from `~/.m2` (a newer Spring generation than Boot 3.3; the APIs used are identical). The pom was checked with `mvn -o validate` after temporarily swapping the parent to the cached Boot 3.3.12. `src/test` was not compiled (spring-boot-starter-test not cached).
