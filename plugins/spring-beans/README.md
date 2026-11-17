# Spring Beans Navigator

Find and follow Spring beans in Java, Kotlin and Spring XML files. Jump from an injection point to the bean Spring would inject, see where a bean is used, and catch common wiring mistakes as you type. MIT; see `LICENSE`.

## Requirements

- OpenIDEA with plugin API 1.9 or later (`engines.openidea = "^1.9"`).
- A project with Java, Kotlin or Spring XML sources. No build tool is needed: the plugin reads your source files.

## Quick start

1. Install the plugin ([how](../../README.md#install-a-plugin)).
2. Open a Spring project and open a class that injects a dependency.
3. Click the gutter arrow next to a constructor parameter or `@Autowired` field. It opens the bean Spring would inject.
4. Open the **Spring Beans** tool window (left side bar) to browse every bean. If it says the cache is cold, click **Refresh**.
5. Run **Check Spring Configuration** from the Tools menu to list configuration problems in the **Report** section.

## Features

### Find the bean Spring would inject

- A gutter arrow on each injection point opens the matching bean.
- Go to Definition on the injected type lists every candidate.
- Candidates are matched by type, then narrowed by `@Qualifier`, `@Named`, `@Resource(name)`, `@Primary` and the parameter or field name.
- `List<T>`, `Map<String, T>` and arrays take every `T` bean.
- `Optional<T>`, `ObjectProvider<T>`, `Provider<T>`, `ObjectFactory<T>` and `SortedSet<T>` take a `T` bean.
- When a line has several targets, the gutter opens a picker. With one target, the arrow jumps straight there.

Injection points covered:

- Constructor parameters of beans.
- `@Autowired`, `@Inject` and `@Resource` fields and setters.
- `final` fields filled by a Lombok constructor.
- `@Bean` method parameters.
- Kotlin primary and secondary constructors, and `lateinit` properties.
- XML `ref`, `parent`, `depends-on` and `factory-bean`, plus `p:`/`c:` `*-ref` attributes.

### See where a bean is used

- A gutter icon on each bean lists the injection points that receive it.
- Gutter markers also link event publishers to listeners, classes to their XML beans, and `@ComponentScan`/`@Bean` overrides.

### Which beans are found

- Stereotype classes such as `@Service("name")` and `@Component`.
- `@Bean` methods, with their names and aliases.
- Spring Data repository interfaces.
- XML `<bean>` definitions and `<alias>` entries.
- `@Primary`, `@Profile`, `<beans profile>` and `@Qualifier` are taken into account.

Some beans are shown as **inactive (condition)** and dropped from injection candidates. This happens only when a condition is clearly false from the source:

- a literal `@ConditionalOnProperty`;
- a `@Profile` against a single `spring.profiles.active` value;
- a `@ConditionalOnMissingBean` when a plain project component of that type exists.

If the plugin can't decide a condition, the bean stays a candidate.

### Complete and navigate bean names

- Bean names complete and navigate in `@Qualifier`, `@Named`, `@Resource(name)`, `@DependsOn`, `getBean("…")`, XML references and SpEL `@bean` references.
- XML `class` attributes complete and navigate to project classes.
- `<property name>` completes and navigates to the setter.
- Scope, profile and `constructor-arg name` values complete.

### SpEL and AspectJ

- `#parameter` completion in `@Cacheable`, `@CachePut` and `@CacheEvict` keys and conditions, `@EventListener` conditions and `@PreAuthorize`-style expressions.
- SpEL keyword and context-variable completion inside `@Value`, cache and event expressions, `@ConditionalOnExpression` and Spring XML expressions.
- In `@Pointcut`, `@Before`, `@After…` and `@Around`: designator completion, named pointcut completion and navigation (`inApp()`, `Aspects.inApp()`), and bean names in `bean(…)`.

### Jump to related files

Go to Declaration follows these links:

- `@Profile`, `@ActiveProfiles` and `<beans profile>` to `application-<profile>.*`.
- `@ImportResource`, `@PropertySource` and `<import>` to the file of that name.
- `@ComponentScan` and `<component-scan>` packages to a package declaration.
- `@Value("${key}")` to the key in `application*.properties` or `.yml`.

Classes in `META-INF/spring.handlers` and files in `spring.schemas` are checked against the project.

### Hover

- On a Java `@Bean` method name: bean name, aliases, return type and any `@Primary`, `@Profile` or `@Qualifier`.
- On a concrete Java class name: its bean name and stereotype.
- Documentation shows name, type, aliases, profile, scope and the declaration's documentation.

### Catch mistakes as you type

Checks run when you open, save or edit a file. You can switch each group off in the settings.

- **Field injection.** `@Autowired` and `@Inject` fields get a warning. ⌥⏎ › **Use constructor injection** adds a constructor, or extends the only existing one.
- **Proxy pitfalls.** `@Transactional`, `@Async`, `@Cacheable` and similar on private, final or static methods. Self-invocation of such methods (off by default).
- **Bean definitions.** Bean names shared by two beans (profiles and `@Conditional*` considered), bean names starting with a capital, private or final `@Bean` methods in proxied `@Configuration` classes, and direct `@Bean` calls when `proxyBeanMethods = false`.
- **Lifecycle methods.** `@Bean(initMethod/destroyMethod)`, `@TestBean(methodName)` and `<bean init-method>` naming a method the class lacks.
- **Scheduling and async.** `@Scheduled` methods with parameters, and `@Async` methods returning a value that isn't `void` or a `Future`.
- **Caching.** Missing cache names, unbalanced SpEL brackets, `key` with `keyGenerator`, and misused `@CacheConfig`.
- **Injection annotations.** Injection on static members, conflicting `@Autowired` constructors, `@Required` outside a setter.
- **Profiles.** Malformed `@Profile` expressions.
- **AOP.** Incomplete AOP XML configuration and pointcut designators Spring AOP doesn't support.
- **Spring XML.** Bean class problems, component-scan filters, missing `<import resource>` files and inactive `<beans profile>` blocks.
- **Kotlin.** `open` made redundant by the kotlin-spring plugin, `pathVariable("x")` naming no route path, and `getBean(Foo::class.java)` where `getBean<Foo>()` exists.
- **Unresolved references** (off by default). Names and files no project source defines. Turn it on with **Warn about names and files no project source defines…** in the settings. It may over-report, because beans from libraries are invisible.

### Browse beans in the Spring Beans tool window

The **Spring Beans** tool window (left side bar) has three sections. It remembers the one you last used.

- **Beans**: every bean, grouped by kind, with its aliases, qualifiers, profiles and declared inputs.
- **Graph**: who depends on whom.
- **Report**: results of **Check Spring Configuration** or **Dump Spring Configuration**.

These are facts read from your source files, not from a running application.

Click **Refresh** to index the project or continue a scan. Run **Spring Beans: Reindex Project Sources** to start over.

### Choose active profiles

Run **Change Active Spring Profiles** (Tools menu). Pick profiles found in `@Profile`, `<beans profile>` and `application-*.yml`/`.properties` files. The duplicate-name and inactive-profile checks then use your choice. Choosing none means every bean is considered.

### Generate code

Right-click in a Java editor to:

- **Generate Setter Dependency**
- **Generate Constructor Dependency**
- **Generate Autowired Dependency**
- **Generate Event Listener**

Each command shows the full resulting source before it changes anything. Dismiss the preview to leave the file unchanged. These need the `editor.write` permission.

### Refactor Spring XML and annotations

From ⌥⏎ in the editor:

- **Introduce bean…** turns an anonymous inner XML bean into a named top-level bean. You pick the ID; the plugin suggests a unique one and previews the result.
- **Create XML bean for literal reference…** adds a new bean for a `ref` that points nowhere. You type the class name and review the XML first.
- **Move annotation string to property…** moves a literal annotation string into an existing `.properties` file. You pick the file and key, then review both files before they change.

The **Move Annotation String to Property** menu command only points you to the ⌥⏎ action.

### Templates

- **File templates** (New menu): **Spring Component**, **Spring Service**, **Spring Configuration**, **Spring Boot Application**, **Spring Boot Test** and **Spring Configuration Properties**, in Java and Kotlin. Also **Spring Config (XML)**, **Spring Config with Component Scan (XML)** and **Spring Config with AOP (XML)**.
- **Live templates**: type an abbreviation and pick it from completion.

| Abbreviation | Java | Kotlin | XML |
|---|---|---|---|
| `scomp` | `@Component` class | `@Component` class | |
| `sserv` | `@Service` class | `@Service` class | |
| `sconf` | `@Configuration` class | | |
| `sbean` | `@Bean` method | `@Bean` function | `<bean>` with class |
| `sauto` | `@Autowired` field | `@Autowired` property | |
| `sval` | `@Value` field | `@Value` property | |
| `sprops` | `@ConfigurationProperties` class | | |
| `slisten` | `@EventListener` method | `@EventListener` function | |
| `ssched` | `@Scheduled` method | | |
| `stest` | `@SpringBootTest` class | | |
| `sprop` | | | `<property>` with value |
| `spropref` | | | `<property>` with ref |
| `sarg` | | | `<constructor-arg>` |
| `simport` | | | `<import resource>` |
| `sscan` | | | `<context:component-scan>` |

## Commands

Find them in Search Everywhere › Actions (⇧⇧) under **Spring**; bind keys in Settings › Keymap. No shortcuts are set by default.

| Command | What it does | Where |
|---|---|---|
| **Check Spring Configuration** | Checks the indexed project files and lists problems in the **Report** section. | Tools menu |
| **Dump Spring Configuration** | Lists the project's beans in the **Report** section. | Tools menu |
| **Change Active Spring Profiles** | Picks the profiles used by the profile checks. | Tools menu |
| **Generate Setter Dependency** | Adds a setter-injected dependency to the Java class. | Java editor context menu |
| **Generate Constructor Dependency** | Adds a constructor-injected dependency. | Java editor context menu |
| **Generate Autowired Dependency** | Adds an `@Autowired` dependency. | Java editor context menu |
| **Generate Event Listener** | Adds an event listener method. | Java editor context menu |
| **Move Annotation String to Property** | Tells you to use ⌥⏎ › **Move annotation string to property…** at the caret. Changes nothing. | Java editor context menu |
| **Spring Beans: Reindex Project Sources** | Starts a fresh scan of the project's sources. | Search Everywhere |
| **Spring: Choose Source Gutter Target** | Opens the target picker. Used by gutter icons with several targets; not meant to be run by hand. | Gutter icons |

## Settings

Settings › Plugins › Spring Beans. Every setting is an on/off switch.

| Setting | Key | Default | What it changes |
|---|---|---|---|
| Warn when @Transactional, @Async, @Cacheable and similar annotations sit on private, final or static methods | `springBeans.checkProxyMethods` | on | Proxy pitfall warnings. |
| Warn when a @Scheduled method declares parameters | `springBeans.checkScheduled` | on | `@Scheduled` check. |
| Warn when two beans of the project share a name | `springBeans.checkDuplicateNames` | on | Duplicate bean name check. |
| Warn about private or final @Bean methods in proxied @Configuration classes, … | `springBeans.checkConfiguration` | on | `@Configuration` and lifecycle method checks. |
| Hint when an explicit bean name starts with a capital letter | `springBeans.checkNaming` | on | Naming hint. |
| Warn when a method calls a @Transactional, @Async or @Cacheable method of its own class … | `springBeans.checkSelfInvocation` | off | Self-invocation warning. Text-based, so it may miss or over-report. |
| Warn about names and files no project source defines … | `springBeans.checkUnresolvedReferences` | off | Unresolved names and file paths. May over-report. |
| Report incomplete AOP configuration … | `springBeans.checkAop` | on | AOP XML and pointcut checks. |
| Report @Autowired, @Value, @Inject and @Resource on static members, … | `springBeans.checkInjectionAnnotations` | on | Injection annotation checks. |
| Report malformed @Profile expressions | `springBeans.checkProfiles` | on | `@Profile` syntax check. |
| Report @Async methods that return a value instead of void or a Future | `springBeans.checkAsync` | on | `@Async` return type check. |
| Report cache annotations without a cache name, … | `springBeans.checkCache` | on | Cache annotation checks. |
| Kotlin: missing kotlin-spring plugin, redundant open modifiers … | `springBeans.checkKotlinSpringPlugin` | on | Kotlin hints. |
| Check Spring XML: bean attributes, values, scopes, … | `springBeans.checkXml` | on | Spring XML checks. |

Long titles are shortened here with "…"; the full text is in Settings and in `settings.schema.json`.

## Permissions

Optional permissions are off until you allow them in Settings › Plugins › Spring Beans.

| Permission | Used for | Without it |
|---|---|---|
| `events` | Updating bean facts when files change on disk. | Facts update only when you open or save a file, or click **Refresh**. |
| `editor.write` | The **Generate …** commands writing code into the editor. | The commands show a warning and add nothing. |
| `storage` | Remembering active profiles and the list of project files across restarts. | **Change Active Spring Profiles** can't save your choice. The file list is rebuilt in memory after each restart. |

## Troubleshooting

See also the [repository guide](../../README.md#troubleshooting).

| Problem | What to do |
|---|---|
| **Spring Beans** says "Source cache cold; Refresh to index this project." | Click **Refresh**. |
| It says the scan is pending, stale or incomplete | Click **Refresh** again until it says the cache is ready. Large projects take several passes. |
| It says "Source files unavailable/oversized/unsupported XML or source cache budget reached" | Some files are too large or are not valid Spring XML. Other beans still show. See the limits under Technical details. |
| "Project identity is unavailable. Allow project.read…" | Check that the plugin is enabled and has its required permissions in Settings › Plugins › Spring Beans. |
| **Report** says "Run Check Spring Configuration or Dump Spring Configuration…" | Run one of those commands from the Tools menu. Reports belong to the project they ran on. |
| A result says "partial source check" | The scan hadn't finished. Click **Refresh** in **Spring Beans**, then run the check again. |
| "Allow editor changes for Spring Beans in Settings › Plugins to add code." | Allow `editor.write` in Settings › Plugins › Spring Beans. |
| **Change Active Spring Profiles** says "No profiles found…" | Add a `@Profile`, a `<beans profile>` or an `application-<profile>.yml`/`.properties` file. |
| **Change Active Spring Profiles** fails to save | Allow `storage` in Settings › Plugins › Spring Beans. |
| A **Generate …** command says "Open a Java class first." or "The open file has no class to add to." | Put the caret in a Java class inside the project. |
| ⌥⏎ › **Use constructor injection** isn't offered | It isn't offered when the class has several constructors, `@Autowired` setters or a body the plugin can't parse. |
| The gutter lists a bean that wouldn't be active | Beans from libraries and most `@ConditionalOn…` conditions aren't evaluated. Check the bean's conditions yourself. |
| An unresolved-reference warning is wrong | The bean likely comes from a library. Turn off **Warn about names and files no project source defines…**. |

## Limitations

- Beans from libraries and auto-configuration aren't seen.
- `@ConditionalOn…` conditions are ignored, except the clearly false cases listed above.
- Matching is text-based, by simple type name. No classpath resolution.
- No running application context, and no runtime SpEL evaluation.
- Rename and Find Usages of bean names aren't supported.
- The **Generate …** commands work in Java only, not Kotlin.
- Hover works on Java only, not Kotlin or XML references.
- No new bean or configuration files are created by refactorings.
- SpEL inside annotations isn't coloured yet.

## Technical details

<details>
<summary>Limits, caching and safety checks</summary>

### How matching works

Matching is text-based, by simple type name, through the project's type hierarchy across files. A Kotlin `@Bean fun x() = Foo(…)` is typed `Foo`. XML `p:`/`c:` `*-ref` attributes are followed like `ref` when their prefixes are bound to the Spring `p`/`c` namespaces. Supplemental navigation returns all known ambiguous targets.

The inspections are text-based: annotations imported from a non-Spring package and computed names are left alone, and beans from libraries are not seen. Bean definition annotations require a canonical explicit import, an exact fully qualified annotation or a proven same-package owner. Foreign, aliased, unresolved or source-shadowed annotations cannot manufacture bean targets, links, Primary selection, profile partitions, Resource/Named/Autowired facts or constructor-injection rewrites.

### Hover details

On a Java `@Bean` factory method name, hover shows the current source model's name, alias spellings and declared return type, with positive `@Primary`/`@Profile`/`@Qualifier` markers (method profiles override inherited class spellings). On a top-level concrete Java class identifier, it shows the source bean name, stereotype and spelling. Foreign, shadowed, dynamic, generic/ambiguous or unsupported contexts stay quiet or omit unproved markers. The documentation provider shows name/type, aliases/profile/scope and documentation for source-owned declarations.

### Navigation graph

Bean/type, event publisher/listener, class/XML, override, profile/resource and package links are kept in a project-owned navigation graph, separate from the host's automatic Find Usages/Rename graph. Several targets are declaration-wide, zero-length, derived names or aliases; those rename anchors are deliberately gated, so no host Rename/Find Usages entries are published, and native language servers keep handling their supported refactorings. Candidate targets are refreshed from current project reads (including unsaved buffers); watched changes invalidate cached files. Project-wide Go to Declaration and gutter markers also come from the host's document index (bean names, aliases, qualifiers, injection points by type), without the provider time budget.

The source navigation chooser rechecks the originating document/version, project and all candidate source snapshots before opening a target.

### Check details

- **Use constructor injection** is not offered with several constructors, `@Autowired` setters or an unparsable body.
- **Lifecycle methods**: init/destroy method checks are skipped for classes with library or Lombok-generated members, abstract types and types with project subtypes.
- **Caching**: unbalanced brackets in `@Cacheable` key/condition/unless; `key` with `keyGenerator`; `@CacheConfig` on a class without cache annotations (Hint) or with both `cacheManager` and `cacheResolver`; cache annotations on interfaces; combined `@Cacheable` and `@CachePut`.
- **Self-invocation of overloads** is reported when every overload carries the annotation.
- **XML bean classes**, for a `<bean class>` naming a project class whose whole hierarchy is in the project: abstract classes; `factory-method` missing, not static or not public (Info); `lookup-method`/`replaced-method` naming a method that is missing or cannot be overridden; `constructor-arg` counts no constructor accepts; `@Required` setters left unset (Info, only with `<context:annotation-config>` or `component-scan`). Also component-scan filter tables, missing `<import resource>` files (and imports that are not beans files, Info), unresolved `class` attributes (opt in, Info), component-scan filter expressions and inactive `<beans profile>` blocks (Hint, after **Change Active Spring Profiles**).
- **Kotlin**: redundant `open` is a Hint; `getBean<Foo>()` replacement is a Hint.
- **Unresolved references** (`springBeans.checkUnresolvedReferences`, off by default): a `@DependsOn("name")` no project bean defines, and a `@ComponentScan` package with no project source in it. Library and auto-configuration beans are invisible, so the check can over-report. When the project declares `@SpringBootApplication` or `@EnableAutoConfiguration`, a `@DependsOn` name is only reported if it is project-shaped (equals, ignoring case, the name of a project class or bean). A `@ComponentScan` is only checked for packages sharing their first two segments (`com.acme`) with project sources, and skipped with `basePackageClasses`, placeholders or computed values.
- **Opt-in references** (same setting): `@Lookup`, `@Scheduled(scheduler)`, `@MockitoBean` and `@MockitoSpyBean` names; relative `@ImportResource`/`@ContextConfiguration`/`@Sql` paths (relative to the class's package) and the default `<Class>-context.xml` of a bare `@ContextConfiguration` (Info); `classpath:` and absolute paths of `@ImportResource`, `@PropertySource`, `@ContextConfiguration`, `@Sql`, read from `src/main/resources` and `src/test/resources` (generated resources are invisible); in XML, `ref`/`parent`/`depends-on` (Info, quiet when the file imports others) and `${}` keys missing from the one `<context:property-placeholder>` classpath file.

### XML parsing

XML checks require a namespace-supported Spring `<beans>` root (default, prefixed and legacy no-namespace forms). Reserved xml/xmlns rebindings, unbound/malformed qualified names, foreign roots and incomplete XML contexts return no Spring diagnostics. Core bean-reference attributes require their exact unqualified names (a foreign-prefixed `x:ref` is not a Spring reference). The guard checks qualified names against XML 1.0 NCName rules and rejects duplicate expanded attribute names. Malformed namespace/structure, excessive depth/events or an oversized source removes that file's cached bean facts and marks the source context incomplete. Ordinary non-Spring XML keeps source-file navigation but creates no bean declarations. No DTDs, entities or schemas are fetched. The XML source boundary is 512 KiB, depth 128 and 65,536 XML events.

### Live checks

Live checks use the diagnostics provider for Java, Kotlin and Spring XML on the supplied unsaved text and cached facts for the owning project. Opening a diagnostic request discovers no files, reads no build/resource files and rebuilds nothing. Without project access they use the current document. The current-document limit is 512 KiB, the cached context limit is 512 files / 4,096 model entries (then current-document fallback), and at most 512 diagnostics are returned. Lifecycle/member checks run only with a complete cached context and an established source type/member chain. Cross-module duplicate-name checks, build-dependent AspectJ/Kotlin checks, missing/imported resources, unresolved bean names/placeholders and Spring metadata checks stay in **Check Spring Configuration**.

### Source cache

Beans/Graph rendering and completion/definition/gutter providers consume a project-owned bounded source cache: they do not discover files or read other editors on render. Navigation callbacks refuse sources over 128 KiB and fall back to current-document facts when the cached context exceeds 512 files. Activation warms at most eight open roots within a cooperative 100 ms budget; file open/save and filtered filesystem batches update source facts. Read failure or an oversized source removes that file's stale facts and makes absence checks incomplete; overflow/time-exhausted batches drop the cache rather than keeping truncated facts.

Limits: eight retained roots, 4,096 inventoried files per root, 512 KiB per source, an 8 MiB cumulative budget and 256 paths/events per lifecycle batch. With optional `storage`, a versioned project-root inventory (paths only, at most eight roots / 4,096 paths per root / 256 KiB encoded) persists across restarts and is re-read before use; denial quietly falls back to the in-memory cache.

### Tool window states

Rendering is cache-only and states are explicit: cold, incomplete-source, pending-scan and stale snapshots stay visibly distinct and cannot prove missing beans. Explicit configuration reports label incomplete inventories/checked-file caps instead of displaying a clean result. Reports are snapshots of their explicit Check/Dump command, bound to their source project; a foreign project cannot render another project's report, and closing a report's owner clears its cached report. **Refresh** in the view advances pending work or restarts a completed/incomplete pass.

### Generation and refactoring safety

- **Generate Setter/Constructor Dependency/Autowired Dependency/Event Listener** show the full resulting Java source and exact project/path before one versioned atomic editor edit; dismissing any prompt leaves the source unchanged. A 12 KiB source bound and the shared 16 KiB confirmation bound prevent clipped review. Uniform CRLF and Unicode source are preserved, and the versioned apply rejects intervening editor changes.
- **Introduce bean…** prompts for a validated unique bean ID (collision-free suggestion against inventoried names/aliases, local IDs, names and aliases), previews the complete resulting XML, and returns one host-managed atomic workspace edit; Cancel returns no edit. Supported subset: an existing, complete XML document up to 12 KiB with a Spring `<beans>` root and a nested anonymous bean as the sole content of a `<property>` or `<constructor-arg>`, with a literal ASCII qualified class name. Nested `<beans>`/profile scopes, inherited local `xml:*` attributes, local namespace rebindings, alias-bearing inner beans, dynamic classes, DTDs, entity references, malformed XML and over 2,048 XML events are refused. Element prefixes, namespace declarations, Unicode and LF/CRLF are preserved; the holder becomes the same namespace-qualified element with a plain `ref`.
- **Create XML bean for literal reference…** works at a plain literal `ref` attribute on a self-closing Spring XML property or constructor-arg. It prompts for an explicit qualified class spelling and reviews the resulting XML. One same-file FileAction adds a new root-level bean ID matching that reference; it never replaces the reference or another definition. Known current-document IDs and aliases suppress the offer. A fresh complete project source inventory is checked before and after review (library/ignored/runtime beans remain unknown). The same namespace validator, 12 KiB/2,048-event bounds, 512-byte paths, literal ASCII qualified-name subset and exact preview cap apply.
- **Move annotation string to property…** offers destination/key prompts and a complete two-file preview, then returns both the Java and properties edits as one host workspace operation. It accepts one unescaped literal annotation argument (up to 2 KiB), POSIX project-owned Java and existing properties paths (up to 512 bytes), each source up to 8 KiB, and a new bounded ASCII property key. Arrays, concatenations, placeholders, escaped Java strings, duplicate keys, escaped property keys, continued lines, BOM/control text and mixed/CR-only line endings are refused. Existing value escapes remain supported and new spaces/non-ASCII values receive properties escapes. No new file is created.

Action tickets are bounded to eight pending entries, single-use, and source/root/version-owned; stale or forged tokens fail. The offered/supplied document path, text, version and exact project owner are validated at resolution, and project-visible source and owner are re-read after confirmation. Exact previews refuse content that would exceed the host's confirmation limit instead of clipping. Final prepare/apply conflict checks, atomic application and native Undo are owned by the host.

### SpEL language

A `spel` language with no filename/extension associations and a bounded lexical highlighter. Canonically owned Java/Kotlin `@Value` template bodies, cache/event expressions and `ConditionalOnExpression` values publish exact physical SpEL ranges, as do supported Spring XML property/constructor/shortcut expressions with known namespace and raw attribute bytes. Java/Kotlin escapes, concatenation, text blocks, Kotlin string templates and entity-decoded XML are withheld rather than remapped incorrectly. The embedded language offers keyword and standard root/this context-variable completion. Native embedded colouring is currently blocked: the pinned host's embedded-colour path loads only built-in CodeMirror grammars and does not call a plugin SyntaxProvider for injected spans. No runtime evaluation, inferred member types, embedded language server or expression-aware refactoring.

### Capabilities

Required: `project.read`, `editor.read`. Optional: `events`, `editor.write`, `storage`. Degraded optional grants keep the plugin working without persistence or editor writes.

### Not implemented

- PSI/UAST inspection lifecycle, version-aware module model, runtime bean context or automatic external source/type resolution.
- Renaming bean anchors through host Find Usages/Rename (spellings and ownership are not all interchangeable).
- Classpath resolution, duplicate simple types and effective bean activation in the generation commands; typed DOM/template generation, class selection, autowiring and runtime validation in XML generation.
- Creating new bean/configuration files, namespace-aware arbitrary child generation and broader generation coverage.
- Hard-coded-property externalization for typed Spring XML and resolved placeholder-configurer resources.
- Java Setter/Constructor/Autowired/Event Listener generation for Kotlin; hover for Kotlin/XML references, meta-annotation inheritance and full bean documentation.

</details>

## Development

    cargo test
    openidea-plugin build
    openidea-plugin link
