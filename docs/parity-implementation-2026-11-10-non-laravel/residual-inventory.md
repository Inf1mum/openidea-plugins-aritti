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
| F09 | Flyway | Quoted/dialect tables, naming, constants, Kotlin entities, XML mappings | A-M | |
| F10 | Liquibase | Other dialects, online snapshot, GUI undo | U | |
| F11 | Hibernate | Live schema compare, effective persistence units, other ORM versions | A-M | |
| F12 | JPQL console | PostgreSQL/MySQL/MariaDB unverified | U | |
| F13 | JPQL console | Mutation mode, entity-graph editing, effective config | A-M | |
| F14 | OpenAPI | Generator JSON config import | A-S | done |
| F15 | OpenAPI | Source-context Add path/operation/response | A-M | partial (YAML Add operation/response) |
| F16 | OpenAPI | Remote catalog, SwaggerHub, typed client URL completion | A-M | |
| F17 | OpenAPI | Full compatibility report | A-M | |
| F18 | OpenAPI | Draft spec from framework mappings | A-L | |
| F19 | OpenAPI | Staged generation apply/Undo on desktop | U | |
| F20 | GitLab | GFM mentions/emoji/math, persistent drafts, timeline | A-M | partial (mentions/emoji) |
| F21 | GitLab | Self-hosted servers, named multi-account | A-M | |
| F22 | GitLab | Native Review Mode, git-log actions | H | |
| F23 | GitLab | No live account/glab acceptance | U | |
| F24 | Dependency Audit | PyPI markers/groups/pylock | A-M | partial (pylock PyPI index) |
| F25 | Dependency Audit | Workspace/link/alias graph | A-M | partial (npm: aliases) |
| F26 | Dependency Audit | Coordinate-to-declaration binding; more build profiles | A-M | |
| F27 | Dependency Audit | Ignore-profile import/export, safe-coordinate rows | A-S | partial (import/export) |
| F28 | Dependency Audit | Vulnerable-call analysis Java/Kotlin/Python | A-L | |
| F29 | Dependency Audit | Native Problems Security tab, semantic member queries | H | |
| F30 | dotenv | Runtime-loader/Compose-service/build-stage ownership | A-L | |
| F31 | dotenv | Native value folding/secret hiding, custom triggers | H | |
| F32 | Ignore | Template-generator controls, preview persistence | A-M | partial (recent-first) |
| F33 | Ignore | Unicode/unsupported-pattern unused-rule model | A-S | done (exact bytes; unprovable misses withheld) |
| F34 | Ignore | Native tree filtering, editor closing, dynamic associations | H | |
| F35 | Properties | Kotlin, modulepath, custom loader, constant-key, inherited, live graph | A-L | |
| F36 | Properties | Native bundle grouping, encoding chooser | H | |
| F37 | Ideolog | Native line hiding, heatmap, terminal filtering, exact rendering | H | |
| F38 | Ideolog | Inline editor syntax from JVM adapter | A-M | |
| F39 | Ideolog | Live-tail dock/grant untested; Java 17 not exercised | U | |
| F40 | Tasks | Native changelist decoration, after-commit closure, stacktrace analyzer | H | |
| F41 | Tasks | Generic family real-host smoke; no live tenant for any bridge | U | partial (installer check done) |
| F42 | Tasks | Owned JQL editor; other trackers need bridges | A-S | done (`jql:` prefix); Bugzilla/Trac dropped |
| F43 | Quarkus | LS broker desktop acceptance; other OS/profiles; multi-module | U | |
| F44 | Quarkus | Config overlay: new unsaved files, YAML, third-party sources | A-M | |
| F45 | Quarkus | Qute Java-embedded routing, custom roots, inherited data model | A-L | |
| F46 | Quarkus | JAX-RS roots/custom annotations; scheduler expressions | A-M | |
| F47 | Quarkus | MPJava diagnostics while config dirty (upstream LS limit) | H | |
| F48 | Kubernetes | Complete built-in/CRD schemas, admission/CEL, defaults | A-L | |
| F49 | Kubernetes | Kustomize transformed/flow/remote refs; generated identity | A-M | partial (generator/replacement file refs) |
| F50 | Kubernetes | Label graph affinity/node-label models, escaped scalar edits | A-S | partial (affinity/spread links) |
| F51 | Helm | Go-template scope, tpl/include, OCI registry intelligence | A-L | partial (`.Values` navigation) |
| F52 | Devcontainer | Full JSON Schema/Compose semantics, template/feature metadata | A-M | |
| F53 | Devcontainer | Native remote IDE backend, per-launch introspection | H | |
| F54 | DDEV | Composer root/hooks/cwd, terminal interception (host issues 2407/2408) | H | |
| F55 | DDEV/Kubernetes | Native Database/debugger/remote/Windows acceptance | U | |
| F56 | Initializr | ZIP timestamps/modes; non-start.spring.io grants | A-S | partial (modes/mtime done; grants not) |
| F57 | Initializr | Linux/Windows exclusive-commit profiles never run | U | |
| F58 | Initializr | Add Starters effective build models | A-M | |
| F59 | Initializr | Native New Project wizard, JDK/SDK config, IDE build import | H | |
| F60 | Runtime | External-library/overloaded source semantics | A-L | |
| F61 | Runtime | Remote/JMX management transport | A-L | |
| F62 | Runtime | Saved-request history, credential selection, health detail | A-M | |
| F63 | Boot run | Custom args, build-plugin resolution, multi-module root | A-M | |
| F64 | Boot metadata | Gradle resolution, inherited properties, alternate repos | A-M | |
| F65 | Integration | Effective context ownership for global usages | A-L | |
| F66 | Integration | DSL flow edges, Kotlin DSL | A-M | |
| F67 | Batch | XML structural/XSD rules; Batch 6 migration | A-M | |
| F68 | Modulith | Type/member/hierarchy semantics, move-to-API | A-L | |
| F69 | Modulith | Kotlin descriptor creation, bean-completion module warning | A-S | skip (user decision) |
| F70 | Batch | Operator next/restart/abandon/recover/stop, datasource authority | A-M | |
| F71 | Integration | SpEL/XPath/embedded-language assistance | A-M | |
| F72 | Spring core | Library assignability, effective conditional contexts, bean graph | A-L | |
| F73 | Spring core | All classpath metadata, dynamic build resolution, converters | A-M | |
| F74 | Spring core | Data projection/query/entity-graph generators | A-L | |
| F75 | Spring core | Security chain, effective authorization | A-L | |
| F76 | Spring core | JPA hierarchy/XML mappings/converters/reverse engineering | A-L | |
| F77 | Spring Web | MVC mapping predicates/params, body-aware request samples | A-M | |
| F78 | Spring Web | MVC OpenAPI export conditions, regex/ANY mappings | A-L | |
| F79 | Spring Web | Thymeleaf selectors, model inference, resolver semantics | A-M | partial (advice model attrs, fragment args) |
| F80 | Spring Web | SpEL/EL injection, URL inlay actions | A-M | |
| F81 | Spring Web | MVC XML imports/profiles, XSD validation | A-M | |
| F82 | Spring Web | Template catalogue (licence); multi-action marker popup (host) | L/H | blocked |
| F83 | Spring Web | Cloud Gateway args/DSL, Stream, discovery/config | A-M | partial (unknown expanded args) |
| F84 | Spring Web | Cloud auto-configured model, HttpExchange/contract Feign | A-L | |
| F85 | Spring Web | GraphQL resolver graph, non-root hints, transport | A-L | partial (Kotlin aliases, BatchMapping) |
| F86 | Spring Web | Messaging/WebSocket dynamic SpEL, SockJS, XML schema model, broker | A-L | partial (XML enum/ref checks) |
| F87 | Spring Web | Web Flow Java/custom flow models | A-M | |
| F88 | Shell | Cross-file registry, dynamic registration, inherited availability | A-L | |
| F89 | Shell | Native terminal command-spec hooks | H | |
| F90 | WS | QName refs across XML/WSDL/XSD, effective context | A-L | |
| F91 | WS | Full XPath validation, WSDL generation, OXM/CXF semantics | A-L | |
| F92 | Twig | Trait/embed/namespaced loader, macro-parameter Rename | A-M | |
| F93 | Twig | Formatter, custom delimiters | A-M | |
| F94 | Twig | Dynamic context, extension types | A-L | |
| F95 | Twig | Embedded type resolution, Enter/pairing, debugger mapping, plugin-lexer rendering | H | |
| F96 | Symfony | Service/route/template/domain reference models, reviewed Rename | A-L | |
| F97 | Symfony | Profiler collector detail, pagination, remote modes | A-M | |
| F98 | Symfony | Doctrine/forms/container types | A-L | |

Totals at 2026-11-11: 98 items; 5 done (F14, F33, F42 plus F27/F56 counted partial), about 14 partial, F69 skipped, F82 blocked.
