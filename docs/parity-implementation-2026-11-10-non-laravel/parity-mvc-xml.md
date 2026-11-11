# Spring MVC XML source-contract implementation — 2026-11-11

## Ownership

Edited only plugins/spring-web/src/xml_checks.rs and new mvc_xml_model.rs / mvc_xml_tables.rs. Parent owns and wired src/lib.rs provider routes; no shared crates, manifests or host APIs were edited by this task.

## Official source baseline

- Current schema: https://raw.githubusercontent.com/spring-projects/spring-framework/main/spring-webmvc/src/main/resources/org/springframework/web/servlet/config/spring-mvc.xsd
- Spring6.2 snapshot: https://raw.githubusercontent.com/spring-projects/spring-framework/v6.2.12/spring-webmvc/src/main/resources/org/springframework/web/servlet/config/spring-mvc.xsd
- Spring5.3 snapshot: https://raw.githubusercontent.com/spring-projects/spring-framework/v5.3.39/spring-webmvc/src/main/resources/org/springframework/web/servlet/config/spring-mvc.xsd
- Archived3.0,3.1,3.2,4.0,4.1,4.2,4.3 schemas: https://raw.githubusercontent.com/spring-projects/spring-framework/v4.3.30.RELEASE/spring-webmvc/src/main/resources/org/springframework/web/servlet/config/spring-mvc-4.3.xsd (replace filename version for other archived profiles).
- Public behavior: https://docs.spring.io/spring-framework/reference/6.2/web/webmvc/mvc-config/enable.html ; https://docs.spring.io/spring-framework/reference/web/webmvc-cors.html

Exact downloaded schema SHA256 values and rule counts are in /tmp/parity-mvc-table-receipt.json. Original XML snapshots are /tmp/parity-mvc-<profile>.xsd. Interface attribute/element facts are independently represented in Rust; no proprietary code was inspected.

The model has54 distinct schema-profile rows and10 admitted Beans-ref parent roles. Explicit schema filenames select a supported source contract; no schema declaration/unversioned filename uses current facts. Unknown/custom/conflicting profile selections remain quiet. This does NOT infer actual runtime Spring version: newer Spring jars may resolve legacy schema aliases to newer bundled resources. No unsupported-element/unsupported-attribute errors are emitted from URI version guesses.

## Implemented capability inventory

| Capability | Status / implementation / tests | Boundary |
|---|---|---|
| Required MVC attributes | Implemented across actual global/nested roles, named resource-chain/version-strategy/cache/view-resolver schema types included; mvc_xml_tables.rs + current_schema_checks_required_fields_in_their_actual_parent_roles | Only sampled known XSD attributes, not complete XML schema validation |
| MVC booleans | Implemented XML Schema true/false/1/0 forms with collapsed whitespace; dynamic placeholders quiet; booleans_accept_schema_lexical_forms_and_integers_keep_their_declared_width | No expression evaluation |
| Integer attributes | Implemented xsd:int vs xsd:long widths and literal parsing; same test above | Does not invent HTTP semantic ranges beyond XSD types |
| Versioned attribute/profile guards | Implemented renamed3.x camelCase annotations, removed/current path-matching fields and version-introduced nested types; versioned_profiles_do_not_apply_renamed_or_later_attribute_contracts | Unknown profiles quiet; source profile is not runtime version authority |
| Boolean value completion | Implemented prefix filtering and exact current-text UTF16 replace spans; completion_uses_current_unsaved_utf16_value_spans_without_host_access | Closed XML subset; no attribute-name completion or full DOM engine |
| Bean reference navigation/completion | Implemented declared unique same-document IDs for schema-backed bean attributes and actual Beans-ref child roles, including converter/resolver/interceptor/view/resource contexts; reference_roles_navigate_and_complete_only_unambiguous_declared_local_ids | No effective Spring contexts, assignability, imported/profile/nested contexts or classpath type proof |
| Bean absence diagnostics | Deliberately absent; reference_absence_is_never_reported_as_missing_runtime_bean proves unknown external beans are not errors | Full effective registry requires a separate semantic adapter |
| Source ownership and malformed XML protection | Bounded closed-element/attribute parser, namespace rebinding/expanded-attribute uniqueness, reserved prefix guards, closed quotes/tags/comments, no DTD/entities, depth/item/byte caps; closed_xml_namespace_and_context_guards_suppress_guessed_schema_roles | Conservative ASCII QName subset, no complete XML parser claim |
| Conventional unnamespaced beans snippets | Diagnostics admitted only for explicit canonical MVC children under a literal root beans wrapper; arbitrary foreign roots stay quiet | Reference ownership still requires canonical Spring Beans namespace |

Current/5.x/6.x schemas contain boolean lexical fields rather than closed MVC string enums. The implementation does not invent enumeration constraints for CORS HTTP methods or extension-friendly string fields.

## Verification

- cargo test --locked --manifest-path plugins/spring-web/Cargo.toml:129 tests passed after provider wiring and compatibility adjustment.
- Fresh release Wasm build passed. Command: cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/spring-web/Cargo.toml.
- Logs: /tmp/parity-mvc-test.log and /tmp/parity-mvc-build.log.
- Scoped git diff --check passes.
- No desktop acceptance, package install, publication or full native/IntelliJ parity claim.

## Remaining concrete plugin/adapter work

Full Spring context imports/profiles/aliases, semantic expected-type filtering for candidate beans, complete XSD/DOM path validation, cross-file safe reference ownership and classpath-aware XML class references remain outside this bounded implementation. These are plugin/adapter work where source models can prove them, not blanket host/API blockers. Native typed DOM equivalence is not implied by54 table rows.
