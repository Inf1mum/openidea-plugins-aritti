# Non-Laravel residual inventory (2026-11-11)

Counted from the family reports; class and size are judgment calls, not report data. **A**=adapter (S/M/L), **H**=host/API limit, **U**=unverified (built, no real-host/live evidence), **L**=licence-blocked. Status after the 2026-11-11 session: `done`, `partial`, `skip` (decided not to do), else open. Rebuild from the family reports if in doubt; do not infer closure from this file.

| ID | Family | Item | Class | Status |
|---|---|---|---|---|
| F01 | all | Safe native indexed Find Usages/Rename | H | |
| F02 | all | Exact-package native desktop/real-host acceptance | U | |
| F03 | Runtime/Quarkus/Security/Tasks/DDEV | Debugger eval, hot-swap, run-config restart, auto-attach, breakpoint restore | H | |
| F04 | Integration | Native Structure window, Project decorators, diagram/type-model hooks | H | |
| F05 | Flyway | Effective DB authority not proven | U | |
| F06 | Flyway | Enterprise diff/snapshot/SQL generation (licensed runtime) | H | |
| F07 | Database | DB/entity-derived init/diff DDL beyond H2 | A-L | |
| F08 | Database | Native datasource inspection, entity queries, console | H | |
| F09 | Flyway | Quoted/dialect tables, naming, constants, Kotlin entities, XML mappings | A-M | partial (constants, implicit naming, quoted/dialect ids, Kotlin entities, orm.xml/persistence.xml; bare root orm.xml not under META-INF open) |
| F10 | Liquibase | Other dialects, online snapshot, GUI undo | U | partial (offline structure checks verified against dbchangelog XSD/docs; other dialects, online snapshot, GUI undo open) |
| F11 | Hibernate | Live schema compare, effective persistence units, other ORM versions | A-M | partial (persistence.xml units, spring.jpa settings, namespace Info; Flyway-side Hibernate adapter unchanged; build-file version detection open) |
| F12 | JPQL console | PostgreSQL/MySQL/MariaDB unverified | U | |
| F13 | JPQL console | Mutation mode, entity-graph editing, effective config | A-M | decided: dry-run only (count preview); commit path not wanted |
| F14 | OpenAPI | Generator JSON config import | A-S | done |
| F15 | OpenAPI | Source-context Add path/operation/response | A-M | partial (YAML+JSON Add operation/response/path/query parameter) |
| F16 | OpenAPI | Remote catalog, SwaggerHub, typed client URL completion | A-M | |
| F17 | OpenAPI | Full compatibility report | A-M | partial (breaking-change report; oneOf/anyOf, constraints, security not analysed) |
| F18 | OpenAPI | Draft spec from framework mappings | A-L | |
| F19 | OpenAPI | Staged generation apply/Undo on desktop | U | |
| F20 | GitLab | GFM mentions/emoji/math, persistent drafts, timeline | A-M | partial (mentions/emoji, math source, local drafts; draft-note request fields verified vs GitLab docs, never run live; native diff-line add open) |
| F21 | GitLab | Self-hosted servers, named multi-account | A-M | |
| F22 | GitLab | Native Review Mode, git-log actions | H | |
| F23 | GitLab | No live account/glab acceptance | U | |
| F24 | Dependency Audit | PyPI markers/groups/pylock | A-M | partial (pylock, pyproject/PEP 735 pins; markers not evaluated) |
| F25 | Dependency Audit | Workspace/link/alias graph | A-M | partial (npm: aliases, workspaces/link local rows) |
| F26 | Dependency Audit | Coordinate-to-declaration binding; more build profiles | A-M | partial (direct binding + chains; uv.lock/poetry.lock/Yarn v1+berry/npm v3/pnpm v9 checked against real files and fixed; workspace link targets not followed in chains) |
| F27 | Dependency Audit | Ignore-profile import/export, safe-coordinate rows | A-S | done (import/export + checked/skipped rows on Scan Project; Maven/Gradle/sbt imports lack rows) |
| F28 | Dependency Audit | Vulnerable-call analysis Java/Kotlin/Python | A-L | |
| F29 | Dependency Audit | Native Problems Security tab, semantic member queries | H | |
| F30 | dotenv | Runtime-loader/Compose-service/build-stage ownership | A-L | partial (Compose interpolation, service ownership, Dockerfile ARG links; inherited stage ARGs, stale ARG usages after Dockerfile-only save open) |
| F31 | dotenv | Native value folding/secret hiding, custom triggers | H | |
| F32 | Ignore | Template-generator controls, preview persistence | A-M | partial (recent-first) |
| F33 | Ignore | Unicode/unsupported-pattern unused-rule model | A-S | done (exact bytes; unprovable misses withheld) |
| F34 | Ignore | Native tree filtering, editor closing, dynamic associations | H | |
| F35 | Properties | Kotlin, modulepath, custom loader, constant-key, inherited, live graph | A-L | partial (same-file constants, @PropertyKey; Kotlin ctor params, modulepath, custom loader open) |
| F36 | Properties | Native bundle grouping, encoding chooser | H | |
| F37 | Ideolog | Native line hiding, heatmap, terminal filtering, exact rendering | H | |
| F38 | Ideolog | Inline editor syntax from JVM adapter | A-M | no change needed for cached JVM tokens (already present); group-to-token mapping beyond time/severity/category open |
| F39 | Ideolog | Live-tail dock/grant untested; Java 17 not exercised | U | |
| F40 | Tasks | Native changelist decoration, after-commit closure, stacktrace analyzer | H | |
| F41 | Tasks | Generic family real-host smoke; no live tenant for any bridge | U | partial (installer check done) |
| F42 | Tasks | Owned JQL editor; other trackers need bridges | A-S | done (`jql:` prefix); Bugzilla/Trac dropped |
| F43 | Quarkus | LS broker desktop acceptance; other OS/profiles; multi-module | U | |
| F44 | Quarkus | Config overlay: new unsaved files, YAML, third-party sources | A-M | partial (saved + dirty YAML, .env, config/ sources; ordinals 255/265/260/250/295/100 verified against SmallRye; live JDT LS tests not run) |
| F45 | Quarkus | Qute Java-embedded routing, custom roots, inherited data model | A-L | partial (unresolved template warnings, suffixes, data-model completion; inherited/generic models open; no template-root property exists) |
| F46 | Quarkus | JAX-RS roots/custom annotations; scheduler expressions | A-M | partial (@Scheduled checks, effective REST path) |
| F47 | Quarkus | MPJava diagnostics while config dirty (upstream LS limit) | H | |
| F48 | Kubernetes | Complete built-in/CRD schemas, admission/CEL, defaults | A-L | partial (CRD pruning/int-or-string/scope checks; null on non-nullable = Info per docs, source not read; built-in schemas, admission/CEL open) |
| F49 | Kubernetes | Kustomize transformed/flow/remote refs; generated identity | A-M | partial (generator/replacement refs, inline patch selectors, bases cycles, helm valuesFile) |
| F50 | Kubernetes | Label graph affinity/node-label models, escaped scalar edits | A-S | done (affinity/spread links + single-line quoted escaped scalars; multiline/block still refused) |
| F51 | Helm | Go-template scope, tpl/include, OCI registry intelligence | A-L | partial (.Values nav, include/template nav, with/range scope; else-branch scope conservative) |
| F52 | Devcontainer | Full JSON Schema/Compose semantics, template/feature metadata | A-M | partial (hostRequirements, Compose refs + Compose spec checks, OCI id shape, local feature options) |
| F53 | Devcontainer | Native remote IDE backend, per-launch introspection | H | |
| F54 | DDEV | Composer root/hooks/cwd, terminal interception (host issues 2407/2408) | H | |
| F55 | DDEV/Kubernetes | Native Database/debugger/remote/Windows acceptance | U | |
| F56 | Initializr | ZIP timestamps/modes; non-start.spring.io grants | A-S | partial (modes/mtime done; grants not) |
| F57 | Initializr | Linux/Windows exclusive-commit profiles never run | U | |
| F58 | Initializr | Add Starters effective build models | A-M | partial (Gradle platform BOM/catalog, Maven aggregator module pick, unmanaged explicit versions; metadata field names verified vs real start.spring.io) |
| F59 | Initializr | Native New Project wizard, JDK/SDK config, IDE build import | H | |
| F60 | Runtime | External-library/overloaded source semantics | A-L | partial (conditions/configprops source links, overload matching; tree view still lenient) |
| F61 | Runtime | Remote/JMX management transport | A-L | |
| F62 | Runtime | Saved-request history, credential selection, health detail | A-M | partial (recent requests, masked health detail tree; gutter bean chooser open) |
| F63 | Boot run | Custom args, build-plugin resolution, multi-module root | A-M | partial (module runs, Run with Profiles/Arguments; include/config.import discovery open) |
| F64 | Boot metadata | Gradle resolution, inherited properties, alternate repos | A-M | partial (profile/import discovery + Gradle-cache metadata via optional capability; alternate repos open) |
| F65 | Integration | Effective context ownership for global usages | A-L | partial (import/scan context graph; scan and @ImportResource rules verified against Spring source; filter/exclude args, selector subclass chains, Kotlin open) |
| F66 | Integration | DSL flow edges, Kotlin DSL | A-M | partial (Java/Kotlin DSL channel edges) |
| F67 | Batch | XML structural/XSD rules; Batch 6 migration | A-M | partial (chunk/tasklet exclusivity) |
| F68 | Modulith | Type/member/hierarchy semantics, move-to-API | A-L | partial (use-site checks for exact single-type imports; Kotlin open) |
| F69 | Modulith | Kotlin descriptor creation, bean-completion module warning | A-S | skip (user decision) |
| F70 | Batch | Operator next/restart/abandon/recover/stop, datasource authority | A-M | partial (job discovery + reviewed launch-command generation; no operator execution) |
| F71 | Integration | SpEL/XPath/embedded-language assistance | A-M | partial (SpEL syntax + @bean refs in XML/DSL; XPath and other *-expression attributes open) |
| F72 | Spring core | Library assignability, effective conditional contexts, bean graph | A-L | partial (literal @ConditionalOnProperty/@Profile/@ConditionalOnMissingBean heuristic, ObjectFactory/SortedSet) |
| F73 | Spring core | All classpath metadata, dynamic build resolution, converters | A-M | |
| F74 | Spring core | Data projection/query/entity-graph generators | A-L | partial (projection, record DTO, @EntityGraph checks) |
| F75 | Spring core | Security chain, effective authorization | A-L | partial (method-security SpEL, verified anyRequest/ROLE_ rules, method-security-disabled Info) |
| F76 | Spring core | JPA hierarchy/XML mappings/converters/reverse engineering | A-L | partial (hierarchy identifier with orm.xml, inheritance and converter checks) |
| F77 | Spring Web | MVC mapping predicates/params, body-aware request samples | A-M | mostly pre-existing (predicates in request gen + OpenAPI export; Name column added; Endpoints row columns need ui-kit change) |
| F78 | Spring Web | MVC OpenAPI export conditions, regex/ANY mappings | A-L | partial (Bean Validation constraints, regex path variables, status codes; ant mappings listed as unexported) |
| F79 | Spring Web | Thymeleaf selectors, model inference, resolver semantics | A-M | partial (advice model attrs, @SessionAttributes names, fragment args) |
| F80 | Spring Web | SpEL/EL injection, URL inlay actions | A-M | |
| F81 | Spring Web | MVC XML imports/profiles, XSD validation | A-M | partial (import closure, profile beans, resource locations; view-controller view-name open) |
| F82 | Spring Web | Template catalogue (licence); multi-action marker popup (host) | L/H | blocked |
| F83 | Spring Web | Cloud Gateway args/DSL, Stream, discovery/config | A-M | partial (arg tables + arity for ~34 factories, .or() DSL chains) |
| F84 | Spring Web | Cloud auto-configured model, HttpExchange/contract Feign | A-L | partial (@HttpExchange clients + drafts, Feign contextId/configuration) |
| F85 | Spring Web | GraphQL resolver graph, non-root hints, transport | A-L | partial (Kotlin aliases, BatchMapping, non-root unmapped-field Info hint) |
| F86 | Spring Web | Messaging/WebSocket dynamic SpEL, SockJS, XML schema model, broker | A-L | partial (XML enum/ref checks, unambiguous placeholders, BindingBuilder chains; producer-side routing key links open) |
| F87 | Spring Web | Web Flow Java/custom flow models | A-M | partial (Java-config flow registries) |
| F88 | Shell | Cross-file registry, dynamic registration, inherited availability | A-L | partial (inherited availability, duplicate provenance; inherited groups, conventional availability open) |
| F89 | Shell | Native terminal command-spec hooks | H | |
| F90 | WS | QName refs across XML/WSDL/XSD, effective context | A-L | partial (XSD attribute/content-model rules, WSDL QName navigation/warnings) |
| F91 | WS | Full XPath validation, WSDL generation, OXM/CXF semantics | A-L | partial (XSD rules, WSDL QNames, dynamic-wsdl pairs, void @ResponsePayload is Info (Spring-WS ignores it); DefaultWsdl11Definition literals in progress) |
| F92 | Twig | Trait/embed/namespaced loader, macro-parameter Rename | A-M | partial (`use`d blocks, templateNamespaces, twig.paths + env overrides + falsy-namespace rule verified vs TwigBundle, reviewed macro-param rename; list form in progress) |
| F93 | Twig | Formatter, custom delimiters | A-M | partial (literal Lexer delimiters in single-project setups; no twig.yaml option found in TwigBundle docs page) |
| F94 | Twig | Dynamic context, extension types | A-L | partial (controller render() literal variables for completion/definition; compact()/array() and @ns templates open) |
| F95 | Twig | Embedded type resolution, Enter/pairing, debugger mapping, plugin-lexer rendering | H | |
| F96 | Symfony | Service/route/template/domain reference models, reviewed Rename | A-L | partial (Doctrine links, autowiring alias navigation, missing-route warning, translation domains) |
| F97 | Symfony | Profiler collector detail, pagination, remote modes | A-M | partial (profiler paging, time/cache/logger collectors; events/request open) |
| F98 | Symfony | Doctrine/forms/container types | A-L | partial (form-field Info check) |

Totals at 2026-11-11: 98 items. Done: F14, F27, F33, F42, F50. Skipped: F69. Blocked: F82. Decided: F13 dry-run only. Partial: about 68 (see status column). About 25 untouched. Counts are judgment calls; partial does not mean near-complete.
