# Latest public feature inventory — Spring core/JPA

This inventories current public original capabilities and maps them to actual local modules. Status is feature-scoped, not full parity. Latest metadata/raw versions are in `/tmp/parity-spring-original-metadata.json` and `/tmp/parity-spring-latest.json`.

| Package | Feature | Status | Implementation | Remaining |
|---|---|---|---|---|
| spring-beans | Bean name/type navigation | source-subset | model.rs, lib.rs, hover.rs | Library and auto-config context binding unresolved. |
| spring-beans | Autowiring assistance | source-subset | injection_checks.rs, annotation_owners.rs, lib.rs | Runtime conditional/proxy/library assignability not supplied. |
| spring-beans | Multi-target source gutter navigation | implemented-source-subset | gutter.rs, lib.rs | Native picker revalidates origin/project and target snapshots; bounded source candidates do not establish effective runtime beans. |
| spring-beans | Bean lifecycle assistance | source-subset | target_checks.rs, xml_class_checks.rs | External hierarchy resolution remains adapter work. |
| spring-beans | Profile configuration assistance | source-subset | annotation_checks.rs, sites.rs, lib.rs | Selected source profiles do not model run configuration/runtime precedence. |
| spring-beans | XML bean editing and checks | source-subset | xml_checks.rs, xml_namespace.rs, xml_create.rs | External imported contexts and full DOM/type model remain unresolved. |
| spring-beans | Bean dependency diagram | source-subset | lib.rs, shared diagram rendering | Only source model; no complete effective context graph. |
| spring-beans | SpEL syntax in annotation and XML expressions | implemented-lexical-subset | spel.rs, languages/spel.toml | Escaped/noncontiguous host strings withheld; released embedded CM renderer omits plugin SyntaxProvider tokens (host gap). |
| spring-beans | SpEL bean/variable completion and navigation | source-subset | sites.rs, lib.rs, spel.rs | Expression member/type resolution and evaluation require a semantic adapter. |
| spring-beans | SpEL expression refactoring | missing-plugin-or-adapter-work | indexer.rs (empty public semantic edges) | No complete scoped binding graph; cannot publish blanket Rename. |
| spring-beans | AOP pointcut assistance | source-subset | aop.rs, sites.rs | Resolved aspect/type/member semantics remain adapter work. |
| spring-beans | Persistence-aware Spring bean links | source-subset | model.rs, lib.rs | Effective EntityManagerFactory/session-factory context unknown. |
| spring-beans | Embedded SQL/HQL/JPQL API expressions | paired-plugin-source-subset | JPA jpql_language.rs api_queries | Fully qualified direct Java EntityManager parameter createQuery/createNativeQuery literals; field/local/factory/nested scope and other API families remain source/type adapter work. |
| spring-beans | Kotlin Spring assistance | source-subset | kotlin.rs, kotlin_build.rs | Compiler-plugin/classpath semantics unresolved. |
| spring-boot | Boot annotation/configuration assistance | source-subset | checks.rs, facts.rs, live.rs | Conditional/effective classpath context unknown. |
| spring-boot | Conditional expression syntax | paired-plugin-subset | spring-beans/spel.rs | Requires installed Core SpEL language; semantic evaluation absent. |
| spring-boot | Configuration key/value completion and navigation | source-subset | meta.rs, nav.rs, values.rs, local_meta.rs | Custom conversion, arbitrary dependency resolution and effective overrides unresolved. |
| spring-boot | Selected dependency-JAR metadata ingestion | version-qualified-source-subset | jar_metadata.rs, jar_versions.rs, jar_archive.rs; lib/live scoped lookup | Declared literal/property/direct-managed GAVs and reviewed official cached Boot parent/Cloud BOM entries;16 coexisting artifacts, hashes/source provenance. Version scopes remove3.5 fallback and incomplete unknown negatives. Real Boot4.1.2/Cloud5.0.4 import verified; resolved runtime classpath remains unknown. |
| spring-boot | Property source occurrences across files | read-only-source-graph | placeholder_nav.rs | Physical spellings separated by file/profile; not semantic usages. |
| spring-boot | Configuration key static-source Rename | implemented-static-source-subset | property_rename.rs; anchor_audit.rs records nonadmitted semantics | Exact complete conventional source module graph incl literal imports/Value/profile/YAML; known binding declarations and transformed/dynamic/nonconventional contexts withheld or explicit exclusions. |
| spring-boot | Dedicated Boot run configuration | implemented-opt-in-source-subset | run_profiles.rs; released RunConfigurations | Explicit native Maven/Gradle project build profiles for directly declared Boot plugins; no arbitrary native configuration editing/debugger attach. Disposable Maven/Gradle Boot4.0 execution verified. |
| spring-boot | Live bean/health/environment insights | distributed-source-subset | spring-runtime owns these workflows | No debugger session/type evaluation API; runtime snapshot scope maintained by sibling plugin. |
| spring-boot | Datasource creation from current configuration | implemented-source-subset | database.rs | Static PostgreSQL/MySQL/MariaDB, ClickHouse HTTP, Oracle thin service and SQL Server databaseName forms; no driver connection acceptance claim. Source/target-bound identity and native name bounds; no auto-connect. |
| spring-boot | Datasource dependency inspections | source-subset | build.rs, live.rs | Declared dependencies only, not full transitive driver resolution. |
| spring-boot | Migration location/datasource navigation | source-subset | nav.rs, fileset.rs; Flyway/Liquibase sibling plugins | Effective inherited/imported migration configuration not complete. |
| spring-boot | Cron completion/validation | source-subset | cron.rs, live.rs | Runtime scheduled-expression resolution remains external. |
| spring-boot | Boot MVC configuration assistance | source-subset | nav.rs, values.rs; spring-web sibling plugin | Effective view resolver/autoconfiguration model absent. |
| spring-boot | Resolved Spring Data query preview | missing-plugin-or-adapter-work | no effective query preview | Current Data grammar can support source query drafts; resolved runtime query needs store/provider facts. |
| spring-boot | Hibernate key assistance | source-subset | bundled metadata and selected metadata JARs | Version-pinned metadata is not all dependency/runtime properties. |
| spring-data | Repository creation | source-subset | generation.rs | Current source entity/scalar IDs; no arbitrary inherited parent/source-root wizard or batch generation. |
| spring-data | JpaSpecificationExecutor repository variant | implemented-source-subset | generation.rs | Reviewed Java/Kotlin JPA variant from admitted current entity/scalar ID; arbitrary external parent bounds unresolved. |
| spring-data | Repository/entity navigation | source-subset | lib.rs, members.rs, live.rs | Fragments beyond known source conventions and classpath hierarchy incomplete. |
| spring-data | Derived query method assistance | source-subset | lib.rs, checks.rs, live.rs | Custom store operators/library property chains unresolved. |
| spring-data | Scalar finder repository generation | implemented-source-subset | generation.rs | Up to eight typed criteria with whole-method grammar/arity roundtrip; numeric/string/Boolean/null/in operators and Page/Pageable or List/Sort variants. Modifying/custom runtime query semantics remain unresolved. |
| spring-data | Finder insertion in existing repository | implemented-source-subset | generation.rs insert_finder | Exact same-package/same-language single JpaRepository parent and explicit interface body; duplicate name/custom parent/foreign domain/Kotlin aliases/ambiguous ownership withheld; JavaBeans property casing must roundtrip. |
| spring-data | Projection interface generation | implemented-source-subset | generation.rs | Chosen direct scalar source fields/getters/records/Kotlin properties; nested/open/SpEL projection generation remains plugin work. |
| spring-data | EntityGraph repository generation | implemented-source-subset | generation.rs | Selected direct source associations on findById; arbitrary nested graphs/existing repository edits remain plugin work. |
| spring-data | JPA query syntax and annotation selection | paired-plugin-lexical-subset | injections.rs; JPA Assist jpql_language.rs | No persistence-unit semantic server; physical strings only; embedded CM coloring omits plugin syntax (host gap). |
| spring-data | JPQL entity/property assistance | source-subset | jpql.rs, types.rs, live.rs | Only proven cached source owners; full library type model unresolved. |
| spring-data | Store-specific Mongo/JDBC/R2DBC assistance | source-subset | mongo.rs, checks.rs, live.rs | No complete resolved store/runtime model. |
| spring-data | Query-to-method/JPQL conversion | missing-plugin-work | generation.rs scalar equality finder subset | Source-owned conversions possible with safe model; do not claim API-blocked. |
| spring-data | DTO projection creation | distributed-source-subset | JPA Assist dto_action.rs | Constructor/nested mapping variants not complete. |
| spring-security | Method security expression completion/navigation | source-subset | lib.rs, ownership.rs, refs.rs | Full typed permission root/member semantics unresolved. |
| spring-security | Security migration and matcher checks | source-subset | lib.rs, modern_matcher.rs, path_matcher.rs, debug_call.rs | Effective filter chain/bean/runtime policy resolution absent. |
| spring-security | Security XML completion/validation | source-subset | xml_namespaces.rs, lib.rs | Imported bean/type/model resolution remains partial. |
| spring-security | Literal secured endpoint links | source-subset | endpoints.rs, refs.rs | Dynamic matcher and effective controller binding unresolved. |
| spring-security | Security expression lexical injection | paired-plugin-lexical-subset | injections.rs; core spel language | Canonical Java/Kotlin annotation physical ranges; requires registered core SpEL language, no type/evaluation semantics. |
| spring-security | Debugger authorization unlock | host-blocked | no debugger mutation API | Current API lacks arbitrary debugger session evaluation/control. |
| spring-osgi | OSGi XML structure assistance | source-subset | lib.rs, ownership.rs | Full descriptor/DOM parity not demonstrated by current source tests. |
| spring-osgi | OSGi reference conversion/navigation | source-subset | lib.rs, bean_facts.rs | Imported effective Spring context and external services unresolved. |
| spring-osgi | Service/listener method checks | source-subset | hierarchy.rs, lib.rs | Source-closed Java hierarchies only; external library signatures need adapter. |
| spring-osgi | OSGi declaration generation | source-subset | lib.rs FileActions | Local reviewed declarations; imported context-wide name uniqueness unknown. |
| spring-osgi | OSGi semantic usages/Rename | missing-plugin-work | no general safe symbol publication | Known scope graph possible; effective global context ownership unresolved. |
| jpa-assist | Entity coding assistance | source-subset | live.rs, checks.rs, local_* modules | Hierarchy/external mappings not semantically complete. |
| jpa-assist | Identifier absence/compatibility checks | source-subset | local_identifiers.rs, local_repository.rs | Current no-super source notes and positive fresh saved-source ID compatibility; complete hierarchy/XML mapping unresolved. |
| jpa-assist | Attribute usages/refactoring | source-subset | attribute_info.rs, attribute_refactor.rs | Local atomic private scalar subset; full external usages unknown. |
| jpa-assist | JPQL language syntax | implemented-lexical-subset | jpql_language.rs, languages/jpql.toml | No persistence-unit type resolution/evaluation/server; embedded CM coloring omits plugin syntax (host gap). |
| jpa-assist | JPQL console and results | implemented-qualified-adapter-subset | query_console.rs, adapters/ReviewedJpaConsole.java; native jpql-console view | Explicit manual JDBC compiled Hibernate6.6.49/7.4.15 SELECT scalar console, typed parameters, bounded results and cancellation. Both H2 versions qualified. Other drivers/provider/runtime semantics and mutation modes unqualified/unimplemented. |
| jpa-assist | Persistence tool window | source-subset | lib.rs native structure view | Source entities only; ORM/library/database facets remain incomplete. |
| jpa-assist | Entity relationship diagram | implemented-source-subset | source_diagram.rs, UI-kit source-diagram view | Qualified module/source-set graph with positive association/inheritance evidence, completeness status and fresh navigation; full effective entity/schema graph needs provider facts. |
| jpa-assist | Entities from database schema | adapter-required | generate.rs SQL text and ddl_action.rs drafts | No introspection API; external approved schema adapter remains feasible integration work. |
| jpa-assist | Entity/attribute/DTO generators | source-subset | file_actions.rs, wizard.rs, dto_action.rs | Broader indexes/named query/validation/designer workflows remain plugin work. |
| jpa-assist | Lifecycle and utility generation | source-subset | local_callbacks.rs, file_actions.rs | Source-owned known contexts; broad listener/hierarchy semantics unresolved. |
