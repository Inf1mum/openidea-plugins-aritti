# Parity specs: G113 G114 G116 G120 G121 G123 G129 S005

Research date 2026-11-11. Sources are public documentation only; nothing decompiled. Class names from `audit/jpa-buddy-263.7307.112/registrations.json` (via the E/A/I ledger rows) are intent hints only and are marked "inferred".

Caveat on quotes: WebFetch returns a model-written extraction of each page, so quoted strings below are as extracted, not a byte-exact copy. Treat them as close paraphrase.

## Sources actually read (all fetched 2026-11-11)

| Tag | URL | Useful content |
|---|---|---|
| JB-DBV | https://www.jetbrains.com/help/idea/jpa-buddy-database-versioning.html | Settings, Diff Changes, Naming Strategy, schema validation fix, Init/Diff DDL, SQL Visual Designer, DDL by Entity, snapshots |
| JB-LQ | https://www.jetbrains.com/help/idea/liquibase.html.md | Changelog file-name macros, Changeset Templates, cross-DB property templates, settings path |
| JB-FW | https://www.jetbrains.com/help/idea/flyway.html.md | Flyway prefix / version pattern / separator / description, macros |
| JB-ED | https://www.jetbrains.com/help/idea/jpa-buddy-entity-designer.html(.md) | Inspections paragraph, Lombok inspections, naming templates, constants, Designer settings |
| JB-PAID | https://www.jetbrains.com/help/idea/jpa-buddy-paid-features.html.md | Ultimate feature list (diff/init changelog, versioned migration, DDL by entities) |
| JB-MAIN | https://www.jetbrains.com/help/idea/jpa-buddy.html.md | Liquibase/Flyway dependency table, version/maintenance notes |
| JB-FAQ | https://www.jetbrains.com/help/idea/jpa-buddy-faq.html.md | Settings path for Designer settings only |
| JPAB-DOC | https://www.jpa-buddy.com/documentation/ , .../database-versioning/ , .../entity-designer/ | Feature summary lines only; no inspection catalogue, no settings page |

Not obtainable: the JetBrains Marketplace page for plugin 15075 returned only its title (no release notes); https://www.jpa-buddy.com/blog/ returned no post list. No blog post on versioning, naming or settings sharing was read. No dedicated "settings", "SQL" or "FAQ" page exists on jpa-buddy.com per the documentation index.

Context: since IntelliJ IDEA 2023.3 JetBrains maintains the plugin; since 2024.1 Liquibase/Flyway features live in bundled plugins (JB-MAIN, JB-PAID, JB-FW). So the "original" documentation for G113-G120 is now the IDE's Liquibase/Flyway help.

---

## G113 Changelog inspector

1. Original, user-visible.
   - JB-MAIN: JPA Buddy "provides visual designers for changelogs". JPAB-DOC: "Edit Liquibase changelog with autocompletion for table and column names." JPA Palette "appears when editing ... Liquibase changelogs" and offers code-generation for the current context (search result summary of the JetBrains JPA Buddy page, not separately fetched).
   - No page read describes the inspector's fields, tag deletion, or the editor notification bar.
   - Inferred from ledger class names only: a tool-window inspector with property editors per changeset attribute (context, labels, dbms, schema name, table name, column names(s), sequence name, boolean expression, column type, file name), a tag item loader, a "Delete tag" action (A041 JpbLiquibaseDeleteTag), line-marker actions, and an editor banner (E097). Flyway has a parallel inspector actions provider (E123/E128/E129).
2. Confidence: visual designer and name completion = documented (one line each); inspector layout/fields/actions = inferred from ledger class names; behaviour = unknown.
3. We already have (STATUS/README): `plugins/liquibase` source completion (changeSet skeleton, change types, `rollback`, `runOnChange`), changeset tree/hover, `Add Liquibase changeset...` array action, duplicate-changeset and incomplete-changeset inspections, YAML/JSON array actions; jpa-assist `liquibase_action.rs` (G112 changelog generation) and the Designer pages (Inspect/Create/Changes, `migration.js` read-only plan).
4. Proposal (buildable): "Edit changeset attributes..." code action on a changeSet in a single Liquibase changelog (XML/YAML/JSON as already parsed by `plugins/liquibase`): form for `context`, `labels`, `dbms`, `runOnChange`, `failOnError`, `runAlways`, preview + single-document edit with Undo. Reuse liquibase `array_actions.rs`/`changelog.rs`. Delete-tag = "Remove changeset..." with preview. Property pickers for table/column names are G114's job. No tool-window inspector in this increment; do it as a UIKit Designer "Changelog" tab only after the single-document form works. Lives in plugins/liquibase (owner of the format), jpa-assist merely links.
5. Open questions: (a) put the editor in `plugins/liquibase` (recommended: owns parsing) or in jpa-assist? (b) is a Designer tab required, or are code actions enough? (c) which attributes beyond the list above matter to you (e.g. `preConditions`)?

## G114 Model completion

1. Original.
   - JPAB-DOC / JB-MAIN: autocompletion of table and column names in Liquibase changelogs, "taken from your data model" (JB-MAIN wording, via search summary). JB-MAIN also says Flyway SQL files get scaffolding of INSERT, UPDATE and DELETE statements for entities.
   - Not stated: whether completion covers XML, YAML, JSON and SQL changelogs, or whether it uses the DB connection or the JPA model.
   - Inferred: descriptors `TableNameDescriptor`, `TableColumnNameDescriptor(s)`, `SchemaNameDescriptor` in the inspector (E111-E114).
2. Confidence: documented that names come from the data model; scope per format unknown.
3. We already have: `plugins/liquibase` completion of keys/change types (no model names); jpa-assist `schema_snapshot.rs` (derived table/column names with provenance, G104) and `naming_strategy.rs` (physical names, G032); `plugins/flyway` `complete.rs` (statement-level completion).
4. Proposal (buildable): a jpa-assist `completion` provider for language ids already used by liquibase (`xml`, `yaml`, `json`) and `sql` that fires only when the cursor is in a value position whose key is `tableName`/`baseTableName`/`referencedTableName` (columns: `columnNames`, `name` inside `<column>` of a table-scoped change). Candidates = tables from `schema_snapshot` (physical names via `naming_strategy::apply_physical`), unknown-naming entities omitted (never guessed). Needs the current entities from the document index; limit by existing 500-entity bounds. Single-document, read-only, no new capability. Cross-provider concern: two plugins registering completion on the same file; verify the host merges them (memory note: hand-combined `register_plugin!` is required for several providers in one wasm; inter-plugin is separate and fine).
5. Open: (a) completion in jpa-assist (needs Liquibase key knowledge duplicated) or an exported model-names file consumed by `plugins/liquibase`? (b) names from entities only, or also from the baseline JSON (`db/jpa-schema-baseline.json`)? (c) SQL-file completion also?

## G116 Change templates

1. Original.
   - JB-LQ "Changeset Templates": preset attributes per change type that "the IDE inserts ... when it generates a changelog": `failOnError`, `runOnChange`, and "Create Preconditions" with behaviour HALT, CONTINUE, MARK_RAN or WARN. Also "Cross-DB property templates": generated changelogs may use Liquibase properties such as `string.type` with `dbms` so one changelog serves several DBMS. New changelogs use the "Liquibase Changelog template from File and Code Templates". Settings: Tools | Database Versioning | Liquibase: primary/secondary changelog directories, changeset `author` (OS username when empty), Liquibase version, file type (XML/YAML/JSON/SQL), option to add an empty `rollback` tag for changes without automatic rollback. JB-DBV: Diff Changes settings let you set Liquibase context and labels per change type.
   - Flyway: JB-FW mentions no templates beyond generated Java migration/callback stubs.
2. Confidence: documented (JB-LQ, JB-DBV).
3. We already have: liquibase `author` setting and `git config user.name` fallback; skeleton creation via `Add Liquibase changeset...`; jpa-assist `liquibase.rs` generator (one changeSet per change, danger policy, no preconditions/rollback/context/labels), `flyway_misc_action.rs` stubs (G117-G119).
4. Proposal (buildable): add structured settings to jpa-assist (`liquibaseChangeSetDefaults`: `failOnError`, `runOnChange`, `preConditionsOnError` in {off, HALT, CONTINUE, MARK_RAN, WARN}, `addEmptyRollback`, `context`, `labels`) and have `persistence-core/liquibase.rs` emit them into the generated changelog (XML only for now, as today). Gate: only attributes the Liquibase 4.29 XSD already verified. `author`: reuse existing resolution. Cross-DB property templates (`${string.type}`): defer; needs `<property>` emission and per-dialect output.
5. Open: (a) per change type or one global default? (original is per change type; per-type is more settings). (b) default for `addEmptyRollback`? (c) approve Liquibase `<property dbms=...>` output as a later item?

## G120 Naming macros

1. Original.
   - JB-LQ (changelog file names) and JB-FW (Flyway "Version pattern") both document three macros: `#increment(<start>, <step>, <decimalFormat>)` "generate versions using a numeric sequence"; `#date(<simpleDateFormat>)` "using the current system date" (SimpleDateFormat pattern); `${semVer.<getterMethod>}` "using the semantic version from the project's build file", getters `getRawVersion()`, `getMajor()`, `getMinor()`, `getPatch()`, `getPreRelease()` (example `1.2.3-SNAPSHOT+build4`).
   - Flyway also: migration prefix (B/R/U/V, default V), migration separator (default `_`, must match Flyway's setting), migration description. Settings path Tools | Database Versioning | Flyway.
2. Confidence: documented.
3. We already have: `plugins/flyway/src/naming.rs` (parse/next_version: bump highest existing, configured prefix/separator) and its re-implementation subset in jpa-assist `migration_action.rs` (`V<next>__desc.sql`); Liquibase generator names the file without macros.
4. Proposal (buildable): a pure `version_pattern` expander in persistence-core: `#increment(start,step,fmt)` (start/step applied over existing versions: needs the highest existing version and DecimalFormat subset `0`, `00`, `000`), `#date(fmt)` (SimpleDateFormat subset `yyyy MM dd HH mm ss`, clock from host or input), `${semVer.getMajor()|getMinor()|getPatch()|getPreRelease()|getRawVersion()}` read from literal `version = "..."` in `build.gradle(.kts)` / `<version>` in `pom.xml` (project root only). Settings `migrationVersionPattern`, `migrationPrefix`, `migrationSeparator`, `migrationDescription` plus `changelogNamePattern` for Liquibase. Unparseable pattern or unknown version => refuse with message, never guess. Wire into `migration_action.rs` and `liquibase_action.rs`. Unit tests only against MockHost; clock must be injectable.
5. Open: (a) the semantic-version macro needs a version catalog/`gradle.properties` rule: literal-only OK? (b) is the host clock accessible to plugins (check SDK; else the user types the date)? (c) should `#increment` mean "highest existing + step" or "start for first file" (JB-FW text: start used for the first file, then step; confirm that matches your expectation).

## G121 Schema-error repair

1. Original.
   - JB-DBV: "A `SchemaManagementException` at startup usually means `ddl-auto` is set to `validate` and Hibernate couldn't map entities to tables." From the stack trace you can "generate DDL to fill up the difference between JPA entities and the database right from the stack trace." JB-PAID lists "Resolve validation fails issue right from the stack trace" as Ultimate-only (JPAB-DOC database versioning page).
   - Not stated: whether it works from Run console only, how it picks the datasource, the output format.
   - The diff uses the live DB, so original needs a connection.
2. Confidence: documented that it exists; mechanism unknown.
3. We already have: offline model-vs-baseline diff (`schema_snapshot.rs`, persistence-core `schema.rs` ChangePlan) and Flyway/Liquibase emission (`migration_action.rs`, `liquibase_action.rs`). No stack-trace parsing.
4. Proposal (buildable, reduced): not buildable as the original (needs live DB and console-text access). Buildable offline variant: a code action on pasted/selected stack-trace text in any text document, matching Hibernate's `SchemaManagementException: Schema-validation: missing table [x]` / `missing column [c] in table [t]` / `wrong column type encountered in column [c] in table [t]; found [..], but expecting [..]` lines (Hibernate 6.x message shapes; must be verified from Hibernate source/docs before building) and generating `CREATE TABLE` / `ALTER TABLE ADD COLUMN` SQL for those named objects from the entity model through the existing SQL emitter, as a new `.sql` file or clipboard text. Blocked sub-part: reading the IDE Run console (no host API known).
5. Open: (a) accept the paste-the-error offline variant? (b) is Hibernate `validate` message format worth fetching and pinning to 6.x only? (c) dialects: PostgreSQL/MySQL as today?

## G123 SQL designer

1. Original.
   - JB-DBV: "SQL Visual Designer: You can generate a range of SQL statements through the JPA Palette or Editor Toolbar, each with its own configuration window." JB-MAIN (Flyway row): "Scaffolds INSERT, UPDATE, and DELETE statements for your entities in SQL files." Related: JB-DBV "Init DDL" and "Diff DDL" toolbar icons in an SQL file or console, "DDL by Entity" (gutter icon, Alt+Enter, New | DDL by Entity).
   - Not documented: the full statement list, the configuration windows' fields. The ledger registers `com.intellij.jpb.flyway.database.console` (E132, inferred: a toolbar group in the database console).
2. Confidence: documented that INSERT/UPDATE/DELETE scaffolding exists; any other statement kinds and options unknown.
3. We already have: copied DDL (`ddl_action.rs`, PostgreSQL/MySQL), SQL-file-to-entities (`ddl_entities_action.rs`), Flyway SQL generation (`migration_action.rs`), `sql` code-actions provider already in plugin.toml.
4. Proposal (buildable): "Insert statement from entity..." code action in `.sql` documents (single-document insert at cursor): choose entity (from index), statement kind in {INSERT, UPDATE, DELETE, SELECT}, column subset (checkbox list), identifier policy (`identifierPolicy`, naming strategy), placeholder style (`?`, `:name`, literal `NULL`), preview then insert. Names from `schema_snapshot`; unknown names excluded with a warning. PostgreSQL/MySQL quoting only. No DB. Reuse `sql.rs` quoting.
5. Open: (a) which statement kinds do you want beyond INSERT/UPDATE/DELETE? (b) placeholder style default? (c) must it appear in a native palette (not available) or is a code action acceptable?

## G129 Shared settings

1. Original.
   - No page read documents sharing of JPA Buddy settings. JB-DBV says a data-model snapshot lets you diff against a release or branch "without keeping a database for each version", but does not describe sharing settings. JB-DBV Hibernate Envers: IDE can "read the existing settings from `.properties` files automatically". JB-FAQ: only "Settings | Tools | JPA Buddy | Designer Settings". JB-ED: naming templates "apply automatically to new entities and attributes".
   - General JetBrains guidance (search result, not JPA-specific): project settings live in `.idea` XML and can be committed. I found no page confirming a `.idea/jpa-buddy*.xml` file, its content, or whether it is meant to be committed. A033 `JpbShowJpaSettings` (inferred) opens the settings from the Structure tool window.
2. Confidence: unknown. The ledger row is "Unverified" for the same reason.
3. We already have: structured settings in `settings.schema.json` (naming case/prefix/suffix, `identifierPolicy`, `inspectionSeverities`, SQL import defaults, `entityLanguage`). OpenIDEA host settings are stored by the host (project/user scope handling belongs to the host; check `ui-ux.md`/host docs: unverified here).
4. Proposal (buildable, our own design, not parity): either (a) rely on host settings sharing and document it, or (b) an `Export conventions...` action that creates one JSON file `.jpa-assist.json` from current settings (create-only, `overwrite:false`) plus `Import conventions...` that applies it through the host settings API if one exists (unverified; if settings are read-only to plugins, the import step is not buildable and the file can only be read by the plugin itself as an overlay). Overlay variant: plugin reads `.jpa-assist.json` at project root and overrides user settings; keys limited to the schema. Needs a decision on precedence.
5. Open: (a) is there any real JPA Buddy artifact you want to be compatible with, or is a new team-conventions file acceptable? (b) precedence: project file over user settings, or the reverse? (c) does the host expose project-scoped settings (answer decides if this is buildable)?

---

## S005 19 screenshot-derived rule families vs current registrations

Finding: the ledger's "19 families" are rows F001-F019 (family "Inspection families"); the screenshot referenced in `docs/jpa-buddy-gap-analysis-2026-11-10.md` is not in the public documentation I could read. JB-ED states only: "JPA Buddy provides a lot of inspections that help during coding. By default, all inspections are enabled in all scopes and have warning severity." (inspection config: Settings | Editor | Inspections | JPA Buddy), shows one image titled "inspections", and names exactly three inspections (all Lombok): avoid `@EqualsAndHashCode` and `@Data` on entities; always exclude lazy attributes when using `@ToString`; add `@NoArgsConstructor` to entities with `@Builder` or `@AllArgsConstructor`. Therefore the 19 cannot be reconstructed from documentation; below are the 19 ledger families (ledger capability text, which came from the screenshot) mapped to our rules per `parity/INSPECTIONS.md`. Documentation support column: D = backed by the JB-ED sentence above, L = ledger/screenshot text only.

| Id | Family (ledger) | Doc | Our rule code(s) | Coverage |
|---|---|---|---|---|
| F001 | Column mapping on a relationship | L | none | missing |
| F002 | Unsupported persistent field type | L | `attribute-type` (Temporal/Enumerated/Version) | partial |
| F003 | Serializable composite key | L | none | missing |
| F004 | Converter type compatibility | L | none | missing |
| F005 | Entity construction | L (D for NoArgs, F018 side) | `jpa-no-arg-constructor`, `jpa-lombok-constructors`, `jpa-kotlin-plugin-missing`, plus 6 other `inspections_more.rs` rules | partial (entities with no explicit constructor not judged) |
| F006 | Entity-valued field without relationship mapping | L | none | missing |
| F007 | Absent identifier | L | `jpa-source-identifier-absent` (info), `entity-missing-id` (legacy, not live) | partial |
| F008 | Conflicting identity declarations | L | `jpa-duplicate-embedded-id`, `jpa-multiple-id-kinds`, `jpa-embeddable-identifier` | partial |
| F009 | Equality owner/class checks | L | none (generation only) | missing |
| F010 | Converter role annotation | L | none | missing |
| F011 | Multiple version fields | L | `jpa-duplicate-version` | partial (source-local) |
| F012 | Paged results without paging input | L | `page-without-pageable` in `plugins/spring-data` | covered elsewhere (other plugin) |
| F013 | Inverse one-to-one lazy caveat | L | none (README says inverse `@OneToOne(mappedBy)` deliberately not flagged) | missing |
| F014 | Table mapping under single-table inheritance | L | none | missing |
| F015 | Kotlin generated equality/string methods | L | `jpa-data-class`, `jpa-val-attribute`, `jpa-immutable-collection` | partial |
| F016 | Lombok string loading/recursion | D | `jpa-lombok-to-string` | partial (exclusion fixes for existing `@ToString` incomplete) |
| F017 | Write query marker | L | `query_needs_modifying` in `plugins/spring-data` | covered elsewhere (other plugin) |
| F018 | Lombok constructor generation | D | `jpa-lombok-constructors` (ledger text predates rule; INSPECTIONS.md says live) | covered (ledger row not re-verified) |
| F019 | Lombok entity equality | D | `jpa-lombok-data`, `jpa-lombok-equals`, `jpa-lombok-equals-field` | partial (replacement workflow absent) |

Tally: covered 1 here (F018) + 2 in other plugin (F012, F017); partial 8 (F002, F005, F007, F008, F011, F015, F016, F019); missing 8 (F001, F003, F004, F006, F009, F010, F013, F014). (1 + 2 + 8 + 8 = 19.)

I-row cross-check (`mappedTo` includes G030; same source): I003 missing; I009 missing; I010 missing; I011 missing; others map as in INSPECTIONS.md. A rule exists for I003's neighbour only via F002.

Buildable increments for the 8 missing (each only if the intent is verified from a spec first; INSPECTIONS.md already refuses unverified ones):
- F001: needs a spec statement that `@Column` on an association is invalid; none found in the Jakarta text previously read. Not buildable until a source is cited.
- F006: field of an entity type without relationship annotation: buildable only when the target is a project entity in the same document index (positive target-role fact); quiet otherwise. Spec basis: Jakarta Persistence 3.2 (an entity-typed attribute must be a relationship); fetch before building.
- F014 (`@Table` on a SINGLE_TABLE subclass): needs closed-world inheritance (exists in `schema_snapshot.rs`); cross-document, read-only, buildable as a live check only if the hierarchy is complete in the index.
- F003, F004, F009, F010, F013: need resolved types/generics or have no verified intent: not buildable now.

Open questions for the user: (a) can you provide the screenshot or the list of 19 titles? Without it F-row names are the only truth. (b) policy: implement missing rules only when a public spec sentence backs them (current policy) or accept inferred intent? (c) should F012/F017 count as covered by `plugins/spring-data` for parity purposes?

---

## Summary verdicts

| Row | Verdict | Confidence |
|---|---|---|
| G113 | buildable reduced: changeset attribute editor in `plugins/liquibase`; tool-window inspector later | documented name only; fields inferred |
| G114 | buildable: model-name completion from `schema_snapshot` in Liquibase/SQL values | documented (one line); scope unknown |
| G116 | buildable: structured changeset defaults feeding `liquibase.rs` | documented |
| G120 | buildable: `#increment`/`#date`/`${semVer}` expander + prefix/separator settings | documented |
| G121 | offline paste-the-stack-trace variant only; original not buildable (live DB, console) | exists documented; mechanism unknown |
| G123 | buildable: INSERT/UPDATE/DELETE/SELECT scaffolding code action in `.sql` | partly documented |
| G129 | unknown: no documentation; only our own export/overlay design, host-scope dependent | unknown |
| S005 | 19 families = F001-F019; documentation backs 3 Lombok ones only; 1 covered here, 2 covered elsewhere, 8 partial, 8 missing | doc cannot reconstruct list |
