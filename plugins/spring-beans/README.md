# Spring Beans Navigator

Spring bean support for Java, Kotlin and Spring XML files in OpenIDEA. MIT; see `LICENSE`.

## Beans

Stereotype classes (`@Service("name")`…), `@Bean` methods (names and aliases), Spring Data repository interfaces, XML `<bean>` definitions and `<alias>`es, with `@Primary`, `@Profile` (and `<beans profile>`) and `@Qualifier`. Beans whose literal `@ConditionalOnProperty`, `@Profile` (single `spring.profiles.active` literal) or `@ConditionalOnMissingBean` (plain project component of the type) is provably false are shown as inactive and leave injection candidates; undecided conditions stay candidates. `ObjectFactory` and `SortedSet` injection points unwrap.

## Navigation, completion and hover

- **Injection navigation**: a gutter arrow on each injection point opens the bean Spring would inject, and Go to Definition on the type lists the candidates. Candidates are matched by type through the project's type hierarchy, then narrowed by `@Qualifier`/`@Named`/`@Resource(name)`, `@Primary` and the parameter or field name. `List<T>`, `Map<String, T>` and arrays take every `T`; `Optional<T>`, `ObjectProvider<T>` and `Provider<T>` take a `T`. Covered: constructor parameters of beans, `@Autowired`/`@Inject`/`@Resource` fields and setters, Lombok-constructor `final` fields, `@Bean` parameters, Kotlin primary and secondary constructors and `lateinit` properties, XML `ref`/`parent`/`depends-on`/`factory-bean`. XML `p:`/`c:` `*-ref` attributes (prefixes bound to the Spring `p`/`c` namespaces) are followed like `ref`; a Kotlin `@Bean fun x() = Foo(…)` is typed `Foo`.
- **Where a bean is injected**: a gutter icon on each bean lists the injection points that receive it.
- **Bean names in strings**: navigation and completion in `@Qualifier`, `@Named`, `@Resource(name)`, `@DependsOn`, `getBean("…")`, XML references, SpEL `@bean` references; XML `class` attributes navigate to and complete project classes.
- **SpEL**: `#parameter` completion in `@Cacheable`/`@CachePut`/`@CacheEvict` keys and conditions, `@EventListener` conditions and `@PreAuthorize`-style expressions.
- **AspectJ pointcuts** in `@Pointcut`/`@Before`/`@After…`/`@Around`: designator completion, named pointcut completion and navigation (`inApp()`, `Aspects.inApp()`), bean names in `bean(…)`.
- **Links in the document index** (Go to Declaration): `@Profile`/`@ActiveProfiles`/`<beans profile>` to `application-<profile>.*`, `@ImportResource`/`@PropertySource`/`<import>` to a file of the same name, `@ComponentScan`/`<component-scan>` packages to a package declaration, `@Value("${key}")` to the key in `application*.properties/yml`. `META-INF/spring.handlers` classes and `spring.schemas` files are checked against the project.
- **Hover**: on a Java `@Bean` factory method name, the current source model's name, alias spellings and declared return type, with positive `@Primary`/`@Profile`/`@Qualifier` markers (method profiles override inherited class spellings). On a top-level concrete Java class identifier, its source bean name, stereotype and spelling. Foreign, shadowed, dynamic, generic/ambiguous or unsupported contexts stay quiet or omit unproved markers. The documentation provider shows name/type, aliases/profile/scope and documentation for source-owned declarations.
- **Source navigation chooser**: when a source line has several known injection, usage or related-file targets, the gutter opens a native picker; single-target arrows remain direct. The chooser rechecks the originating document/version, project and all candidate source snapshots before opening a target.

Bean/type, event publisher/listener, class/XML, override, profile/resource and package links are kept in a project-owned navigation graph, separate from the host's automatic Find Usages/Rename graph. Several targets are declaration-wide, zero-length, derived names or aliases; those rename anchors are deliberately gated, so no host Rename/Find Usages entries are published, and native language servers keep handling their supported refactorings. Candidate targets are refreshed from current project reads (including unsaved buffers); watched changes invalidate cached files. Supplemental navigation returns all known ambiguous targets.

## Checks

Run on open, save and typing, each group switchable in the settings:

- **Field injection** warning on `@Autowired`/`@Inject` fields, with a _Use constructor injection_ quick fix that adds the constructor, or extends the class's only one (not offered with several constructors, `@Autowired` setters or an unparsable body).
- **Bean inspections**: `@Transactional`/`@Async`/`@Cacheable`-style annotations on private, final or static methods, `@Scheduled` methods with parameters, bean names shared by two beans of the project (profiles and `@Conditional*` considered), private or final `@Bean` methods in proxied `@Configuration` classes, bean names starting with a capital, and (off by default) self-invocation of such annotated methods.
- **More inspections**: direct `@Bean` method calls in `@Configuration(proxyBeanMethods = false)` classes; unbalanced brackets in `@Cacheable` key/condition/unless and `key` with `keyGenerator`; `@Bean(initMethod/destroyMethod)`, `@TestBean(methodName)` and `<bean init-method>` naming a method the project class lacks (skipped for classes with library or Lombok-generated members, abstract types and types with project subtypes); `@Required` outside a setter; `@CacheConfig` on a class without cache annotations (Hint) or with both `cacheManager` and `cacheResolver`; self-invocation of overloads when every overload carries the annotation.
- **XML bean classes**: for a `<bean class>` naming a project class whose whole hierarchy is in the project: abstract classes, `factory-method` missing, not static or not public (Info), `lookup-method`/`replaced-method` naming a method that is missing or cannot be overridden, `constructor-arg` counts no constructor accepts, `@Required` setters left unset (Info, only with `<context:annotation-config>` or `component-scan`). `<property name>` completes and navigates to the setters. Also component-scan filter tables, missing `<import resource>` files (and imports that are not beans files, Info), unresolved `class` attributes (opt in, Info), component-scan filter expressions and inactive `<beans profile>` blocks (Hint, after Change Active Spring Profiles).
- **Unresolved references** (off by default, `springBeans.checkUnresolvedReferences`): a `@DependsOn("name")` no project bean defines, and a `@ComponentScan` package with no project source in it. Library and auto-configuration beans are invisible, so the check can over-report and stays off until enabled. When the project declares `@SpringBootApplication` or `@EnableAutoConfiguration`, a `@DependsOn` name is only reported if it is project-shaped (equals, ignoring case, the name of a project class or bean); a `@ComponentScan` is only checked for packages sharing their first two segments (`com.acme`) with project sources, and skipped with `basePackageClasses`, placeholders or computed values.
- **Opt-in references** (also `springBeans.checkUnresolvedReferences`): `@Lookup`, `@Scheduled(scheduler)`, `@MockitoBean` and `@MockitoSpyBean` names; relative `@ImportResource`/`@ContextConfiguration`/`@Sql` paths (relative to the class's package) and the default `<Class>-context.xml` of a bare `@ContextConfiguration` (Info); `classpath:` and absolute paths of `@ImportResource`, `@PropertySource`, `@ContextConfiguration`, `@Sql`, read from `src/main/resources` and `src/test/resources` (generated resources are invisible); in XML, `ref`/`parent`/`depends-on` (Info, quiet when the file imports others) and `${}` keys missing from the one `<context:property-placeholder>` classpath file.
- **Kotlin**: `open` made redundant by the kotlin-spring plugin (Hint), `pathVariable("x")` naming no route path of a router DSL file, and `getBean(Foo::class.java)` where `getBean<Foo>()` exists (Hint).

The inspections are text-based: annotations imported from a non-Spring package and computed names are left alone, and beans from libraries are not seen. Bean definition annotations require a canonical explicit import, an exact fully qualified annotation or a proven same-package owner; foreign, aliased, unresolved or source-shadowed annotations cannot manufacture bean targets, links, Primary selection, profile partitions, Resource/Named/Autowired facts or constructor-injection rewrites.

XML checks require a namespace-supported Spring `<beans>` root (default, prefixed and legacy no-namespace forms); reserved xml/xmlns rebindings, unbound/malformed qualified names, foreign roots and incomplete XML contexts return no Spring diagnostics. Core bean-reference attributes require their exact unqualified names (a foreign-prefixed `x:ref` is not a Spring reference), and the guard checks qualified names against XML 1.0 NCName rules and rejects duplicate expanded attribute names. Malformed namespace/structure, excessive depth/events or an oversized source removes that file's cached bean facts and marks the source context incomplete; ordinary non-Spring XML keeps source-file navigation but creates no bean declarations. No DTDs, entities or schemas are fetched.

## Tool window

The **Spring Beans** tool window (left) has three sections: Beans (grouped by kind, with typed source details for declarations, aliases, qualifiers, profiles and declared wiring inputs), Graph (who depends on whom) and Report (configuration problems); the choice is remembered. These facts do not represent a running Spring application context.

Rendering is cache-only and states are explicit: cold, incomplete-source, pending-scan and stale snapshots stay visibly distinct and cannot prove missing beans. Explicit configuration reports label incomplete inventories/checked-file caps instead of displaying a clean result. Reports are snapshots of their explicit Check/Dump command, bound to their source project; a foreign project cannot render another project's report, and closing a report's owner clears its cached report.

## Commands

- **Tools → Check Spring Configuration** runs the diagnostics over the indexed project files into the Report section.
- **Tools → Dump Spring Configuration** lists the project's beans there.
- **Tools → Change Active Spring Profiles** selects the profiles used for the inactive-profile checks.
- **Spring Beans: Reindex Project Sources** explicitly starts a new source pass; **Refresh** in the view advances pending work or restarts a completed/incomplete pass.

## Generation and refactoring

- **Generate Setter/Constructor Dependency/Autowired Dependency/Event Listener** show the full resulting Java source and exact project/path before one versioned atomic editor edit; dismissing any prompt leaves the source unchanged. A 12 KiB source bound and the shared 16 KiB confirmation bound prevent clipped review; uniform CRLF and Unicode source are preserved, and the versioned apply rejects intervening editor changes.
- **Introduce bean…** on an anonymous inner XML bean prompts for a validated unique bean ID (collision-free suggestion against inventoried names/aliases, local IDs, names and aliases), previews the complete resulting XML, and returns one host-managed atomic workspace edit; Cancel returns no edit. The supported subset is an existing, complete XML document up to 12 KiB with a Spring `<beans>` root and a nested anonymous bean as the sole content of a `<property>` or `<constructor-arg>`, with a literal ASCII qualified class name. Nested `<beans>`/profile scopes, inherited local `xml:*` attributes, local namespace rebindings, alias-bearing inner beans, dynamic classes, DTDs, entity references, malformed XML and over 2,048 XML events are refused. Element prefixes, namespace declarations, Unicode and LF/CRLF are preserved; the holder becomes the same namespace-qualified element with a plain `ref`.
- **Create XML bean for literal reference…** at a plain literal `ref` attribute on a self-closing Spring XML property or constructor-arg prompts for an explicit qualified class spelling and reviews the resulting XML. One same-file FileAction adds a new root-level bean ID matching that reference; it never replaces the reference or another definition. Known current-document IDs and aliases suppress the offer; a fresh complete project source inventory is checked before and after review (library/ignored/runtime beans remain unknown). The same namespace validator, 12 KiB/2,048-event bounds, 512-byte paths, literal ASCII qualified-name subset and exact preview cap apply.
- **Move annotation string to property…** (editor intention; the _Move Annotation String to Property_ menu command directs to it) offers destination/key prompts and a complete two-file preview, then returns both the Java and properties edits as one host workspace operation. It accepts one unescaped literal annotation argument (up to 2 KiB), POSIX project-owned Java and existing properties paths (up to 512 bytes), each source up to 8 KiB, and a new bounded ASCII property key. Arrays, concatenations, placeholders, escaped Java strings, duplicate keys, escaped property keys, continued lines, BOM/control text and mixed/CR-only line endings are refused; existing value escapes remain supported and new spaces/non-ASCII values receive properties escapes. No new file is created.

Action tickets are bounded to eight pending entries, single-use, and source/root/version-owned; stale or forged tokens fail. The offered/supplied document path, text, version and exact project owner are validated at resolution, and project-visible source and owner are re-read after confirmation. Exact previews refuse content that would exceed the host's confirmation limit instead of clipping. Final prepare/apply conflict checks, atomic application and native Undo are owned by the host.

## Templates

- **File templates** (New menu): Spring Component, Service, Configuration, Boot Application, Boot Test and Configuration Properties, in Java and Kotlin, plus three beans XML files (plain, with component scan, with AOP).
- **Live templates** (`sbean`, …) complete by abbreviation in Java, Kotlin and XML. Scope, profile and `constructor-arg name` values complete.

## SpEL

A `spel` language with no filename/extension associations and a bounded lexical highlighter. Canonically owned Java/Kotlin `@Value` template bodies, cache/event expressions and `ConditionalOnExpression` values publish exact physical SpEL ranges, as do supported Spring XML property/constructor/shortcut expressions with known namespace and raw attribute bytes. Java/Kotlin escapes, concatenation, text blocks, Kotlin string templates and entity-decoded XML are withheld rather than remapped incorrectly. The embedded language offers keyword and standard root/this context-variable completion. Native embedded colouring is currently blocked: the pinned host's embedded-colour path loads only built-in CodeMirror grammars and does not call a plugin SyntaxProvider for injected spans. No runtime evaluation, inferred member types, embedded language server or expression-aware refactoring.

## How it works

Matching is text-based, by simple type name. Beans from libraries and auto-configuration aren't seen, and conditions (`@ConditionalOn…`) are ignored (except the literal inactive-bean cases above), so the gutter may list beans that would not be active.

Live checks use the diagnostics provider for Java, Kotlin and Spring XML on the supplied unsaved text and cached facts for the owning project; opening a diagnostic request discovers no files, reads no build/resource files and rebuilds nothing. Without project access they use the current document. The current-document limit is 512 KiB, the cached context limit is 512 files / 4,096 model entries (then current-document fallback), and at most 512 diagnostics are returned. Lifecycle/member checks run only with a complete cached context and an established source type/member chain; cross-module duplicate-name checks, build-dependent AspectJ/Kotlin checks, missing/imported resources, unresolved bean names/placeholders and Spring metadata checks stay in **Check Spring Configuration**.

Beans/Graph rendering and completion/definition/gutter providers consume a project-owned bounded source cache: they do not discover files or read other editors on render. Navigation callbacks refuse sources over 128 KiB and fall back to current-document facts when the cached context exceeds 512 files. Activation warms at most eight open roots within a cooperative 100 ms budget; file open/save and filtered filesystem batches update source facts. Read failure or an oversized source removes that file's stale facts and makes absence checks incomplete; overflow/time-exhausted batches drop the cache rather than keeping truncated facts. Limits: eight retained roots, 4,096 inventoried files per root, 512 KiB per source, an 8 MiB cumulative budget and 256 paths/events per lifecycle batch. With optional `storage`, a versioned project-root inventory (paths only, at most eight roots / 4,096 paths per root / 256 KiB encoded) persists across restarts and is re-read before use; denial quietly falls back to the in-memory cache. The XML source boundary is 512 KiB, depth 128 and 65,536 XML events.

Required capabilities: `project.read`, `editor.read`; optional: `events`, `editor.write`, `storage`. Degraded optional grants keep the plugin working without persistence or editor writes.

## Not implemented

- PSI/UAST inspection lifecycle, version-aware module model, runtime bean context or automatic external source/type resolution.
- Renaming bean anchors through host Find Usages/Rename (spellings and ownership are not all interchangeable).
- Classpath resolution, duplicate simple types and effective bean activation in the generation commands; typed DOM/template generation, class selection, autowiring and runtime validation in XML generation.
- Creating new bean/configuration files, namespace-aware arbitrary child generation and broader generation coverage.
- Hard-coded-property externalization for typed Spring XML and resolved placeholder-configurer resources.
- Java Setter/Constructor/Autowired/Event Listener generation for Kotlin; hover for Kotlin/XML references, meta-annotation inheritance and full bean documentation.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
