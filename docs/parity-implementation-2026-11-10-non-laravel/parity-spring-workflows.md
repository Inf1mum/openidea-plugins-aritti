# Spring workflow implementation — 2026-11-10

Owned: spring-web, spring-integration, spring-shell, spring-ws, spring-runtime, spring-initializr. Laravel and host/SDK changes excluded. Existing shared-kit changes preserved; packaged resource sync performed externally by root is not attributed to this worker.

## Latest behavior sources

- https://www.jetbrains.com/help/idea/endpoints-tool-window.html (2026.3 help: request generation and OpenAPI export)
- https://www.jetbrains.com/help/idea/spring-support-tutorial.html (HTTP client generation and endpoint navigation)
- https://docs.spring.io/spring-framework/reference/web/webmvc/mvc-controller/ann-requestmapping.html (mapping contracts)
- https://www.thymeleaf.org/doc/tutorials/3.1/usingthymeleaf (literal fragment expressions)

Root is collecting exact Marketplace versions. Child receipts record Shell/WS/Integration/Initializr/Runtime versions independently.

## Spring Web implemented

- Correct annotated endpoint identity: route handler now retains method name alongside class and exact physical method location; previously annotations lost their method while functional routes retained it. Updated endpoint display assertions plus source-slice identity regression.
- HTTP `.http`/`.rest` request paths complete and navigate to local mappings. Provider/activation registered for native `http` language. Literal relative, HTTP(S), and `{{host}}` authorities admitted; replacement excludes authority, query and fragment. HEAD completion uses GET fallback. Body text cannot manufacture request references; dynamic runtime paths withheld from navigation.
- Literal HTML/JSP resource links (`src`, `href`, Thymeleaf `@{/asset}` and JSP URL tag values) navigate to Spring static roots. Comments/dynamic expressions ignored, query/fragment suffixes retained outside lookup, traversal/glob paths rejected.
- Source-declared MVC model-name completion/navigation scoped to literal handler-returned views; explicit ModelAttribute methods and Model/ModelMap.addAttribute calls admitted; th:each/th:with locals excluded.
- Opt-in exact Thymeleaf single-expression injection into installed core `spel` language; no entities, composites or phantom standalone target.
- Literal Thymeleaf fragment/whole-template navigation for `th:insert`, `th:replace`, legacy include and HTML5 data attributes. Local `this`/empty-template fragment references use current unsaved source. Named fragment targets use exact declaration range; ambiguous matching template files return all matching physical targets. Dynamic selectors, parameterized calls and stack-of-runtime-template resolution remain unsupported.
- API1.4 source-backed endpoint code lenses generate HTTP files after active source/version/endpoint validation; unrestricted mappings require explicit method choice. Foreign annotation imports are rejected.
- Tools command `copy-endpoints-openapi` exports declaration-only OpenAPI3.0.3 JSON using serde_json, explicit HTTP methods, unique required path parameters and all matching handler identities. Does not invent DTO schemas. ANY/wildcard/regex mappings remain in `x-openidea-unexported-mappings`. No network; optional existing clipboard permission. Added direct command and format behavior tests.
- Gateway completion includes project-defined predicate/filter factories and distinguishes kind. Expanded `.name` property/YAML completion excludes shortcut `=`; validation uses same admitted kinds. Added YAML/properties and kind-separation regressions.

## Validation

Logs: `/tmp/parity-spring-web-test.log`, `/tmp/parity-spring-web-build.log`, `/tmp/parity-cloud-test.log`, and per-module `/tmp/parity-{websocket,messaging,graphql,webflow}-test.log`.

Root Web test suite: **118 passed, 0 failed**; fresh release Wasm build passing. Nested module suites: WebSocket31, Messaging21, GraphQL36, WebFlow13, Cloud21 passed. No native desktop acceptance or signed distribution performed.

## Remaining feasible plugin work (not host blockers)

- Complete original mapping predicates/parameter/headers/consumes/produces request metadata and source-derived request samples; exact DTO/body inference needs optional semantic adapters, not blanket host excuse.
- OpenAPI export remains deliberately declaration-only; schema generation, runtime conditions, normalized regex/wildcards and runtime dynamic mappings need separate adapters/modeling.
- Thymeleaf markup selectors, parameterized fragment invocation, model-variable inference and template resolver semantics remain plugin/adaptor work. Literal known-template/fragment completion is implemented, with exact replacement ranges and unsaved same-template fragments.
- Gateway expanded args validation, Java DSL routes, Stream destination/group/binder settings, service discovery/config/circuit-breaker assistance, raw WebSocketHandler/SockJS and live broker adapters remain feasible plugin/adaptor scope.
- No general read-only usages/atomic rename model added to MVC template model or merged GraphQL/Messaging destinations in this delivery.
- No claim that finite changes close all six maintained packages or whole original counterparts. Native integration acceptance remains outstanding.

Child evidence: `/tmp/parity-shell-ws.md`, `/tmp/parity-integration.md`, `/tmp/parity-initializr-runtime.md` (agents continue further implementation and update receipts).

Detailed merged-counterpart feature inventory: `/tmp/parity-spring-web-feature-inventory.md`.

## Later checkpoint

- API1.4 Web language features now implement source-backed HTTP request code lenses, current-editor/source/version revalidation, explicit method choice for unrestricted ANY mappings, foreign annotation suppression and opt-in literal Thymeleaf SpEL ranges backed by the core plugin's real language.
- MVC model-name completion/navigation now covers explicit Model/ModelMap.addAttribute and named ModelAttribute producers scoped to literal returned views, excluding template lexical locals. No typed-member/global absence claim.
- OpenAPI export now includes source-proven query/header parameter scalar schemas, Optional handling, declared body media/type metadata, produces and literal class+method constraints. Unknown/default/dynamic binding cases are withheld instead of guessed.
- Subagents continued beyond initial receipts: Initializr complete approved-Python ZIP workflow and CLI result verification/open; Runtime concrete browser/send/request-file workflows; Shell modern v3/v4/Kotlin profiles; WS package/class/method namespace authority; Integration explicit import-tree canonical/alias rename, NamedInterface fix and Modulith source badges. See their continuously updated detailed inventories for final counts/residuals.

GraphQL query/mutation handler request lenses now use exact fresh SDL wrappers/custom roots/leaf-vs-object selection and current configured servlet/GraphQL path. Conflicting schema signatures/profile paths and stale source are refused. Generated JSON POST files preserve optional clipboard fallback, never overwrite user request files, and explicitly mark custom input null samples for review.

Renderer qualification (2026-11-11): exact API1.4 frontend injectedHighlights uses a CodeMirror grammar, not Rust SyntaxProvider. Thymeleaf SpEL ranges therefore establish routing only; rendered expression colorization remains unverified/unimplemented. Setting wording corrected from highlighting to routing. No injection-rendering parity promotion.

Current Web checkpoint:132 root tests, GraphQL37, WebSocket31, Messaging21, WebFlow13, Cloud21. MVC XML54 profile rows/7 regressions integrated; source-qualified required request examples and current-source action chooser integrated. Source model links include directly returned ModelAndView constructor/addObject chains. Native ordinary HTTP completion ownership is preserved; plugin supplements HEAD→GET only. Runtime source-inlay checkpoint72 tests. No current-host visual/behavior signoff inferred from these checks.

Runtime checkpoint77: added reviewed native Database setup from one captured property source, target-bound SHA256 ownership and optional native capability; no connect/SQL or active-datasource identity claim. Web checkpoint133: source-save endpoint exports and template-creation listings now refresh before cacheTTL through owned lifecycle events; template text does not enter absence-check bounds. Shell checkpoint94 includes native safe occurrence highlights/lenses, keeping unsafe automatic index Rename disabled by exact released-host contract evidence.

## Latest verification checkpoint — 2026-11-11

This checkpoint supersedes earlier incremental counts above: Web136, GraphQL38, Runtime78 tests pass. Web/Runtime release Wasm builds passed before the final GraphQL source-binding correction and are being refreshed again. Kotlin literal controller views now navigate and complete via the same physical source model as Java. Source-backed model links continue to revalidate physical source before publishing stale cached facts.

GraphQL shortcut mapping parents are now fixed Query/Mutation/Subscription, as specified by official Spring annotation source (https://raw.githubusercontent.com/spring-projects/spring-graphql/main/spring-graphql/src/main/java/org/springframework/graphql/data/method/annotation/QueryMapping.java). Existing custom-SDL-root remapping was incorrect and has been removed from source definitions/completion/diagnostics. Request generation refuses fixed shortcuts targeting a different operation root; explicit SchemaMapping custom operation roots generate valid requests and are verified through the registered code-lens/command/file workflow. Subscription transport remains explicit residual.

Runtime native source display names are UTF8-clamped below native256-byte maximum; full captured provenance stays in the review and hashed ownership. Literal target parsers cover PostgreSQL/MySQL/MariaDB/ClickHouse HTTP/Oracle thin service/SQLServer explicit databaseName. Unknown options/credentials/multi-host formats stay manual; no network, connect, introspection or SQL occurs during setup.

The precise remaining work is the feature inventory table and child inventories, not an assertion that all plugin-only behavior has been completed. Full semantic/type/effective runtime models still require substantial optional adapters; released native rendering/debugger/rename-policy boundaries remain separately identified.

Final GraphQL source-doc tranche:44 module tests pass, including Java/Kotlin Mutation/Subscription fixed-parent contracts, exact imported/FQN ownership, alias/shadow/foreign/dynamic/escaped refusal and bounded exact SDL list/non-null field+argument signature hovers. See /tmp/parity-graphql-hover.md. Full Web tests and Wasm are refreshed after this final source change.

## Final executable evidence

Fresh final logs: Web136 (/tmp/parity-spring-web-test.log), GraphQL44 (/tmp/parity-graphql-hover-test.log), Runtime78 (/tmp/parity-runtime-inlays-test.log), Integration170 (/tmp/integration-test.log), Batch41 (/tmp/integration-batch-test.log), Modulith28 (/tmp/integration-modulith-test.log), Shell94 (/tmp/parity-shell-test.log), WS125 (/tmp/parity-ws-test.log), Initializr134 (child inventory). Final Web, Runtime and Integration release Wasm builds pass in /tmp/parity-spring-web-build.log, /tmp/parity-runtime-inlays-build.log and /tmp/integration-build.log; other package fresh builds recorded in child reports. Owned diff-check clean. This supersedes all incremental counts above. Native IDE acceptance, packaging/signing/publication and whole-original parity are not inferred.

## Requested concrete Web continuation — 2026-11-11

Completed source-owned Feign HTTP request/native lenses without server endpoint pollution (seven behavior tests); positive Java record/public-field/ordinary-bean-getter/Kotlin-data/enum/collection/map/recursive source DTO OpenAPI shapes (eight behavior tests, consistent captured route/source model, no guessed media/validation/runtime serializer); exact literal Gateway Java DSL route source info (Cloud25 default+16 embedded); reviewed optional Node>=22 graphql-transport-ws subscription runtime (12 Rust tests+8 actual Node24.18 disposable RFC6455 cases); and source/schema-to-editable subscription document bridge (three owner behavior tests). Native GraphQL actions now require proved complete-SDL operation ownership. Explicit schema definitions correctly disable omitted implicit roots, and generated client docs do not enter SDL source models.

Final combined tests: Web166, GraphQL47 pass; Cloud25 default/16 embedded pass. Latest Web Wasm refresh /tmp/parity-spring-web-build.log; native acceptance remains separate. Exact receipts and remaining grammar/model/runtime work: /tmp/parity-feign-requests.md, /tmp/parity-source-dto-schemas.md, /tmp/parity-gateway-dsl.md, /tmp/parity-graphql-subscriptions.md, /tmp/parity-graphql-subscriptions-receipt.json, /tmp/parity-subscription-drafts.md. Full original-plugin parity remains open; this checkpoint closes the concrete requested source workflows, not every broader semantic/effective model.
