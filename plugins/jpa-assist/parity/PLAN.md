# Full JPA parity implementation plan

## Contract

Reference: JPA Buddy `com.haulmont.jpab`, Marketplace 15075, version 263.7307.112, plus the audited historical documentation workflows. The original binary is distinct from Marketplace 20202 Jakarta EE Persistence. `requirements.json` pins the reference hash and preserves the captured original registration metadata.

Functional parity covers action availability, supported source/provider contexts, generated results, navigation, editing/refactoring effects, failure handling and Undo. Reorganizing the interface is intentional; original strings, layout and implementation are not production dependencies.

A requirement is complete only as **verified-parity** with differential acceptance evidence. Unverified, missing, partial and blocked requirements are open gaps. Preview/export/clipboard alternatives do not close apply, synchronization or refactoring parity.

The released SDK remains a hard boundary. No vendored SDK, host patch or shell-write workaround may bypass missing source-edit guarantees. Full parity cannot be released while an SDK blocker is open.

**Contract amendment 2026-11-11.** Approved by the user: documentation may serve as reference evidence because the original plugin cannot be obtained. An acceptance case may set `referenceType: documentation` (default `observed`) and then carries `referenceSources` (https URLs), `referenceClaims` (each citing the documented statement it verifies), `docsRetrievedOn`, a passing result, an existing `implementationEvidence` file and a test. A row is `verified-parity` only if every claim listed in `parity/claims/<ROWID>.json` (when that file exists) is covered by an acceptance case's `claimIds`. Documentation-derived verification is weaker than observed verification: the checker reports the two counts separately and marks every row verified that way. Behavior the documentation does not state cannot be verified this way and stays an open gap. `--release` still fails while any gap is open.

## Accepted UX

Two persistence tool windows maximum:

- **Persistence**, left: Model and Migrations navigation; scopes, search, creation menu, related artifacts and source navigation. Preserve the `structure` view identity.
- **Persistence Designer**, right: Inspect, Create and Changes. Introduce tabs only when their workflows work. Property groups follow the selected declaration; Pin and Inspect at Caret control selection. Forms retain drafts. Review shows exact affected source/SQL and separates generation, saving and execution.

Use our UI kit and IntelliJ conventions: host fonts/themes, compact controls, native icons, keyboard access, visible focus, accessible labels, contextual actions and narrow-dock layouts. Do not recreate IDE chrome or add dashboard cards. Errors go to native notifications; invalid fields have inline feedback. Settings/small choices are on demand, not extra permanent windows. Preserve editor intentions and command-palette access.

Existing independent Spring Data/Flyway/Liquibase windows can remain available, but the unified JPA experience must not require them.

## Architecture

`crates/persistence-core` owns shared contracts for qualified source identities, provenance, persistence roles, mappings, relationships, query/DTO associations, schema snapshots and change plans. Unknown facts never establish absence. Editor providers consume supplied documents and cached facts; indexing/build/database work stays outside editor requests.

JPA Assist coordinates the experience. Versioned command adapters reuse Spring Data, Flyway and Liquibase engines without callback cycles. Rust validates project ownership, request context and all mutations; browser messages never provide authority by themselves.

An optional managed JVM helper supplies Java/Kotlin semantic facts, configured classpaths, provider mapping and JDBC metadata. Use released managed-process APIs. Explicit actions govern classpath resolution, dependency downloads and database access. Do not automatically execute application code/schema generation. Source-local tools remain useful without the helper.

Mutation pipeline: resolve → prepare → validate → review → revalidate → apply → verify. Use versioned editor edits and supported atomic FileActions. Revalidate every affected source, preserve non-overwrite and Undo. Where per-target expected versions are unavailable, application remains blocked rather than advertised as safe full parity.

## Workstreams

| Stage | Required delivery |
| --- | --- |
| 0. Reference and feasibility | Complete ledger; inspect delegated dependencies; resolve decompilation uncertainty; pin historical references where required; run original-runtime scenarios; record exact SDK limitations. |
| 1. Model/workspace | Shared identities/provenance, source overlays, project/module ownership, model/unit scopes, two-window shell, selection/drafts and source navigation. |
| 2. Entity Designer | Entity/embeddable/mapped-superclass creation, parent selection, ID types and generators, scalar/enum/embedded/collection/transient fields and editing existing mappings. |
| 3. Relationships/schema mappings | Cardinalities, ownership/inverses, cascade/fetch/orphans/order, join/collection tables, overrides, indexes/constraints, inheritance/discriminators and primary-key joins. |
| 4. Utilities/inspections | Listeners/callbacks, constructors, validation, constants, Lombok and proxy-aware equality/hash/string behavior, converters/custom types, Hibernate features and every audited inspection family. |
| 5. Refactoring | Superclass extraction, related-entity creation, semantic usages/rename, safe deletion and coordinated multi-file updates, with verified application guarantees. |
| 6. Spring Data | Repository options/batches/specifications, complete query/method builders and editing/extraction, parameters/results/paging/sorting/async/dynamic projection, named queries, graphs, auditing and revision repositories. |
| 7. DTO/mapping | Field selection/rename/order, nested/flattened/reused types, Java/Kotlin forms, mutable/fluent/record/class choices, serialization/Jackson/Lombok, validation transfer, MapStruct/ModelMapper/Blaze, synchronization and call-site injection. |
| 8. Reverse engineering | Configured connections and schema cache, tables/views/columns, view IDs, inheritance reconciliation, reference/shared-key/junction inference, unknown/deferred types, enums, comments, naming rules and existing-entity synchronization. |
| 9. Versioning | Model/DB/snapshot comparisons, initialization/scopes, dependency/risk review, rename merging/backfill/ignore/group policy, Liquibase formats/includes/templates, Flyway migrations/callbacks, DDL/DML/test-data/schema-repair and Envers schema. |
| 10. Closure | Related navigation, unresolved-reference workflows, conventions/settings sharing, minimalistic entry points, compatibility migration and every remaining acceptance case. |

Database certification starts with PostgreSQL/MySQL, then covers the audited MariaDB, H2, HSQLDB, SQL Server, Oracle and Db2 workflows. Respect installed engine versions/editions. Current JPA Buddy delegates connection management to Database Tools; functional connection support does not require recreating the historical standalone manager.

## Verification and release

Current implementation checkpoint: the UIKit Designer exposes Inspect/Create/Changes
within the existing two-window contract. Empty-editor Java/Kotlin creation handles
entity/embeddable/mapped-superclass roles, scalar ID types, compatible generator
strategies and explicit sequence/table parameters. Editor FileActions additionally
support reviewed sibling entity creation and adding a source-local identifier to
eligible existing classes. An Embedded ID intention selects a conservative existing
Java key for a Java/Kotlin owner in the same source package/set, rechecks current
key bytes and ownership, and returns only the invoking-document edit. Its source
method checks do not verify equality behavior; the final host application cannot
atomically guard the read-only dependent key. Typed payloads, source ownership/version checks, collision
checks, exact preview and rejection/draft retention have automated coverage. These
are partial Stage 2 deliveries. Module-aware language selection, inheritance,
broader composite ID creation/picking, effective provider models and native/reference differential
acceptance remain open; this checkpoint closes no full-parity requirement.

Each requirement needs a reference fixture, identical implementation fixture, observable expected results, source/SQL output, mutation/navigation semantics, language/provider/backend scope and original/ours evidence. Captured extension registrations are coverage boundaries, not a count of independent user-facing features.

Test Java/Kotlin, javax/jakarta, field/property access, inheritance/composite IDs, duplicate qualified names, modules, dirty buffers, nested projects, generated sources and unknown models. Compile generated code and mappers. Exercise provider proxies/lazy collections/recursive string rendering. Validate schema round trips and migrations against disposable databases.

UI acceptance covers Chromium/WebKit, actual native embedding, light/dark, host fonts, narrow/wide docks, keyboard-only use, accessibility, draft retention, stale responses, cancellation, denied grants, helper failure and project closure. Native preview/apply/Undo must be observed separately from browser and MockHost checks.

Commands:

```sh
python3 scripts/check-jpa-parity.py
python3 scripts/test-jpa-parity.py
python3 scripts/check-jpa-parity.py --release
```

The first validates the ledger, the second tests false-green regressions, and the third must fail until every pinned requirement has verified differential acceptance evidence. Useful increments may ship as partial parity; full parity must not be claimed prematurely.
