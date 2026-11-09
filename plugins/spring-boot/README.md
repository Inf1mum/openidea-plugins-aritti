# Spring Boot Tools

Completion and checks for Spring Boot's `application*.properties|yml|yaml` and `bootstrap*` files in OpenIDEA:

- Key completion: full dotted keys in `.properties`. In YAML, the next segment under the current parent (a property inserts `name: `, a group or map inserts `name:` and a new indented line). Each item shows the property's type, and its documentation gives the description, the default and any deprecation. Map keys hinted by the metadata (`logging.level.root`) are offered too.
- Value completion: `true`/`false` for booleans and the values hinted by the metadata (`logging.level.*` levels, `spring.jpa.hibernate.ddl-auto`…), in both formats.
- Deprecated keys: a warning (struck through) that names the replacement, and a quick fix that renames the key. In YAML the fix is offered only when the replacement keeps the key's written parent (otherwise the key would have to move).
- Unknown keys: a warning when a key isn't a property but its parent is a known group of two segments or more (`spring.datasource.urll`). These are never reported: keys under maps (`logging.level.*`, `spring.jpa.properties.*`), list items (`[0]`) of known properties, unknown groups (third-party starters, Spring Cloud modules not listed below, your own keys) and unknown sub-groups of known groups (`spring.datasource.druid.*`, `management.health.consul.*`). The check runs only once the whole project has been read.
- Cron expressions: Spring's six-field syntax (second minute hour day-of-month month day-of-week), macros (`@daily`…), `-`, `?`, ranges, steps, lists, `L`, `L-n`, `LW`, `nW`, `nL`, `n#k`, month and day names. Checked in `@Scheduled(cron = "…")` literals and in config values whose key ends with `cron` (`app.cleanup.cron`, `app.report-cron`). Placeholders (`${…}`) and SpEL (`#{…}`) are skipped.

- Test constructors: a Spring test class (`@SpringBootTest`, `@WebMvcTest`, `@DataJpaTest`, `@ContextConfiguration`…) whose only constructor takes parameters without `@Autowired` (on the constructor or a parameter, also `@Qualifier`/`@Value`) gets a warning, since Spring autowires them only with `@Autowired`, `@TestConstructor(autowireMode = ALL)` or `spring.test.constructor.autowire.mode=all` in `spring.properties`/a system property; otherwise JUnit fails with a ParameterResolutionException. Quiet until the whole project is read and while any `spring.properties`, `application*`, build file or `gradle.properties` mentions the key. Setting `springBoot.checkTestConstructors`.
- `@EventListener` methods: more than one parameter is an error (Spring throws), a plain `String`/primitive return is a hint (the value is published as a new event). `void`, event objects, collections and async types are not reported. Setting `springBoot.checkEventListeners`.
- Property values: a warning (`invalid-property-value`) when a value cannot convert to the key's metadata type: `Integer`, `Long`, `Short`, `Double`, `Boolean`, `Duration` (`10s`, `PT10S`) or `DataSize` (`10MB`); an enum the project declares or a well-known Spring Boot/JDK one (`Access`, `Show`, `TimeUnit`, `DatabaseInitializationMode`, `WebApplicationType`…), matched the relaxed way (`safe-mode`, `SAFE_MODE`); a `Class` value naming no class in a package the project owns (library classes are never judged); and the items of `List<X>`, `Set<X>` and `X[]` keys (comma separated, or the items of a YAML sequence). `${…}`, `#{…}`, maps and other custom types are never checked. Setting `springBoot.checkPropertyValues`.
- Configuration file set: besides `application*` and `bootstrap*`, a `.properties`/`.yml`/`.yaml` file is read as configuration when a standard file points at it with `spring.config.import`, `spring.config.location` or `spring.config.additional-location` (`classpath:`, `file:`, `optional:`; other schemes and directories are skipped), or by `spring.config.name`. One level only: files imported by an imported file are not followed. The project search skips dot directories and gitignored paths.
- Spring Boot: Show Configuration Problems (command): reads every configuration file and metadata file of the project and lists the problems (errors first) in a pick list; choosing one opens the file at the problem. Same checks as the editor diagnostics, without the opt-in JDBC driver hint.
- `@ConfigurationProperties` classes: a warning for a prefix that is not lower-case kebab-case words (`config-properties-prefix`), a hint for a missing prefix; constructor binding: `@ConstructorBinding` on a component-scanned bean (`constructor-binding-component`, warning) or on two constructors (`constructor-binding-ambiguous`), and an info for several constructors with none annotated and no default one (`constructor-binding-missing`). Setting `springBoot.checkConfigurationProperties`.
- Application setup: a `@SpringBootApplication` class in the default package (`default-package-application`) and a redundant `@EnableAutoConfiguration` or `@ComponentScan` next to it (`redundant-enable-auto-configuration`, `redundant-component-scan`; a quick fix removes the annotation). Setting `springBoot.checkApplicationSetup`.
- Early events: an `@EventListener` for `ApplicationStartingEvent`, `ApplicationEnvironmentPreparedEvent`, `ApplicationContextInitializedEvent` or `ApplicationPreparedEvent` never fires, because those events are published before the context exists (`early-event-listener`). Setting `springBoot.checkEarlyEvents`.
- Test lifecycle: `@DirtiesContext(methodMode)` on a class, `classMode`/`hierarchyMode` on a method (`dirties-context-mode`), and `@BeforeTransaction`/`@AfterTransaction` methods that do not return `void` (`transaction-callback-signature`). Setting `springBoot.checkTestLifecycle`.
- Navigation: Go to Definition from `spring.config.import` and from `Resource` keys with `classpath:`/`file:` values (placeholders and other schemes are skipped) to the file, and from a `Class` valued key to the project class. Completion of logger packages under `logging.level` (from the project's `package` lines), profile names (from `application-*` files) in `spring.profiles.active|include`, keys inside `${…}`, project class names for `Class` keys, enum constants (project enums and the well-known ones), and `classpath:` resources under `src/main/resources` for `Resource` keys and the `spring.config.*` location keys.
- Metadata files: `(additional-)spring-configuration-metadata.json` is checked for unknown fields, a malformed `type`, duplicate names, a missing description (hint), a `defaultValue` that does not convert to the `type`, a `deprecation` that is not an object, has unknown fields or a `level` other than `warning`/`error`, a `deprecation.replacement` or hint name that no property defines (info), and a `type` in a package of the project naming no project class. Library classes are never judged. A quick fix on an unknown key adds it to an existing `additional-spring-configuration-metadata.json` (the plugin cannot create files). Needs the host to give JSON files to plugins; not verified in the IDE. Setting `springBoot.checkMetadataFiles`.
- Build files: `Hooks.onOperatorDebug()` gets a hint when `reactor-tools` is a dependency (`ReactorDebugAgent.init()`). Opt-in (`springBoot.checkDriverDependency`, off): an info when `spring.datasource.url` names a JDBC driver no build file declares; transitive drivers are invisible, hence off by default.
- File templates: Spring Boot Application, Spring Boot Test and Spring Boot ConfigurationProperties for Java and Kotlin in the New File menu.

Go to Definition from `${}` placeholders and from plain keys to Java code, Find Usages between keys and Java code, Spring Boot run configurations and HTTP endpoints are not done by this plugin.

Where keys come from:

- Spring Boot 3.5.16's own metadata (`spring-boot`, `spring-boot-autoconfigure`, `spring-boot-actuator-autoconfigure`) and Spring Cloud 2025.0.3 (Northfields, the train for Boot 3.5; commons, context, loadbalancer, config, Netflix Eureka, gateway, OpenFeign, Stream and its Kafka binders, circuit breaker Resilience4j, bus, Consul, Zookeeper, Kubernetes, Vault, contract, task, function: commons 4.3.2, config 4.3.2, netflix 4.3.2, gateway 4.3.4, openfeign 4.3.2, stream 4.3.2, circuitbreaker 3.3.2, bus 4.3.1, consul 4.3.2, zookeeper 4.3.2, kubernetes 3.3.2, vault 4.3.2, contract 4.3.3, task 3.3.1, function 4.3.2), bundled in `data/boot-properties.tsv` (Apache-2.0, see `data/NOTICE`). Boot's keys win over Cloud's. Regenerate it with `python3 -I scripts/gen-metadata.py 3.5.16 data/boot-properties.tsv 2025.0.3`; the script reads the artifact versions from the train's BOM and reports artifacts without metadata on stderr.
- The project's `META-INF/spring-configuration-metadata.json` and `additional-spring-configuration-metadata.json`. Project search honours `.gitignore`, so in practice these are the files under `src/main/resources`, not the build output.
- `@ConfigurationProperties(prefix|value = "…")` classes and records in the project's Java sources: the prefix plus the dashed name of each field, record component and setter, and one level down into classes of the same file. A prefix on a `@Bean` method, or on a class with a superclass, accepts any key below it, because its properties aren't visible.

Editor diagnostics now use API 1.3's versioned current-document provider for the existing Java annotation/cron, metadata JSON, and configuration checks. A bounded project cache is warmed during activation and file events; diagnostics, hover and inlay requests only read that cache and the supplied document text. Unsaved edits replace the current document's cached facts. Missing, conflicting, dynamic or incompletely read declarations suppress absence-based findings. Line markers no longer publish duplicate diagnostics. Requests are limited to 128 KiB of current text, 512 configuration keys, 500 diagnostics and 200 inlays; oversized inputs remain quiet.

Configuration key hover shows established metadata type, default, description and deprecation. Configuration inlays show the declared type only. These do not report effective runtime values. Unknown keys, placeholders, ambiguous declarations and open property prefixes do not get speculative hints. Existing completion, navigation and quick fixes retain their existing behavior; the plugin does not start or take ownership from a language server. File events are optional; without them, checks requiring a complete project view remain quiet.

P17.1 is partial. The audited original `SpringBootApplicationPropertiesDocumentationProvider` and `SpringBootApplicationYamlDocumentationProvider` use configuration-key documentation; this implementation covers the local metadata subset. Original starter Maven/Gradle/run and AOT repository-query inlays remain separate gaps. Dependency-JAR metadata now has the explicit literal Maven slice below; effective configuration/Actuator data, Kotlin configuration-property extraction and broader semantic hover/inlays remain separate work. Desktop debounce/rendering and close/discard lifecycle still need host acceptance. The original class contracts were inspected locally; no proprietary implementation was copied.

The plugin reads source text only (Java and Kotlin sources for classes and enums). It reads library metadata only through the explicitly reviewed Maven JAR workflow below; enum values come from the metadata hints, the project's enums and a short built-in list, and bean references in values are not resolved.

Not ported: datasource/database tool integration, Flyway/Liquibase support, Spring MVC view resolution, the run dashboard's live data (beans, health, environment), SpEL in `@ConditionalOnExpression`, Kotlin configuration classes, repository query preview.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

P17.2 configuration-key indexing is audited and pending an explicit owner model;
indexed usages and automatic rename remain unregistered. Existing YAML paths,
relaxed keys and Java-derived binding names are not unconditional replaceable
source tokens. See [the anchor audit](ANCHOR_AUDIT.md) for original contracts,
ambiguity fixtures and the required project/module/Properties ownership work.
Existing navigation and live providers are preserved.


## Selected Maven dependency metadata (partial P17.3)

**Spring Boot: Read Metadata from Selected Maven Dependency JAR…** reads one JAR
from the standard `~/.m2/repository` layout, after optional
`fs.read:~/.m2/repository` is granted. It lists only supported direct literal
root `pom.xml` dependencies, derives the exact group/artifact/version path and
shows the project, current POM, declared scope and archive before reading bytes.
POM reads use the project's visible text, including unsaved content. The same
project and unchanged POM are checked after review and after parsing. No Maven,
Gradle, Java, shell, network, home/cache discovery or extraction is performed.
The filesystem API enforces its real canonical path grant; a stat preflight checks
permission/type/size, then metadata is read only after confirmation.

Supported source declarations have literal groupId/artifactId/version, ordinary
compile/runtime/provided/test scopes, JAR/default type, no classifier and no true
optional flag. Dependency-management/BOM/property interpolation, profiles, parent
resolution, Gradle, systemPath, alternate local repositories, timestamped snapshots,
transitives and inferred runtime classpaths are not resolved. Malformed, duplicate,
foreign-namespace or oversized POMs are rejected; this bounded parser also rejects
DTD/entity/CDATA coordinate forms instead of interpreting them. Well-formed XML
comments between elements and before/after the root are ignored; commented-out
dependencies never authorize an archive read. Comments or processing instructions
inside coordinate text remain unsupported. Invalid comments and non-whitespace
content outside the root are rejected. A declaration is
**provenance**, not proof that an artifact is on the effective runtime classpath.

Only top-level `META-INF/spring-configuration-metadata.json` and
`META-INF/additional-spring-configuration-metadata.json` are consumed. Both can
contribute disjoint keys/hints; duplicate canonical names are rejected instead of
guessing precedence. Existing project-local metadata wins over the selected JAR,
which wins over bundled fallback metadata. Completion, known-property checks and
hover use the in-memory JAR catalog; that lookup performs no filesystem read or
discovery. Archives are read only by the explicit command. Existing source
completion/navigation behavior is unchanged.
For a key supplied by the JAR, hover labels the declared GAV/scope, exact archive,
entry names and SHA-256 fingerprint, with an explicit unverified-classpath caveat.
No classes are loaded and no library enum/type resolution or Rename anchors are added.

The snapshot is one selected JAR/project, up to eight exact project roots, with a
10-minute lifetime. It is session-only and disappears on reload/idle reclaim;
import again explicitly. POM open/save/change notifications invalidate the matching
snapshot when events are granted. Provider requests never reread the POM or archive;
unobserved unsaved/source/cache changes remain historical snapshot limitations.
A cancelled, denied, missing, corrupt or unsupported replacement retains the previous
still-valid snapshot within the same POM source scope and imports no partial data.
An explicit command that observes changed POM bytes invalidates the previous
digest-bound snapshot even when the new import fails. Expired/invalidated snapshots do
not supply facts. There is no persistent path that silently reopens an archive.

Bounds: 256 KiB POM, 4,096 XML elements, nesting 32 and 128 direct dependencies;
8 MiB archive (the host byte API itself caps at 16 MiB), 2,048 entries, 256-byte
UTF-8 entry names, 1 MiB central directory, 4 KiB per-entry extra/comment fields
and archive comment; 64 MiB total declared expansion. Metadata is limited to
512 KiB/entry, 768 KiB combined and a 100:1 expansion ratio, with 4,096 properties,
1,024 hints and 128 bounded values/hint. Stored/deflate classic ZIP only; ZIP64,
encryption, unsupported compression, nested/fat-JAR layouts, path tricks,
duplicate entries, overlapping entry regions and inconsistent local/central/
data-descriptor sizes are refused. Metadata length, UTF-8 and CRC are verified;
SHA-256 records the source bytes, not publisher-signature verification.

Original evidence: decompiled `SpringBootConfigurationMetadataLoader` reads
JarFileSystem PSI files in a module and invokes the parser with resolved classes;
`SpringBootConfigurationMetadataCollectorKt` caches module-scoped class lookup via
Java library modification tracking. Pinned SDK `host::fs::read_bytes` and
`wit_host::StoreData::granted_file` provide binary reads only under explicit FsRead
or private-directory grants, not ProjectRead. This explicit source-declared subset
avoids pretending those APIs provide an IDE dependency graph. Deterministic mock
workflow tests use an injected `/m2` root because the pinned MockHost does not
expand `~/`; production is fixed to the declared Maven grant. Native grant/canonical
symlink behavior and desktop acceptance remain separate verification steps.

## P17.2 current-file placeholder source candidates

Simple `${key}` / `${key:literal-default}` references in current standard
`application[-profile].properties` and `bootstrap[-profile].properties` text now
navigate to one exact same-file key token, before metadata/source discovery.
A reference hover gives the source line and explains unresolved profiles, imports,
relaxed binding, environment overrides and runtime values. Neither declaration
values nor defaults are rendered. Unsaved changes immediately replace these facts;
no source read, file scan, network or process occurs in this local candidate path.

This is read-only source navigation, not an effective configuration owner or a
complete usages/Rename set. Missing/duplicate keys suppress the local target.
Decoded escaped keys, relaxed alternate spellings, escaped/continued values, SpEL,
nested defaults, multi-document properties, YAML and nonstandard imported files
are outside this slice. Parent metadata hover and class/resource definition paths
remain intact. Work is limited to32KiB source,256 keys,256 references,256-byte ASCII
names and a cooperative10ms budget, and requires a currently owning open project.
Token ranges are half-open and UTF-16; profile/sibling files are never searched.

The original20211 `SpringBootApplicationPropertiesPlaceholderReferenceResolver`
uses `SpringBootPlaceholderReference.findConfigFiles`, PSI property keys and
resolved configuration declarations. Its BeanProperty Rename handler requires
a resolved BEAN_PROPERTY path/simple accessor. These contracts informed the
conservative source-only boundary; no proprietary implementation was copied.
Automatic host indexing/Find Usages/Rename remain unregistered. Full module/profile/
import context ownership, Properties-plugin co-ownership and YAML/Java-derived
coordinated edits remain P17.2 gaps. Tests are mocks/parser receipts, not native
UI/screenshot or effective Boot runtime verification.

## P17.4 native settings organization

Pinned API1.3 native Settings now has **Java Source Checks** (Tests, Application
Events, Configuration Classes) and **Configuration and Metadata** (Configuration
Files, Static Metadata, Static Build Facts) pages. All nine existing boolean keys,
types and defaults remain unchanged, including the driver-dependency opt-in default.
Existing user overrides continue to apply; no value migration or duplicated setting
storage is needed. The old aggregate `settings.schema.json` is retained as a
compatibility receipt; the manifest loads only the two page schemas.

There was no scalar-list setting to convert into a table. The new configuration
page links the existing explicit **Read static metadata from a selected Maven
dependency JAR** command. It still requires an open project, a supported literal
Maven dependency, user selection/confirmation and the existing optional filesystem
grant. Viewing Settings starts no JAR read, scan, process or network operation.
These are inspection switches and static-source import controls; environment,
Actuator values, effective profiles and metadata defaults are not editable runtime
settings. Hover continues to label declared metadata defaults separately from
unresolved runtime values.

Original20211 `SpringBootSettingsUi` groups Initializr/run endpoint preferences;
`SpringBootSettingsImpl` persists auto-created-run-configuration preference. Those
feature owners are not implemented by this settings slice. Native page/group/action
contracts were verified in pinned `plugins/ui_points.rs` and the package checker.
No matching Settings screenshot or native desktop acceptance was performed here.

Native settings acceptance: isolated pinned v0.1.39 desktop26 verified both pages, project-scoped Save, default restoration and Cancel without enabling events or Maven-cache access. The contextual JAR metadata action was visible but not invoked.

### Dependency metadata ownership/provenance follow-up (P17.3)

The selected-JAR command now checks namespace-aware root-POM ownership before
archive selection. Reserved `xml`/`xmlns` bindings, unbound prefixes, malformed
qualified names and foreign/mixed element namespaces cannot authorize an import.
Legal fully prefixed Maven layouts and legal predefined `xml` attributes retain
support, as does the legacy no-namespace layout. This is a conservative literal
source parser: valid XML using unsupported mixed/escaped/entity forms can remain
quiet; no Maven resolution, Gradle layout or transitive discovery is inferred.

Config-key hover explicitly labels the catalog that supplied the declaration:
project-local current/cached metadata, selected dependency-JAR snapshot (with the
existing GAV/scope/archive/entry/hash provenance), or bundled fallback. Bundled
origin is read from the packaged TSV header, including its dataset versions;
it is not presented as the project's resolved Spring Boot/Cloud version. Existing
project-local → selected JAR → bundled precedence is unchanged. These labels use
cached/static data only and never expose the current configured/runtime value.
All existing grants, explicit review, root/source/TTL/event invalidation and
archive size/CRC/entry bounds remain. Native import/hover acceptance is separate.

### Current-file placeholder occurrences (P17.2 bounded source UI)

With optional `editor.read`, place the caret on a literal property key or a
`${key}` name and run **Spring Boot: Current-File Placeholder Occurrences…**.
The native picker lists the one unique exact property spelling and matching
placeholder spellings in this current unsaved file, with line and UTF-16 column.
Choosing a row navigates to that exact range. It reads only the invoking editor;
there is no workspace scan, metadata lookup, network, write or index publication.
Values/default bodies are not included in rows.

This reuses the strict 32 KiB/256-key/256-reference literal model. Duplicate or
escaped/relaxed property spellings, dynamic/nested/escaped placeholder values,
multi-document property sections and unsupported file contexts stay quiet or
refuse the action. Cancel/invalid selection performs no navigation. Before opening,
the command rechecks its captured source text/version/path and project membership.
Invocation identity is an event snapshot; arbitrary active-window changes while a
native modal is open have not been verified. No write authority is prepared.

The picker is explicitly current-file **source occurrences**, not effective Boot
configuration usages: profiles, imports, environment overrides, binding methods,
module/classloader ownership and external callers are unresolved. No automatic
Find Usages/Rename graph is registered. Original property-path BeanProperty rename
and resolved module reference search remain unimplemented. Safe-delete/move and
broader refactoring are not claimed. Existing literal navigation/hover is preserved;
native picker/selection acceptance remains separate.

### Current project ownership of source facts

Cached source metadata and live facts are filtered against the current open-project
roots, including nested projects opened after a cache was warmed. Only facts owned
by the requesting document's longest matching root are admitted. The same source
ownership gate filters legacy completion catalogs, type/package facts, imported
configuration recognition, profile/resource suggestions, navigation candidates and
the metadata file selected by Define-key quick fixes. Ownership checks use bounded
project metadata only; existing explicit/legacy source-scan budgets are unchanged.
Malformed or unknown scope stays quiet, and an ownership-incomplete cached pass
cannot establish absence. Selected JAR/runtime module resolution and broader
refactoring ownership are separate from these source facts.

Java metadata extraction requires an exact fully qualified annotation or one
unique canonical nonstatic single-type import for ConfigurationProperties and
Bean. Missing imports, wildcards, explicit foreign spellings, same-file/local or
generic shadows, Unicode-escaped identities and unresolved inherited scopes stay
quiet. Both owners are required for Bean-method prefixes. This proves only the
admitted explicit source spelling: composed annotations, classpath/module identity
and effective binding remain unresolved. This is not semantic Rename authority.

Extracted Java configuration prefixes require exactly one explicit, unescaped
string literal in either prefix or value. Concatenation/constant fragments,
conflicting or duplicate aliases, escaped strings and text blocks stay quiet;
no partial Java constant evaluation is inferred.

The Define-key metadata quick fix requires complete valid JSON with unambiguous
plain top-level fields, at most128KiB source and256 bytes in the new key. It
serializes the key as a JSON string before constructing the insertion, preserving
quotes/backslashes as data. Malformed/trailing JSON and escaped/duplicate top-level
field contexts refuse the edit. Diagnostic scanning remains a separate parser.

Deprecated-key quick fixes insert only bounded ordinary dotted ASCII key names.
Replacement metadata containing property/YAML syntax stays diagnostic-only; no
partial encoding or normalization is guessed. YAML edits require the exact same
source parent and refuse ambiguous plain-scalar names. An existing target key
(including a relaxed spelling collision) prevents the edit. Supported literal edits
preserve the source value and surrounding format; this is not global Rename.

Java annotation removal quick fixes require explicit supported owners for both
SpringBootApplication and the argument-less EnableAutoConfiguration/ComponentScan
being removed. The edit path uses the same import/FQN, shadow and current-project
source checks as the strict metadata admission. Broader source diagnostics retain
their separate textual checks; no semantic diagnostic registry closure is implied.

Show Configuration Problems filters its cached metadata, type facts, configuration
locations and listed files against the current invoking project owner. An inner
project opened later no longer supplies outer report facts or rows. Before opening
a selected row, the command rechecks its current owner and exact source hash;
project reads include current unsaved editor content. The picker remains an
invocation-scoped source report, without effective runtime/import closure or a
native cross-window freshness claim.

Same-file nested binding expansion follows unique declared enclosing type scopes
rather than globally matching a simple class name. It is limited to plain source
without angle syntax and plain non-array field types; imported, qualified,
ambiguous or inherited targets stay opaque as declared types. Other declared
fields remain available. Java metadata expansion is capped at512 properties; an
oversized expansion publishes no partial Java metadata. This does not resolve
module/classpath types or establish semantic Rename ownership.
