# Liquibase Changelogs

Liquibase changelog support for XML, YAML, JSON and formatted SQL in OpenIDEA. The plugin reads the source text only. A file is treated as a changelog when it is an XML file with a `databaseChangeLog` root, a YAML or JSON file with a `databaseChangeLog` key, or a `.sql` file that starts with `--liquibase formatted sql` or contains a `--changeset` line. Other files, such as a `pom.xml`, `application.yml` or a plain SQL script, get no checks and no completion.

MIT; see `LICENSE`.

## Project source selection

Use **Choose browser sources…** in the Changesets toolbar, or **Liquibase: Choose Browser Root and Configuration…**, to choose a changelog root and an optional configuration source. The view then walks that root and its includes; **Browse all project changelogs** clears the choice. Choices persist in optional plugin storage under a concrete project-root identity; anonymous contexts cannot save or clear one. Cancellation saves nothing.

Configuration selection provides source provenance only. The status names the selected file, the supported declaration key/line and whether its literal path is a candidate for the chosen root. Supported keys are `changeLogFile`, `liquibase.command.changelogFile`, `liquibase.command.changeLogFile` and `spring.liquibase.change-log`, compared with project/main-resource/test-resource paths. Multiple declarations, placeholders and profile/runtime precedence stay unresolved. URLs, usernames, passwords and other connection values are not displayed or stored. The browser selection does not configure CLI commands or establish an effective database target.

Source metadata or declaration changes are shown as changed selections; the current source remains browsable. Missing, unreadable, oversized or no-longer-recognized selected files stop selected browsing with an explicit reselect/clear message. Corrupt stored records require explicit recovery; records carrying another project owner are not reused, overwritten or deleted. When storage is denied, ordinary project browsing remains available. Modification timestamps are second-resolution metadata: the status reports matching metadata or unverified freshness, not byte-level equality.

Bounds: root candidates use bounded XML/YAML/JSON/formatted-SQL parsing; configuration candidates are at most 64 project `*liquibase*.properties` and `application*.properties/yaml/yml` paths. Changelog discovery/reading is bounded to 2,000 source reads, 512 KiB per file and a three-second budget; tree traversal limits depth to 64, rows and include resolutions to 4,096. The chosen root follows explicit includes lazily; the broader catalog is discovered only for `includeAll`, which reports partial results and never silently replaces the chosen root. Symlink/unknown file kinds are unavailable. Saved source paths are project-relative and limited to 1,024 bytes; stored records are limited to 16 KiB. Active profiles, build/environment/property precedence, module dependency scopes, data-source association, runtime target resolution and CLI defaults-file binding remain unresolved.

## Changesets view

The bottom panel lists the project's changesets in execution order. Every changelog that no other changelog includes is a root node (an include cycle with no root is shown from its first file). A changelog node holds its changesets, labelled `id (author)` with its comment or first change type, and the changelogs it includes, nested where the `include` sits. `includeAll` expands to the changelogs under the directory, in path order (`maxDepth`, `filter` and `resourceComparator` are ignored). A changelog reached a second time is a leaf flagged `already run`; a missing include is flagged `missing`, an include with a `${property}` is flagged `unresolved`, and a file that includes one of its ancestors is flagged `include cycle` and not expanded again. Double-click a node to open the file at the changeset. Nothing is run: the view reads text only (up to 2,000 changelogs and 3 seconds; when either limit hits, the status line says `partial`).

The view uses the IDE's native tree, toolbar and status line. **Filter…** searches the captured row labels/state and retains include ancestors; **Clear** restores rows; filters belong to the current project. **More…** exposes the review and CLI workflows through a native picker, preserving their source checks and database-write confirmations. Double-click or Enter opens the owned source location.

## Checks

Live diagnostics (API 1.3, versioned) inspect the current unsaved XML/YAML/JSON/formatted-SQL source, and hover shows source facts. Neither performs file reads, discovery, configuration inspection, CLI, database or network operations. Explicit foreign XML namespaces are quiet. These are tolerant source checks, not full schema or Liquibase execution validation.

Automatic local rules:

| Code | Severity | Meaning |
|---|---|---|
| `duplicate-changeset` | error | A changeset with the same id and author is already defined in the file. The first definition is linked. Ids compare as text, so `id: 1` and `id: "1"` are the same. |
| `incomplete-changeset` | error | A changeset without an id, without an author, or without both. In SQL the message shows `--changeset author:id`. |
| `unknown-change-type` | warning | A YAML or JSON entry under `changes` that isn't a built-in Liquibase change type (`createTabel`). Reported in YAML and JSON only. |
| `missing-header` | warning | A formatted SQL changelog whose first non-blank line isn't `--liquibase formatted sql`. |

A quoted SQL author with spaces (`--changeset "Jane Doe":2`) is one author. Unresolved-include and unresolved-property diagnostics are no longer automatic; the historical project-wide rules and their quiet conditions are kept below for reference, since their discovery algorithms only run as explicit regression fixtures now.

Includes were resolved as the changelog's directory (with `relativeToChangelogFile`), or else against `src/main/resources` or `src/test/resources` (and, from a test changelog, `src/main/resources` too) when the changelog sits under one, then the project root; `classpath:` is stripped first. An include was not flagged when its path contains a `${property}`, leaves the project root, is empty (half-typed), when an `includeAll` directory exists but is empty, when it sets `errorIfMissing: false` (`errorIfMissingOrEmpty: false`), or when the file exists but can't be read for any reason other than being missing. The historical `unresolved-property` check was quiet when no config file could be read, when a scan cap was hit (1,000 listed files, 200 changelogs, 256 KB per file, 20 config files), when any changelog used `includeAll`, an unresolvable include or `property file=`, when the configuration used `defaultsFile` or `spring.config.import`/`location`/`activate`, for escaped (`$${x}`, `\${x}`), numeric or dynamic names, and inside comments; `database.*` names are built in. Parameters passed with `-D` or the environment are invisible: turn the check off with `checkProperties` if you rely on them.

Included-file duplicate changesets, property precedence/contexts, extension schemas, configuration/JVM/environment parameters and database history remain unresolved; the `checkProperties` setting does not authorize new scanning.

## Hover

Hover shows source changeset ID/author metadata, literal include options and, when unambiguous, property declaration provenance through cached relative includes. It never displays property values. Include targets are positive cached source facts only for literal relative file paths; classpath/absolute/dynamic paths, `includeAll`, runtime precedence and missing targets remain unestablished. Cached gutter navigation uses the same conservative resolver; cold or expired targets have no cached gutter link — open the target or browse/refresh the Changesets view to establish source facts.

The cache belongs to exact verified project roots and full paths, is warmed only from supplied documents, optional open/save events or already-read browser sources, and expires positive facts after 120 seconds. Files-changed events invalidate facts; current text replaces or removes a document's facts. Limits: 256 KiB/document, 4,096 parsed records/document, 1,024 placeholder markers and 8,192 XML tag markers, eight roots, 128 files and 4 MiB source-size accounting per root, 32 files / 128 inspected include edges per property-provenance traversal, the first 256 include candidates for markers and 256 diagnostic results; browser warming is capped at 128 files / 4 MiB / 30 ms between documents. Exhausting the property traversal edge budget withholds the property hover. Cached paths are accepted only while the host innermost project owner still matches the exact cache root; a separately opened nested project cannot lend its files to an outer root. Caches are partial; they never prove an include or property absent. Denied events and project access leave current-source checks available without cross-project fallback.

## Completion

In YAML and JSON changelogs, keys are offered by where the caret is:

| Under | Offers |
|---|---|
| `databaseChangeLog:` list item | `changeSet`, `include`, `includeAll`, `property` and the other top-level keys |
| `changeSet:` | `id`, `author`, `changes`, `rollback`, `runOnChange`, … |
| `changes:` and `rollback:` | the built-in change types |
| a change type | that change type's attributes |
| `columns:` / `column:` / `constraints:` | the column and constraint attributes |
| `include:` / `includeAll:` | their attributes |

`changeSet` inserts a skeleton with id, author and `changes`. A key already followed by a colon is replaced without adding a second one. JSON completion replaces the current quoted property name, including in an unfinished document; existing values are preserved and new keys get a value placeholder. Core change attributes include sequence, procedure, lookup, merge, constraint-removal, remarks and CSV-update controls checked against the public latest XSD.

In formatted SQL, typing `--` offers `--liquibase formatted sql` on the first line and, in a changelog, `--changeset author:id`, `--rollback` and `--comment`. After a literal `--changeset author:id `, attribute names are completed with `key:value` syntax, including execution, context, label and delimiter controls; existing attributes are omitted and existing values preserved. Completion describes syntax; it does not enable licensed executors or validate database-specific SQL.

In XML changelogs the plugin completes, inside a `<changeSet>` only, the elements after `<` (change types, `comment`, `rollback`, `column`, `constraints`) and the attribute names of `changeSet`, `include`, `includeAll`, `column`, `constraints` and the change types. It stays quiet when the root element declares a `schemaLocation`, because the XML language server completes those files.

In every format, the value of an `include` file is completed with the project's changelog files (`.xml`, `.yaml`, `.yml`, `.json`, `.sql`), written relative to the changelog with `relativeToChangelogFile` and otherwise relative to `src/main/resources` or the project root; the list comes from the host's file search, which skips hidden directories and ignored paths. An `author` value (XML attribute, YAML key, `--changeset ` line before the colon) is completed with the `author` setting or, when it is empty, with `git config user.name` (needs the optional `process:git` permission; nothing is offered without it).

## Navigation

Go to Definition on the value of an `include` file (XML, YAML or JSON) opens the included changelog. A gutter link on the `include` line opens a fresh, positively cached literal relative target; cold, dynamic and classpath candidates remain available through direct navigation. `includeAll` directories have no single target. Include-file navigation validates the full current XML document (including legacy unnamespaced core changelogs) before scanner-backed navigation; malformed declarations/attributes/namespaces and opaque `xml:base` or `logicalFilePath` owners stay unresolved.

Definition requests can also open a unique literal property declaration in the current namespaced XML, validated YAML/JSON, or formatted SQL changelog. XML/YAML/JSON rollback references with explicit literal `changeSetId` and `changeSetAuthor` navigate to a unique same-file tuple only when there are no includes, opaque logical paths or unknown identities; explicit `changeSetPath`, duplicate names, dynamic values and ambiguous scanner paths stay quiet. These identify source declarations; conditional property precedence, inherited values and runtime usage completeness are unknown.

Go to Definition also opens literal `sqlFile.path`, `loadData.file` and `loadUpdateData.file` values on direct core XML changeset children or YAML/JSON changes array entries when `relativeToChangelogFile="true"`. The current unsaved source must be structurally valid, namespace supported and free of `xml:base`/`logicalFilePath` ambiguity; YAML/JSON requires a literal Boolean true and uniquely located unescaped scalar spellings (aliases, tags and duplicate mapping keys are refused). Resolution uses the physical changelog directory, requires an existing file in the same innermost project, and refuses project escapes, denied reads, substitutions, encoded aliases and URI/search-path interpretation. No SQL/CSV contents are read or executed. Rollback-nested resource changes are outside this.

No replaceable index anchors are published: a file-local table name cannot prove a closed set of references, so automatic table Rename and Find Usages are withheld and the index export returns an empty graph (existing persisted symbols disappear when the host reindexes).

## Templates

New › Liquibase Changelog (XML), (YAML) and (Formatted SQL). Files named `db.changelog-*` and `liquibase.properties` get the Liquibase icon.

## Generators

Generation is performed through the editor's code actions (⌥⏎); **Liquibase: New Changeset** and **Liquibase: New Changelog…** are command entry points that explain which intention to choose. The SDK applies FileActions through editor intentions; there is no command-side apply call.

- **Add Liquibase changeset…** accepts an id and editable author hint, then shows the exact file, editor version, UTF-16 range and insertion/replacement before returning one undoable workspace edit. XML supports the standard `http://www.liquibase.org/xml/ns/dbchangelog` namespace, including prefixed and self-closing roots. Formatted SQL requires its header and safely representable author/id values. Existing incomplete/dynamic identities and duplicate id/author pairs in the current file are refused. YAML/JSON changelog arrays support source-preserving appends: strict parsing rejects duplicate keys, aliases, tags, multiple documents and depth/event limits before mutation; block/flow sequences and Unicode/CRLF are preserved; the completed result is reparsed before returning an edit; existing source bytes/comments are not rewritten.
- **Create Liquibase changelog…** chooses XML, YAML, formatted SQL or JSON, a concrete project path and initial author, reusing the bundled templates with escaped literal values. From a validated XML/YAML/JSON parent, choose standalone or linked creation; linked creation previews the exact new contents and parent include edit, then returns one transaction containing a non-overwriting Create plus the parent edit — failure applies neither half. New files open after application. From other editor formats, creation is standalone.
- **Include existing changelog…** previews one parent edit for an existing validated XML/YAML/JSON leaf changelog beside or below the parent; it refuses a child containing includes or opaque ownership rather than claiming to resolve an arbitrary include graph or cycle.

Each action revalidates the owning project and complete current source snapshot before and after prompts. Destinations cannot escape the project or overwrite files/directories/open target buffers. Duplicate includes, `includeAll`, unresolved include paths, `xml:base` and `logicalFilePath` ambiguity block linked editing. Namespace-less, foreign-namespace, malformed and DTD-based XML is not mutated; namespace admission checks qualified names (XML 1.0 NCName ranges), reserved XML/XMLNS bindings and duplicate expanded attribute names on every element/attribute; prefixed namespace undeclarations and declarations misplaced after the root are outside the supported XML 1.0 mutation subset and withhold edits. Source editing is bounded to 256 KiB, 50,000 XML events and depth 64. Resolution only uses supported pick/input/confirm prompts and read APIs; it never launches Git, reveals a view or writes a file while resolving; cancelling leaves all files unchanged, and the host owns atomic application and undo. This is namespace/well-formedness and local identity validation, not full XSD, effective root configuration or database-history validation. Generated changesets are empty skeletons; fill and review them before execution.

## Run configurations

Three dynamic API 1.4 configurations appear for an owned open project with a root `liquibase.properties`: `status --verbose`, `validate` and `update-sql`. They use the project root as cwd and native Run Stop/Rerun. Database-writing update/rollback remain freshly reviewed terminal commands, outside native Rerun. The CLI resolves the effective target and credentials. These runs need the `liquibase` CLI on PATH, the `process:liquibase` permission (granted in Settings › Plugins), and `changeLogFile`, `url` and the credentials in `liquibase.properties`. Set `searchPath` (the CLI's `--search-path`) to `src/main/resources` so changesets keep the file paths your application records.

**Liquibase: Update (Asks First)…** and **Liquibase: Rollback Count…** keep their cancellation/count/final-confirmation semantics and exact terminal argv. The final review shows the captured owning project/working directory, program, exact argv and root `liquibase.properties` project/editor-text SHA256. A unique literal changelog and recognized PostgreSQL/MySQL/MariaDB JDBC host/database are source hints only; query/semicolon parameters and credential-bearing/ambiguous/dynamic values are withheld, and raw configuration, usernames and passwords are never displayed. The effective database, contexts and labels remain unresolved: environment, JVM/defaults/extension precedence and unsaved-versus-disk differences can override source hints. Missing project scope or configuration drift across the prompts refuses startup; root configuration text is bounded to 128 KiB.

## Offline snapshot diff

**Liquibase: Review Offline Snapshot Diff** opens a cached native report view; **Run Reviewed Offline Snapshot Diff** explicitly selects reference and target project JSON snapshots plus tables, columns, indexes, views, or all four. Snapshots must have the same supported shortName (h2/postgresql/mysql/mariadb), recognized core object-map/array structure, safe project-relative paths and at most 128 KiB per file / 4,096 objects. JSON decoding rejects duplicate decoded object keys, depth over 64, over 50,000 values and trailing data before any staging or process work. Discovery is bounded to 128 JSON candidates / two seconds; unknown snapshots are not inferred absent. No online URL, username, password, datasource selection, JPA model or live snapshot command is offered.

The argument/report contract is the Liquibase v4.29.2 text diff; the installed CLI binary, version and edition remain unverified. Before any process starts, an exact confirmation shows project, source paths and editor-text SHA256 identifiers, program and argv. Project reads may contain unsaved buffers: the exact reviewed current text is copied into owner-only plugin-private staging, serialized as UTF-8 JSON with database connection identity fields removed, generated empty private defaults supplied, and UTF-8 output encoding/private search path selected; selected project files are unchanged. Command-specific environment URL/diff-type variables (and, for generation, output/author variables) are reserved to the reviewed values; a collision with an inherited key is rejected before spawning. Source/root/staged-byte membership is revalidated before start, during Poll Status and before accepting completed output.

The approved process inherits environment/JVM/extensions: process permission is not a database/network/filesystem sandbox, and ambient settings can still affect behavior including output destinations. Permission denial or cancellation never falls back to an unscoped terminal command. One instance-owned Child runs at a time with stdin closed; **Poll Status** drains at most eight bounded chunks per click; **Stop Process** kills only the owning project's child; a 120-second polling lease and idle host reclamation can also end it. No automatic polling, durable job or real-time tailing. 64 KiB combined stdout/stderr, 512 observations / 1,024-byte labels / 4,096-byte detail limits reject oversize output and retain the last good report; raw stderr is withheld; known snapshot connection strings are redacted.

The text adapter accepts only requested Missing/Unexpected/Changed category headers, exact known object/detail indentation and bounded report shapes; unknown, duplicate or missing categories retain the old report and yield no absence conclusion. Select a reported observation for its read-only detail, or **Choose Observations to Preview…** to pick multiple rows and inspect their combined read-only text (128 KiB maximum). Picker IDs bind the exact owning report/root/content and the accepting generation; cancel, forged, duplicate, stale keys or foreign roots preserve the prior review. The combined preview is observed report text, not SQL, DDL, a changelog or deployable changes, and no object spelling is converted into inferred DDL. Rendering/selection never runs or polls Liquibase. Even zero observed rows is not proof of a complete match.

Private staged inputs are overwritten empty on normal completion, Stop and observed failure only while their bytes still match this invocation's staged text; changed/unreadable slots are retained with a cleanup warning. This is not an atomic file-identity guarantee: crash/reclaim may leave owner-only copies until manual cleanup/uninstall. There is no SDK delete: at most 96 staging files (32 distinct input sets) are permitted. No snapshot/report content is saved to plugin settings or user files.

## Generated offline changelogs

**Generate Reviewed Offline Changelog…** reuses the owning project's two supported snapshot choices and category selection, asks for a literal author, then previews the exact `diff-changelog` argv, input text SHA256 identifiers and a new private XML output path. The command contract is Liquibase 4.29.2; installed executable, version and edition remain unverified. It requests offline source generation, not update/rollback or a project output path; a private output that already exists is refused. The single instance-owned Child, closed stdin, source/staging checks, explicit Poll/Stop, output/time limits and foreign-project refusal apply as above.

Accepted output must be UTF-8 core Liquibase XML, at most 128 KiB / 512 unique literal changesets / 8,192 elements, with known 4.29 change-type children, the requested author and no opaque `logicalFilePath`, external-file/include/property dependencies, DTDs, custom execution or foreign elements. Missing/empty/invalid/oversized output retains the previous candidate and never proves no differences. The stock serializer emits an XML 1.1 declaration: the generated-source reader accepts its XML 1.0-compatible subset while preserving exact source bytes and rejecting XML 1.1-only line/control semantics.

**Choose Generated Changesets…** selects complete source fragments in their original order, retaining root namespace declarations, comments and untouched bytes; the exact selected XML and snapshot provenance are shown. Dependencies across omitted changesets, SQL correctness, runtime parameters, full schema coverage and output transport completeness are not inferred. The candidate is instance memory only: reload/reclamation discards it until another generation; a changed snapshot/selection/owner makes an old save action stale.

**Organize Migrations…** manages the generated-source review: create/switch/remove migration groups, move complete changesets with the selection picker, ignore changesets and restore ignored ones. A changeset belongs to at most one group; each group preserves original source order; operation-class selection separates add/create, drop/delete and other/SQL based on the actual serialized change names (lexical classes, not a risk score or dependency analysis).

To save, open a project XML/YAML/JSON/SQL editor and choose **Save reviewed offline changesets…** from code actions. Select a destination and review the complete contents; the resolver rechecks the exact cached selection, project, source snapshots, current editor and destination before and after prompts, returning one non-overwriting, undoable FileAction Create (never a direct write, and CLI never runs on selection/save). Oversized 16 KiB confirmations are refused — choose a smaller subset. When multiple groups exist, **Save grouped offline migrations…** prompts for distinct XML paths, previews every exact output and returns all non-overwriting Creates in one atomic workspace action; group names/selection/source changes invalidate old save actions; ignored changesets are excluded and ordering/omitted dependencies must be reviewed before execution. Grouping itself neither starts a CLI nor writes source files. Generated schema/default text can be sensitive and is not redacted, preserving the actual XML for review.

## Limits

- XML files with a `schemaLocation` get completion from lemminx and the changelog's XSD, not from this plugin.
- JSON changelogs get the checks, the changesets view, Go to Definition and the project index, but no completion and no run icon on the gutter marker.
- The gutter marker only navigates; the run icon is not used.
- Includes resolve against `src/main/resources`, `src/test/resources` or the project root. Other classpath layouts aren't modelled.
- Only open files are checked.
- After you create a missing included file, the master's "Cannot find" stays until the master is edited or reopened.
- Unknown change types are reported in YAML and JSON only, under `changes`.
- Run configurations take fixed arguments, so `rollback-count` and per-changelog runs aren't offered.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
