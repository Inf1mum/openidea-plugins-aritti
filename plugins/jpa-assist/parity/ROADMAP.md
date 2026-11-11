# JPA parity: implementation roadmap

Written 2026-11-11 from the ledger (`requirements.json`: 355 entries; 28 partial, 326 unverified, 1 blocked, 0 verified-parity) and `STATUS.md`. `PLAN.md` stays the contract; this file orders the remaining work and says what can and cannot be closed by code alone.

## What "done" means and what code alone cannot close

A requirement closes only as `verified-parity` with original-runtime and native-IDE evidence. Every wave below ends in source-level evidence (unit tests, compile checks, browser checks). That moves entries to `partial` at best. Three things sit outside code and gate the release:

| Gate | State | Needed |
| --- | --- | --- |
| Native IDE apply/Undo/embedding acceptance | none observed | Run the plugin in the desktop IDE per `docs/ide-verification-checklist.md`; record receipts per requirement family. |
| Original-runtime differential evidence | none | Run JPA Buddy `263.7307.112` on shared fixtures; diff generated source/SQL and availability. Licence and the "no copying decompiled code" rule apply: observe behavior only. |
| S007 atomic, version-checked multi-file edits | blocked by released SDK | Upstream request to `l0ng-ai/openidea` (per-document expected version in FileAction edits). No SDK patching here. Until then multi-file refactors stay single-document + create. |
| Live database access (G080-G103, G104-G129 live paths) | host exposes no IDE datasource API | Either an upstream host API, or a managed JVM/JDBC helper (needs user decision on capabilities: `process:*`, `net`, file access). |

Decision log: (3) 2026-11-11 user answered '3' to the scope question; recorded as YES, the Hibernate/Envers/Blaze/ModelMapper tail is in scope for 'full' (Waves 7-8 P3 rows stay in the plan). (1) 2026-11-11 live DB access: managed JDBC helper route chosen (JVM helper via released managed-process APIs; Wave 10 design must state the JVM requirement and failure behavior). (2) capabilities: optional `process:*` pre-approved; net and extra fs still require asking. Launch order chosen: ledger cleanup + Waves 5 and 6.

Decisions needed from the user before the affected waves: (1) approve opening the two upstream requests (S007 edit guard, datasource access) or the managed JDBC helper route; (2) approve any new manifest capability; (3) whether Hibernate/Envers/Blaze/ModelMapper tail (P3) is in scope for "full".

## Hygiene pass first (no new features)

1. **Map every E/A/I entry (139 + 44 + 12) to a user-visible capability** (G/S row) or mark it `not-applicable` with a reason (IntelliJ-internal extension points such as `ide.dynamicPluginVetoer`, `webHelpProvider`, `postStartupActivity`). Many E rows (27 `group`, 11 `inspectorItemProvider`) collapse onto existing G rows. Output: ledger `covers`/`mappedTo` fields and `check-jpa-parity.py` validation of the mapping.
2. **Fix known tooling gaps**: Kotlin parser drops properties with string-literal initializers; `find_files` host skips dot-dirs (mock does not); Spring Data version should be read from the build instead of inferred from the namespace; centralise opt-in compile-test env in `scripts/jpa-compile-env.sh`.
3. **Reconcile support claims** (settings schema, README, change-notes) with live behavior (gap analysis item 1). Write change-notes only; never edit `version`.

## Waves (each: 2-3 disjoint agents, coordinator verifies, ledger updated by coordinator)

Rules for every agent: new module + small anchored `file_actions.rs` hunks; docs/spec fetched, not memory; no `cargo fmt`; no `plugin.toml` edits; opt-in `generated_*` compile test; report unverified claims. Coordinator reruns the full suite, compile tests with env, Chromium/WebKit, package check, `git diff --stat`.

### Wave 5 - utilities and inspections (Stage 4; pure source, no new host needs)
- Lombok family G020-G024: equality, lazy `@ToString` exclusion, constructor checks, annotation editor, generation defaults (diagnostics + quick fixes; shared module `lombok_checks.rs`).
- Lifecycle/equality utilities G034/G035, name constants G033, naming templates G031/G032 (configurable via structured settings; physical naming only from proven config).
- Inspection configuration G030 and the 19+1 inspection families (F rows, I rows): inventory each family against existing `checks.rs`, implement missing ones with settings toggles.
- Kotlin parser fix and `find_files` caveat handling.

### Wave 6 - Spring Data completion (Stage 6)
- G045 unresolved-call method creation, G047 edit existing query, G049 async conversion, G050 dynamic projection, G051 derived-to-JPQL extraction, G052 JPQL completion, G046 keyword conventions.
- G048 graph builder (NamedEntityGraph/subgraphs), G037 batch repositories (needs a bounded multi-entity ID proof helper first), G054 projection sync on rename, G055 conventions, G056 auditing, G060 fluent setters.
- G057/G058 EntityView/revision repositories (P3, after Wave 8 Envers).

### Wave 7 - DTO/mapper (Stage 7)
- DTO forms G059-G068: nested/flattened/reused, Lombok, Jackson, ordinary classes, POJO conversion, SDK gating.
- Mapper G069-G073, G070 mapping methods, G071 generic mappers, G132 mapper calls; ModelMapper G073 and Blaze G074 (P3).
- G075/G076 rename and attribute propagation (needs indexed usages; check `DocumentIndexer` limits, exact-name rule), G078 validation transfer.

### Wave 8 - Hibernate/Envers (P3) and entity designer tail
- G026 custom types, G027 events, G028/G029 audited + revision entity, G125 Envers schema, G124 Hibernate 6 mappings, G126-G127 custom SQL types/naming strategies.
- Designer tail: G003 structure creation, G005 palette, G009 association advice, G011 toolbar, G012 panel layout, G014 module language selection, G039 repository grouping, G079 related navigation.
- Existing-mapping editing breadth (G006): precision/scale/write policies, element collections, embedded overrides, remove-annotation edits.

### Wave 9 - model snapshot and migration generation (Stage 9; offline first, needs no DB)
- G104/G105 offline model-to-snapshot diff in `persistence-core` (tables, columns, FKs, indexes, unique constraints; inheritance-aware; unknown stays unknown).
- G108-G111 danger classification, rename merging with explicit ambiguity review, null backfill, change grouping.
- G112-G119 Flyway/Liquibase generation by reusing the existing engines (`plugins/flyway`, `plugins/liquibase`) through versioned command adapters, never a second runner; G120 naming macros; G122/G123 single-entity DDL and SQL designer.
- Output stays create-new-file (overwrite:false) plus single-document edits until S007 is resolved.

### Wave 10 - reverse engineering and connections (Stage 8; gated by the datasource decision)
- If the host/helper route is approved: G080-G086 connections/schema selection, G087-G103 table-to-entity (mapped-object inventory, field selection, indexes, enums, unknown/deferred types, view IDs, existing entity updates, cardinality inference, schema cache, comments, name cleanup, vendor mappings), G096 sync.
- Otherwise implement the DDL-text path only (paste/open a `.sql` file: CREATE TABLE parsing for the supported dialects, PostgreSQL/MySQL first) and record the connection rows as host-blocked with the exact missing API.

### Wave 11 - closure
- G130-G134 injection/call-site workflows, unresolved-object creation, G129 shared settings, G077/G134 conventions, minimalistic entry points, compatibility migration.
- Re-run the hygiene mapping; every unverified row must be implemented, justified not-applicable, or blocked with an upstream issue link.

## Verification program (runs alongside, starts now)

1. **Native acceptance**: after each wave, install via `openidea-plugin link` and exercise each new action in the real IDE (apply, Undo, cancel, stale source, denied grants); store receipts under `audit/jpa-native/<wave>/`; update ledger `evidence`.
2. **Differential fixtures**: for each family build one shared fixture project (Java/Kotlin, jakarta/javax, field/property access, inheritance, composite IDs, duplicate names, modules); capture JPA Buddy output manually or by recorded sessions; compare normalized source/SQL; store hashes, never decompiled code.
3. **Compile everything generated** with the opt-in harness; add Spring Data 3.x consistent jar set and MapStruct/Hibernate/Envers/Lombok jars to the documented tool bundle.
4. **Release gate**: `check-jpa-parity.py --release` must pass with zero open gaps; it is never weakened.

## Order and dependencies

Hygiene -> Wave 5 and Wave 6 in parallel -> Wave 7 (needs Wave 6 query/DTO facts) -> Wave 9 (needs the shared model from earlier waves) -> Wave 8 and 10 (decision-gated) -> Wave 11. Native acceptance runs after every wave; upstream requests (S007, datasource) should be filed immediately because they have the longest lead time.

## Honest scope estimate

About 25 more bounded agent increments for source-level coverage of Stages 4-7 and 9, plus the decision-gated stages. Full functional parity as defined (differential and native evidence for 355 rows) is a multi-week program with human acceptance work; it cannot be completed by agent runs alone.
