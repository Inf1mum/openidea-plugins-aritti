# A2: Spring plugins (rest), gap audit

API 1.3 review (2026-11-08): every originally B row below has been reassessed; [the complete decision ledger](reclass-api-1.3.md) preserves its old basis and gives current row totals. Existing F/F* rows and the Ours column were not re-audited for implementation status. Original prose counts and group summaries below are historical and may count features rather than rows.


Inventories list only a handful of named features per plugin; the long tail of extension points (indexes, usage providers, statistics, MCP toolsets) is not a user feature and is not counted. Ours evidence: `plugins/<name>/README.md`, problem codes in `src/*.rs`, `plugin.toml` (no plugin here contributes `fileTemplates`, snippets or `fileIcons`: grep of `plugins/spring-*/plugin.toml`).

## Spring Boot -> spring-boot
Original: 10 inspections, 8 intentions (4 named), 5 actions, 0 templates, about 50 extension-point groups. Counted as 23 gap rows + 3 Done (test-constructor autowire `test-constructor-autowire`; cron expressions `invalid-cron`; key/value completion in properties and YAML). Gaps: 5 Partial / 18 Missing; F 4 / F* 9 / B 10.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Invalid properties/YAML config (2 inspections), value part | Besides unknown/deprecated keys, flags values that cannot convert to the key's type (port=invalid, bad enum, duration) | Partial: `unknown-property`, `deprecated-property` only; README: values are completed, never validated | F* | Metadata types are bundled text; false positives for custom converters and keys without metadata |
| 2 | Same, library/starter keys | Keys of every dependency on the classpath | Partial: bundled Boot 3.5.16 + Cloud 2025.0.3 only; third-party starters unknown | F* | API 1.3: Scoped fs.read_dir/read_bytes can discover and parse dependency JAR configuration metadata; byte reads are capped at 16 MiB. Requires explicit filesystem grants, archive parsing and build-layout discovery; this does not provide a resolved classpath. |
| 3 | `@ConfigurationProperties` validity | Missing/empty prefix, prefix not kebab-case, bad constructor binding | Missing: classes only indexed for key completion | F | Annotation text on project sources; the binding part is F* |
| 4 | Early application events with `@EventListener` | Warns on listeners for events fired before the context exists (never delivered) | Missing: `event-listener-signature` is a different check (arity, return) | F | Event class simple names in the listener parameter |
| 5 | Invalid application setup | `@SpringBootApplication` in the default package; redundant `@EnableAutoConfiguration`/`@ComponentScan`, quick fix removes them | Missing | F | Package declaration and annotation text; code action edits existing file |
| 6 | Suspicious `Hooks.onOperatorDebug()` | Flags the hook when the Reactor debug agent is active | Missing | F* | Needs to know the agent is on the classpath (build file text) and the property default |
| 7 | additional-spring-configuration-metadata.json check | Missing/deprecated properties, unresolved classes, bad values in that JSON | Missing (file only read as a key source) | F* | JSON text check via `publish_diagnostics`; class resolution is project-sources-only, so library types risk false positives |
| 8 | Driver class availability (2 inspections) | Reports a datasource URL/driver whose driver is not a dependency | Missing | F* | Build file text; transitive dependencies invisible, so false positives |
| 9 | Define key in local metadata (2 intentions) | Adds the key to `META-INF/additional-spring-configuration-metadata.json` | Missing | F* | Caret code action; works only if the JSON file exists, otherwise U2 (file creation) |
| 10 | Open/create datasource from config (2 intentions) + datasource detectors | Opens a database connection dialog filled from the file | Missing | B | API 1.3: Prompts are available, but opening/configuring the IDE database connection model remains unavailable. |
| 11 | Add Starters action | Dialog to add starter dependencies to the build file | Missing | F* | API 1.3: Pick/input plus existing-file edits can add starters using the fixed Initializr host. Preserve build syntax/BOM/repository rules. The command path was already feasible in 1.2; current plugin implementation is tracked separately. |
| 12 | Edit Default Path, Show/Hide Values (3 actions) | Run-config level endpoint path and environment value exposure | Missing | B | API 1.3: Input is available, but the row requires changing native run-configuration path/environment exposure. No run-configuration editing API. |
| 13 | Spring Boot Diagnostics action | Diagnostic helper for Boot projects | Missing; content of the original unverified | F* | A command plus view is possible; scope unknown |
| 14 | Run configuration, update policy, run endpoints (live beans/env/health) | Boot-aware run/debug with live data | Partial: `spring-runtime` Actuator tool window, localhost only | B | API 1.3: Still blocked: No run-configuration contribution (only runDetectors). No API 1.3 replacement for this required integration was found. |
| 15 | Completion beyond keys/values | Logger/package names, profiles, class and resource names in values, `${}` placeholders | Partial: only metadata hints and booleans | F* | Project-source text; library classes unknown |
| 16 | Value references | Go to class/file/bean from a value (`spring.mvc.view.prefix`, class-valued keys) | Missing | F* | Definition provider on properties/yaml, project classes by text |
| 17 | Config file set detection | `spring.config.import`, additional locations, profile files, meta-annotations | Partial: name patterns `application*`/`bootstrap*` | F* | Text reads; imports can be placeholders |
| 18 | Hover docs, rename/find-usages handlers for keys | Docs popup, rename a key across Java and files | Missing (README: no hover docs; IDE gives Go to Definition/usages) | F* | API 1.3: HoverProvider supplies key docs; indexed exact key anchors provide Find Usages/Rename. Keep bundle/profile/context identity and escaped key spellings correct; unknown/library references remain incomplete. |
| 19 | Inlay providers (10) | Inline values/defaults in config and code | Missing | F* | API 1.3: InlayHintsProvider can show known metadata/config values. Effective runtime/environment values still need an explicit source and provenance; do not invent resolved defaults. |
| 20 | Injection (6) | SpEL, placeholders, JSON in values | Missing | B | API 1.3: Still blocked: No injection API. No API 1.3 replacement for this required integration was found. |
| 21 | Logical structure (10) | Structure view of config/classes | Missing | B | API 1.3: Still blocked: No outline provider for built-in languages. No API 1.3 replacement for this required integration was found. |
| 22 | Auto-configuration bean model (conditional contributors 20, run/test implicit contexts) | Beans created by starters and conditions | Missing | B | API 1.3: Scoped archive bytes are readable, but evaluating conditional auto-configuration and deriving its effective bean types still needs a semantic classpath/type model (U4). |
| 23 | Icons for config files | Icon for `application*` files | Missing | F | `fileIcons` contribution |

Ours beyond the original: bundled Spring Cloud metadata, `unknown-property` with maps/lists safety rules, deprecation quick fix.

## Spring Data -> spring-data
Original: 10 inspections, 2 intentions (1 named), 2 actions, 2 file templates (Java/Kotlin), 1 live template, about 40 extension-point groups. 17 gap rows + 3 Done (method inconsistency via `unknown-property` and `query-argument-count`; derived-name completion; repository->domain gutter and definition). Gaps: 1 Partial / 16 Missing; F 4 / F* 8 / B 5.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Method parameter types | Parameter type incompatible with the property it binds | Missing: only arity checked | F* | Text-based field types; conversions give false positives |
| 2 | Method return types | Return type not supported for the query kind | Missing | F* | Projections/DTOs are not visible by text |
| 3 | Pageable missing for `Page` return | Page-returning query lacks a `Pageable` | Missing | F | Signature text |
| 4 | `@Modifying` on update/delete queries | Update/delete `@Query` without `@Modifying` | Partial: `modifying-without-query` is the reverse check | F | Query string starts with update/delete |
| 5 | JDBC empty sequence name | Blank `@Sequence`/sequence attribute | Missing | F | Annotation text |
| 6 | JDBC `@MappedCollection` key column | List/Map without explicit `keyColumn` | Missing | F | Annotation and field type text |
| 7 | JDBC entity in several aggregates | Reference to an entity of another aggregate | Missing | F* | Needs aggregate-root detection by simple name |
| 8 | JDBC associated DB elements | `@Table`/`@Column` names checked against the datasource | Missing | B | API 1.3: Still blocked: No database API. No API 1.3 replacement for this required integration was found. |
| 9 | MongoDB `@Query` JSON fields + sort-properties intention | Unresolved fields in JSON queries; intention sorts keys | Missing: JSON queries skipped | F* | JSON in a string literal, nested/dotted paths risk false positives |
| 10 | Show Inherited Members | Lists members inherited from CrudRepository etc. | Missing | F* | Static list of well-known parents in a view; custom parents invisible |
| 11 | Run Query in Console | Runs the query in the database console | Missing | B | API 1.3: Still blocked: Database APIs. No API 1.3 replacement for this required integration was found. |
| 12 | File templates Spring Repository (Java/Kotlin) | New-file repository from the New menu | Missing | F* | `fileTemplates`; placeholder support unverified |
| 13 | Live template `repository` | Interface skeleton | Missing | F* | Completion item with insert text; no caret control |
| 14 | Resolved query preview | Shows the query a derived method produces | Missing | F* | Line-marker tooltip text only |
| 15 | Language injection/highlighting in `@Query` | JPQL/SQL/Mongo highlighting | Missing | B | API 1.3: Still blocked: No injection API. No API 1.3 replacement for this required integration was found. |
| 16 | Repository generator | Creates a repository for an entity | Missing | F* | API 1.3: FileActions creates a repository with import and package handling, optionally prompting during resolve. Project-source entity detection is sufficient for a conservative generator; library/generic inference remains limited. |
| 17 | Folding, surround descriptors | Folding of repository/query constructs | Missing | B | API 1.3: Still blocked: No folding control. No API 1.3 replacement for this required integration was found. |

Ours beyond the original: JPQL entity and alias-property checks and completion (original does it via the JPA plugin), `unbound-query-parameter`, Kotlin support.

## Spring MVC -> spring-web
Original: 6 inspections, 4 actions, 6 file templates (Java/Kotlin pairs), about 16 distinct live templates, about 50 extension-point groups. 16 gap rows + 3 Done (`unbound-path-variable`/`unknown-path-variable`; `init-binder-signature`; `api-version`/`api-version-mismatch`). Gaps: 1 Partial / 15 Missing; F 3 / F* 9 / B 4.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Unresolved view reference | Warns when a returned view name finds no template | Missing by design (README: another resolver may handle it) | F* | View resolvers unknown: false positives |
| 2 | Endpoint version SemVer format | Malformed `version` attribute | Missing | F | Literal text of the attribute |
| 3 | API version inserter missing in tests | `apiVersion()` on MockMvcTester/WebTestClient without an inserter | Missing | F* | Test configuration spread over setup code |
| 4 | Generate Request Mapping / Exception Handler (2 actions) | Inserts a handler method | Missing | F | Caret code action in an existing controller |
| 5 | File templates Controller, REST Controller, Controller Advice | New-file skeletons | Missing | F* | `fileTemplates`; placeholders unverified |
| 6 | Live templates (getm, postm, reqm, controller, router DSL) | Mapping snippets | Missing | F* | Completion insert text; no caret control |
| 7 | Endpoints tool window (22 endpoint providers) | Lists all endpoints of the project | Missing (README: not ported); the index exists for routes and test URLs | F | Tree/Table view over the existing mapping scan |
| 8 | Open endpoint in HTTP client, URL inlay actions | Generate and run requests | Missing | F* | API 1.3: EndpointsProvider feeds .http completion and Open in HTTP Client; reveal_view and generated .http files also work. InlayHintsProvider supplies labels, but arbitrary clickable inlay actions are not exposed; use the HTTP gutter/code action for execution. |
| 9 | URL completion in tests/HTTP clients | Completes known mapping URLs | Missing | F* | Completion in string literals; path templates only |
| 10 | Kotlin controllers | Controller checks in Kotlin | Partial: Kotlin indexed for functional routes only | F* | Same text scan with Kotlin syntax |
| 11 | Handler gutter marker | Mapping icon with URL on each handler | Missing: gutters only on view names, tests, routes | F* | One target per marker; icon allowlist |
| 12 | Template model variables (JSP, Thymeleaf, bundle/resolver providers) | Resolves model attributes in templates | Missing | B | API 1.3: Indexed references are available, but typed template model attributes and resolver semantics still require U4. Literal known model-name navigation is a feasible subtask. |
| 13 | Static resource and path references | Resolves paths to resources | Missing | F* | Definition on literals only |
| 14 | MVC XML namespace model (28 DOM classes) | Checks `<mvc:...>` configuration | Missing | F* | XML text checks; no XSD available |
| 15 | Injection (EL, multi-host) | Language injection in literals | Missing | B | API 1.3: Still blocked: No injection API. No API 1.3 replacement for this required integration was found. |
| 16 | Documentation, rename handler | Hover docs, rename of path variables | Missing (quick fix renames the parameter only) | F* | API 1.3: HoverProvider plus scoped path-variable definition/reference anchors enables docs and indexed rename. Anchor only literal, unambiguous variable names; exclude composed mappings that cannot be represented safely. |

Ours beyond the original: functional routes (Java and Kotlin DSL) as endpoints, test URL linking, `@PathVariable` rename quick fix, view completion.

## Spring Security -> spring-security
Original: 7 inspections, 4 intentions (2 named), 0 actions. 7 gap rows + 4 Done (`matcher-order`; `matchers-after-any-request`; XML `debug-enabled`; SpEL and role completion). Gaps: 2 Partial / 5 Missing; F 1 / F* 4 / B 2.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Obsolete request pattern format for Spring 7 | Ant patterns invalid under PathPatternParser (`**` mid-path etc.) | Missing: only removed `antMatchers` flagged | F | Pattern literal text |
| 2 | Unresolved beans in security annotations | `@WithUserDetails(userDetailsServiceBeanName=...)` etc. | Missing | F* | Needs the bean index (a spring-beans concern); library beans false positives |
| 3 | Security XML context model | Bean refs, required tags/attributes, property types, enums, `<https>` configuration | Partial: `intercept-url`, `duplicate-pattern`, `use-expressions-false`, `unknown-security-function` | F* | Hard-coded schema tables; library XSDs unreadable |
| 4 | Self-invocation of `@PreAuthorize`/`@PostFilter` | Internal call bypasses the proxy | Missing | F* | Call-to-annotated-sibling by text; aspect mode unknown |
| 5 | Matcher inlay hints (+ enable/disable intentions, 6 inlay providers) | Inline matcher meaning | Missing | F* | API 1.3: InlayHintsProvider and settings/code actions can show and toggle matcher explanations. Handle framework-version and matcher semantics conservatively. |
| 6 | SpEL language support | Highlighting, navigation inside expressions | Partial: completion only | B | API 1.3: Hover/indexed names are available, but embedded SpEL language support still requires injection and semantic expression resolution. |
| 7 | Matcher to endpoint navigation | Links matchers to controllers | Missing | F* | Needs the mapping scan of spring-web duplicated here |

Ours beyond the original: Security 6 migration fixes (`web-security-configurer-adapter`, `enable-global-method-security`, `authorize-requests`), `role-prefix`, plain-text password checks, Kotlin support.

## Spring Cloud -> spring-cloud
Original: 3 inspections, no intentions/actions, about 17 extension-point groups. 6 gap rows + 2 Done (Feign client to endpoint gutters and definition; stream binding and function references). Gaps: 1 Partial / 5 Missing; F 0 / F* 5 / B 1.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Unresolved message channel (legacy `@StreamListener`/`@SendTo`/`@Output`/`@Input`) | Channel names that have no binding | Missing (PARITY.md: legacy channels not covered) | F* | Project channel interfaces by text |
| 2 | Stream handler method errors | Constraint violations on handler signatures | Missing | F* | Signature text |
| 3 | Bootstrap configuration included by component scan | Flags `@BootstrapConfiguration` classes in scanned packages | Missing; `bootstrap-config-missing-dependency` is a different check | F* | Component-scan resolution by package text |
| 4 | Endpoints provider (Feign, URL resolver) | Feign requests appear as endpoints | Missing | F* | Tree view over the Feign scan |
| 5 | Cloud auto-configured model (model extender, autodetected models) | Cloud beans and bootstrap contexts | Missing | B | API 1.3: Scoped archive reads are available; automatic Cloud bean/bootstrap context construction still needs effective dependency, condition and type models that the host does not expose. |
| 6 | Custom property hint references | Value hints for Cloud keys | Partial: from `spring-boot` metadata, none from this plugin | F* | Bundled metadata text |

Ours beyond the original: gateway predicate/filter checks and completion, `stream-channel-unbound`, `feign-method-without-mapping`, bootstrap dependency check. Not in the original inventory: Gateway DSL.

## Spring GraphQL -> spring-graphql
Original: 2 inspections, 0 intentions/actions, 5 extension-point groups. 3 gap rows + 2 Done (handler/schema line markers and definition; `unknown-schema-field`, `unknown-graphql-argument`). Gaps: 1 Partial / 2 Missing; F 1 / F* 2 / B 0.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | `@BatchMapping` return type | Only Flux, List, Mono<Map>, Map are valid | Missing | F | Return type text |
| 2 | Unmatched schema mapping | Schema fields lacking a handler | Partial: `unmapped-root-field` for root types only | F* | Non-root fields may come from DataFetcher beans or `@SchemaMapping` in libraries |
| 3 | Endpoints/app path provider | GraphQL endpoint listed with its path | Missing | F* | View from `spring.graphql.path` text; no HTTP client (U5) |

Ours beyond the original: client document navigation, completion of type/field names in annotations, Kotlin controllers.

## Spring Messaging -> spring-messaging
Original: 0 inspections, about 180 DOM classes (XML namespaces), 6 message-queue resolvers, 1 line marker. 3 gap rows + 3 Done (producer/consumer gutters and definition; destination completion; Messaging tool window). Gaps: 1 Partial / 2 Missing; F 0 / F* 3 / B 0.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | XML namespaces (Rabbit, JMS, others) model | Typed elements, required attributes, bean refs | Partial: only `<jms:listener destination>`, `<rabbit:listener queue-names>` | F* | Hard-coded tables; XSDs unreadable |
| 2 | Bean link from XML `ref`/`method` | Opens the listener bean | Missing (README: no bean index here) | F* | Own text scan of `@Component`/`<bean>` |
| 3 | Constants and SpEL as destinations | Resolves `static final` names and expressions | Missing | F* | Constants by text; SpEL itself not parsed |

Ours beyond the original: `${}` placeholder resolution from config files, Kotlin sources.

## Spring Batch -> spring-batch
Original: 1 inspection (XML model), 11 DOM classes, job/step scope contributors. 3 gap rows + 1 Done (transition targets `unknown-transition-target`, bean refs `unknown-bean`). Gaps: 1 Partial / 2 Missing; F 0 / F* 2 / B 1.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Remaining XML model checks | Required tags/attributes, enum values, property types | Partial: `chunk-missing-commit-interval`, transitions, bean names | F* | Hard-coded schema tables |
| 2 | Job/step scope and late binding | `#{jobParameters[...]}` and scope beans | Missing (README) | F* | Parameter-key completion by text; SpEL itself needs injection (B) |
| 3 | Typed bean references | Reader/writer ref must have the right type | Missing: names only | B | API 1.3: Still blocked: U4 (type resolution). No API 1.3 replacement for this required integration was found. |

Ours beyond the original: Batch Jobs diagram tool window, `unknown-job` for `spring.batch.job.name`, `deprecated-builder-factory`, `enable-batch-processing-with-boot`.

## Spring WebSocket -> spring-websocket
Original: 1 inspection (XML model), 2 DOM classes, 1 messaging URL provider. 2 gap rows + 0 Done as defined (the Java side substitutes). Gaps: 1 Partial / 1 Missing; F 1 / F* 1 / B 0.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | XML WebSocket configuration check | Bean refs, required attributes in `<websocket:...>` | Missing: provider not registered for `xml` | F* | Hard-coded tables, XSD unreadable |
| 2 | Messaging URL provider (destinations in the Endpoints list) | STOMP destinations listed as endpoints | Partial: gutters and Go to Definition, no list | F | Tree view over the destination index |

Ours beyond the original: `unrouted-destination` check, JS/TS STOMP client linking, Kotlin support.

## Spring Debugger -> spring-runtime
Original: 1 action, 0 inspections, about 100 extension points (session listeners, run configuration extenders, inlay providers, renderers, code vision, Project view decorators). 8 gap rows, 0 Done in kind (ours is a different mechanism: Actuator snapshot). Gaps: 3 Partial / 5 Missing; F 0 / F* 1 / B 7.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | Debug-session integration (session listeners, extra nodes, context providers, renderers, code fragments) | Beans/watches/evaluation at a breakpoint | Missing | B | API 1.3: Still blocked: No debugger API (LIMITATIONS.md). No API 1.3 replacement for this required integration was found. |
| 2 | Inlay hints of effective values | Value next to `@Value`/config | Partial: gutter tooltips from the snapshot | F* | API 1.3: InlayHintsProvider can present values already captured by the runtime snapshot. Label snapshot freshness and unavailable values; this does not add debugger evaluation. |
| 3 | Code vision on config | Actionable lenses | Partial: gutter markers | B | API 1.3: Still blocked: No code vision. No API 1.3 replacement for this required integration was found. |
| 4 | Project view node decorators | Badges in the Project view | Missing | B | API 1.3: Still blocked: No decoration API. No API 1.3 replacement for this required integration was found. |
| 5 | Run configuration extender | Adds the debug agent to Boot runs | Missing | B | API 1.3: Still blocked: No run-configuration API. No API 1.3 replacement for this required integration was found. |
| 6 | Clone and Persist datasource, active connection info | Saves the run's datasource | Missing | B | API 1.3: Still blocked: Database APIs. No API 1.3 replacement for this required integration was found. |
| 7 | Runtime application provider | Finds running apps incl. remote | Partial: one localhost Actuator URL | B | API 1.3: Still blocked: `net:` hosts fixed (LIMITATIONS.md `net:user`). No API 1.3 replacement for this required integration was found. |
| 8 | Completion (2) | Unverified purpose | Missing | F* | Unverified |

Ours beyond the original: tool window for beans, health, env, conditions, mappings, configprops, scheduled tasks, loggers, caches; bearer/Basic secret.

## Spring Initializr -> spring-initializr
Original: nothing user-facing in the descriptor extraction (a settings provider and a performance command only); the New Project wizard lives outside the extracted files and is judged from public knowledge. 2 gap rows + 1 Done (server URL setting). Gaps: 1 Partial / 1 Missing; F 0 / F* 1 / B 1.

| # | Original feature | What it does | Ours | Class | Basis |
|---|---|---|---|---|---|
| 1 | New Project wizard creating and importing the project | Generates, extracts, opens | Partial: tool window opens the zip in the browser or runs `spring init` | F* | API 1.3: pick_directory, project.write/scoped fs writes, open_project and an approved spring CLI or ZIP handling enable a wizard. Outside-project extraction needs a matching filesystem grant; test local/remote paths and archive handling. |
| 2 | Dependency and version pick lists from the server | Metadata catalogue selection | Missing (ids typed by hand) | F* | `net:start.spring.io` fixed host; custom server URLs cannot be fetched; no multi-select UI beyond a form |

## Ours beyond the original
`spring-integration`, `spring-shell`, `spring-ws`, `spring-modulith`, `spring-webflow` have no counterpart in the inventories (Integration channel diagram, Shell command checks, WS XSD/WSDL, Modulith module diagram, Web Flow diagram).

## Group summary

| Pair | Gap rows | Partial | Missing | F | F* | B |
|---|---|---|---|---|---|---|
| Boot -> spring-boot | 23 | 5 | 18 | 4 | 9 | 10 |
| Data -> spring-data | 17 | 1 | 16 | 4 | 8 | 5 |
| MVC -> spring-web | 16 | 1 | 15 | 3 | 9 | 4 |
| Security -> spring-security | 7 | 2 | 5 | 1 | 4 | 2 |
| Cloud -> spring-cloud | 6 | 1 | 5 | 0 | 5 | 1 |
| GraphQL -> spring-graphql | 3 | 1 | 2 | 1 | 2 | 0 |
| Messaging -> spring-messaging | 3 | 1 | 2 | 0 | 3 | 0 |
| Batch -> spring-batch | 3 | 1 | 2 | 0 | 2 | 1 |
| WebSocket -> spring-websocket | 2 | 1 | 1 | 1 | 1 | 0 |
| Debugger -> spring-runtime | 8 | 3 | 5 | 0 | 1 | 7 |
| Initializr -> spring-initializr | 2 | 1 | 1 | 0 | 1 | 1 |
| **Total** | **90** | 18 | 72 | 14 | 45 | 31 |
