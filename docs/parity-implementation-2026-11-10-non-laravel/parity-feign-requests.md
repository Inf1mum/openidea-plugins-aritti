# Source-owned Feign HTTP request workflow

Implemented in `plugins/spring-web/src/feign_requests.rs`, with public wrappers in `src/source_lenses.rs`. Parent wired native lenses, command dispatch and manifest registration. Client requests remain separate from server EndpointsProvider.

## Public baseline

Checked the current public reference and current SpringMvcContract implementation:
- https://docs.spring.io/spring-cloud-openfeign/reference/spring-cloud-openfeign.html
- https://github.com/spring-cloud/spring-cloud-openfeign/blob/main/spring-cloud-openfeign-core/src/main/java/org/springframework/cloud/openfeign/support/SpringMvcContract.java
- Retrieved contract source retained in `/tmp/parity-SpringMvcContract.java`.

Implemented positive literal default SpringMvcContract subset: owned Java/Kotlin FeignClient interface methods, literal URL/path and name, Spring method mappings (RequestMapping without method uses GET), literal positive query/header conditions, explicit scalar path/query/header bindings, single body, first consumes media type, all produces media types. URL/configuration interpolation, custom configuration, class mappings, inheritance, conflicting or unmodeled bindings and foreign/shadowed annotations withhold the workflow.

Each request reviews target origin and parameter/body samples and previews exact content. Samples encode path/query segments; header/control injection and invalid JSON samples are rejected. Media wildcards require reviewed matching concrete Content-Type. Lens arguments bind source path, text digest, document version, physical method and request identity; source/root are checked again after prompts and confirmation. Output filename includes source identity digest to distinguish identical source names in different modules. HTTP content identifies physical source.

Existing examples open unchanged; denied writes offer the exact reviewed content through clipboard. No network/process operation occurs. SDK offers no atomic exclusive-create primitive, so the existence check cannot guarantee exclusion of an external filesystem writer racing creation.

## Meaningful verification

Seven Feign tests cover native lens-to-command execution and full HTTP content; absence of server endpoint publication; unchanged existing files and clipboard fallback; source identity filename separation; Java enum/default GET and Kotlin literal support; foreign/dynamic/configured/inherited/unmodeled source refusal; stale same-version source/project changes; target/sample injection; media wildcards; cancellation and invalid JSON. Existing source_lenses GraphQL custom-root test retains positive request generation and now asserts fixed QueryMapping does not produce a wrong-root native lens, plus global command refusal.

Full Spring Web test receipt: `/tmp/parity-feign-test.log` (165 passed before final source-comment assertion, final rerun below). Parent owns final merged Wasm build.

## Remaining scope

Partial plugin-side coverage: inherited interfaces; composed mappings; property/configuration resolution; collection/query-map/multipart bindings; custom contracts; configured encoders/interceptors; discovery/load-balancing targets; runtime bodies. Next actions require separate source models or explicit reviewed adapters, not guessing runtime execution. This workflow emits reviewed source examples and does not establish full OpenFeign/runtime parity. No host/API changes were made.
