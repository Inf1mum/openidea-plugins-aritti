# Spring core/JPA implementation checkpoint — 2026-11-10

Scope: spring-beans, spring-boot, spring-data, spring-security, spring-osgi, jpa-assist. No Laravel, host, SDK or shared-crate edits. Read writing-openidea-plugins skill and existing ownership/diagnostic constraints.

## Fresh original baseline

Official Marketplace `/api/plugins/<id>/updates?size=1` fetched successfully for all 19 Spring/JPA original modules; raw full metadata in `/tmp/parity-spring-latest.json`. Relevant frozen versions:

| Local package | Original ID | Latest returned version |
|---|---:|---|
| spring-beans | 20221 | 261.27258.144 |
| spring-boot | 20211 | 263.7307.112 |
| spring-data | 20199 | 263.7307.112 |
| spring-security | 20215 | 263.7307.112 |
| spring-osgi | 16877 | 253.28294.218 |
| jpa-assist | 20202 | 263.7307.200 |

Public behavioral sources checked: https://www.jetbrains.com/help/idea/spring-support.html ; https://www.jetbrains.com/help/idea/spring-tool-window.html ; https://www.jetbrains.com/help/idea/jpa-buddy-spring-data.html ; https://www.jetbrains.com/help/idea/jpa-buddy-entity-designer.html . Proprietary bytecode was not downloaded or inspected.

## Implemented changes

1. JPA Assist: registered live `jpa-duplicate-embedded-id` check for directly owned, closed plain Java source entities declaring multiple EmbeddedId fields. Exact attribute ranges (including multibyte prefix), current-text clearing, option gate and unknown inheritance/property/foreign/incomplete contexts tested. Existing mixed Id/EmbeddedId rule was discovered and deliberately left responsible for mixed kinds to avoid duplicate diagnostics.
2. JPA Assist: owned Embeddable source classes now report directly attached Id/EmbeddedId annotations (`jpa-embeddable-identifier`) under existing checkAttributeSignatures. Entity role, nested metadata/body and foreign owners cannot borrow that meaning. Test checks exact annotation range, recovery and disable setting. Rule contract: https://jakarta.ee/specifications/persistence/3.2/apidocs/jakarta.persistence/jakarta/persistence/EmbeddedId.html and https://jakarta.ee/specifications/persistence/3.2/apidocs/jakarta.persistence/jakarta/persistence/embeddable .
3. Spring Data: extra reviewed atomic **Generate JpaRepository** action for proved JPA entities, alongside existing CrudRepository. Inherits exact snapshot/project ownership, conventional package, scalar ID admission, collision checks, fresh source revalidation, cancellation and non-overwrite host FileOperation. Common MongoDB/JDBC entities never receive JPA-specific action. Generated Java/Kotlin fully qualified repository interfaces avoid import collisions. Added regression for action selection, exact generated Java type and MongoDB exclusion; all pre-existing lifecycle tests still exercise shared implementation.

## Existing feature implementation mapping and unresolved scopes

This is a feature-family inventory, NOT an exhaustive latest-version method or inspection inventory. Complete parity remains unproven.

| Family | Existing implementation mapping | Remaining work/boundary |
|---|---|---|
| Beans XML structural/annotation diagnostics, refs, generation | xml_checks, xml_namespace, xml_create, checks and provider modules | Library assignability/effective conditional contexts remain unavailable without semantic adapter. Missing arbitrary source-local checks must still be compared feature-by-feature; old audit F/F* rows are not current absence evidence. |
| Beans indexed usages/rename and report/graph | ownership-scoped current provider/index modules; bean graph UI | Complete runtime/auto-config bean graph and effective dependency injection remain unresolved. |
| Boot key/value metadata, JAR metadata, source annotations | meta, local_meta, jar_metadata/jar_archive, values, checks/facts/live, fileset | Bundled version coverage is not all classpath metadata; dynamic build resolution/custom converters need adapter evidence. |
| Boot property Find Usages/Rename | anchor_audit proves boundaries, placeholder navigation exists | P17.2 marked present in historical ledger does NOT imply safe general Rename. Owner/profile/property-set graph for relaxed names/escaped keys/YAML leaves still requires implementation. This is plugin work, not a missing generic host provider excuse. |
| Data derived queries/JPQL, domain links, hints | checks, jpql, types, members, live, mongo | Effective runtime/classpath hierarchy/query/type resolution remains unimplemented. Projection/query/entity-graph generators beyond admitted repository workflow remain plugin/adapter work, not blanket host-blocked. |
| Security Java/Kotlin/XML rules and expression sites | existing plugin source model/provider implementations | Full chain/effective authorization/SpEL type evaluation and debugger unlock are not available from current source subset. More literal/code generation assistance could be plugin work. |
| OSGi XML references, local hierarchy/bean roles | lib, hierarchy, ownership, bean_facts | External resolved libraries/effective Spring reference types remain unresolved. Current native tests are not latest-original completeness. |
| JPA class/attribute/callback/listener/graph live checks | live, checks, attribute_info, local_callbacks/listener/graph/versions and new local_identifiers | Hierarchy missing-ID, repository ID and derived-name diagnostics remain withheld pending bounded completeness; external XML mapping/hierarchy/listener facts need adapters. New check adds an identifier family subset, does not close all P09.3. |
| JPA reviewed file operations | file_actions, wizard, dto_action, ddl_action, attribute_refactor | Database reverse engineering requires an external introspection adapter; owned database creation API is not schema reading. Full inheritance/converter semantics, broader designer/generation features remain open. |

## Verification

Fresh cargo test --locked and cargo build --locked --release --target wasm32-wasip2 run for all six packages; logs under `/tmp/parity-spring-checks/`. Latest modified-package reruns replace same logs. No desktop/package installation or release claims. Parent must perform matching API1.4 package validation and desktop acceptance. New manifest change-notes and README scope describe actual additions; no release versions were changed.

## Exact next work

1. Build feature-level latest official baseline beyond these families, retaining compound rows split by capability.
2. Implement safe Boot property context owner graph (properties exact names + explicit physical spans first; profile/import/source-set boundaries), read-only usages before Rename.
3. Extend Spring Data projection/query/entity-graph generation using exact current entity owner and reviewed atomic operations rather than assuming classpath semantics.
4. Restore JPA missing-ID/repository-ID live subsets only with explicit bounded source model completeness, mapping override status and fresh lifecycle invalidation.
5. Add separately approved semantic/build/introspection adapters for remaining facts; they are substantial implementation work, not closed by this checkpoint.

## Further implementation

4. Spring Data reviewed atomic identifier interface projection generation added for the admitted scalar-ID source model (Java fields/record/getter properties and Kotlin properties). Uses actual source ID property name, fully qualified boxed type, correct JavaBeans acronym decapitalization, exact source snapshot and existing host transaction workflow. Explicit narrow feature, not complete arbitrary projection designer parity. Regression tests verify generated key property name/type and Mongo/JPA action differentiation.

5. Boot project physical-source property graph command: explicit bounded discovery across owned application/bootstrap properties (128 files/1 MiB/1024 occurrences), exact declaration/placeholder spans, distinct profile/file provenance, current editor versus saved-file source, no secret values/defaults, fresh root/source revalidation before navigation. Unsupported/unreadable files fail visibly. Nested registered project separation and multibyte/profile regressions added. This is read-only and does not claim effective configuration ownership or publish unsafe Rename anchors.
6. JPA positive saved-source repository identifier model: cache admission retains strict source-local scalar field ID evidence; fully qualified canonical CrudRepository/JpaRepository entity+boxed ID arguments compare only with a unique same-project src/main/java entity identity, exact saved snapshot freshness and TTL. New mismatch warnings use exact ID token ranges and source-only language. Cold/foreign/inherited/property/duplicate/unknown/stale source yields no negative inference. Tests cover warm/cold cache, exact multibyte ranges, matching IDs, disk drift, foreign parent, transformed/short names and option gate. Generic cross-hierarchy and effective XML/runtime identity remain unresolved.

7. JPA complete current plain entity with no superclass/source identifier now receives a source-absence **Info** note rather than an effective-model error. Exact entity name range, ID addition recovery, generic/inherited/foreign/incomplete quietness tested. No XML override absence or runtime identifier claim is made. Final JPA total: 299 unit tests passing; current Boot 154, Data 106, Beans 364, Security 99 and OSGi 62. Latest six-family tests and builds are under `/tmp/parity-spring-checks/`.

## Subsequent concrete work after initial checkpoint

8. Spring Data selected direct scalar interface projections, inferred field/property access from the source ID site, Kotlin nullability, source association EntityGraph repository generation, derived scalar equality finder generation with grammar round-trip checks, and JpaSpecificationExecutor parent variant. These are reviewed atomic new-file operations, with selection, collision/stale/cancel guards and no external writes. Broader nested/DTO/open projections and arbitrary custom parent/classpath facts remain separate.
9. Boot native Database source setup from current literal properties/YAML: simple PostgreSQL/MySQL/MariaDB TCP JDBC targets, native secure credential store, optional database.sources, concrete review, source/project recheck, no connect/introspection/SQL. Four targeted tests cover raw target/credential separation, YAML/IPv6, unsupported and ambiguous values and actual mock-owned setup/cancel.
10. Core no-association SpEL lexical language plus exact Java/Kotlin annotation/template and Spring XML attribute injections; generic embedded keyword/context-variable completion. Foreign/escaped/interpolated/noncontiguous/entity-decoded ranges withheld. Security method annotations publish canonical physical SpEL ranges using the Core language when installed. No runtime/type/evaluation or embedded LSP claim.
11. JPA no-association JPQL lexical language plus exact owned NamedQuery/NamedNativeQuery ranges, and Data Query/countQuery/NativeQuery Java/Kotlin ranges; native SQL remains distinguished. JPQL source entity/property semantics remain separate from lexical syntax; no persistence-unit/server claim.
12. Boot reviewed atomic exact static-source property Rename. Admitted scope is the complete host-visible conventional current source module, default application/profile/bootstrap graph plus literal local imported/property sources/directories and canonical literal Java/Kotlin Value refs (single canonical Kotlin aliases/use-sites supported). Exact source key edits preserve nested YAML parent; known framework/catalog/JAR/local metadata/ConfigurationProperties binding ownership is withheld rather than broken. Collisions/relaxed aliases/encoded/malformed/unreadable sources refuse; external/dynamic/nonconventional sources are explicit exclusions. Rediscovery and every snapshot rechecked after a concrete file/line/column old→new preview (values never shown). Resolution returns only atomic FileAction edits; no filesystem writes. Seven targeted feature tests pass.

Expanded latest public feature inventory (61 rows, not only these additions) is in `/tmp/parity-spring-core-features.json` and `.md`, with actual module mappings and actionable residuals. It does not pretend that generic proprietary public descriptions enumerate every undisclosed inspection implementation.

## Released renderer qualification (source verified)

Exact released commit37c2a60 `src/features/plugins/languageVisuals.ts:33` loads only built-in CodeMirror grammar support for injection coloring. It does not call external plugin SyntaxProvider tokens. The API1.4 semantic TYPE_CLASS also omits literal/keyword token styles (known prior Ideolog boundary). Consequently SpEL/JPQL language lexer output, accurate physical ranges and embedded provider routing are implemented, but native embedded coloring is NOT claimed. Root explicitly chose correct language identity and recorded host-renderer residual over misleading JS/SQL approximations or semantic kind hacks. Web owner was informed and does not depend on unproved native coloring. This is a concrete host implementation boundary, not a reason to omit plugin-side ranges, definitions or completion.


## Native chooser, source diagram and final verification — 2026-11-11

Core now implements released GutterActions with a declared choose-source-gutter-target command. Physical lines with several positive injection/usages/related targets show a native picker, with exact originating editor/root/version and fresh candidate source/model guards before and after selection. Single-target arrows remain direct. Lists are bounded and labeled when capped. Three focused chooser regressions cover ambiguity, stale source/target/root, cancellation and forged tickets. The legacy usages test now verifies all five receiving bean names, seven physical injection choices and actual navigation to the chosen UserController site. No effective runtime autowiring claim.

JPA Source Diagram is a packaged UI-kit view in native persistence group (id source-diagram), command source-diagram, toolbar diagram-refresh. It scans bounded source-set-qualified Java/Kotlin models, suppresses ambiguous/unproved edges, shows source-only completeness/unknown provenance, and rechecks all captured source snapshots plus membership before node navigation. Five tests include actual UI-kit refresh/render protocol and navigation guards. Parent performed scoped generated UI sync, representative fixture and Chromium4 light/dark layout verification. No generated files/scripts were manually edited here.

Boot native Database setup now uses source+driver+host+port+database+username-bound SHA256 identities so changed targets cannot retain old credentials. Password rotations keep same identity; old target sources remain manual cleanup. UTF8-safe display name max256 bytes. Admitted forms include exact PostgreSQL/MySQL/MariaDB TCP, ClickHouse plain/HTTP, Oracle thin service and SQL Server databaseName. Unknown URL/transport options are refused, never silently discarded. Five database regressions include exact default ports/IPv6 and rejection paths. No real driver connection acceptance claim.

62 current public feature rows are mapped in /tmp/parity-spring-core-features.json and .md, with original version metadata and official public source references. These do not exhaust unpublished proprietary inspections.

Fresh complete checks: spring-beans374, spring-boot166, spring-data115, spring-security100, spring-osgi62, jpa-assist307 =1124 passing tests; all six locked release wasm32-wasip2 builds pass. Logs/results /tmp/parity-spring-checks. Package/native runtime acceptance is still a separate check.

Exact remaining plugin/adaptor work includes broader Data multi-condition/sort/page query generation, nested/open projections and custom/batch repository parents; Boot binding-declaration refactoring and owned run adapter; source-API query injection enumeration; resolved classpath/context/ORM metadata integrations; OSGi scoped symbol graph/refactoring. These remain genuine work, not reclassified as API blockers. Native embedded coloring alone has the concrete released renderer/API gap established above.


## Additional source-owned query generation

Data finder generation now admits up to eight selected direct scalar equality conditions, preserving parameter order and roundtripping the ENTIRE derived method grammar. Duplicate/ambiguous/nested/underscore paths are refused. Separate JpaRepositoryWithPagedFinder and JpaRepositoryWithSortedFinder actions emit Page+Pageable or List+Sort; generated framework parameter names avoid selected-property collisions. Source snapshots, collision detection, concrete review and atomic non-overwrite FileActions remain unchanged. Two new regressions cover operator-like conjunction spelling/duplicates and paging/sorting parameter collisions. Full Data117 tests and locked release Wasm build pass. Fresh current aggregate1126 tests across six packages. Arbitrary predicate/operator/modifying query generation and edits to existing repositories remain work.


## Direct source API query literals

JPA jpql_language::api_queries now supplies exact physical JPQL/SQL ranges for direct Java method parameters explicitly typed jakarta/javax.persistence.EntityManager and createQuery/createNativeQuery first string literal arguments. Flat method bodies only; all receiver uses must be unqualified member accesses, which rejects reassignment/foreign receiver borrowing. Namespace-shadow/wildcard, nested scopes, escape/textblock/concatenation forms are withheld. Regression verifies positive physical spans and foreign/reassignment/shadow/nested/escaped negatives. Full JPA308 tests and release Wasm pass; aggregate1127 passing six-package tests. Field/local/factory receiver ownership, Kotlin and other API families remain actionable source/type adapter work. Native embedded color rendering remains the separately proven host gap; no native coloring claim added.


## Further requested query and native run work

Data typed source-criteria generation now supports Equals/Not/In/NotIn/IsNull/IsNotNull, String Like/NotLike/Containing/StartingWith/EndingWith, known numeric comparisons/Between and Boolean True/False. Up to8 unique direct scalar properties, complete-method actual spelling/arity roundtrip; In is Collection<T>, Between2 parameters, zero-argument predicates preserve zero-argument signatures including paging. Framework and generated argument names avoid collisions. No arbitrary spatial/regex/custom-store operator claim.

Add finder to existing JpaRepository is a reviewed atomic TextEdit, not overwrite: exact same package/language source ownership, one direct fully qualified JpaRepository<Entity,ID> parent, explicit interface body, no custom/generic/foreign parent or wildcard/namespace shadow. Existing method names refuse overload insertion. CRLF retained. Source+target buffers are rechecked after preview; released Project.read_file includes host.buffer_text before disk (wit_host.rs638). Java/Kotlin positive/cancellation/duplicate/foreign parent tests added. Full Data120 tests/release Wasm pass.

A TEMPORARY test harness exported ACTUAL generated FileAction source (then was removed from product tests) into /tmp/parity-data-query-fixture. Actual generated operator List, null Page and In+Sort repositories compiled with Maven compiler3.15.0 against SpringDataJPA4.1.0/Jakarta Persistence3.2.0. Real SpringData PartTree parsed the three generated methods, resolved the source User fields and verified query argument counts3/0/1. Marker DATA_QUERY_GENERATED_FIXTURE_OK; maven.log and verify.log retained. This proves compilation and PartTree structure, not provider/database query execution.

Boot owned native RunConfigurations adapter added by child boot_run and integrated by Core: explicit default-false nativeRunProfiles setting; direct source Boot build plugin plus applied JVM Gradle plugin proof; native system mvn/gradle program grants; read-only discovery, no wrapper/automatic execution or debugger claims. Real disposable Boot4.0 apps execute EXACT emitted Maven and Gradle commands with successful marker. Evidence /tmp/parity-boot-run.md and hashed receipt /tmp/parity-boot-run-fixture-receipt.json. Root native UI acceptance remains separate.


## Version-qualified metadata and independent review

Metadata child implemented jar_versions.rs and extended jar_metadata.rs with literal root properties/direct dependency-management resolution plus explicitly reviewed bounded official local Boot parent/Cloud imported-BOM POM catalogs. Selected source catalog entry is NOT proof of an active dependency. Up to16 conflict-refusing GAV snapshots coexist, source/JAR SHA256 and exact version-source provenance retained. Provider/command lookup integration now removes bundled3.5 fallback once version-scoped; incomplete selected catalogs suppress unknown-property/local-metadata absence negatives. Positive TTL expiry does not revive old bundled authority. No product network/build execution or editor discovery.

Actual official artifacts imported successfully: Boot4.1.2 core105 properties SHA2561fbf6443a301eb9a098bd024149c0edc6578b073f75da8c98bc1af84c43a8f59; Cloud Context5.0.4 properties30 SHA2563c19afc1934270689bb1a1f29c397b1e08b5fcf7c08d53ebea60e0ab89b5233a, declared Cloud train2025.1.4;20 version-sourcePOMs and426 candidate catalog entries. Receipt/test /tmp/spring-versioned-metadata-real; full evidence /tmp/parity-boot-metadata.md. Boot172 pass,1 explicit real fixture ignored in normal suite; that fixture was invoked separately and passed. Release Wasm pass.

Independent query review found and fixed2 concrete source-ownership issues: target Kotlin import aliases now withheld, and JavaBeans capitalization/decapitalization must roundtrip source property names. This prevents Name→missing name and aB→competing AB binding. Casing guard also protects scalar/identifier projection getters. Regressions added; Data121 full tests/release Wasm pass. Query fixture additionally clean-compiled/re-executed against CURRENT stable SpringDataJPA4.1.2 (official Maven metadata lastUpdated20261022083919;4.2.0-RC1 separately recorded as prerelease). Actual PartTree verification marker passed; new /tmp/parity-data-query-fixture/receipt.json hashes source/logs.

JPA optional read-only manual JDBC Hibernate console module is owned by Database agent. Core has integrated state/native persistence-group table, run/status/cancel commands, process:java optional grant, serde/serde_json/sha2 and deactivation cleanup. Console tests/actual H2 qualification still in progress; do not mark console feature complete until its report.


## Final integrated optional JPQL console / check status

Database agent finalized separate version-qualified manual JDBC SELECT adapter and its H2 proof. Core owns completed native integration: JPQL Console left/Persistence table, run/status/cancel declared commands and native actions, process:java optional grant, approved profile templates, state/render/event/deactivation wiring, dependency/lock changes. Database owns helper/runtime locks/profiles/tests. Both exact final Hibernate6.6.49/H2 and7.4.15/H2 fixture runs qualify all7 typed params, actual SELECT/scalars/bounds/truncation, source/profile drift, mutation/entity-render refusal, trusted converter construction/no application bootstrap, and real slow-JDBC cancellation followed by unchanged-data verification. Other drivers/servers remain unqualified and no mutation mode/effective app configuration is claimed. Report /tmp/parity-jpql-console.md; hash-bound receipt /tmp/parity-jpql-console-receipt.json. Helper SHA256b986d745efd211edd9dba5c72b9592e238bee8ec8911074d8929a580aa9f833f.

FINAL assigned-package counts: Beans374 Boot172 Data121 Security100 OSGi62 JPA315 =1144 passing tests. Boot's1 real official-artifact fixture is ignored in default runs but was explicitly invoked and passed separately. All6 locked release Wasm builds pass; final perpackage logs /tmp/parity-spring-checks and counts/results.json. Native desktop/exact packaged-artifact acceptance is separate. All declared asset paths across the6 manifests were checked present. No host/API or shared scripts/assets edited by Core; parent centralized generated UI work.

Current latest public feature inventory63 rows /tmp/parity-spring-core-features.json and.md incorporates typed operators/insertion, native Boot profiles, project-version metadata and qualified manual console. It remains scoped evidence, not a full proprietary inspection inventory or blanket parity claim.
