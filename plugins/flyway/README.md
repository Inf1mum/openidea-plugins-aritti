# Flyway Migrations

One shared UI-kit window contains the Flyway, Command Output, Comparison tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

One UI-kit tool window contains the Migrations, Command Output and Comparison tabs. Commands reveal the corresponding custom tab; CLI execution, cancellation, source previews, comparison controls and editor actions keep their existing handlers and ownership checks. Tab navigation itself starts no command or database connection.

Flyway support for SQL migrations in OpenIDEA. There is no database to ask:
the plugin reads migration file names and the project's Flyway configuration,
and explicitly runs approved Flyway commands in a terminal or an instance-owned finite process.

## Checks

Migrations are checked when a `.sql` file or a config file opens or is saved,
and by **Flyway: Check Migrations**, which also reports how many were checked.

| Check | Severity |
|---|---|
| File name doesn't match the naming scheme (prefix, version, separator, description, suffix) | warning |
| Same version used twice (versioned, undo and baseline migrations, each kind on its own) | error |
| Same description used twice by repeatable migrations | error |

Duplicates are looked for within one module (the part of the path before
`src/main/resources`), so the same version in two modules is fine. Each
duplicate points at the other files. Callbacks (`beforeMigrate.sql`…) are
accepted.

A rename done outside the IDE isn't seen until the next open or save; run
**Flyway: Check Migrations**.

## Where migrations are found

In this order:

1. The `locations` setting (comma-separated), if not empty.
2. `flyway.locations` in `flyway.toml`, `flyway.conf`, `conf/flyway.toml` or
   `conf/flyway.conf` (the first one that exists), and
   `spring.flyway.locations` in `application*.properties`/`.yml`.
3. `classpath:db/migration`, plus `filesystem:sql` when a Flyway config file
   exists.

`classpath:` locations are searched in every `src/main/resources`;
`filesystem:` locations relative to the project root or as an absolute path
inside it. `{vendor}` matches any directory. Other schemes (`s3:`, `gcs:`)
are ignored. The same files give the prefixes (`sqlMigrationPrefix`,
`undoSqlMigrationPrefix`, `repeatableSqlMigrationPrefix`,
`baselineMigrationPrefix`), the separator and the suffixes; defaults are
`V`, `U`, `R`, `B`, `__` and `.sql`.

## Commands

- **Flyway: New Migration…** opens a naming form in the existing Migrations
  view. Edit the directory, category (Versioned, Undo, Baseline, Repeatable),
  version, description and configured suffix. Description words use ASCII letters
  and digits, joined by underscores; the preview shows any normalization. The final path and bundled SQL
  template contents are read-only previews. **Prepare editor action** validates
  the plan without writing a file or running the CLI. In a project SQL or Flyway
  config editor, invoke code actions (⌥⏎) and choose **Create Flyway migration from
  prepared form…**. Review the exact target/content confirmation, then the host
  creates and opens the file as one undoable, non-overwriting workspace action.
  The SDK exposes atomic FileActions through editor intentions, not a command
  apply call; preparing the form is therefore a separate step from applying it.
- **Create Flyway SQL migration…** in editor code actions is a shorter Versioned
  workflow: enter version, description and directory, then confirm the exact
  target and template. It uses the same validation and atomic create.
- **Rename Flyway migration…** in a configured SQL migration editor accepts a
  complete corrected filename, previews old/new paths and returns one atomic
  rename. File contents and references are unchanged. Callbacks/unrelated SQL,
  same-name moves, existing/open-buffer targets and invalid names are refused.
  Save unsaved source changes first: the host refuses dirty resource-buffer
  renames. Applied database history is not queried; review the effect before renaming an
  applied migration.
- **Flyway: Copy Next Migration Version** copies the next versioned prefix
  (`V3__`) to the clipboard, and the notification names the folder of the
  newest migration. Without the `clipboard` permission, the notification
  shows the name instead.
- **Flyway: Info / Validate / Migrate / Repair / Clean** show concrete project/runner/source
  provenance before launch. The default `executionMode=terminal` uses native Run for admitted Info/Validate
  and a terminal for other commands; `managed` uses the scoped finite command view described below. Clean warns that
  it drops objects and always requires confirmation; there is no unscoped “run twice” fallback.
  Flyway's own `cleanDisabled` setting still applies.

API 1.4 native Run configurations offer **Flyway: info** and **Flyway: validate**
for observed root or `conf/` Flyway configuration files. They follow `runner`
(`flyway` or Maven), use the owning project root as cwd and provide native Stop/Rerun.
Custom `arguments` withdraw these configurations so credentials/options are not
persisted in Run configuration records; reviewed command launches remain available.
Default interactive Info/Validate use native Run when the same configuration is
available. Managed JSON/private collection stays instance-owned. Migrate, Clean
and Repair remain freshly reviewed commands, outside native Rerun. The CLI still
resolves the effective configuration, environment and database target.

The setting `runner` picks `flyway` (the CLI, goal `info`) or `mvn` (goal
`flyway:info`); `arguments` are added after the goal, split on spaces
(`-environment=dev`). Running a program needs the `process:flyway` or
`process:mvn` permission, which a notification asks you to allow in
Settings › Plugins.

## Migrations view

The Flyway Migrations view is cached. Opening/rendering it never starts a CLI command or connects
to a database. **Refresh Info…** reviews source provenance, starts a managed explicit JSON Info
command (`-outputType=json`, or Maven `-Dflyway.outputType=json`), and opens **Flyway Command
Output**. **Refresh Status** polls bounded stdout/stderr and completion; it may need another press
to finish draining output after the process exits. A successful completed JSON result updates the
owning project's migration rows. **Cancel Owned Command** kills only the stored child handle;
it does not roll back database changes. Source links retain the existing version/description or
reported-filepath lookup, with exact project membership checked before opening. Results are
labelled cached; failures/cancellation do not fabricate an empty successful runtime state.

Managed mode is finite, **instance-lifetime** ownership: plugin/window/project shutdown or roughly
five-minute idle reclamation may terminate the child. Status is explicitly refreshed, not a durable
background job/continuous stream. Keep native terminal mode for work needing native terminal
lifetime/control or full raw output. Terminal output is outside plugin redaction/status tracking.
Managed status/cancel controls refuse another project's handle, and a second start cannot silently
replace a running child. Failed starts, permission failures, unknown/disappeared handles, exit codes,
closed roots and cancellation remain distinct; no exit/cancel state claims a database rollback.
A failed project-list/root query is an unavailable ownership check, not evidence of project closure:
it retains the owned handle and labels that state explicitly. A later successful check can
revalidate the owner, while a successful inventory proving closure still closes the handle.
The owner can explicitly poll/cancel once access is restored; no denied query fabricates cancellation.

The concrete review records the invoking project, approved runner, option-token keys and bounded
**observed source paths/presence**, with every argv/config value hidden. It snapshots exact contents
of four conventional Flyway sources, project-local `flyway.user.toml` overlays,
up to 16 explicit owned `-configFiles=` (CLI) / `-Dflyway.configFiles=` (Maven)
paths in comma-separated lists, and up to 32 discovered Spring application configs
(64 KiB each),
then checks their content, root and runner/argv/mode again after confirmation. Source text is held
only in the temporary private approval proof; session UI stores masked provenance. External/imported
configs, stdin/dynamic/external configFiles inputs, redirected working directories,
Maven loader settings, CLI-selected environments, inherited environment and runtime
precedence remain unresolved. Effective database target is explicitly **UNKNOWN**; editor naming/
location hints are not silently treated as runner arguments or runtime loader truth. Unsupported,
missing-permission or capped source discovery is labelled incomplete, never proof of target identity.
Unresolved explicit sources or working-directory overrides conservatively suppress managed
output; no outside-project/nested-project source is read for this proof. Relative
configFiles observations use the invoking project root only when no working-directory
override is present. Conventional Flyway files, personal overlays and discovered Spring
configs now receive the same innermost-project check as explicit configFiles before any
source read, including absent candidate paths. A foreign/unavailable owner is recorded
as incomplete provenance, excluded from accepted source facts, and suppresses output.
Opening a nested project changes the approval proof even when source bytes are unchanged.
Explicit sources/personal overlays also invalidate captured Info on
observed edits/removal. A final external filesystem/auth/config race after approval remains possible.

Managed output buffers complete lines per stream before display, so configured literal secrets split
across poll chunks are replaced together. Connection/auth-labelled lines, URLs/JDBC strings, controls/
ANSI and oversized lines are conservatively suppressed. Known credential/URL/user literals from
observed sources and CLI options are masked without recursively rewriting mask markers. Incomplete
secret extraction suppresses output. This is conservative classification, **not universal secret
recognition**: arbitrary runtime/Maven/environment output can contain values absent from inspected
sources. Output is capped at 16 KiB per stream, lines at 4 KiB and raw JSON Info at 1 MiB; capped raw
JSON is refused, never parsed as complete. Polls read at most 16 KiB per stream and drain the final
status buffers before accepting Info. Extra argv retains whitespace tokenization (no shell parsing),
is capped at 8 KiB/128 tokens and refuses bare additional goals; use `-key=value` option tokens.

Original comparison: decompiled `FlywayRunProfileState` builds a process from an explicit IDE data
source, module classpath and configured migration directories, and listens for termination to refresh
the data source. `FlywayConfigurationSettingsEditor` separates data source, directories, baseline and
schema fragments. The inspected documentation screenshots `flyway-0.png`/`flyway-1.png` show explicit
DB/model and source/target selectors. This CLI adaptation keeps invocation source/output ownership
visible but does not claim that native data-source identity, schema diff, driver/classpath resolution
or refreshed database metadata. No matching command-console screenshot or new native desktop
acceptance is claimed. Existing SQL create/rename forms and FileActions are preserved unchanged.
Maven JSON compatibility still depends on the installed Flyway Maven plugin; Flyway10+ CLI JSON
is the supported parser shape.

## Completion

- In SQL files, after `${`: Flyway's built-in placeholders
  (`flyway:defaultSchema`, `flyway:user`, `flyway:timestamp`…) and the
  `flyway.placeholders.*` names from the project's config. The scanned config
  now supplies literal `placeholderPrefix`/`placeholderSuffix` delimiters,
  including multi-character and comma delimiters. `placeholderReplacement=false`
  disables placeholder suggestions; a config save refreshes the owning project.
  This is editor assistance, not proof of effective CLI configuration.
- In `flyway.conf` (and `flyway-*.conf`): `flyway.*` keys, written as
  `key=`.
- In `flyway.toml`: keys under `[flyway]`, and the connection keys under
  `[environments.<name>]`, written as `key = `.

## Templates and icons

New menu: Flyway SQL Migration, Flyway Java Migration, Flyway Kotlin Migration,
Flyway Java Callback and Flyway SQL Callback. The class is named after the file
you create (`V2__Add_users`), so name the file first. A template cannot fill in
the version, so take it from Flyway: Copy Next Migration Version. The Java and
Kotlin templates have no `package` line; add it, or move the file into place.
`V*__*.sql`, `U*__*.sql` and `R__*.sql` files get the Flyway icon. Not verified
in the IDE: that `$NAME$` is the file name without extension, and that the icon
patterns match.

## Not supported

Live database/model-derived DDL generation and executed schema diffs are not implemented.
An offline comparison-report/definition review and non-executing CLI recipe are described below.
The bundled SQL template starts blank; this does not reproduce the original
Flyway change-model/database generator. Existing Java/Kotlin/callback templates
remain available from the New menu; the prepared form handles SQL only.
Custom placeholder prefix and suffix, `src/test/resources` locations and quoted
values with spaces in `arguments` remain unsupported.

Generation and rename revalidate project ownership, source snapshot, naming and
locations before returning operations. Destinations stay inside configured
locations and cannot traverse paths or contain unresolved wildcards. Collision
checks preserve the existing module/vendor/category semantics, including
normalized version equality; a discovery cap or read failure blocks the action.
The host's non-overwriting Create/Rename operations also protect against a file
appearing after the preview. There are no shell writes, direct filesystem writes
or new process permissions. Source discovery/configuration still uses the
existing text-based Flyway configuration subset, not an effective database model.

Native mock tests validate operation contracts, previews and rejection paths;
actual desktop apply/undo, dock layout and light/dark visual comparison remain
unverified. The inspected original `FlywayChangePanel` / constraint-panel sample
links a change model to generated SQL; these receipts do not establish parity
for this blank-template workflow.

```sh
cargo test                       # logic, against the SDK's mock host
openidea-plugin build            # → dist/plugin.wasm, validated like an install
openidea-plugin link             # load this folder in the IDE (Settings › Plugins)
```


## Offline comparison report and SQL definition review (P05.4 partial)

Run **Flyway: Review Local Comparison Report…** to open the native supporting
review pane. Enter an existing project-relative standalone Flyway `diffText`
JSON report, load it, and explicitly select change IDs. The pane shows each
selected source and target object definition separately with its Add/Edit/Delete
label. A Delete entry's target `CREATE` definition is the old object's definition,
not a `DROP` migration. No SQL is inferred, combined into a migration, executed,
copied to a terminal or written to the project. A diff-only summary with null
definitions explicitly asks for diffText output. Rendering reads the imported
snapshot only and never starts a process or contacts a database.

The bounded parser accepts the documented `differences[]` shape with unique IDs,
Add/Edit/Delete types, compatible source/target objects and literal definitions.
It rejects CLI banners/logs, explicit error payloads, malformed or duplicate-key
JSON, ambiguous/unsafe IDs, unknown types and mismatched sides. Input is limited
to 1 MiB/500 changes, each SQL side to 64 KiB, and a selection to 32 IDs/64 KiB
combined SQL. No truncation is presented as a complete preview. Other report
fields are ignored and never displayed; only selected object SQL is shown.
SQL may contain arbitrary user-authored text and is displayed verbatim as
read-only text. No dialect/dependency/deployment correctness is asserted.

A separate **Review CLI recipe…** button records exact argv arrays after a full
bounded confirmation. It supports only the explicitly reported **13.12.1
Enterprise** CLI recipe profile checked against the current documentation:

1. Snapshot a project-local schema model into a fresh local snapshot file.
2. Compare that snapshot against `empty` or an existing project-local target snapshot,
   writing a fresh local diff artifact.
3. Request selected object definitions with `diffText` and its explicit change IDs.

The program is `flyway`, with an explicit project working directory and scoped
argument names. Existing Maven/CLI settings and extra argv are not silently
reused. Source/model/report/target/artifact paths must be concrete, distinct and
inside the project; the model/output parents and optional target must exist,
and output files must be absent. Root, report text and relevant path facts are
rechecked after confirmation. There is no Run/Apply control. The recipe is a
review aid, not approval to execute it, and future external execution must
recheck all files/configuration and IDs. An imported report is **not proven** to
belong to an artifact that a later comparison would generate; rerun and reselect
against the actual artifact before using those IDs externally.

Version and edition are self-reported inputs, not an executable/license probe.
Unknown/other profiles refuse recipe generation while offline report inspection
remains available. Sessions and controls carry fresh tokens and exact project
ownership; old windows, anonymous roots, stale reports and forged read-only
fields cannot change a new review. Source freshness uses project-visible text,
which may be unsaved; cache age, binary producer, artifact/report pairing,
source snapshot format and effective runtime database target remain unknown.
The session is in-memory only. Report/model/target/output path access is checked
against the innermost open project; a nested project's report cannot be imported
into the outer project's review. Missing/foreign project renders replace the old
body with a safe message without discarding another project's owned cache.
Observed report edits/removal retire selected SQL and recipe, rotate controls and
notify the mounted pane using a model-free message. Recipe-input changes retire
the recipe while leaving the explicitly imported report snapshot available.
Bounded event filters retain migration patterns and the draft's source subtrees,
including arbitrary report extensions/parent removal. Render reads cached data
and ownership metadata only; it performs no file reads, discovery or CLI calls.
Unobserved external/unsaved changes remain possible; actions still reread their
source before use. Existing migration generation/FileActions and managed
execution/cancellation/settings remain separate and unchanged.

Primary contracts checked on 2026-11-09:
[Snapshot](https://documentation.red-gate.com/flyway/reference/commands/snapshot),
[Snapshot source setting](https://documentation.red-gate.com/fd/flyway-snapshot-source-setting-277579127.html),
[Diff](https://documentation.red-gate.com/flyway/reference/commands/diff),
[Diff Text](https://documentation.red-gate.com/flyway/reference/commands/diff-text),
[Generate](https://documentation.red-gate.com/flyway/reference/commands/generate)
and [release notes](https://documentation.red-gate.com/flyway/release-notes-and-older-versions/release-notes-for-flyway-engine).
Current comparison docs label these commands Enterprise. The release notes put
the diff/generate family out of preview at 11.0.0, but that does not attest any
installed binary or prove this adapter's current JSON contract for every older
version. `generate` requires a real diff artifact and can write migration files;
it is deliberately not represented by object-definition concatenation here.

The original decompiled `FlywayChangePanel` uses a database/change model and SQL
generator linked to an editor preview; its incomplete `init` decompilation has a
bytecode fallback. `FlywayAddNotNullConstraintPanel` and the retained official
screenshots establish model/database choices rather than this CLI report format.
This adapter does not reproduce database/model selection, dependency inclusion,
entity-derived DDL, editable migration generation, rollback/deployment or native
schema metadata. Execution integration still needs real supported binary/license
and artifact provenance, reviewed configuration/target ownership, bounded managed
jobs and per-step failure/partial-output handling. Native desktop acceptance and
matching visual parity have not been run. P05.4 remains partial.

Native offline review follow-up: isolated pinned desktop31 verified report loading/selection, raw definition preview, profile gate and non-executing recipe Cancel/record. Stale reports clear both selected preview and recipe visibly, including the error path. No real Flyway/DB operation was performed.


### Safe source navigation and Rename gate

The old document-index graph is withheld: table names and Java entity names are
not interchangeable replacement tokens, and placeholder config keys previously
included their `flyway.placeholders.` prefix. Automatic indexed Rename/usages and
table/JPA gutter links are no longer advertised. Atomic migration file rename and
the migrations tree's source-opening actions remain separate and unchanged.

Read-only Go to Definition and SQL gutter navigation now link `${name}` to the
exact placeholder-name token in one supported owned `flyway.conf`/`flyway.toml`.
Names match case-insensitively, following Flyway's documented placeholder contract;
values and credentials are not cached or displayed. Configuration-name Go to
Definition can show previously supplied cached SQL occurrences; that list is not
a complete usages/rename set. Providers never discover or read files or execute
anything. Open/save events or Check Migrations populate configuration facts;
current SQL provider documents populate source references.

The bounded subset requires one canonical root/conf Flyway configuration, literal
in-project filesystem or main-resource classpath locations and supported literal
SQL migration names. Competing files, Spring configuration overlays, environments,
custom delimiters/replacement disabling, non-ASCII/escaped/dynamic names, unknown layouts and
malformed input yield no result. SQL line/block comments are excluded; ordinary
quoted strings are supported; dollar-quoted bodies are quiet. Excess occurrences
invalidate that file rather than returning a truncated graph. Source/provider input
is limited to 128 KiB, config to 32 KiB, 512 declarations/references per file,
128 supplied SQL files/4,096 references per root and eight roots. Reverse navigation
returns at most 256 locations or none. Config and per-file source facts expire
independently after 120 seconds; delivered file changes invalidate ownership.
Lifecycle warming has a cooperative 200 ms budget (individual host calls cannot be
interrupted). After invalidation/expiry, reopen/save or Check Migrations to warm the
source configuration again. External CLI/environment/runtime overrides and live DB
schemas are not resolved. See [NAVIGATION_AUDIT.md](NAVIGATION_AUDIT.md) for evidence
and the exact feature reduction. Native navigation acceptance is still separate.

### Mounted migrations refresh and captured source ownership

Accepted explicit Info results and changed managed command status now send a
model-free `ui-kit-refresh` notification to an already mounted Migrations page.
Start/failure, status draining/finish, Cancel and observed source retirement
update it; unchanged polls and automatic render do not send notifications. The
native Command Output view retains its existing revision refresh path. External
New Migration also updates the mounted generation preview without changing its
atomic FileAction behavior.

Info acceptance compares the completed command's bounded invocation-source proof
with the current known config/runner/arguments/mode before publishing rows. The
cached view is labelled with capture age and source provenance; effective CLI,
environment/import and database target precedence stays unknown. Observed config,
SQL or source-parent changes retire cached rows. On automatic rendering, only
current project ownership and settings metadata are checked—no discovery, file
read, CLI or HTTP. Missing/denied/foreign identity or changed runner/settings
withholds cached rows. Unobserved inactive-buffer/external changes can leave a
historical capture until an event or explicit refresh; no continuous freshness
claim is made. Known-source proof is retained privately in bounded memory and is
never broadcast or shown as source values.

A foreign/missing-project render never cancels or replaces another project's
owned job. Source/config retirement also leaves that job intact; its status is
explicitly a captured invocation, not current connection truth. Existing explicit
Cancel and project-close cleanup remain unchanged. This follow-up is covered by
95 Rust tests and matching API1.3 package checks; it has no new native acceptance.
Dated Native35 and staged CLI receipts prove their original artifacts only.

Flyway105 completed Info output requires one migrations-bearing JSON object with unique object keys; conflicting roots or duplicate keys no longer become a false empty/selected migration result. Noisy Maven banners remain supported with bounded candidate scanning; accepted JSON roots are skipped as units. The callback rejects non-object/overlarge row collections and CLI error objects. This validates output framing only, not effective database/config identity or runtime success beyond captured status. Full105 source/MockHost tests and pinned API1.3 build/check pass; no real CLI/DB/network or new native acceptance.

Flyway106 known-source credential inventory retains both raw and comment-aware bounded sensitive values. Inline # in Java-properties credentials no longer leaks a suffix through output/Info label redaction. Escaped, interpolated or compound sensitive values with unresolved decoding conservatively suppress classified output/labels. The static source/Output regression uses fabricated credentials, with no real CLI/database/network. This remains conservative known-source handling, not universal ambient-secret classification or effective target binding.

### Managed comparison readiness and orchestration core

**Managed adapter readiness…** captures up to 64 owned schema-model text files
(1 MiB total), an optional owned target snapshot and fresh output paths. It
reviews candidate stage argv without executing. Exact source bytes and output
vacancy are rechecked after review. Start and Continue remain unavailable: no
installed, versioned offline CLI profile has been accepted, and entering
`13.12.1` / `Enterprise` does not attest one.

The managed core owns one child, closes stdin, separates snapshot/diff/diffText
stages, requires explicit Check/Continue, drains bounded output and retains
observed output-file correspondence. New reports are parsed before sharing the
existing explicit raw-definition picker; imported change IDs are never reused
for a new artifact. Nonzero exit, stderr, oversized output, stale inputs or
changed output refuse publication. Cancellation keeps partial files and claims
no rollback; input changes request stop, unknown project membership retains the
handle, and deactivation requests stop under the host instance/window lifetime. These paths are exercised with
MockHost only behind the profile gate, not accepted runtime CLI behavior.

The outstanding prerequisite is an authoritative installed version/edition and
configuration/startup profile that establishes the proposed local source modes
and offline behavior. Current primary pages differ on `schemaModel` versus
`schemaSnapshot`; configFiles documentation does not by itself establish that
all automatic overlays are excluded. Telemetry is environment-only and the SDK
cannot replace/remove inherited environment variables. Offline Enterprise
licensing/startup and artifact formats remain unverified. The core does not
work around those gaps with a shell, copied credentials, or user self-report.
P05.4 remains partial; the offline imported-report workflow stays available.

## Native migration browser

The main migration browser now uses the IDE's native tree, toolbar and status
line. **Filter…** searches the captured row labels/state (and retains include
ancestors in Liquibase); **Clear** restores rows. Filters belong to the
current project. **More…** exposes existing review and CLI
workflows through a native picker, preserving their source checks and database
write confirmations. Double-click or Enter opens the owned source location.
The generation and offline review forms remain native; rendering does not start
CLI work.

Compared against [JetBrains' official flyway workflow](https://www.jetbrains.com/help/idea/flyway.html):
source browsing, generation preview and reviewed execution remain connected.
This is a native presentation of the supported workflow, not full datasource,
entity-derived DDL/diff or screenshot parity. Filter uses a native input dialog
rather than the former inline web search. Exact desktop light/dark, keyboard,
narrow-dock and native prompt acceptance remain to be verified.

The compact toolbar uses native Refresh, New, Open, Filter, Clear and More icons.
Captured migration rows are grouped by status, with working expand/collapse.
Selecting a source-linked migration enables Open and seeds New's directory from
that migration's configured location. The generation form shows a target filename
and readable SQL preview; empty templates are explicitly identified as empty.
Stale or foreign selections cannot open or seed another source.

## Source table navigation

Go to Definition on unquoted table names in admitted migration SQL now links
`FROM`, `JOIN`, `UPDATE`, `INSERT INTO`, `ALTER/DROP/TRUNCATE TABLE` and
`REFERENCES` uses to cached `CREATE TABLE` declarations in the configured
migration files. Schema-qualified names match exactly; an unqualified name is
not guessed to belong to another schema. Declarations themselves do not link.
Comments and string literals are excluded. Quoted identifiers, dollar-quoted
SQL and malformed/oversized sources are withheld rather than assigned guessed
identity. Navigation is project-scoped, bounded and refreshed on migration/config
open/save; external file notifications withdraw affected caches. It is neither
runtime table introspection nor a Rename graph or entity-class mapping.

## Parameterized source generators

Editor FileActions now offer Java/Kotlin migrations and Java/SQL callbacks in
addition to SQL migrations. Migration actions prompt for version, description,
source directory and package; callback actions prompt for supported event(s),
class/description and, for Java, transaction handling. The final preview returns
one non-overwriting undoable Create. No files or processes are created during
resolution. Numeric versions are compared across SQL/Java/Kotlin sources in the
same known module; incomplete source discovery refuses creation. Java/Kotlin
packages and class names must be literal non-keyword identifiers. These actions
create source skeletons; callback scanning locations, classpath, dependencies and
runtime edition/version support still need project configuration.

For literal Java `@Entity` plus `@Table(name=..., schema=...)` mappings whose
annotations resolve to explicit Jakarta/javax persistence imports, SQL table
navigation additionally offers the entity class. Qualified persistence wildcard
imports are admitted only without competing wildcard imports/local annotation
owners. Association stays inside the known source module. Implicit physical
naming strategies, constants, Kotlin metadata, XML mappings and classpath models
are not guessed, and this navigation never supplies Rename identity.

## Optional compiled Hibernate model adapter

**Generate Reviewed Offline Hibernate Schema SQL…** captures creation DDL from
explicitly selected compiled annotated classes using checksum-pinned
Hibernate6.6.49 or Hibernate7.4.15 runtimes and an explicit dialect choice. Each
version offers16 qualified presets, including PostgreSQL, MySQL and MariaDB; the
latest profile also accepts Jakarta Persistence3.2 annotations. An exact classpath/source inspection precedes the
trusted-code approval: metadata can instantiate project converters/custom types
or contributors with arbitrary side effects. No application bootstrap or JDBC
operation is requested by the adapter. The result stays a dated, instance-owned
preview until **Save Hibernate schema SQL as migration…** returns one exact
non-overwriting undoable Create. Source/compiled inputs are checked again before
saving. The preview records the actual dialect and its stock default database version.
Live DB diff and automatic effective application models require separate support. See [setup, contract and explicit runtime
fixture](adapters/README.md).
