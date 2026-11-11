# Database plugin parity implementation — 2026-11-10

## Fresh original baseline

Official Marketplace API fresh queries (2026-11-10) confirm Flyway23740 and Liquibase23741 latest versions both **263.7307.101**, same versions as prior pinned receipts. JSON responses: `/tmp/parity-flyway-original-updates.json`, `/tmp/parity-liquibase-original-updates.json`. Official JetBrains documentation pages now identify IDEA2026.3:

- https://www.jetbrains.com/help/idea/flyway.html
- https://www.jetbrains.com/help/idea/liquibase.html

Public docs establish current workflow families: init/diff/empty/Java migration and SQL/Java callback generation, table navigation, run configurations and naming for Flyway; init/diff/empty changelog generation, logical source structure, execution and source configuration for Liquibase. No proprietary JAR/source was acquired.

## Implemented changes

### Flyway

- Configuration model retains literal placeholderPrefix/placeholderSuffix (including comma and multi-character delimiters), plus placeholderReplacement=false. Project-scoped completion consumes that configuration, replaces an already present complete suffix without duplication, handles UTF16, and refreshes after config save.
- New bounded source table navigation from unquoted FROM/JOIN/UPDATE/INSERT INTO/ALTER TABLE/DROP TABLE/TRUNCATE TABLE/REFERENCES uses to CREATE TABLE declarations in configured migration SQL. Schema-qualified identity must match exactly. CREATE declarations themselves do not navigate, matching the original public workflow. Comments/literals are excluded; quoted/dollar-quoted or malformed SQL is withheld rather than guessed. This does not publish Rename symbols or infer live DB/entity identity.
- Literal qualified JPA @Entity/@Table mappings additionally link table uses to entity source classes within the known source module. Explicit imports/qualified annotations or an unambiguous persistence wildcard are required; implicit naming/constants/foreign imports are withheld.
- Parameterized FileActions for Java/Kotlin migration source and SQL/Java callbacks now ask for source root/package/version/description/events/transaction behavior, validate names and SQL/Java/Kotlin normalized version collisions, then return exact non-overwriting undoable Creates. Event metadata covers38 supported source-callback events, with edition/version wording; beforeConnect arbitrary scripts and internal operation-finish aliases are not misrepresented as SQL callback events.
- Updated README/change-notes; no release version changed.

### Liquibase

- JSON property completion reuses bounded lexical source-key scanner to establish actual parent even in unfinished JSON. Existing values preserved; literal value strings and unrelated JSON scopes stay quiet.
- Formatted SQL `--changeset author:id` attribute completion supplies proper key:value syntax, omits already present attributes and preserves existing values. Includes contexts/labels/execution/statement and rollback delimiters.
- Expanded18 previously name-only core change-type attribute catalogs against public latest XSD (https://www.liquibase.org/xml/ns/dbchangelog/dbchangelog-latest.xsd), covering sequences, procedures, lookups, merge, constraint removals, remarks and CSV updates.
- Acquired official Liquibase OSS4.29.2; core JAR independently matched Maven Central SHA1 and runtime inputs have local SHA256 inventory. Real disposable H2 source/target create, JSON snapshots, scrubbed offline diff and generated XML all passed.
- Real runtime exposed a product bug: stock generated XML declares1.1 while capture admitted1.0 only. Fixed validation of XML1.0-compatible1.1 output without altering retained source bytes/fragment offsets; XML1.1-only control and line-normalization semantics remain rejected. Actual output now passes subset-selection capture tests.
- Real conflict probes prove command-specific env URL variables override generic CLI URL args. Offline commands now reserve active command-specific URL/diff-type and generated output/author env keys to reviewed values. API1.4 refuses inherited-key collisions before spawning, rather than silently connecting to the conflicting target. Installed-runtime probes verified all reserved env names accepted and intended output retained. This is a bounded known-conflict fix, not a general clean-environment/extension isolation claim.
- Generated source organizer now supports up to32 migration groups; moving selected complete changesets; group switch/removal; ignored changeset restore; lexical Add/create, Drop/delete and Other/SQL operation-class selection. At most one group owns each changeset and each group preserves source order.
- New grouped-save FileAction returns all selected group XML files in one atomic non-overwriting workspace action after per-file path selection and full concrete preview. All changes to groups/sources expire old actions. No grouping action starts CLI or writes source files.
- Actual sanitized runtime snapshots/diff/generated outputs, provenance and regression tests added under `plugins/liquibase/tests/fixtures/liquibase-4.29.2/`.

## Validation

- Flyway `cargo test --locked`: **127 passed, zero failed**. Log `/tmp/parity-flyway-test.log`.
- Liquibase `cargo test --locked`: **169 passed, zero failed**. Log `/tmp/parity-liquibase-test.log`.
- Both `cargo build --locked --release --target wasm32-wasip2` pass. Logs `/tmp/parity-flyway-build.log`, `/tmp/parity-liquibase-build.log`.
- `git diff --check` for both plugin directories passes.
- Runtime evidence `/tmp/parity-liquibase-runtime/`: `probe-results.json`, `bound-results.json`, `conflict-results.json`, `generated-conflict-results.json`, `runtime-sha256.json`, exact logs and reproducible `probe.py`, `bound.py`, `conflict.py`. Official tar retained there. Core license is Apache2.0; distribution license inventory retained in extracted runtime.
- Runtime tests used isolated test processes and disposable local H2 files, never a user DB. No Update/Rollback on a real DB. No GUI acceptance, packaging check, installation or publication performed by this agent.

## Residuals — full parity is NOT established

- Flyway P05.3 effective database authority is not proven universally: inherited environment/config/JVM/extensions still determine actual runtime target. Basic source-observed managed review/output/cancel remains implemented.
- Flyway P05.4 production orchestration remains deliberately unavailable without an installed/licensed Flyway Enterprise13.12.1 profile and proven schemaModel snapshot/diff/SQL-generation format. No paid runtime/license acquired or fabricated. Public Redgate diff docs: https://documentation.red-gate.com/flyway/reference/commands/diff.
- Liquibase P12.4 has fresh real OSS4.29.2/H2 offline snapshot/diff/generated-source contract evidence and fixed capture behavior; it is not equivalent to all dialects, full source/entity mapping, online snapshot acquisition, GUI transport/undo acceptance or general effective-target assurance. No historical whole-task promotion.
- Both originals' complete DB/entity-model-derived init/diff DDL still needs a separate semantic model/driver adapter (plugin work where an explicit supported external adapter can supply semantics). Existing source templates/offline reports are not equivalent. Native datasource inspection, entity-model queries, console/Project decorations and core run-widget integration require host interfaces unavailable in API1.4. Those must remain distinct from feasible adapter work.
- Flyway quoted/dialect-complete table resolution, implicit entity naming strategies, constants, Kotlin entities and XML/classpath mappings still require richer semantic adapters. Explicit Java JPA associations are implemented and tested; they do not establish full persistence-unit semantics.
- Parameterized source-root/package/class/event dialogs are now implemented and tested. Liquibase multi-file grouping/ignore/restore is implemented for actual CLI-generated XML; operation classes are deliberately not an invented semantic danger/risk score. Flyway imported object definitions still are not executable generated migration SQL, so grouping them into executable migrations remains coupled to the unavailable Enterprise runtime/semantic adapter.
- Existing native migration/source tree structure, source generation/FileActions and managed command lifecycles retained. Shared generated web resources were changed by root/another worker concurrently, not edited by this agent; all such files preserved.
