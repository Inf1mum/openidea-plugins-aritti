# Spring Shell / Spring WS implementation receipt — 2026-11-10

## Baseline refresh

Marketplace API queried live at https://plugins.jetbrains.com/api/plugins/19992/updates?size=1 (Shell) and https://plugins.jetbrains.com/api/plugins/18506/updates?size=1 (WS). Both still report 263.7307.112, update IDs 1207346 and 1207457 respectively, matching pinned repository metadata. Raw responses: /tmp/parity-spring-shell-marketplace.json and /tmp/parity-spring-ws-marketplace.json. No proprietary archive download or decompilation was performed.

Behavior checked against official public sources:
- https://docs.spring.io/spring-shell/docs/3.0.2/docs/index.html — explicit command keys may contain multiple words and multiple aliases; availability selectors can name commands.
- https://github.com/spring-projects/spring-ws/blob/main/spring-ws-core/src/main/resources/org/springframework/ws/config/web-services-2.0.xsd — interceptor collections and scoped payloadRoot/soapAction elements support Beans ref children.
- https://docs.spring.io/spring-ws/docs/current/api/org/springframework/ws/server/endpoint/annotation/PayloadRoots.html — current public API confirms multiple payload roots (already supported).

## Implemented

Spring Shell:
- Extend the existing reviewed local Java rename model to multiple explicit aliases and normalized multiword keys.
- Selected alias declaration and exact availability selectors are edited together; other aliases, command-side availability-method names and wildcard selectors retain identity.
- Duplicate aliases and collisions across the known closed source scope remain rejected. Single-space lowercase word grammar preserves exact unescaped physical source edits.
- Read-only occurrence navigation shares this larger model automatically. CamelCase Java availability indicator names now retain separate method identity instead of making the whole command model disappear. Added an exact source-range regression for isReady.
- Added integration regression through FileActions proving only the selected multiword alias receives two exact edits; retained collision/cancel/staleness tests.

Spring WS:
- Bean definition/completion recognizes namespace-owned Beans ref children in top-level sws:interceptors and scoped sws:payloadRoot / sws:soapAction selectors.
- Existing closed same-document XML context guards apply: imports/profiles/nested contexts remain unknown; duplicate bean IDs have no guessed target.
- Added regression proving all three reference roles navigate/complete and foreign namespaces/unrelated parent roles do not.

Both plugin READMEs and change-notes updated. No shared, Laravel, host/API or release version edits.

## Verification

- Shell: cargo test --locked --manifest-path plugins/spring-shell/Cargo.toml — 78 passed, 0 failed.
- WS: cargo test --locked --manifest-path plugins/spring-ws/Cargo.toml — 104 passed, 0 failed.
- Both fresh cargo build --locked --release --target wasm32-wasip2 commands pass.
- git diff --check scoped to both packages passes.
- Logs: /tmp/parity-shell-test.log, /tmp/parity-ws-test.log, /tmp/parity-shell-build.log, /tmp/parity-ws-build.log.

## Residual parity: NOT closed

These deliveries close concrete source-model gaps; they do not establish full feature or desktop parity.

Shell: reviewed edits/read-only occurrences remain closed single-class Java legacy ShellComponent operations. Modern Command aliases, Kotlin transaction ownership, implicit command names, all allowed source spellings, cross-file/global command registry, arbitrary dynamic command registration, inheritance/type-complete availability, custom converters and host indexed usages/Rename remain unresolved. Some are plugin/model work; semantic models can require a separate adapter. Native terminal command-spec hooks require host support. P24.2 must remain partial.

WS: new interceptor assistance covers written same-document bean IDs only, without interceptor assignability/runtime registration proof. QName occurrences remain read-only rather than global host Rename. Complete QName reference closure across arbitrary XML consumers, library/classpath and effective Spring context resolution, full XPath namespace model, OXM/CXF typed DOM, automatic WSDL generation and native acceptance remain unresolved; plugin/adapters can cover some subsets but existing implementation does not. P26.1 must remain partial.

No desktop session or freshly packaged install acceptance was performed. Root should integrate matching API1.4 package checks and desktop acceptance separately.

## Further implementation after first receipt

The first delivery was extended; supersede its counts/residuals with this section.

- Shell now passes **89 tests**: modern 3.x alias extraction including separate class alias prefixes; canonical 4.x Command.name/alias and CommandGroup.prefix; 4.x value-as-description hover; separate core-vs-legacy provider identity for completion, navigation and gutter markers; Kotlin legacy reviewed key/alias edits; modern Java/Kotlin explicit reviewed edits; implicit command edits materialize key/name properties while preserving method identifiers and descriptions and updating legacy selectors.
- WS now passes **108 tests**: explicit Namespace/Namespaces Java/Kotlin URI assistance; namespace inheritance/override order checked against current NamespaceUtils source; plain absolute XPath root QName navigation/completion and read-only graph; XML payloadRoot interceptor selectors share QName identities and lookup. Current namespace XML props support remains.
- Both release Wasm builds pass after these extensions. No desktop acceptance or host-indexed Rename is claimed.

### Current public baseline capability inventory

Status is by named behavior, not blanket parity. `Implemented` is source/test implementation, still requiring current-host acceptance. `Partial/plugin` means concrete remaining plugin work; `Adapter` means external semantic/runtime model needed through supported local tooling; `Host` means unavailable native surface. The latest Marketplace descriptions are intentionally broad; this inventory makes their bounded subfeatures explicit rather than inventing a count of proprietary implementation internals.

Primary sources:
- M-Shell: https://plugins.jetbrains.com/plugin/19992 and live API metadata /tmp/parity-19992-metadata.json (four advertised features: command validation; availability reference validation; command/availability method completion; group completion).
- M-WS: https://plugins.jetbrains.com/plugin/18506 and /tmp/parity-18506-metadata.json (navigation, completion, inspections for Spring WS/OXM/CXF; no enumerated proprietary inspection list).
- S3: https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/support/CommandAnnotationUtils.java
- S4: https://github.com/spring-projects/spring-shell/wiki/v4-migration-guide and https://github.com/spring-projects/spring-shell/blob/main/spring-shell-core/src/main/java/org/springframework/shell/core/command/annotation/support/CommandFactoryBean.java
- W-NS: https://github.com/spring-projects/spring-ws/blob/main/spring-ws-core/src/main/java/org/springframework/ws/server/endpoint/support/NamespaceUtils.java
- W-XSD: https://github.com/spring-projects/spring-ws/blob/main/spring-ws-core/src/main/resources/org/springframework/ws/config/web-services-2.0.xsd
- W-REF: https://docs.spring.io/spring-ws/docs/current/reference/html/

| Capability | Current status and exact implementation/test evidence | Remaining action / boundary |
|---|---|---|
| Shell advertised command declaration validation (M-Shell) | Partial/plugin: lib.rs::analyze and rules.rs::check; duplicate-command and legacy missing-description checks; rules::tests suite | Dynamic registrations, full modifiers and complete modern option/argument models remain separate plugin/adapters work; no full validation claim |
| Legacy explicit key and aliases (M-Shell/S3) | Implemented: lib.rs::commands, command_rename.rs::model_source; multiword_alias_rename_updates_only_selected_alias_and_its_selectors | Runtime global duplicate authority not supplied |
| Legacy implicit method-to-kebab command names (S3) | Implemented: lib.rs::commands + implicit_transaction_tests; reviewed edit materializes key | Unicode Java method naming outside admitted ASCII source model remains plugin work |
| Legacy availability selectors/wildcards/method-side role (M-Shell) | Implemented bounded source: rules.rs, lib.rs::availability_owner/analyze; command_side_availability_method_name_is_not_a_command_key_hover, rules::tests | Effective inherited/classpath command set remains adapter work |
| Availability reference validation (M-Shell) | Partial/plugin: wrong-return, arguments, ambiguous indicators and literal-selector checks in rules.rs | Arbitrary inherited availability methods, runtime beans and complete generic assignability require semantic adapter |
| Availability method names completion (M-Shell) | Implemented bounded canonical source: lib.rs::completion; existing completion/ownership regressions | Aliased import/typealias resolution intentionally unknown; extend source symbol ownership before admitting |
| Command key/selector completion (M-Shell) | Implemented cached class-scoped source: lib.rs::completion, completes_names test | Dynamic runtime registry completion requires adapter |
| Command group completion (M-Shell) | Implemented declared group values: lib.rs::groups/completion; existing completes_names; 4.x CommandGroup.name extraction added | Computed groups and runtime inheritance remain unknown |
| Legacy and modern description hover | Implemented literal-only: hover.rs; modern_contract_tests::v4_hover_uses_value_as_description_and_aliases_share_method_identity | Dynamic descriptions remain unknown, no evaluation |
| Modern 3.x commands, class prefixes, aliases (S3) | Implemented: lib.rs::commands; modern_contract_tests::v3_aliases_are_separate_routes_with_the_class_alias_prefix | Global environment naming modifiers remain adapter/runtime work |
| Modern 4.x names/aliases/group prefixes (S4) | Implemented canonical source: rules.rs::modern_field, lib.rs::commands; v4_owned_names_aliases_implicit_methods_and_group_prefixes_work_in_java_and_kotlin | Meta-annotations/global environment prefixes/effective registration require adapter |
| Modern availability provider completion/navigation/gutters (S3/S4) | Implemented separate interface inventories: lib.rs::beans_for/core_beans; core_provider_tests | Complete context/conditional bean resolution remains adapter work |
| Option name/default/arity validation (M-Shell/S3) | Partial/plugin: options.rs legacy/3.x literal checks, rules.rs 3.x arity rules | 4.x singular explicit option-name duplicate and primitive Option/Argument default checks now implemented in options.rs::core_options (core_option_tests); computed/custom-converter values and broader positional argument contracts remain plugin/adapter work |
| Java/Kotlin local key/alias transactions | Implemented bounded source: command_rename.rs; kotlin_implicit_tests, modern_transaction_tests, implicit_transaction_tests | Prefixed/split-expression keys and cross-file/global automatic Rename remain plugin graph work |
| Global Find Usages/Rename for all Shell declarations | Partial/plugin: local reviewed edits and read-only occurrences only; no DocumentIndexer registration | Build full source-owner graph including external command contexts before automatic Rename; do not enable raw-string index blindly |
| Shell terminal command completion | Host: no command-spec extension surface in established API1.4 | Needs host extension; editor key completion is separate |
| WS endpoint -> global schema navigation (M-WS/W-REF) | Implemented literal/constant namespace subset: lib.rs::endpoints_with_syntax/elements_for; endpoint_namespace_source_safety tests | Full classpath/imported schema semantics need adapter |
| WS schema -> endpoint markers (M-WS/W-REF) | Implemented cached known QName subset: lib.rs::xml_markers; existing marker tests | Runtime endpoint registration and inherited/meta annotations require adapter |
| PayloadRoot/PayloadRoots Java/Kotlin support | Implemented: annotation_owners.rs + lib.rs::payload_annotations; java_payload_roots_share_endpoint_navigation_completion_and_diagnostics | Dynamic namespace expressions remain unknown |
| QName localPart completion and missing-element inspection | Implemented complete-known-schema subset: lib.rs provider/diagnostics and schema_qnames.rs completeness tests | Library/import/redefine closure remains adapter/plugin graph work |
| ClassPathResource and XML scalar/constructor resource navigation | Implemented admitted source roles: resource_paths.rs, xml_resources.rs, resource_completion.rs test suites | Class-relative/effective external classpaths remain adapter work |
| Marshaller/unmarshaller bean refs | Implemented closed same-document IDs: xml_refs.rs::model/definition/completion, existing ownership tests | Full imported/profile contexts and assignability require adapter |
| Interceptor bean refs (W-XSD) | Implemented local written ID: xml_refs.rs::model; interceptor_reference_children_resolve_only_in_the_sws_vocabulary | Interceptor type inspection/effective registration not claimed |
| Interceptor payloadRoot element selectors (W-XSD) | Implemented QName links/completion and occurrence graph: xml_refs.rs::payload_qnames, xpath.rs::xml_selector_tests | Dynamic selectors quiet; no automatic global Rename |
| Namespace URI assistance in XPath adapter XML props | Implemented admitted scalar props model: namespace_values.rs + namespace_property_provider_tests | Full sws historical xpath-endpoints vocabulary not implemented; removed from current 2.0 XSD and must be version-gated if added |
| Namespace/Namespaces source annotation URI assistance (W-NS) | Implemented Java/Kotlin explicit bindings: xpath.rs::namespace_sites, tests | Package-info.java namespace loading remains concrete plugin cache work |
| XPath root QName navigation/completion (W-NS/W-REF) | Implemented plain absolute roots: xpath.rs::references/definition/completion; source tests verify namespace merging/override and UTF16 | Descendant paths, predicates, namespace aliases, package bindings and complete XPath grammar require expanded parser/context model |
| WS QName read-only occurrences | Implemented bounded source observations: indexing.rs includes PayloadRoot, XPath root and XML interceptor selectors with exact namespaces | Full QName references in arbitrary WSDL/XSD/instance XML remain plugin graph work |
| WS complete indexed usages/automatic Rename | Partial/plugin: no implicit raw string Rename graph exported | Complete physical QName role graph and ownership closure first; retain read-only current feature until complete |
| WS endpoint generation | Implemented local DOM scaffold: generate.rs atomic preview/create tests | Automatic schema-to-complete-WSDL/server toolchain requires optional CLI adapter |
| OXM effective mapping validation | Partial/plugin/Adapter: written marshaller references and admitted resource roles implemented | Add explicit mapping-language parsers and versioned JAXB/OXM source adapter; no complete typed DOM/type graph currently |
| Apache CXF effective configuration/inspection | Partial/plugin/Adapter: advertised technology not fully modeled by current plugin | Inventory public CXF configuration roles then implement known QName/resource/type adapters; cannot label all CXF semantics host-blocked |
| Native semantic/type queries, injected XPath editing and remote debugger hooks | Host or separate external adapter | API1.4 lacks native type-query/debug-evaluation surfaces; source parsers can still cover additional feasible subsets |

The table identifies concrete remaining plugin/adapters work, not only API blockers. Therefore full requested parity is not closed by this delivery. P24.2 and P26.1 remain partial until the stated reference-graph and acceptance work is completed.

Additional final source-contract delivery: 4.x singular `Option.longName` / `shortName` duplicate checks and standard primitive defaults for `Option` and `Argument` now have a distinct profile; no 3.x arity rules leak into 4.x. Dynamic values, foreign annotations, generic types and empty defaults remain quiet. Two dedicated tests pass; total Shell89/WS108.

Implementation caveat: package-level NamespaceUtils bindings, arbitrary XPath grammar and full OXM/CXF models remain concrete unfinished plugin/adapter work. These are not API blockers. The current delivery expands class/method scope and literal QName roles without enabling an incomplete automatic Rename graph.

## Package namespace cache follow-up

Implemented the concrete package-info gap identified above. `xpath.rs::PackageNamespaces/scan_package/cached_references` and `WsTools.xpath_packages` warm only `**/package-info.java`; cache facts are bound to adjacent physical directory, exact declared package and current project ownership. Providers use fresh watched cache only. Watch events replace or remove facts; unreadable paths reset the cache. Class and method bindings override matching package prefixes, consistent with NamespaceUtils. Namespace URI assistance also handles the package annotation document.

The explicit QName occurrence command reads bounded sibling package-info content afresh and re-establishes the selected and target QName keys after the picker, so it does not silently promote cached namespace bindings into current source authority. Tests cover changed URI, mismatched package, removal, cold/denied watches, nested project isolation and a package file changed without a delivered event.

Latest verification: Shell89 tests, WS112 tests, fresh Wasm release builds pass. Package namespace row above is now implemented for adjacent package-info source; nonadjacent/effective classpath package metadata remains adapter-dependent. These new exact artifacts still need package/desktop acceptance.

Shell cross-file rename assessment: legacy availability selectors select command names only within their declaring class. The closed class transaction already updates those admitted selectors. A second file cannot safely be added merely for matching spelling: inheritance, dynamic command registration, command execution strings and runtime registry ownership require a broader semantic/registry model. No readily available cross-file closure was found; no heuristic workspace string Rename was enabled. This remains concrete plugin/adapter work, not an API-blocker claim.

## Further public XML contract implementation

Fresh public primary contracts inspected (no proprietary code):
- https://raw.githubusercontent.com/spring-projects/spring-ws/main/spring-ws-core/src/main/resources/org/springframework/ws/config/web-services-2.0.xsd
- https://raw.githubusercontent.com/spring-projects/spring-framework/main/spring-oxm/src/main/resources/org/springframework/oxm/config/spring-oxm.xsd
- https://raw.githubusercontent.com/spring-projects/spring-framework/main/spring-oxm/src/main/java/org/springframework/oxm/jaxb/Jaxb2Marshaller.java (`afterPropertiesSet` binding-source exclusivity)
- https://raw.githubusercontent.com/apache/cxf/main/rt/frontend/jaxws/src/main/resources/schemas/jaxws.xsd
- https://cxf.apache.org/docs/jax-ws-configuration.html

| Added capability | Implementation and tests | Remaining boundary |
|---|---|---|
| Current SWS required attributes/schema child/boolean contracts | xml_contracts.rs diagnostics/completion; known_required_attributes_children_and_boolean_values_are_current_text_checks; exact UTF16 completion test | Explicit old/custom schemas quiet; no whole XSD validation engine |
| Current annotation-driven marshaller refs | xml_refs.rs annotation_driven_tests | Same-document declared IDs; no effective assignability |
| OXM marshaller exported ID | xml_refs.rs oxm_cxf_reference_tests | Imported/aliased context bindings unknown |
| CXF bus, #implementor, #serviceBean refs | xml_refs.rs oxm_cxf_reference_tests preserves # replace spans | Runtime bean registry/type resolution unknown |
| OXM bound-class/CXF class attributes | typed_xml.rs fresh watched source cache; tests cover concrete/interface role filtering, ambiguity, changes, foreign namespaces | Complete library classpath and inheritance remain adapter work |
| JAXB binding source exclusivity and required class names | xml_contracts.rs oxm_contract_tests, based on public runtime contract | Dynamic property values quiet; no custom mapping engine claim |
| XPath namespace prefixes beyond absolute roots | xpath.rs prefix_tests + unknown_prefix_completion_test; class/method/package targets and quoted-content/axis guards | Full XPath expression validation and child-schema path graph remain plugin/parser work |
| Explicit source cache refresh | refresh-models command, typed_xml.rs refresh_command_tests | A capped/failed inventory reports partial, never complete |

Latest package verification: **Spring WS124 tests pass**, Shell remains89; fresh release build scheduled after this source set. Prior narrower residual statements about completely absent OXM/CXF assistance are superseded by these concrete capabilities. Full OXM/CXF effective semantics and complete QName Rename remain unfinished plugin/adapter scope, not blanket API blockers.

Final extension: OXM JAXB context-path source package navigation/completion now handles colon-separated components independently through the same watched source-type cache (`typed_xml.rs::context_path_tests`). It does not assert runtime JAXB context initialization, ObjectFactory or jaxb.index availability. Latest suite now **WS125 tests**; source contracts and hashes are recorded in /tmp/parity-shell-ws-official-source-receipt.json. Shell remains89.

## P24.2 native occurrence follow-up

Native local occurrence highlights and code lenses now ship through API1.4 LanguageFeatures, with exact key/alias/indicator ownership and snapshot/project revalidation. Native Find Usages through DocumentIndexer cannot be separated from unchecked automatic string Rename in the released contract, so raw anchors remain unexported; this is proven against the exact SDK/host implementation in /tmp/parity-shell-p242-native.md. Current Shell94 tests and fresh Wasm build pass. Read-only native occurrence surfaces and existing reviewed key edits are implemented; global Find Usages/Rename and runtime registry semantics are not claimed.
