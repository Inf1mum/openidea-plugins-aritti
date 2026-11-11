# JPA parity — session handoff

Updated: 2026-11-11 (second session: relationships + inheritance increments). Work stopped at the user's request to continue in another session.

## Resume here

- Worktree: `/Users/aritti/orca/workspaces/openidea-plugins/jpa-parity`
- Branch: `arnaud-ritti/jpa-parity`
- Base/head at this checkpoint: `b89c4ba2956791f2b7561957259dc45a882e1792`
- No implementation commits have been made. Changes are in the working tree, including untracked source files. Preserve this worktree; do not reset, clean or replace it.
- Original checkout `/Users/aritti/Projects/openidea-plugins` has unrelated concurrent work. Do not implement there or copy its current contents over this worktree.
- Some dirty files were inherited when the worktree was prepared. Their inventory is local/ignored: `audit/jpa-parity-baseline/inherited-files.json`. Do not assume every dirty file belongs to this implementation or bulk-commit them without review.
- The user requested continuation through a sub-agent. The latest increment was implemented by a sub-agent and reviewed by the coordinator. No agent remains working on this checkpoint. Resume with bounded sub-agent increments and coordinator review.

Read `CLAUDE.md`, `.claude/skills/writing-openidea-plugins/SKILL.md` and its `ui-ux.md`, then [PLAN.md](PLAN.md), [requirements.json](requirements.json) and this file.

## Goal and constraints

Implement full functional parity with JPA Buddy `com.haulmont.jpab` version `263.7307.112`, plus the audited historical documentation workflows. The original archive SHA-256 is `8402d2c11e69e2747d3ceb8d1d8985c6f7d35af6a2db2bdf965317e52d05a176`.

- At most two persistence tool windows: native **Persistence** on the left, UIKit **Persistence Designer** on the right, with Inspect/Create/Changes pages.
- Use `crates/ui-kit`, compact IntelliJ-style controls, keyboard access, theme tokens and retained drafts. Small on-demand choices are acceptable.
- Use released SDK `v0.1.40`, plugin API `1.4`, host requirement `^1.4`. Do not patch the host/SDK or use shell writes to bypass edit guarantees.
- Preserve release-managed plugin versions.
- Implement independently; do not copy decompiled original implementation into product code.
- Reuse the existing Spring Data/Flyway/Liquibase engines where appropriate; their integration remains future work.
- Preview/export is not application/refactoring parity. Full parity requires original/runtime differential evidence and native acceptance.

## Current progression

Engineering estimate communicated to the user: approximately **10–15% toward full functional parity**, with low confidence. This is a scope estimate, not a measured completion percentage.

Ledger: **355 coverage requirements; 7 partial, 347 unverified, 1 blocked, 0 verified-parity**. The entries include action/extension registrations, so they are not 355 independent missing features. Some existing capabilities are implemented but have not been compared to the reference. All entries remain open for the full-parity release gate.

Partial entries: G006 mapping inspector; G013 entity generation; G015 ID type choice; G016 sequence parameters; G017 Embedded ID selection; G018 add identifier; S006 selection/draft/keyboard behavior.

Confirmed blocker S007: released FileAction edits do not expose an expected version for each existing target document. Safe, verified coordinated multi-file edits remain blocked. A selected read-only dependent key also cannot be atomically guarded through final host application. Do not mark these guarantees complete.

## Implemented in this worktree

### Designer and shared contracts

- `crates/persistence-core`: source identities/provenance, persistence roles, validated literal column edits, typed class/identifier/generator drafts and byte-edit validation.
- `plugins/jpa-assist/src/designer.rs`: typed private bridge, project/current-owner/source/version/selection binding, bounded requests/frame state, exact preview, single-use prepared tokens and versioned current-editor edits.
- UIKit property grid and Designer Inspect/Create/Changes UI. Rejected requests retain input; stale/foreign responses cannot redirect edits. Keyboard tabs, reset and narrow docks have browser coverage.
- Plain Java scalar `@Column` editing preserves unrelated arguments/comments/CRLF. Unknown expressions, inherited/generic/property/ambiguous mappings and unsupported shapes remain read-only. Kotlin inspection exists; column editing remains Java-only.

### Class and scalar identifier generation

- Verified empty-editor class creation under conventional named `src/main` or `src/test` Java/Kotlin packages. Path determines name/package/language.
- Entity, Embeddable and MappedSuperclass roles; optional version where appropriate.
- ID types Long, Integer, Short, String, UUID and BigInteger. Compatible assigned/identity/sequence/table/AUTO/UUID strategies with explicit sequence/table settings and bounds validation.
- New configured Kotlin entities/mapped superclasses use open classes/properties without depending on all-open.
- Reviewed sibling entity creation and Add Identifier FileActions. Insertion refuses existing IDs, inherited/generic/property-access contexts, source/accessor collisions and prefix shadows.
- Existing reviewed role/attribute creation includes embedded values, element collections and transient fields.

### Latest sub-agent increment: Embedded ID selection

- `plugins/jpa-assist/src/embedded_id_action.rs`, wired through `file_actions.rs`.
- **Add embedded identifier with preview…** chooses an eligible existing Java embeddable for a plain Java/Kotlin entity or mapped superclass.
- Same package and conventional source set, exact qualified identity, matching namespace, current project ownership, bounded discovery, duplicate checks and fresh key bytes after picker/preview.
- Conservative candidate subset: public Java key, 2–16 supported scalar fields, Serializable, explicit/implicit eligible no-arg constructor, declared equals/hashCode; canonical annotation/scalar spellings. Literal serialVersionUID supported.
- Serializable is a conservative older-API-compatible restriction, not a universal Jakarta 3.2 requirement. Method presence does not verify database-consistent equality semantics.
- Only the invoking document is edited; the key is not modified. FQ names, cancellation/staleness, CRLF and Java/Kotlin accessor collisions are covered.
- Key creation, Kotlin/record/inherited keys, broader classpath/source scope, semantic equality and final dependent-key atomicity remain open.

### Second session (single agent after the first sub-agent increment)

- Relationship options in Add JPA attribute (S001/S002 partial): per-type cascade, orphanRemoval, fetch, @OrderBy/@OrderColumn, @JoinColumn, @JoinTable; Kotlin cascade arrays fixed. Designer UI added later this session; no editing of existing attributes.
- Inheritance on class creation (S003 partial): `InheritanceDraft` (root/subclass) in `crates/persistence-core/src/generation.rs`, rendering in `wizard.rs::configured_class_source`, Designer Create-page fields in `web/creator.js` + `designer.html`. Serde cannot `deny_unknown_fields` on enum variants, so unknown keys inside `inheritance` are ignored. Subclass cannot cross-check the root strategy.
- Designer Inspect page: Add attribute form (`web/attribute.js`, `prepare-attribute` operation in `designer.rs`, shared `attribute_text_edits` in `lib.rs`) reuses the prompt-flow renderer; exact preview + versioned apply, same token path as other edits.
- Existing relationships (G006 partial): `relationship_edit.rs` + `web/relationship.js`; Inspect page shows fetch/orphanRemoval/optional/cascade for a selected relationship and `prepare-relationship` replaces or appends only those literal arguments (plain-literal guard, imports added, CRLF/Kotlin handled). `designer::locate` was extracted from `field()` and is shared.
- Sub-agent increments (coordinator-verified): join column/join table/order editing in `relationship_edit.rs`; Java-only superclass extraction (G019 partial) in `src/extract_superclass.rs` (declared via `#[path]` from file_actions.rs; edits invoking doc + one create, no cross-file version guard).
- Wave 2 (sub-agents, coordinator-verified): native tree grouping by persistence unit + related artifacts (`structure_related.rs`, G001/G002/G004 partial); Spring Data repository creation with options (`repository_action.rs`, G036/G038 partial, G037 batch NOT done); MapStruct mapper creation (`mapper_action.rs`, G069 partial). All declared via `#[path]` in file_actions.rs.
- Wave 3 (sub-agents, coordinator-verified): projection creation (`projection_action.rs`, G053/G059 partial), repository query-method action (`query_method_action.rs`, G040-G044 partial), Kotlin compile coverage for repository/mapper (Jakarta only; MapStruct processing not run for Kotlin). Real bug found by the CRLF regression test and fixed: mapper creation did not convert line endings.
- Wave 4 (sub-agents, coordinator-verified): validation constraints (`validation_action.rs`, G010), attribute converter (`converter_action.rs`, G025, Java only), index/unique constraint + named query (`index_query_action.rs`, G007/G008). Known parser limit: Kotlin properties with string-literal initializers are dropped by the model parser (existing behaviour, not fixed). Validation env also needs JPA_JAKARTA_VALIDATION_API (~/.m2 jakarta.validation-api 3.1.1) and JPA_JAVAX_VALIDATION_API (validation-api-2.0.1.Final.jar from the JetBrains spring-bootstrap plugin dir).
- Round 5 (cleanup + waves 5/6, coordinator-verified): ledger E/A/I mapping (176 mapped, 14 N/A, 5 unmapped in `unmapped.md`; checker validates, release still fails), `scripts/jpa-compile-env.sh` (SOURCE it in the same shell, not through a pipe), Kotlin parser string-initializer fix in `model.rs::mask/members`, Lombok checks (`lombok_checks.rs`, G020-G024), naming constants + templates (`naming_action.rs`, G031/G033), derived-to-@Query (`query_extract_action.rs`, G051). Decisions: managed JDBC helper route for live DB; optional process:* capabilities pre-approved; Hibernate/Envers/Blaze/ModelMapper tail in scope.
- Round 6 (coordinator-verified): async/dynamic projection (`query_convert_action.rs`, G049/G050), entity graphs (`entity_graph_action.rs`, G048), DTO class forms/Jackson/validation transfer (`dto_action.rs`, G060/G061/G062/G068/G078), embedded JPQL completion (`jpql_language.rs`/`repository_completion.rs`, G052). G045 deliberately skipped: it edits a document other than the invoking one (S007). Several helpers became `pub(super)` (index_query_action, query_method_action, mapper_action, validation_action) for reuse.
- Round 7 (coordinator-verified): Envers (`envers_action.rs`, G028/G029), Hibernate types/JSON/events (`hibernate_action.rs`, G026/G027/G124/G126), nested/flattened DTOs (`nested_dto_action.rs`, G063/G065), offline schema snapshot + diff core (`crates/persistence-core/src/schema.rs`, `src/schema_snapshot.rs`, G104/G105/G108/G109; library only). `scripts/jpa-compile-env.sh` now also exports HIBERNATE_CORE_JAR (hibernate-core 6.6.49.Final).
- Round 8 (coordinator-verified): SQL emitter + Flyway migration (`crates/persistence-core/src/sql.rs`, `src/migration_action.rs`, G112/G115 partial), inspection audit + 7 new rules (`src/inspections_more.rs`, `parity/INSPECTIONS.md`, F005/I008 partial), JDBC helper design (`parity/JDBC_HELPER.md`, design only).
- PENDING USER DECISIONS for the live-database route (JDBC_HELPER.md D1-D10): D5 asks to approve new capabilities `tasks.begin` and `ui.input` (only optional `process:*` is pre-approved), D2 `database.sources`, D3 driver acquisition, D4 password handling, D7 first databases, D8 confirmation for repo-sourced URLs. Rows G080/G084/G086 are blocked on these plus a P0 host spike (process:java string, stdin logging, driver loading under source launch). Correction: plugins/spring-boot/plugin.toml has only fs.read:~/.m2/repository; the earlier-assumed Gradle cache grant does not exist there.
- Round 9 (coordinator-verified): Liquibase changelog generation (`liquibase.rs`, `liquibase_action.rs`, G112), Designer Migration tab (read-only plan/SQL, G104/G109/G111), SQL-file-to-entities (`ddl_parse.rs`, `ddl_entities_action.rs`, G087/G093/G097/G103), repository injection (`injection_action.rs`, G130/G134). Coordinator added a `[[contributes.providers]] kinds=["code-actions"] languages=["sql"]` block to plugin.toml (the SQL-file action is unreachable without it; `sql` is a language id other plugins in this repo already use). plugin.toml version untouched.
- Round 10 (coordinator-verified): mapper methods/generic mapper/settings/inject mapper (`mapper_more_action.rs`, G070-G072/G132), equality utilities (`equality_action.rs`, G035; G034 audit), SQL-file extensions (G088-G090/G092/G100-G102), Envers/scope/baseline snapshot (G125/G106/G107). OPEN DECISION: Kotlin lifecycle template is final (`fun postLoad() {}`) though the spec forbids final callbacks; not changed, local_callbacks.rs does not check Kotlin. baseline_json/scoped snapshot not wired into the migration actions.
- Round 11 (coordinator-verified): Flyway Java migration/SQL callback/Java callback (`flyway_misc_action.rs`, G117-G119; verified by javap on flyway-core 9.22.3/10.22.0 because documentation.flyway.dev was unreachable), ModelMapper/Blaze (`modelmapper_action.rs`, G073/G074; stubs only), query keyword settings + 'Edit query method…' (`query_edit_action.rs`, G046/G047/S004). `scripts/jpa-compile-env.sh` now exports FLYWAY_CORE_JAR (flyway-core-10.22.0.jar copied to /tmp/jpa-parity-compile-api, which is a /tmp tool dir: recreate from Maven Central if wiped).
- Round 12 (coordinator-verified): auditing fields/configuration + batch repositories (`auditing_action.rs`, `repository_action.rs`, G056/G037), typed backfills (`sql.rs`, `liquibase.rs`, migration/designer, G110), association advice + `inspectionSeverities` (`inspections_more.rs`, `live.rs`, G009/G030). Note: `scripts/check-jpa-designer.mjs` was edited by an agent for the new `backfills` field (expected). plugin.toml: only the coordinator's `sql` code-actions provider block was added; version untouched.
- Round 13 (coordinator-verified): SQL-file views/JOINED parents/deferred columns/defaults settings (G091/G094/G095/G099), naming strategy detection (`naming_strategy.rs`, G032/G127/G128), shared ProjectFacts + `entityLanguage=auto` + `gate` (`project_facts.rs`, G014/G066). Remaining buildable-but-skipped: G077 declaration conventions, other build-proof sites migration to `gate`, Java source-level facts for records.
- Round 14 (coordinator-verified): datasource discovery/config (`datasource_facts.rs`, `datasource_action.rs`, G081-G083/G085), revision + entity view repositories (`revision_repo_action.rs`, G057/G058), DTO reuse + record/class conversion (`dto_convert_action.rs`, G064/G067). Coordinator added a `[[contributes.providers]] kinds=["code-actions"] languages=["properties"]` block to plugin.toml (needed by the datasource action; host language id `properties` assumed, as plugins/flyway uses it) and SPRING_DATA_ENVERS_CLASSPATH / BLAZE_SPRING_DATA_CLASSPATH to `scripts/jpa-compile-env.sh` (spring-data-envers 4.1.2 and the Blaze base jar were copied into /tmp/jpa-parity-compile-api; recreate from Maven Central if /tmp is wiped).
- NOTE: the main checkout /Users/aritti/Projects/openidea-plugins showed uncommitted jpa-assist changes (SDK tag v0.1.40 -> v0.1.42 in Cargo.toml/Cargo.lock, plugin.toml, src/lib.rs, new icons/) that are NOT from this worktree's agents; this worktree still builds on v0.1.40. Expect a merge to reconcile.
- Env for the new opt-in compile tests: `MAPSTRUCT_JAR`/`MAPSTRUCT_PROCESSOR_JAR` (~/.m2 mapstruct 1.6.3), `SPRING_DATA_CLASSPATH` (commons 3.4.13 + jpa 4.1.2 + spring 6.2.19 + slf4j + jakarta api; a mixed set, no spring-data-jpa 3.x jar locally).
- Unverified from agents: JpaSpecificationExecutor on 2.x (memory); Spring Data version inferred from entity namespace; Kotlin mapper syntax never compiled.
- Checks: 836 plugin tests, 70 persistence-core tests, 33 compiler checks (all pass with scripts/jpa-compile-env.sh sourced; it now also exports SPRING_CONTEXT_CLASSPATH). Source the script in the SAME shell: piping `source` loses the variables. + 5 explicit compiler checks (incl. 12 generated hierarchies), 9 persistence-core tests, Chromium and WebKit Designer checks, ledger tests, API 1.4 build/check with the 0.1.40 CLI all passed. Still no native IDE or original-runtime evidence.

## Validation at checkpoint

- **336 plugin tests passed**, with 3 compiler tests opt-in.
- **6 persistence-core tests passed**.
- All **3 explicit compiler checks passed**: 66 generated Java classes, 66 generated Kotlin classes, and 4 generated entity/Java-key pairs covering Java/Kotlin owners × Jakarta/javax. Pinned APIs: Jakarta 3.2.0, javax 2.2; Kotlin compiler 2.2.21; JVM target 17.
- Bytecode checks cover public no-argument constructors and non-final configured entity/mapped-superclass classes/accessors.
- Chromium and WebKit each passed six Designer layouts: light/dark × 280×400, 400×700, 800×600, including class creation, typed generators, preview/apply bridge, stale responses, reset and keyboard behavior. Latest Embedded ID increment did not change web assets.
- Scoped UI sync, ledger validation and its 4 regression tests passed; `git diff --check` passed.
- Matching API 1.4 WASM build and package check passed; exactly two views remain.
- **No native IDE apply/Undo acceptance or original-runtime differential acceptance has been observed.** No live-IDE installation/deployment performed; no full parity release claim.

Latest local ignored evidence: `audit/jpa-embedded-id-qa/implementation-receipt.json` and neighboring logs. Coordinator checked recorded exit codes and hashes against current source/WASM. Earlier browser/creation evidence is in `audit/jpa-creation-qa/`; initial evidence is in `audit/jpa-designer-qa/` and `audit/jpa-parity-baseline/`. These local artifacts are not tracked and must be preserved separately if moving machines.

## Commands and local tooling

Run from this worktree:

```sh
cargo test --manifest-path plugins/jpa-assist/Cargo.toml --quiet
cargo test --manifest-path crates/persistence-core/Cargo.toml --quiet
python3 scripts/sync-plugin-ui.py --plugin jpa-assist --check
python3 scripts/test-jpa-parity.py
python3 scripts/check-jpa-parity.py
python3 scripts/check-jpa-parity.py --release
git diff --check
```

`--release` is expected to exit 1 while gaps remain. Do not weaken it to obtain a green release.

Matching CLI, if still present:

```sh
/tmp/laravel-parity-final-target/debug/openidea-plugin build plugins/jpa-assist
/tmp/laravel-parity-final-target/debug/openidea-plugin check plugins/jpa-assist
```

The default installed CLI was too old (`0.1.38`); check its version before using it. Recreate a released `0.1.40` CLI if the temporary binary disappears. Use scoped UI sync, not whole-repository sync. Avoid whole-plugin formatting that churns unrelated inherited files.

Compiler tests use `cargo test --manifest-path plugins/jpa-assist/Cargo.toml generated_ -- --ignored --nocapture`, with these environment variables:

- `JPA_JAKARTA_API=/tmp/jpa-parity-compile-api/jakarta.persistence-api-3.2.0.jar`
- `JPA_JAVAX_API=/tmp/jpa-parity-compile-api/javax.persistence-api-2.2.jar`
- `KOTLIN_COMPILER_CLASSPATH`: contents of `/tmp/jpa-parity-compile-api/compiler-classpath.txt`
- `KOTLIN_STDLIB`: the kotlin-stdlib jar entry from that classpath
- Prepend `/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home/bin` to PATH for the tested Java toolchain.

These are temporary local dependencies; restore pinned artifacts if absent. Compiler tests do not resolve or execute user application code.

Browser test: `node scripts/check-jpa-designer.mjs` (add `--webkit` for WebKit), with:

- `PLAYWRIGHT_MODULE=/tmp/jpa-parity-ui-tools/node_modules/@playwright/test/index.mjs`
- Chromium `PLAYWRIGHT_EXECUTABLE_PATH=/Users/aritti/Library/Caches/ms-playwright/chromium_headless_shell-1248/chrome-headless-shell-mac-arm64/chrome-headless-shell`
- WebKit `PLAYWRIGHT_EXECUTABLE_PATH=/Users/aritti/Library/Caches/ms-playwright/webkit-2370/pw_run.sh`
- `UI_QA_DIR`: an ignored audit directory.

## Next implementation priorities

1. Reconfirm worktree/status and read the ledger before choosing the next bounded increment. Preserve current uncommitted/untracked work.
2. Continue Entity Designer relationship and inheritance workflows: cardinality/ownership, cascade/fetch/orphans/order, join/collection tables, overrides, indexes/constraints, superclass selection/extraction and discriminator/primary-key joins. Inventory existing generators first; reuse them rather than duplicate engines.
3. Broaden composite ID creation/picking and module-aware language/type resolution without asserting compiler/provider facts from lexical guesses.
4. Complete persistence-unit/model scopes, pinning/continuous selection and related navigation. Build semantic/helper contracts before workflows that require effective mappings.
5. Follow the remaining plan for inspections/utilities, safe refactoring, Spring Data, DTO/mappers, database reverse engineering and Flyway/Liquibase integration.
6. Add native embedding/apply/Undo and original-plugin differential fixtures. Existing browser/MockHost/compiler checks alone cannot close verified parity.

Reference audit, available locally but ignored: `docs/jpa-buddy-gap-analysis-2026-11-10.md`. Captured registration metadata: `audit/jpa-buddy-263.7307.112/registrations.json` and `receipt.json`. Original decompiled material remains under the original checkout's ignored audit directory; consult only as behavioral/reference evidence, never copy implementation.

## Acceptance kit (branch arnaud-ritti/jpa-acceptance, 2026-11-11)

- `ACCEPTANCE.md` (96 scenarios, all 115 partial rows covered), `receipts/` (schema, template, examples), `fixtures/` (six projects with GOLDEN.md generated from the plugin's own live provider), `differential/` (24 scenarios, capture sheet) and `scripts/{check-jpa-receipts,compare-jpa-output}.py` with tests. `check-jpa-receipts.py --require-pass` fails until real receipts exist: that is the gate, run it after the native sessions. The kit never edits `requirements.json`.
- CLAIM MISMATCH FOUND: 14 rule codes exist in `checks.rs` and are advertised in `settings.schema.json` (some in the README) but are emitted only by the legacy full-context path, never by the live provider, so they cannot be observed in the IDE: jpa-kotlin-plugin-missing, jpa-redundant-open, jpa-redundant-constructor, jpa-listener-without-callbacks, jpa-relationship-target, jpa-repository-id, jpa-unknown-attribute-override, jpa-unknown-entity-graph, jpa-unknown-entity-graph-path, jpa-unknown-map-key, jpa-unknown-maps-id, jpa-unknown-named-query, jpa-xml-class-not-persistent, jpa-xml-class-unknown. Decide per code: wire it into the live provider, or remove the setting/README claim. Affected rows are listed in ACCEPTANCE.md 'Not observable in the IDE'.
- Differential scenario original-action names: 15 come from memory of the public docs and are unverified, 9 are unknown; the tester corrects them from the installed menus.

## Round 15 (branch arnaud-ritti/jpa-templates)

G116 changeset templates and G120 naming macros (`persistence-core/src/liquibase.rs`, `naming.rs`, `src/template_settings.rs`): both partial; defaults unchanged. 877 plugin tests, 85 persistence-core tests. Open: Liquibase `<property dbms>` substitution, runAlways/dbms, other preconditions; the `#increment`/`#date` semantics are our choices (JetBrains pages give no example outputs); no host clock API (date typed or WASI clock).
