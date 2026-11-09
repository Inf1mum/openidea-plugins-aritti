# Spring Beans Navigator

Spring bean support for Java, Kotlin and Spring XML files in OpenIDEA:

- **Beans**: stereotype classes (`@Service("name")`…), `@Bean` methods (names and aliases), Spring Data repository interfaces, XML `<bean>` definitions and `<alias>`es, with `@Primary`, `@Profile` (and `<beans profile>`) and `@Qualifier`.
- **Injection navigation**: a gutter arrow on each injection point opens the bean Spring would inject, and Go to Definition on the type lists the candidates. Candidates are matched by type through the project's type hierarchy, then narrowed by `@Qualifier`/`@Named`/`@Resource(name)`, `@Primary` and the parameter or field name. `List<T>`, `Map<String, T>` and arrays take every `T`; `Optional<T>`, `ObjectProvider<T>` and `Provider<T>` take a `T`. Covered: constructor parameters of beans, `@Autowired`/`@Inject`/`@Resource` fields and setters, Lombok-constructor `final` fields, `@Bean` parameters, Kotlin primary and secondary constructors and `lateinit` properties, XML `ref`/`parent`/`depends-on`/`factory-bean`.
- **Where a bean is injected**: a gutter icon on each bean lists the injection points that receive it.
- **Bean names in strings**: navigation and completion in `@Qualifier`, `@Named`, `@Resource(name)`, `@DependsOn`, `getBean("…")`, XML references, SpEL `@bean` references; XML `class` attributes navigate to and complete project classes.
- **SpEL**: `#parameter` completion in `@Cacheable`/`@CachePut`/`@CacheEvict` keys and conditions, `@EventListener` conditions and `@PreAuthorize`-style expressions.
- **AspectJ pointcuts** in `@Pointcut`/`@Before`/`@After…`/`@Around`: designator completion, named pointcut completion and navigation (`inApp()`, `Aspects.inApp()`), bean names in `bean(…)`.
- **Spring Beans** tool window (left) with three sections: Beans (grouped by kind), Graph (who depends on whom) and Report (configuration problems). The choice is remembered.
- **Field injection** warning on `@Autowired`/`@Inject` fields, with a _Use constructor injection_ quick fix that adds the constructor, or extends the class's only one (not offered with several constructors, `@Autowired` setters or an unparsable body). XML `p:`/`c:` `*-ref` attributes (prefixes bound to the Spring `p`/`c` namespaces) are followed like `ref`; a Kotlin `@Bean fun x() = Foo(…)` is typed `Foo`.
- **Bean inspections** on Java files, run on open, save and typing, each group switchable in the settings: `@Transactional`/`@Async`/`@Cacheable`-style annotations on private, final or static methods, `@Scheduled` methods with parameters, bean names shared by two beans of the project (profiles and `@Conditional*` considered), private or final `@Bean` methods in proxied `@Configuration` classes, bean names starting with a capital, and (off by default) self-invocation of such annotated methods. Text-based: annotations imported from a non-Spring package and computed names are left alone; beans from libraries are not seen.
- **Unresolved references** (off by default, `springBeans.checkUnresolvedReferences`): a `@DependsOn("name")` no project bean defines (Java beans, `@Bean` methods, XML `<bean>`s and aliases all count), and a `@ComponentScan` package with no project source in it. Beans from libraries and auto-configuration are invisible to the plugin, so the check can over-report and stays off until you enable it. To keep it quiet: when the project declares `@SpringBootApplication` or `@EnableAutoConfiguration`, a `@DependsOn` name is only reported if it is project-shaped, meaning it equals (ignoring case) the name of a project class or bean (a typo, or a class never made a bean); a `@ComponentScan` is only checked for packages sharing their first two segments (`com.acme`) with project sources, and is skipped with `basePackageClasses`, placeholders or computed values.
- **More inspections** (each under its group in the settings): direct `@Bean` method calls in `@Configuration(proxyBeanMethods = false)` classes; unbalanced brackets in `@Cacheable` key/condition/unless and `key` with `keyGenerator`; `@Bean(initMethod/destroyMethod)`, `@TestBean(methodName)` and `<bean init-method>` naming a method the project class lacks (skipped for classes with library or Lombok-generated members, abstract types and types with project subtypes); `@Required` outside a setter; `@CacheConfig` on a class without cache annotations (Hint) or with both `cacheManager` and `cacheResolver`; self-invocation of overloads when every overload carries the annotation.
- **XML bean classes**: for a `<bean class>` naming a project class whose whole hierarchy is in the project: abstract classes, `factory-method` missing, not static or not public (Info), `lookup-method`/`replaced-method` naming a method that is missing or cannot be overridden, `constructor-arg` counts no constructor accepts, `@Required` setters left unset (Info, only with `<context:annotation-config>` or `component-scan`). `<property name>` completes and navigates to the setters. Also component-scan filter tables, missing `<import resource>` files (and imports that are not beans files, Info), unresolved `class` attributes (opt in, Info), component-scan filter expressions and inactive `<beans profile>` blocks (Hint, after Change Active Spring Profiles).
- **Opt-in references** (`springBeans.checkUnresolvedReferences`): `@Lookup`, `@Scheduled(scheduler)`, `@MockitoBean` and `@MockitoSpyBean` names; relative `@ImportResource`/`@ContextConfiguration`/`@Sql` paths (relative to the class's package) and the default `<Class>-context.xml` of a bare `@ContextConfiguration` (Info); `classpath:` and absolute paths of `@ImportResource`, `@PropertySource`, `@ContextConfiguration`, `@Sql`, read from `src/main/resources` and `src/test/resources` (generated resources are invisible); in XML, `ref`/`parent`/`depends-on` (Info, quiet when the file imports others) and `${}` keys missing from the one `<context:property-placeholder>` classpath file.
- **Commands** (Tools menu): _Check Spring Configuration_ runs the diagnostics over the indexed project files into the Report section of the Spring Beans window, _Dump Spring Configuration_ lists the project's beans there, and _Move Annotation String to Property_ (editor menu) directs to the _Move annotation string to property…_ editor intention, which reviews and returns both Java and existing properties-file edits as one atomic workspace operation.
- **Links in the document index** (Go to Declaration): `@Profile`/`@ActiveProfiles`/`<beans profile>` to `application-<profile>.*`, `@ImportResource`/`@PropertySource`/`<import>` to a file of the same name, `@ComponentScan`/`<component-scan>` packages to a package declaration, `@Value("${key}")` to the key in `application*.properties/yml`. `META-INF/spring.handlers` classes and `spring.schemas` files are checked against the project.
- **File templates** (New menu): Spring Component, Service, Configuration, Boot Application, Boot Test and Configuration Properties, in Java and Kotlin and three beans XML files (plain, with component scan, with AOP) (`$PACKAGE$`, `$NAME$` and `$END$` placeholders are not yet confirmed in the IDE). **Live templates** (`sbean`, ...) complete by abbreviation in Java, Kotlin and XML. Scope, profile and `constructor-arg name` values complete.
- **Kotlin**: `open` made redundant by the kotlin-spring plugin (Hint), `pathVariable("x")` names no route path of a router DSL file declares, and `getBean(Foo::class.java)` where `getBean<Foo>()` exists (Hint).

Endpoints, Spring Boot run configurations and `@Value` key navigation are built into the IDE.

Matching is text-based, by simple type name. Beans from libraries and auto-configuration aren't seen, and conditions (`@ConditionalOn…`) are ignored, so the gutter may list beans that would not be active.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

### Navigation-only relationships

Bean/type, event publisher/listener, class/XML, override, profile/resource and
package links are kept in a project-owned navigation graph, separate from the
host's automatic Find Usages/Rename graph. Several targets are declaration-wide,
zero-length filename targets, derived names or aliases; treating them as literal
rename edits would corrupt source. Those graph anchors are deliberately gated
until their exact spelling and semantic ownership are proven. Native language
servers continue to handle their supported refactorings.

Related-file Go to Definition and gutter navigation remain available. Existing
profile/qualifier/primary-aware injection resolution has priority over generic
relationship matching. Candidate navigation targets are refreshed from current
project reads (including unsaved buffers), and watched changes invalidate cached
files. Supplemental navigation is bounded and returns all known ambiguous targets;
it does not yet reproduce native detected-module preference or complete effective
Spring-context semantics.

The packaged Beans view shows typed source details for declarations, aliases, qualifiers, profiles and declared wiring inputs. It reports incomplete source indexes explicitly; these facts do not represent a running Spring application context.


Live checks use API 1.3 `DiagnosticsProvider` for Java, Kotlin and Spring XML.
They analyze the supplied unsaved text and cached facts for its owning project;
opening a diagnostic request does not discover files, read build/resource files,
or rebuild the project. Without project access they use the current document.
The host publishes the returned results; gutter requests and open/save events do
not publish a second copy. The current-document limit is 512 KiB, the cached
context limit is 512 files / 4096 model entries (then current-document fallback),
and at most 512 diagnostics are returned.

The live subset preserves field-injection advice; Java proxy, scheduled,
configuration, naming, injection/required-target, profile, async and cache rules;
Kotlin router/reified-expression hints; and local XML structure, scope, value,
filter-regex and reference-shape rules. Lifecycle/member checks run only with a
complete cached context and an established source type/member chain. Missing
filter classes stay quiet while the source context is incomplete. Unknown library
hierarchies do not establish missing members. Cross-module duplicate-name checks,
build-dependent AspectJ and Kotlin compiler-plugin checks, missing/imported
resources, unresolved bean names/placeholders and Spring metadata checks remain
in **Check Spring Configuration**, rather than doing filesystem work during live
requests. The source-context status is shown in the Beans view, without an extra
warning on every source file. Existing code actions remain available.

This is source-text inspection, not the original plugin's PSI/UAST inspection
lifecycle, version-aware module model, runtime bean context or automatic external
source/type resolution. The inspected original `SpringAutowiredFieldsWarningInspection`
requires configured-module, Spring-version and mapped-bean facts that this plugin
does not reproduce; `DomSpringBeanInspectionBase` uses the original Spring model.
No new semantic rules or rename anchors are inferred from those class names.


### Atomic Spring XML bean extraction (P16.4 subset)

The existing **Introduce bean…** action on an anonymous inner XML bean now uses lazy API 1.3
FileActions instead of immediately available CodeAction edits. Selecting it resolves the current
source inventory, prompts for a validated unique bean ID (with a collision-free suggestion),
previews the complete exact resulting XML, and asks for confirmation before
returning one host-managed atomic workspace edit. Cancel returns no edit. The source file is the
only target: no files are created, deleted or overwritten, and no process/network/direct editor
write runs. The generated source ID avoids currently inventoried names/aliases and existing local
IDs; the inventory is freshly checked again after confirmation. Provider discovery itself performs
only bounded source parsing and project-owner metadata lookup.

The supported subset is an existing, complete XML document up to 12 KiB with a conventional
default-namespace or explicitly prefixed Spring `<beans>` root, a nested anonymous
bean as the sole content of a Spring `<property>` or `<constructor-arg>`, with a literal ASCII
qualified class name, and no nested `<beans>`/profile scopes, inherited local `xml:*` attributes or local
namespace rebindings. Alias-bearing inner beans, dynamic classes, DTDs, text entity references,
malformed/duplicate-attribute XML, undeclared prefixes and over 2,048 XML events are refused.
Root-declared prefixes remain usable without inferring a classpath. UTF-16 edit ranges, Unicode
content and LF/CRLF are preserved. Absolute source/root paths are limited to 512 bytes. The offered/supplied Document path, text, version and exact project owner are
validated at resolution; project-visible source text and owner are re-read after
confirmation. API1.3 provides no independent post-prompt current-version query
here; the host owns subsequent prepare/apply conflict checks. Action tokens are bounded to eight pending entries and
single-use; stale/forged tokens fail. Exact preview uses `confirm_exact`, refusing content that
would exceed the host's confirmation limit instead of presenting clipped output.

A fresh command-owned index (three-second scan budget, existing 10,000-path discovery sentinel)
suggests the ID and validates the chosen spelling; capped/unreadable/incomplete inventories refuse extraction rather than using an
absence guess. This leaves the navigation/diagnostics cache unchanged. SDK inventory/ignore rules
and runtime imports, library beans, compiled contexts and concurrent changes after the final check
remain outside this source-level collision proof. Existing-file application is delegated to the
host's atomic workspace edit and native Undo, not a plugin filesystem overwrite.

Original comparison: pinned `SpringIntroduceBeanIntention` and its before/after XML fixtures extract
a local definition, assign an ID and replace the holder with a reference, using typed DOM and a linked
ID template inside an IDE write action. This slice preserves that source transformation with an exact
preview instead of the original live template. `SpringBeanGenerateProvider` delegates typed DOM generation and
`BasicSpringDomGenerateProvider` resolves a Spring `Beans` root. This adaptation migrates the
already supported nested-bean extraction; it does not recreate typed DOM/template generation,
class selection, autowiring or runtime validation. Java Setter/Constructor/Autowired/Event Listener
commands, property migration's existing workflow, and p-namespace/schema quick fixes are unchanged.
Creating a new bean/configuration file, namespace-aware arbitrary child generation and broader
original generation parity remain open. No new screenshots/native acceptance are claimed.


### Project-owned bounded source cache (P16.1)

Beans/Graph rendering and completion/definition/gutter providers now consume cached source facts;
they no longer discover files, read other editors/files, or trigger a render/refresh scan loop. The
current callback document updates its own cached model. Navigation callbacks refuse source over
128 KiB and fall back to current-document facts when the cached context exceeds 512 files. Cross-file unsaved changes become visible
when their current-text provider callback or supported lifecycle event delivers them; this is not a
claim of a global unsaved-document stream. XML extraction keeps its separate fresh command-owned
collision inventory and exact preview guards.

Activation warms at most eight open roots within a cooperative 100 ms budget (25 ms per root).
FileOpened/FileSaved and filtered created/modified/removed filesystem batches update source facts
and advance bounded warm work, once per affected root. Java/Kotlin/XML/properties/YAML/SQL sources
are relevant; Maven/Gradle settings invalidate the affected root. Unrelated paths do not read or
cold-scan the project. Directory removal prunes cached descendants. Read failure or an oversized
source removes that file's stale facts, keeps unaffected positives and makes absence checks
incomplete. Closed roots are pruned; nested open projects use the host's innermost owner. Reports
are bound to their source project, and tree/graph/report activation validates actual cached row
membership and project ownership before opening a location.

With optional `storage`, a versioned project-root inventory persists across actor restarts: at
most eight roots / 4,096 paths per root / 256 KiB encoded total. It contains paths only, never
source text, beans, reports or credentials. Restoration occurs only during lifecycle/explicit
warming, rechecks the host's innermost project owner and re-reads every source before parsing.
The restored inventory remains incomplete: only a subsequent fresh discovery pass can establish
absence. Missing/deleted/unreadable files cannot restore stale facts; malformed/foreign/traversal,
duplicate, unsupported-version and over-budget inventories are refused. Build changes, observed
removal/overflow and closed-root lifecycle pruning replace/invalidate the persisted inventory.
Provider/render zero-budget paths neither restore nor read/write storage. Optional storage denial
quietly falls back to the bounded in-memory cache. Storage is an optimization, not a transactional
multi-instance source snapshot; unobserved changes still require lifecycle delivery or Refresh.

**Refresh** advances pending work, or restarts a completed/incomplete pass. **Spring Beans: Reindex
Project Sources** explicitly starts a new source pass. Limits are eight retained roots, 4,096
inventoried files per root, 512 KiB per source, an 8 MiB cumulative source-size budget for retained
models, and 256 paths/events per lifecycle batch. Overflow/time-exhausted update batches drop the
cache instead of keeping truncated stale facts. Discovery/read failures, caps, cold/current-document
only state, pending scans and two-minute stale snapshots remain visibly distinct and cannot prove
missing beans. Render does not retry these states implicitly. Explicit configuration reports also
label incomplete inventories/checked-file caps instead of displaying a clean result. Underlying
host discovery/read/parse operations are synchronous, so cooperative timing cannot preempt one
such operation. SDK ignored/unsupported sources, libraries and effective runtime contexts remain
outside this source inventory.

Original evidence: `SpringBeansModificationTracker` combines project-owned outer-model and XML
modification tracking; `SpringModificationTrackersManagerImpl` additionally tracks profiles,
contexts, parser registrations, libraries/facets and indexing state. This source cache adapts the
project/change ownership concept, not that PSI/module/runtime model. Deterministic tests cover
cold reads, explicit warming, unrelated/filtered updates, deletion, unreadable/oversized retention,
cap/stale/permission states, nested/closed roots, report scope, row membership and batch/source
budgets. Prior full-context fixtures now explicitly warm rather than relying on hidden provider
I/O; cold live-provider fixtures remain cold. No new native desktop acceptance is claimed.

## External packaged Beans view refresh

Observed source lifecycle updates, explicit Reindex, report commands and view state
changes now retain native refresh and send only model-free `ui-kit-refresh` to the
Beans packaged view. The receiving hub obtains its own project/frame-correlated
model; no source/report payload is broadcast. Report publication notifies before
attempting to reveal its view, so a failed reveal cannot hide a committed report
from an already open page. Closing a report's owner also clears and notifies its
cached report even when no file event is present.

Rendering remains cache-only: cold/incomplete source states stay explicit and no
scan-progress refresh loop is added. Bean/graph resources use their existing exact
project-root cache, reports retain explicit source-root binding and navigation
membership checks, and missing/unreadable identity returns a successful replacement
message instead of retaining an old packaged payload behind an error. A foreign
project cannot render another project's report. Reports remain snapshots of their
explicit Check/Dump command, not automatic reruns on source changes. Existing
FileActions/source-generation guards and metadata/completion ownership are unchanged.

Original SpringBeansViewSettingsListener publishes a project-bearing bean-items
change on its message bus; the bounded invalidation follows that ownership shape,
not a claim of its full framework/runtime model or original visual parity. Tests
and pinned package validation do not imply native Beans refresh acceptance. Native
identity remains a per-call snapshot; unobserved source changes and final lookup
races remain separate limits. No process/network/GUI ran for this slice.


The extraction ID prompt accepts a bounded literal ASCII name (letter/underscore,
then letters/digits/underscore/dot/hyphen; maximum 128 bytes), refusing aliases,
expressions, XML injection and current inventoried names/aliases. Cancel at either
the ID prompt or exact preview produces no edit and consumes the action ticket.
Default namespace and prefix-only roots preserve the existing element prefixes,
namespace declarations, Unicode text and line endings when moving the inner bean;
the holder becomes the same namespace-qualified element with a plain `ref` attribute.
Namespace-aware admission rejects reserved xml/xmlns rebinding, unbound prefixes and
malformed qualified names. Local namespace/xml-context changes and nested profile
roots remain unsupported because moving them could change inherited meaning.
No new class is guessed or generated; the copied class spelling is the source's
existing literal. Broader create-file and Java generator migration remains separate.

### Source-owned bean declaration hover (P16.2 subset)

Hover on a Java `@Bean` factory method name shows the current source model's name,
alias spellings and declared return-type spelling. Known source-owned positive
`@Primary`, `@Profile` and `@Qualifier` markers add their literal facts; method
profiles override inherited class profile spellings. Explicit imports or fully
qualified annotation names are required independently for each marker. Foreign,
shadowed, dynamic, generic/ambiguous or unsupported contexts stay quiet or omit
unproved markers. This does not select an effective injection candidate.

The provider uses the current document and project-owned cache only, without
scanning, storage restoration, filesystem/network/process access or rendering a
panel. Canonical Java paths and 128 KiB source bound apply; alias/profile/qualifier
lists are capped at eight and display strings are bounded/escaped. Current unsaved
changes replace prior source facts; missing/foreign owners cannot reuse another
root's model. Kotlin/XML/reference hovers and resolved type/description
metadata remain outside this slice. No source-name/type/event associations are
added to host Rename/Find Usages: the automatic indexer stays empty because those
relationships do not all have interchangeable source spellings/complete owners.
Native hover acceptance and full original bean documentation remain separate.

### Live XML context admission (P16.3 follow-up)

Live Spring XML checks require a namespace-supported Spring `<beans>` root,
including default, prefixed and legacy no-namespace forms. Reserved xml/xmlns
rebindings, unbound/malformed qualified names, foreign roots and incomplete XML
contexts return no Spring diagnostics. The namespace reader does not fetch or
resolve DTDs, entities or schemas; legacy external-DTD spellings remain supported
without claiming their expansion or runtime semantics. Ordinary non-Spring XML
keeps source-file navigation but does not create Spring bean declarations.

Malformed namespace/structure, excessive depth/events and oversized current source
remove that file's old cached bean facts and mark source context incomplete, while
unaffected models stay. The Beans cache status identifies unsupported XML; absence
checks cannot use that incomplete inventory. Fixing the current document restores
its facts. The shared source boundary is 512 KiB, depth 128 and 65,536 XML events;
the stricter generation bounds remain unchanged. This adds no provider I/O or
legacy diagnostic publication. Original compiler/DOM inspections and full runtime
context remain outside these source-local checks; native debounce/render acceptance
is separate.

### Source-owned stereotype class hover (P16.2 follow-up)

Hover on a top-level concrete Java class identifier now shows the known current
source bean name, explicit Component/Service/Repository/Controller stereotype and
class spelling. Explicit Spring imports or qualified annotation names establish
each owner independently. Positive Primary/Profile/Qualifier facts are bounded
and only literal source lists are shown; computed/concatenated markers are omitted.
Dynamic names, multiple stereotype owners, shadowed/foreign annotations, nested
and generic classes remain quiet. Missing runtime activation/type resolution is
explicit. Current unsaved text replaces old facts and foreign roots cannot reuse
the cached declaration. Existing source/path/UTF-16 and provider-I/O bounds apply.

The original documentation provider resolves a CommonSpringBean and shows its
name/type, aliases/profile/scope and documentation. This source-local extension
adds owned stereotype declarations to the existing factory method subset; it
does not resolve classpaths, effective beans or runtime state. Kotlin/XML/reference
hover, meta-annotation inheritance and full original documentation remain open.
The automatic document index remains empty; no host usages/Rename is enabled.
Native hover acceptance remains unverified.

### Reviewed Java generation commands (P16.4 subset)

Generate Setter, Constructor Dependency, Autowired Dependency and Event Listener
now show the full resulting Java source and exact project/path before one
versioned atomic editor edit. Dismissing the chooser, event-type prompt or final
review leaves the source unchanged. A 12 KiB source bound and the shared 16 KiB
UTF-8 confirmation bound prevent clipped review. Uniform CRLF and Unicode source
are preserved. Literal event identifiers use a bounded ASCII Java name subset.

The editor source/version and owning project are checked across input prompts and
again after review; the final project-visible source must match. The versioned
apply rejects intervening editor changes. This remains a supported source-level
command adaptation of original caret-class dependency selection/template actions;
classpath resolution, duplicate simple types, effective bean activation and the
existing class selection heuristic remain separate limitations. Dependency picker
metadata is a bounded discovered snapshot, not runtime identity. The original XML
extraction FileAction, XML namespace rules, constructor quick fix and property
migration command remain separate. Native preview/apply/Undo acceptance is unverified.

### Atomic annotation property externalization (P16.4 follow-up)

The _Move annotation string to property…_ Java editor intention now offers
destination/key prompts and a complete two-file preview, then returns both edits
as one host workspace operation. It never runs tee, copies an uncommitted value,
or changes either file during resolution. The former menu command validates the
current literal and directs to that named editor intention, leaving both files
unchanged. The unused `process:tee` capability is removed.

This source-level operation accepts one unescaped literal annotation argument
(up to 2 KiB), POSIX project-owned Java and existing properties paths (up to 512
bytes), each source up to 8 KiB, and a new bounded ASCII property key. Arrays,
concatenations, placeholders, escaped Java strings, unknown project identity and
nested-project destinations are refused. Discovery is bounded at 100 properties
files. Duplicate keys, escaped property keys, continued lines, BOM/control text
and mixed/CR-only line endings are refused rather than guessed. Existing value
escapes remain supported. New spaces/non-ASCII values receive properties escapes;
uniform LF/CRLF and existing target content are retained. No new file is created.

Action tickets are bounded to eight, source/version/root-owned and single-use.
The resolver checks its supplied Document against the offered ticket; both
project-visible source texts (including open dirty buffers) and owners are
re-read after review. Complete formatted preview above the host's 16 KiB limit is
refused. FileAction preparation/apply owns subsequent native version/conflict
checking and atomic Undo; API1.3 exposes no independent current-source version
query after prompts here. Concurrent changes between the last read and host
preparation remain a native acceptance gap. A same-content version change after
the resolver's supplied Document snapshot is not independently observed.

The original hard-coded-property intention targets typed Spring XML and resolved
placeholder-configurer resources; this adaptation migrates the existing Java
annotation workflow. It does not certify that an arbitrary annotation resolves
`${...}`, select an effective resource/profile or implement the original XML
context. Native two-file apply/dirty-conflict/Undo acceptance remains unverified.

### Create a source XML bean for a literal reference

At a plain literal `ref` attribute on a self-closing Spring XML property or
constructor-arg, _Create XML bean for literal reference…_ prompts for an explicit
qualified class spelling and reviews the complete resulting XML. One same-file
FileAction adds a new root-level bean ID matching that reference; it does not
replace the reference, another definition or a file. Known current-document IDs
and aliases suppress the offer. Resolution requires a fresh complete project
source inventory with no matching ID/alias, then repeats that check after review.
This proves source-inventory absence only; library/ignored/runtime beans remain
unknown. No class, constructor, configuration import or runtime activation is
inferred from the prompt.

The existing XML generation namespace validator, 12 KiB/2,048-event source bounds,
512-byte POSIX paths and exact preview cap apply. Root prefixes and namespace
declarations, existing text, Unicode and uniform LF/CRLF survive. Foreign or
unbound namespace contexts, local namespace/xml-context rebinding, nested profile
roots, DTD/entities, dynamic/multi-name IDs, value-plus-ref holders and mixed
newlines are refused. Class spellings use a bounded literal ASCII qualified-name
subset; no type is generated or resolved. The action has bounded single-use
source/root/version tickets and the same supplied-version/content-read/native
prepare boundary described above.

Original SpringBeanGenerateProvider/BasicSpringDomGenerateProvider create a typed
Spring bean beneath a Beans root; GenerateSpringBeanDependenciesUtil creates a
bean with a resolved class spelling and suggested ID. This source-level adaptation
uses the selected literal reference as the ID and asks for the class explicitly.
It covers a bounded existing-file creation journey, without claiming typed DOM
or arbitrary new configuration-file generation. Native create/apply/conflict/Undo
acceptance remains unverified.

Core XML bean-reference attributes require their exact unqualified names. A
foreign-prefixed `x:ref` (even when its local name is `ref`) is not a Spring bean
reference; supported p/c reference attributes retain their own namespace binding
checks. This source guard does not infer effective contexts or runtime beans.
Native315 creation/property actions remain validated only on their frozen package.

XML editor semantic navigation/completion/markers require a validated Beans
namespace envelope, matching diagnostics. Malformed/foreign roots cannot borrow
beans or setters from other cached files; current source still replaces cached
facts before returning quiet. Static live-template snippets remain separate from
source-derived semantic candidates. No DTD/schema content is fetched.

The XML envelope guard checks qualified name segments against XML1.0 NCName
character rules and rejects duplicate expanded attribute names, including aliases
bound to the same namespace. Valid Unicode prefixes remain supported. Attribute
uniqueness stores borrowed namespace/name spans within the existing source/event
bounds; it does not copy a large namespace URI per attribute. This is namespace
well-formedness, not external schema/DTD validation or effective bean resolution.

Core XML schema fields (`id`, `name`, `class`, `scope`, profile/factory and related
fields) use exact written attribute names throughout the source model and local
checks. Foreign-prefixed lookalikes cannot supply a bean name, class/setter owner,
scope or filter enum. Generic XML rule checks receive only ordinary schema
attributes; the original elements retain p/c attributes and namespace bindings.
Known unqualified fields remain authoritative beside foreign lookalikes.

XML reference sites require an established Beans element/attribute role (or the
known AOP config/aspect ref role). Class/scope/profile sites require their owned
Beans elements. Expression navigation accepts ordinary property/constructor/map
value roles and namespace-bound p/c attributes on beans; foreign-prefixed values
and foreign element lookalikes do not become Spring reference/class/SpEL owners.
Ancestor context and effective custom namespace registrations remain separate
unknowns, not inferred semantic graph facts.

Bean and stereotype definition facts in Java/Kotlin require a canonical explicit
annotation import, exact fully qualified annotation, or proven same-package owner.
Foreign, aliased, unresolved and source-shadowed annotations remain unknown.
Generic parameter and import-root collisions also withhold declaration hover.
This source proof covers bean definition roles and Primary selection. Java
Qualifier literal navigation and Java/Kotlin Qualifier selection also require
canonical source ownership. Profile partitions require canonical source ownership;
other injection and profile navigation/diagnostic roles,
meta annotations, effective types and runtime registration remain separate limits.

Resource and Named source facts require the established javax/jakarta annotation
owners. Autowired model facts and constructor-injection rewrites require canonical
source ownership; foreign or shadowed Autowired annotations cannot drive that edit.
Other raw inspection annotation roles and effective context semantics remain
separate limitations.
