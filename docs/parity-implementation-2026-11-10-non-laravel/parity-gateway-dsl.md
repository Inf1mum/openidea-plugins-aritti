# Gateway Java DSL source information (2026-11-11)

Scope: only `plugins/spring-web/modules/cloud/src/gateway_dsl.rs` and Cloud `src/lib.rs` changed for this task. Existing concurrent Cloud changes were preserved. No parent Web source, window, manifest, shared crate or host/API edits.

## Official semantic baseline

Primary sources refreshed for this implementation:
- https://spring.io/guides/gs/gateway — Java Gateway configuration via a RouteLocator bean and injected RouteLocatorBuilder.
- https://docs.spring.io/spring-cloud-gateway/reference/spring-cloud-gateway-server-webflux/request-predicates-factories.html — request Path and Method predicates.
- https://raw.githubusercontent.com/spring-cloud/spring-cloud-gateway/main/spring-cloud-gateway-server-webflux/src/main/java/org/springframework/cloud/gateway/route/builder/RouteLocatorBuilder.java — `routes()`, explicit string route ID + predicate lambda, final `build()` contract.
- https://raw.githubusercontent.com/spring-cloud/spring-cloud-gateway/main/spring-cloud-gateway-server-webflux/src/main/java/org/springframework/cloud/gateway/route/builder/PredicateSpec.java — literal String varargs `path` and `method` overloads.
- https://raw.githubusercontent.com/spring-cloud/spring-cloud-gateway/main/spring-cloud-gateway-server-webflux/src/main/java/org/springframework/cloud/gateway/route/builder/UriSpec.java — literal String destination URI contract.

Original parser/implementation authored locally; no proprietary code or third-party implementation copied. This is a physical-source inventory, not runtime activation or effective routing proof.

## Implemented capability / code / evidence

- `gateway_dsl::routes`: Java-only, max256KiB, explicit Spring import or fully qualified ownership for Bean, RouteLocator and RouteLocatorBuilder; same-file type/import shadows, generic type-parameter uncertainty and Java Unicode preprocessing are withheld.
- Complete direct method-body return grammar: the sole injected builder parameter feeds `.routes().route("id", r -> r.path("/x", ...).and().method("GET", ...).uri("lb://service")).build()`. Path-only, Method-only and reversed predicate order work. Each route lambda owns its own predicates and URI; repeated same-kind AND groups are withheld instead of flattening their semantics.
- `GatewayRoute`: explicit ID, upstream URI, Path/Method values and exact source ranges for every literal (UTF-8 offsets converted through spring_text to SDK positions).
- `JavaFacts.gateway_routes` is separate from `clients` and `servers`. Gateway destinations never enter controller endpoints or Feign matching.
- Existing Cloud view: distinct Gateway file group, route ID and destination/predicate details, route count, existing activation handler opens exact route ID location. Live source update reparses/removes obsolete DSL facts.
- Tests: literal two-route ownership and exact range contents; dynamic ID/URI/path expression refusal; foreign imports, source/type-parameter shadows and reassignment refusal; nested helper/lambda route confusion, OR/unknown calls and Unicode escapes withheld; comments cannot contribute fake URI; Bean is required; Kotlin is withheld. Registered Cloud view test checks displayed details, actual source opening, no server/client pollution and invalidation after source change.

## Boundaries and next actions

Supported source shape is deliberately exact and incomplete. Filters, custom predicates, other predicate kinds, `or`/`negate`, block lambdas, method references, helper/local builder variables, multiple parameters/statements, anonymous/random route IDs, constants, HttpMethod enum arguments, boolean path overload, escaped/text-block literals, generic configurations, Kotlin/WebMVC Gateway DSL and imported/composed configuration are withheld. Any unsupported call in an admitted method withholds its entire route chain, avoiding partial ownership claims. Next plugin work is a typed expression/receiver/boolean model with independently tested overloads before broadening this grammar. Effective profiles/classpath/application activation and live route inspection remain separate adapter/runtime work. No host/API limitation is asserted for the implemented capability; no native desktop acceptance was performed.

## Verification

- `cargo test --locked --manifest-path plugins/spring-web/modules/cloud/Cargo.toml`: **25 passed** (`/tmp/gateway-dsl-test.log`).
- `cargo test --locked --features embedded --manifest-path plugins/spring-web/modules/cloud/Cargo.toml`: **16 passed** (`/tmp/gateway-dsl-embedded-test.log`).
- `cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/spring-web/modules/cloud/Cargo.toml`: passed (`/tmp/gateway-dsl-build.log`).
- `git diff --check -- plugins/spring-web/modules/cloud`: passed.
