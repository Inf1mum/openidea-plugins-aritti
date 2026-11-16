# Flyway Migrations

Flyway support for SQL migrations in OpenIDEA. There is no database to ask: the
plugin reads migration file names and the project's Flyway configuration, and
explicitly runs approved Flyway commands in a terminal or an instance-owned
finite process. MIT; see `LICENSE`.

One UI-kit tool window contains the Migrations, Command Output and Comparison
tabs; commands reveal the corresponding tab and tab navigation itself starts no
command or database connection.

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
accepted. A rename done outside the IDE isn't seen until the next open or save;
run **Flyway: Check Migrations**.

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

- **Flyway: New Migration…** opens a naming form in the Migrations view:
  directory, category (Versioned, Undo, Baseline, Repeatable), version,
  description and configured suffix. Description words use ASCII letters and
  digits joined by underscores; the preview shows any normalization. **Prepare
  editor action** validates the plan without writing a file; then invoke code
  actions (⌥⏎) in a project SQL or Flyway config editor and choose **Create
  Flyway migration from prepared form…** for the exact target/content
  confirmation, applied as one undoable, non-overwriting workspace action.
- **Create Flyway SQL migration…** (editor code actions): a shorter Versioned
  workflow (version, description, directory) with the same validation and
  atomic create. Editor FileActions additionally offer Java/Kotlin migrations
  and Java/SQL callbacks (version/description/directory/package, events,
  class/description, transaction handling): one non-overwriting undoable
  Create, numeric versions compared across SQL/Java/Kotlin in the same known
  module, literal non-keyword package/class names, no files or processes
  during resolution.
- **Rename Flyway migration…** in a configured SQL migration editor: a complete
  corrected filename, an old/new path preview and one atomic rename. Contents
  and references are unchanged. Callbacks/unrelated SQL, same-name moves,
  existing/open-buffer targets, invalid names and dirty source buffers are
  refused. Applied database history is not queried; review the effect before
  renaming an applied migration.
- **Flyway: Copy Next Migration Version** copies the next versioned prefix
  (`V3__`) to the clipboard and names the folder of the newest migration;
  without the `clipboard` permission the notification shows the name instead.
- **Flyway: Info / Validate / Migrate / Repair / Clean (Drops All Objects)…**
  show concrete project/runner/source provenance before launch. The default
  `executionMode=terminal` uses native Run for admitted Info/Validate and a
  terminal for other commands; `managed` uses the scoped finite command view
  below. Clean warns that it drops objects and always requires confirmation;
  there is no unscoped "run twice" fallback. Flyway's own `cleanDisabled`
  setting still applies. The setting `runner` picks `flyway` (CLI, goal
  `info`) or `mvn` (goal `flyway:info`); `arguments` are added after the goal,
  split on spaces (`-environment=dev`). Running a program needs the
  `process:flyway` or `process:mvn` permission, which a notification asks you
  to allow in Settings › Plugins.

API 1.4 native Run configurations offer **Flyway: info** and
**Flyway: validate** for observed root or `conf/` Flyway configuration files;
they follow `runner`, use the owning project root as cwd and provide native
Stop/Rerun. Custom `arguments` withdraw these configurations so credentials
are not persisted in Run configuration records. Migrate, Clean and Repair
remain freshly reviewed commands, outside native Rerun. The CLI still resolves
the effective configuration, environment and database target.

## Migrations view

The browser is native (tree, toolbar, status line) and cached; opening or
rendering it never starts a CLI command or connects to a database. **Filter…**
searches the captured row labels/state, **Clear** restores rows, filters
belong to the current project, and **More…** exposes the review/CLI workflows
through a native picker. Rows are grouped by status; double-click or Enter
opens the owned source location. Selecting a source-linked migration enables
Open and seeds New's directory from that migration's configured location; the
generation form shows a target filename and SQL preview with empty templates
identified; stale or foreign selections cannot open or seed another source.

**Refresh Info…** reviews source provenance and starts a managed explicit
JSON Info command (`-outputType=json`, or Maven `-Dflyway.outputType=json`),
opening the Command Output tab. **Refresh Status** polls bounded
stdout/stderr and completion (may need another press to drain after exit);
**Cancel Owned Command** kills only the stored child handle — no exit or
cancel state claims a database rollback. A successful completed JSON result
updates the owning project's rows, labelled cached with capture age and
provenance; failures and cancellation do not fabricate a successful state.
Observed config/SQL/source-parent changes retire cached rows; only current
project ownership and settings metadata are checked on automatic render (no
discovery, file read, CLI or HTTP). Accepted Info requires one
migrations-bearing JSON object with unique object keys; noisy Maven banners
are scanned within bounds, conflicting roots or duplicate keys do not become a
false result, and error objects are rejected.

Managed mode is finite, **instance-lifetime** ownership: shutdown or roughly
five-minute idle reclamation may terminate the child; status is explicitly
refreshed, not a durable stream. Keep terminal mode for work needing native
terminal lifetime or full raw output. Status/cancel controls refuse another
project's handle; a second start cannot silently replace a running child; a
failed project-list query is an unavailable ownership check, not evidence of
closure. The concrete review records the invoking project, approved runner,
option-token keys and bounded observed source paths/presence with every
argv/config value hidden: four conventional Flyway sources, project-local
`flyway.user.toml` overlays, up to 16 explicit owned `-configFiles=` (CLI) /
`-Dflyway.configFiles=` (Maven) paths and up to 32 discovered Spring
application configs (64 KiB each), rechecked after confirmation. The
effective database target is explicitly **UNKNOWN**; unresolved or
foreign-owned sources conservatively suppress managed output. Output buffers
complete lines per stream; connection/auth lines, URLs/JDBC strings,
controls/ANSI and oversized lines are suppressed, known credential/URL/user
literals are masked — conservative classification, not universal secret
recognition. Caps: 16 KiB per stream, 4 KiB lines, 1 MiB raw JSON (capped JSON
is refused), 8 KiB/128 tokens extra argv with whitespace tokenization and no
bare additional goals (use `-key=value`).

## Completion

- In SQL files, after `${`: Flyway's built-in placeholders
  (`flyway:defaultSchema`, `flyway:user`, `flyway:timestamp`…) and the
  `flyway.placeholders.*` names from the project's config, with literal
  `placeholderPrefix`/`placeholderSuffix` delimiters (including multi-character
  and comma delimiters); `placeholderReplacement=false` disables suggestions.
  This is editor assistance, not proof of effective CLI configuration.
- In `flyway.conf` (and `flyway-*.conf`): `flyway.*` keys, written as `key=`.
- In `flyway.toml`: keys under `[flyway]`, and connection keys under
  `[environments.<name>]`, written as `key = `.

## Source navigation

- Go to Definition links `${name}` in SQL to the exact placeholder-name token
  in one supported owned `flyway.conf`/`flyway.toml`, case-insensitively
  (Flyway's documented contract); values and credentials are never cached or
  displayed. One canonical root configuration, literal in-project filesystem
  or main-resource classpath locations and supported literal migration names
  are required; competing files, Spring overlays, custom delimiters and
  non-ASCII/escaped/dynamic names yield no result. Bounds: 128 KiB SQL/32 KiB
  config, 512 declarations/references per file, 128 SQL files, 4,096
  references per root, eight roots, at most 256 reverse locations; facts
  expire after 120 seconds; open/save or Check Migrations warms them again.
- Table names in admitted migration SQL link `FROM`, `JOIN`, `UPDATE`,
  `INSERT INTO`, `ALTER/DROP/TRUNCATE TABLE` and `REFERENCES` uses to cached
  `CREATE TABLE` declarations in the configured migration files
  (schema-qualified names match exactly; quoted identifiers with
  doubled-delimiter escapes and per-dialect case folding are supported, the
  dialect coming only from an explicit datasource or Flyway JDBC URL/driver).
  Declarations, comments and string literals don't link; malformed or
  oversized sources are withheld. For literal Jakarta/javax `@Entity` +
  `@Table(name=…, schema=…)` mappings (and literal `orm.xml`/`persistence.xml`
  mapping-file names), the entity class is additionally offered. Kotlin
  named-argument classes and same-file constants are covered under the Spring
  Boot gate. Not runtime introspection or a Rename graph.
- The former document-index graph (table/JPA interchangeability, automatic
  indexed Rename/usages) is deliberately withheld.

## Templates and icons

New menu: Flyway SQL Migration, Flyway Java Migration, Flyway Kotlin
Migration, Flyway Java Callback and Flyway SQL Callback. The class is named
after the file (`V2__Add_users`), so name the file first; take the version
from Flyway: Copy Next Migration Version. Java/Kotlin templates have no
`package` line. `V*__*.sql`, `U*__*.sql` and `R__*.sql` files get the Flyway
icon; placeholders get their own gutter marker.

## Offline comparison review

**Flyway: Review Local Comparison Report…** opens a native review pane for an
existing project-relative standalone Flyway `diffText` JSON report: select
change IDs and see each source and target object definition with its
Add/Edit/Delete label (a Delete's target `CREATE` is the old object's
definition, not a `DROP`). No SQL is inferred, combined, executed or written.
The parser accepts the documented `differences[]` shape (unique ids,
Add/Edit/Delete, literal definitions) bounded to 1 MiB/500 changes, 64 KiB per
SQL side, 32 ids/64 KiB selection; banners, logs, error payloads and malformed
input are rejected; nothing is truncated into a "complete" preview. Rendering
reads the imported snapshot only; the session is in-memory and project-scoped;
observed report edits retire selections; stale reports clear visibly.

A separate **Review CLI recipe…** records exact argv arrays after a bounded
confirmation, only for the explicitly reported **13.12.1 Enterprise** profile
(snapshot a schema model, compare against `empty` or a local snapshot, request
`diffText` with explicit change IDs; `flyway` program, explicit project working
directory, concrete distinct in-project paths, absent outputs). It is a review
aid, not approval to execute; version and edition are self-reported inputs,
not an executable probe, and an imported report is not proven to belong to a
future artifact's IDs. **Managed adapter readiness…** captures up to 64 owned
schema-model files (1 MiB total), an optional target snapshot and fresh output
paths and reviews candidate stage argv without executing; Start/Continue stay
unavailable until an authoritative installed offline CLI profile is accepted.
The managed core (one child, closed stdin, separated snapshot/diff/diffText
stages, explicit Check/Continue, bounded output, refused publication on
nonzero exit/stderr/oversized/stale output) exists behind that gate.

## Optional compiled Hibernate model adapter

**Flyway: Generate Reviewed Offline Hibernate Schema SQL…** captures creation
DDL from explicitly selected compiled annotated classes using checksum-pinned
Hibernate 6.6.49 or 7.4.15 runtimes and an explicit dialect choice (16
qualified presets each, including PostgreSQL, MySQL and MariaDB; the latest
profile accepts Jakarta Persistence 3.2). An exact classpath/source inspection
precedes the trusted-code approval: metadata can instantiate project
converters/custom types with arbitrary side effects; no application bootstrap
or JDBC operation is requested. The result stays a dated, instance-owned
preview until **Save Hibernate schema SQL as migration…** returns one exact
non-overwriting undoable Create; inputs are rechecked before saving (needs the
optional `process:java` permission). Live DB diff and automatic effective application models need separate support. See
[setup, contract and explicit runtime fixture](adapters/README.md).

## Not supported

Live database/model-derived DDL generation, executed schema diffs, and
connection to a change model for generated SQL. Custom placeholder prefix and
suffix in `arguments`, `src/test/resources` locations and quoted values with
spaces in `arguments` remain unsupported. Generation and rename revalidate
project ownership, source snapshot, naming and locations before returning
operations; destinations stay inside configured locations, cannot traverse
paths or contain unresolved wildcards; collision checks preserve the
module/vendor/category semantics with normalized version equality. The text
based configuration subset is used, not an effective database model.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
