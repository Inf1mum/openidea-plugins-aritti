# Liquibase Changelogs

Liquibase changelog support for XML, YAML, JSON and formatted SQL in OpenIDEA. The
plugin reads the source text only. A file is treated as a changelog when it is
an XML file with a `databaseChangeLog` root, a YAML or JSON file with a
`databaseChangeLog` key, or a `.sql` file that starts with `--liquibase
formatted sql` or contains a `--changeset` line. Other files, such as a
`pom.xml`, `application.yml` or a plain SQL script, get no checks and no
completion.

Change type and attribute names come from the Liquibase 4.29 XSD (Apache-2.0).
This is an independent implementation, not affiliated with Liquibase Inc. or
JetBrains.

## Project source selection

Use **Choose browser sources…** in the Changesets toolbar, or **Liquibase: Choose Browser Root and Configuration…**, to choose a changelog root and an optional configuration source. The view then walks that root and its includes. **Browse all project changelogs** clears the choice. Choices persist in optional plugin storage under a concrete project-root identity; anonymous contexts cannot save or clear one. Cancellation saves nothing. A detected storage change during the prompts returns a stale result; storage has no atomic compare-and-set, so a final read/write gap remains. Old navigation rows cannot open files under another project's context.

Configuration selection provides source provenance only. The status names the selected file, supported declaration key/line and whether its literal path is a candidate for the chosen root. Supported keys are `changeLogFile`, `liquibase.command.changelogFile`, `liquibase.command.changeLogFile` and `spring.liquibase.change-log`, compared with project/main-resource/test-resource paths. Multiple declarations, placeholders and profile/runtime precedence stay unresolved. URLs, usernames, passwords and other connection values are not displayed or stored. Persistence contains source paths, owner identity, file size/modification metadata and the bounded provenance summary.

Source metadata or declaration changes are shown as changed selections; the current source remains browsable. Missing, unreadable, oversized or no-longer-recognized selected files stop selected browsing with an explicit reselect/clear message. Corrupt stored records require explicit recovery; records carrying another project owner are not reused, overwritten or deleted. When storage is denied, ordinary project browsing remains available. Modification timestamps are second-resolution metadata and may be unavailable: the status reports matching metadata or unverified freshness, not byte-level equality. The browser selection does not configure CLI commands or establish an effective database target. Existing update/rollback confirmations and terminal arguments remain in place.

Saved source paths are project-relative and limited to 1,024 bytes; stored records are limited to 16 KiB.

P12.1 is partial. Root candidates use bounded existing XML/YAML/JSON/formatted-SQL parsing; configuration candidates are at most 64 project `*liquibase*.properties` and `application*.properties/yaml/yml` paths. Changelog discovery/reading is bounded to 2,000 source reads, 512 KiB per file and a three-second budget checked between host calls; tree traversal limits depth to 64, rows and include resolutions to 4,096. The chosen root follows explicit includes lazily; the broader catalog is discovered only for `includeAll`. Partial discovery cannot silently replace the chosen root. Symlink/unknown file kinds remain unavailable. `includeAll` still uses the bounded discovered catalog and reports partial results.

The local original audit inspected `LiquibaseProjectConfigs`, `LiquibasePropertiesConfigsProvider.findLiquibasePropertiesFile` and `LiquibaseProjectConfigsUtils.readLiquibaseProperties`: the original associates module-scoped properties/profile/root settings with IDE data sources and reads connection/schema settings. This implementation supplies an explicit project-source chooser with provenance. Active profiles, build/environment/property precedence, module dependency scopes, data-source association, runtime target resolution and CLI defaults-file binding remain separate work. Native atomic FileActions are preserved; the later P12.2 audit below withholds replaceable index anchors. The new chooser/persistence UI has native logic tests; desktop acceptance remains unverified.

## Liquibase Changesets view

The bottom panel lists the project's changesets in execution order. Every
changelog that no other changelog includes is a root node (an include cycle with
no root is shown from its first file). A changelog node holds its changesets,
labelled `id (author)` with its comment or first change type, and the changelogs
it includes, nested where the `include` sits. `includeAll` expands to the
changelogs under the directory, in path order (`maxDepth`, `filter` and
`resourceComparator` are ignored). A changelog reached a second time is a leaf
flagged `already run`. Double-click a node to open the
file at the changeset. A missing include is a node flagged `missing`, an include
with a `${property}` is flagged `unresolved`, and a file that includes one of its
ancestors is flagged `include cycle` and not expanded again. Nothing is run:
the view reads the text of XML, YAML and formatted SQL changelogs (up to 2000
changelogs and 3 seconds; when either limit hits, the status line says
`partial`).

## Live diagnostics and source hover (partial P12.2)

API 1.3 versioned diagnostics inspect current unsaved XML/YAML/JSON/formatted-SQL
source. Live rules cover local duplicate/incomplete changesets, unknown YAML change
types and missing formatted-SQL headers. Gutter requests no longer manually publish
diagnostics. Neither live diagnostics nor hover performs file reads, discovery,
configuration inspection, CLI, database or network operations. Explicit foreign
XML namespaces are quiet. These are tolerant source checks, not full schema or
Liquibase execution validation; external language services keep syntax ownership.

Hover shows source changeset ID/author metadata, literal include options and,
when unambiguous, property declaration provenance through cached relative includes.
It never displays property values. Include targets are positive cached source facts
only for literal relative file paths; classpath/absolute/dynamic paths, includeAll,
runtime precedence and missing targets remain unestablished. Cached gutter navigation
uses the same conservative resolver. Direct Go to Definition remains separately
available. Cold or expired targets have no cached gutter link: open the target or
browse/refresh the Changesets view to establish source facts.

The cache belongs to exact verified project roots and full paths, is warmed only
from supplied documents, optional open/save events or already-read browser sources,
and expires positive facts after 120 seconds. Files-changed events invalidate facts;
current text replaces or removes a document's facts. Limits: 256 KiB/document,
4,096 parsed records/document, 1,024 placeholder markers and 8,192 XML tag markers, eight roots, 128 files and 4 MiB source-size accounting
per root, 32 files /128 inspected include edges per property-provenance traversal,
and the first256 include candidates for markers /256 diagnostic results. Exhausting
the property traversal edge budget withholds the property hover.
Browser warming is additionally capped at 128 files/4 MiB/30 ms between documents.
Cached paths are accepted and consumed only while the host innermost project owner
still matches the exact cache root. A separately opened nested project cannot lend
its files to an outer root, including after cache warming. Invalid/denied ownership
is quiet; these identity checks do not read/discover files.
Caches are partial; they never prove an include/property absent. Denied events and
project access leave current-source checks available without cross-project fallback.

**Behavior changes:** unresolved-include and unresolved-property diagnostics are no
longer automatic: their old project/configuration discovery algorithms remain only
explicit full-context regression fixtures. Included-file duplicate changesets,
property precedence/contexts, extension schemas, configuration/JVM/environment
parameters and database history remain unresolved. Unknown/ambiguous property hover
is quiet. The existing `checkProperties` setting does not authorize new scanning.

**Automatic table Rename is withheld.** A file-local `createTable` spelling does
not prove all references: other changelogs, included SQL, schemas/catalogs, dynamic
names and database objects can share it. The index export returns an empty graph to
remove previously published replaceable anchors upon reindexing; include definitions and dedicated
navigation remain. No synthetic file/changeset or guessed table anchors are added.
Historical range/token fixtures are retained separately from actual exported output.

Original comparison: the decompiled descriptor registers
`DuplicateChangesetIdAndAuthorInspection` (recursive included files) and
`LiquibaseXmlUnresolvedPropertyInspection`; `LiquibasePropertiesDocumentationProvider`
uses a service-backed property catalog. Our subset provides local checks and source
provenance, not equivalent catalog/schema/runtime resolution. Reviewed Marketplace
screenshot `23741-0.png` shows root changelog, database connection, contexts and labels
in the Update dialog; this work preserves source selection and atomic generator
flows and adds no database connection or execution claims.

## Historical full-resolution check inventory

The following inventory includes legacy full-context fixtures. Only the local rules
listed above run automatically; include/property absence checks below are historical.

Changelogs are checked as you type, without saving.

| Code | Severity | Meaning |
|---|---|---|
| `duplicate-changeset` | error | A changeset with the same id and author is already defined in the file. The first definition is linked. Ids compare as text, so `id: 1` and `id: "1"` are the same. |
| `incomplete-changeset` | error | A changeset without an id, without an author, or without both. In SQL the message shows `--changeset author:id`. |
| `unknown-change-type` | warning | A YAML or JSON entry under `changes` that isn't a built-in Liquibase change type (`createTabel`). |
| `unresolved-include` | error | An `include` file or an `includeAll` directory that can't be found. Not reported when `errorIfMissing` (`errorIfMissingOrEmpty` for `includeAll`) is `false`. |
| `unresolved-property` | warning | A `${name}` placeholder that no changelog of the project declares (properties are global, so the master and sibling changelogs count) and the project's `liquibase.properties` / `application*.properties|yaml|yml` (profile and module files included; `liquibase.parameters.<name>`, `spring.liquibase.parameters.<name>`) do not set. `database.*` names are built in. Quiet when none of those config files can be read, when a scan cap is hit (1000 listed files, 200 changelogs, 256 KB per file, 20 config files), when any changelog uses `includeAll`, an include that cannot be resolved or `property file=`, when the configuration uses `defaultsFile` or `spring.config.import`/`location`/`activate`, for escaped (`$${x}`, `\${x}`), numeric or dynamic names, and inside comments. Quiet when a `pom.xml` or `build.gradle(.kts)` above the changelog mentions `expressionVariables`, `expressionVars`, `changelogParameters`, `changeLogParameters`, `liquibaseProperties` or `propertyFile`. Parameters passed with `-D` or the environment are invisible: turn the check off with `checkProperties` if you rely on them. |
| `missing-header` | warning | A formatted SQL changelog whose first non-blank line isn't `--liquibase formatted sql`. |

A quoted SQL author with spaces (`--changeset "Jane Doe":2`) is one author.

Includes are resolved as the changelog's directory (with
`relativeToChangelogFile`), or else against `src/main/resources` or
`src/test/resources` (and, from a test changelog, `src/main/resources` too) when the
changelog sits under one, then the project root.
`classpath:` is stripped first. An include is not flagged when:

- its path contains a `${property}`;
- its path leaves the project root;
- its path is empty, because it is half-typed;
- an `includeAll` directory exists but is empty (only a missing directory is reported);
- it sets `errorIfMissing: false` (`errorIfMissingOrEmpty: false`);
- the file exists but can't be read for any reason other than being missing.

## Completion

In YAML changelogs, keys are offered by where the caret is:

| Under | Offers |
|---|---|
| `databaseChangeLog:` list item | `changeSet`, `include`, `includeAll`, `property` and the other top-level keys |
| `changeSet:` | `id`, `author`, `changes`, `rollback`, `runOnChange`, … |
| `changes:` and `rollback:` | the built-in change types |
| a change type | that change type's attributes |
| `columns:` / `column:` / `constraints:` | the column and constraint attributes |
| `include:` / `includeAll:` | their attributes |

`changeSet` inserts a skeleton with id, author and `changes`. A key that is
already followed by a colon is replaced without adding a second one.

In formatted SQL, typing `--` offers `--liquibase formatted sql` on the first
line and, in a changelog, `--changeset author:id`, `--rollback` and
`--comment`.

In XML changelogs the plugin completes, inside a `<changeSet>` only, the
elements after `<` (change types, `comment`, `rollback`, `column`,
`constraints`) and the attribute names of `changeSet`, `include`, `includeAll`,
`column`, `constraints` and the change types. It stays quiet when the root
element declares a `schemaLocation`, because the XML language server completes
those files.

In every format, the value of an `include` file is completed with the project's
changelog files (`.xml`, `.yaml`, `.yml`, `.json`, `.sql`), written relative to
the changelog with `relativeToChangelogFile` and otherwise relative to
`src/main/resources` or the project root. The list comes from the host's file
search, which skips hidden directories and ignored paths. An `author` value
(XML attribute, YAML key, `--changeset ` line before the colon) is completed
with the `author` setting or, when it is empty, with `git config user.name`
(needs the optional `process:git` permission; nothing is offered without it).

## Definition

Go to Definition on the value of an `include` file (XML, YAML or JSON) opens the
included changelog. A gutter link on the `include` line opens a fresh, positively cached literal relative target;
cold, dynamic and classpath candidates remain available through direct navigation. `includeAll` directories have no single target.

## Templates

New › Liquibase Changelog (XML), (YAML) and (Formatted SQL). Files named
`db.changelog-*` and `liquibase.properties` get the Liquibase icon.

## Generators

Generation is performed through the editor's code actions (⌥⏎). **Liquibase: New
Changeset** and **Liquibase: New Changelog…** are command entry points that explain
which intention to choose. The SDK applies FileActions through editor intentions;
there is no command-side apply call. Commands no longer write through `tee` or
perform a separate, non-atomic parent edit.

- **Add Liquibase changeset…** accepts an id and editable author hint, then shows
  the exact file, editor version, UTF-16 range and insertion/replacement. Confirm
  before returning one undoable workspace edit. XML supports the standard
  `http://www.liquibase.org/xml/ns/dbchangelog` namespace, including prefixed and
  self-closing roots. Formatted SQL requires its header and safely representable
  author/id values. Existing incomplete/dynamic identities and duplicate
  id/author pairs in the current file are refused. YAML/JSON changelog arrays now
  support source-preserving appends: strict parsing rejects duplicate keys,
  aliases, tags, multiple documents and depth/event limits before mutation.
  Block/flow sequences and Unicode/CRLF are preserved; the completed result is
  reparsed before returning an edit. Existing source bytes/comments are not rewritten.
- **Create Liquibase changelog…** chooses XML, YAML, formatted SQL or JSON, a
  concrete project path and initial author. The bundled templates are reused
  with escaped literal values. From a validated XML/YAML/JSON parent, choose standalone or
  linked creation. Linked creation previews the exact new contents and parent
  include edit, then returns **one** transaction containing non-overwriting
  Create plus the parent edit. Failure applies neither half. New files open
  after application. From other editor formats, creation is standalone.
- **Include existing changelog…** previews one XML/YAML/JSON parent edit for an existing
  validated XML/YAML/JSON leaf changelog beside or below the parent. It refuses a child
  containing includes or opaque ownership, rather than claiming to resolve an
  arbitrary include graph or cycle.

Each action revalidates the owning project and complete current source snapshot
before and after prompts. Destinations cannot escape the project or overwrite
files/directories/open target buffers. Duplicate includes, includeAll, unresolved
include paths, xml:base and logicalFilePath ambiguity block linked editing.
Namespace-less, foreign-namespace, malformed and DTD-based XML is not mutated.
Namespace admission also checks qualified names and explicit XML/XMLNS reserved
bindings on every element/attribute. Prefixed namespace undeclarations are outside
the supported XML1.0 mutation subset and withhold changeset/include edits; legal
predefined xml attributes and coherent Liquibase prefixes remain supported.
Source editing is bounded to 256 KiB, 50,000 XML events and depth 64. This is
namespace/well-formedness and local identity validation, not full XSD, effective
root configuration or database-history validation.

Resolution only uses supported pick/input/confirm prompts and read APIs. The
configured author or an already-cached author can be offered as a hint; it never
launches Git, reveals a view or writes a file while resolving. Cancelling leaves
all files unchanged; other preparation failures produce a notification. The
host owns atomic application and undo. No `tee`, editor-write or project-write
permission is needed for these user-initiated FileActions.

P12.3 is partial: the atomic source subset above is implemented. YAML/JSON parent
editing, model/database-derived changes, complete changeset execution identity
across logical paths/includes and Update/Rollback target previews remain separate
work. Existing Update/Rollback confirmation behavior is preserved. Generated
changesets are empty skeletons; fill and review them before execution.

The original ChangeLogPanel/ChangeSetPanel receipts show structured fields and a
generated-change preview, with partial decompilation/bytecode fallback. These
source workflows do not reproduce its entity/database generator. Native mock
operation contracts and package checks are separate from desktop apply/undo,
dock-width/theme checks and original screenshot comparison; those remain
unverified for this slice.

## Run configurations

Three dynamic API 1.4 configurations appear for an owned open project with a
root `liquibase.properties`: `status --verbose`, `validate` and `update-sql`.
They use the project root as cwd and native Run Stop/Rerun. Database-writing
update/rollback remain freshly reviewed terminal commands, outside native Rerun.
The CLI resolves the effective target and credentials. These runs need:

- the `liquibase` CLI on PATH;
- the `process:liquibase` permission, granted in Settings › Plugins;
- `changeLogFile`, `url` and the credentials in `liquibase.properties`.

Set `searchPath` (the CLI's `--search-path`) in `liquibase.properties` to
`src/main/resources`, so changesets keep the file paths your application
records, rather than paths relative to wherever the CLI runs.

## Limits

- XML files with a `schemaLocation` get completion from lemminx and the
  changelog's XSD, not from this plugin.
- JSON changelogs get the checks, the changesets view, Go to Definition and the
  project index, but no completion and no run icon on the gutter marker.
- The gutter marker only navigates; the run icon is not used.
- Includes resolve against `src/main/resources`, `src/test/resources` or the
  project root. Other classpath layouts aren't modelled.
- Only open files are checked.
- After you create a missing included file, the master's "Cannot find" stays until the master is edited or reopened.
- Unknown change types are reported in YAML and JSON only, under `changes`.
- Run configurations take fixed arguments, so `rollback-count` and per-changelog
  runs aren't offered.

```sh
cargo test                       # logic, against the SDK's mock host
openidea-plugin build            # → dist/plugin.wasm, validated like an install
openidea-plugin link             # load this folder in the IDE (Settings › Plugins)
```

### Native navigation anchor audit

The v0.1.39 host rejects zero-width anchors. Earlier synthetic file definitions
caused native index warnings even for valid generated XML; those remain suppressed.
The P12.2 audit additionally withholds all automatic table definitions/references:
exact ASCII tokens and file-owned keys satisfy range validation but cannot prove a
closed set of references to a database table. Cross-file SQL, included changelogs,
schemas/catalogs and runtime mappings remain unknown. The registered index export
returns an empty graph; existing persisted symbols disappear when the host reindexes.

Include Go to Definition remains a dedicated provider. Cached gutter navigation
uses the bounded source resolver described above. Source checks and hover do not
publish any replaceable symbol. Historical lexical UTF-16/range/budget fixtures are
kept under test-only code, separately from an actual-export regression that requires
empty definitions/references/aliases. Atomic changelog creation and native-tested
source selection are unaffected; this slice's desktop acceptance is still separate.

## P12.4 partial: reviewed offline snapshot diff observations

**Liquibase: Review Offline Snapshot Diff** opens a cached native report view.
**Run Reviewed Offline Snapshot Diff** explicitly selects reference and target
project JSON snapshots plus tables, columns, indexes, views, or all four. Snapshots
must have the same supported shortName (h2/postgresql/mysql/mariadb), recognized
core object-map/array structure, safe project-relative paths and at most128KiB
per file/4096 objects. Standard JSON decoding additionally rejects duplicate decoded
object keys (including escaped spellings), depth over64, over50,000 values and
trailing data before any staging or process work. Discovery is bounded to128 JSON candidates/two seconds;
ignored/unknown snapshots are not inferred absent. No online URL, username,
password, datasource selection, JPA model or live snapshot command is offered.

Before any process starts, exact confirmation shows project, source paths and
editor-text SHA256 identifiers, program and argv. The argument/report contract is
source-audited Liquibase **v4.29.2 text diff**; the installed CLI binary, version
and edition remain unverified. That source uses Pro-gated JSON diff formatting,
so the adapter deliberately requests text. Current docs also distinguish Secure
features and diff-type scope; newer edition/version behavior is not promised.
See the [official diff reference](https://docs.liquibase.com/community/reference-guide-5-0/database-inspection-change-tracking-and-utility-commands/diff)
and [v4.29.2 DiffCommandStep](https://github.com/liquibase/liquibase/blob/v4.29.2/liquibase-standard/src/main/java/liquibase/command/core/DiffCommandStep.java).

Project reads may contain unsaved buffers. The adapter therefore copies the exact
reviewed current text into owner-only plugin-private staging, serializes UTF-8
JSON, removes database connection identity fields, supplies generated empty
private defaults, and explicitly selects UTF-8 output encoding/private search
path. Selected project files are unchanged. The
[v4.29.2 snapshot parser](https://github.com/liquibase/liquibase/blob/v4.29.2/liquibase-standard/src/main/java/liquibase/parser/core/yaml/YamlSnapshotParser.java)
constructs an OfflineConnection from shortName; stored connection URL/user data
are not required for this scope. Source/root/staged-byte membership is revalidated
before start, during explicit Poll Status and before accepting completed output.
Final executable/filesystem TOCTOU remains unavoidable with this SDK.

The approved process inherits environment/JVM/extensions. Process permission is
not a database/network/filesystem sandbox: ambient settings can still affect
behavior, including output destinations. No SQL/changelog/update or output-file
flag is requested, but universal absence of external CLI side effects is not
proved. Confirmation explains this authority boundary; permission denial or
cancellation never falls back to an unscoped terminal command.

One instance-owned Child runs at a time with stdin closed. **Poll Status** drains at most eight
bounded chunks per click; **Stop Process** kills only the owning project's child.
A120-second polling lease stops it on the next poll, not via a background timer;
idle host reclamation (about five minutes) can also end it. No automatic polling,
durable job or real-time tailing is claimed.64KiB combined stdout/stderr,512
observations/1024-byte labels/4096-byte detail limits reject oversize output and
retain the last good report. Raw stderr is withheld. Known snapshot connection
strings are redacted; arbitrary CLI output cannot be universally classified.

The text adapter accepts only the requested Missing/Unexpected/Changed category
headers, exact known object/detail indentation and bounded report shapes;
unknown, duplicate or missing categories retain the old report and yield no
absence conclusion. Select a reported observation for its read-only detail, or **Choose Observations to
Preview…** to pick multiple rows and inspect their combined read-only text (128KiB
maximum). Picker IDs bind the exact owning report/root/content and acceptance
generation; cancel, forged/duplicate/stale keys or foreign roots preserve the prior
review. A newly accepted report expires old selections, including identical report
content; selection never starts a process or writes a file. The combined preview
is observed report text, **not SQL, DDL, a changelog or deployable changes**.
No object spelling is converted into inferred DDL. Rendering/selection never
runs or polls Liquibase. Rows retain source digests, roles, category, age and
unverified version/coverage provenance. Pinned Child chunks expose process exit
without a pipe-EOF flag, so output-drain completeness is explicitly unverified;
even zero observed rows is not proof of no differences or complete schema parity.

Private staged inputs are overwritten empty on normal completion, Stop and
observed failure only while their bytes still match this invocation's staged text.
Changed/unreadable slots are retained with a cleanup warning; absent slots are not
recreated. This content check does not provide an atomic file-identity/CAS guarantee. Crash/reclaim may leave owner-only copies, including sensitive
schema/default text, until manual cleanup/uninstall. Existing nonempty staging
is never overwritten. There is no SDK delete: at most96 staging files (32 distinct
input sets) are permitted; empty files can be removed manually before more sets
are used. No snapshot/report content is saved to plugin settings or user files.

Original23741 includes a4.23.2 core receipt and a DataSource/JPA-backed
CreateDiffLiquibaseChangeLog action. Its full model/DDL dialog is not this adapter.
The official v4.29.2
[DiffToReport contract](https://github.com/liquibase/liquibase/blob/v4.29.2/liquibase-standard/src/main/java/liquibase/diff/output/report/DiffToReport.java)
was independently inspected. Online snapshot capture, verified installed CLI
identity/edition, complete output transport and broad DB/extension support remain
P12.4 limits. The generated XML subset/save workflow below is separate from report
observations and never runs a database update. All tests
use SDK mocks; no real Liquibase, database, credentials or network service was
executed, and no matching native viewer screenshot is claimed.

## Source-browser packaged refresh

Committed browser changelog/configuration selection and clear now retain native
refresh and send only model-free `ui-kit-refresh` to **changesets**. The receiving
hub requests its own project/frame-correlated model. Cancelled, identical or absent
selection changes emit no notification. Browser presentation/Refresh actions use
the same browser-only path; stable renders emit no invalidation loop. Native
CLI/offline output views are excluded.

Browser selection remains project-owned and configuration provenance stays
source-only: it does not resolve an effective database/runtime target. Missing,
unreadable or nonconcrete project identity returns a safe replacement message;
foreign roots load only their own selection/discovery and old navigation is
rejected. Existing bounded browser discovery still occurs on explicit render;
no watcher, new process or network feature is added. Selection prompts act on the
captured reviewed project; repeated invocation reads are not atomic active-window
freshness. Storage/source race and same-project window limits remain.

Foreign/missing browser renders do not touch another project's owned offline CLI
handle, staging, report or cancellation state. A mock regression verifies the
owner child remains alive and staging unchanged; native73/107 CLI acceptance is
historical, not recredited to this source-only refresh slice. Atomic FileActions,
index ownership, managed CLI source/nonce/cancel guards are unchanged.


### Update/Rollback execution review (P12.3)

Update and Rollback Count keep their existing cancellation/count/final confirmation
semantics and exact terminal argv. The final review now shows the captured owning
project/requested terminal working directory, program, exact argv and root
`liquibase.properties` project/editor-text SHA256. A unique literal changelog and
recognized PostgreSQL/MySQL/MariaDB JDBC host/database are source hints only;
query/semicolon parameters and credential-bearing/ambiguous/dynamic values are
withheld. Raw configuration, usernames and passwords are never displayed.

The **effective database, contexts and labels remain UNRESOLVED**. Environment,
JVM/defaults/extension precedence and unsaved-versus-disk differences can override
source hints. The browser's source/config selection does not bind this command.
This is a concrete review of the existing invocation, not the original plugin's
resolved native datasource/module/config dialog. No new CLI target flag or
configuration mutation is inferred. Missing project scope or configuration drift
across the prompt refuses startup. Root configuration text is bounded to128KiB;
oversized concrete confirmation is refused before the host's16KiB clipping point.
Final filesystem/executable/environment races remain outside the SDK guarantee.
Managed offline-diff Child ownership/staging/Stop logic is unchanged.


JSON generator parents use the same bounded decoded-key-unique standard JSON
reader as snapshot inputs. Semantic metadata is normalized separately so valid
UTF-16 surrogate-pair escapes are accepted; insertions still address and retain the
original source bytes, escaping and comments/whitespace. YAML remains guarded
before loading; no aliases or tags are expanded.


### Reviewed offline generated changelog (P12.4)

**Generate Reviewed Offline Changelog…** reuses the owning project's two supported
snapshot choices and category selection, asks for a literal author, then previews
exact `diff-changelog` argv, input text SHA256 identifiers and a new private XML
output path. The command contract is Liquibase **4.29.2**; installed executable,
version and edition remain unverified. It requests offline source generation, not
update/rollback or a project output path. The existing single instance-owned Child,
closed stdin, source/staging checks, explicit Poll/Stop, output/time limits and
foreign-project refusal apply. A private output that already exists is refused.

Accepted output must be UTF-8 core Liquibase XML, at most128KiB/512 unique literal
changesets/8192 elements, with known4.29 change-type children, the requested author
and no opaque logicalFilePath,
external-file/include/property dependencies, DTDs, custom execution or foreign
elements. Missing/empty/invalid/oversized output retains the previous candidate;
it never proves no differences. **Choose Generated Changesets…** selects complete
source fragments in their original order, retaining root namespace declarations,
comments and untouched bytes. The exact selected XML and snapshot provenance are
shown. Dependencies across omitted changesets, SQL correctness, runtime parameters,
full schema coverage and output transport completeness are not inferred.

To save, open a project XML/YAML/JSON/SQL editor and choose **Save reviewed offline
changesets…** from code actions. Select an XML destination and review the complete
contents. The resolver rechecks the exact cached selection, project, source
snapshots, current editor and destination before/after prompts, returning one
non-overwriting, undoable FileAction Create. It never writes source directly or
runs CLI on selection/save. Existing buffers/files are preserved. Oversized16KiB
confirmation is refused; choose a smaller subset rather than silently truncating.
The candidate is instance memory only: reload/reclamation discards it until another
generation. A changed snapshot/selection/owner makes an old save action stale.

Generated schema/default text can be sensitive and is not redacted, preserving the
actual XML for review. Still-matching private staged inputs and started-command output are overwritten
empty on normal completion/Stop/failure; changed/unreadable input slots are retained
for owner review. Content checks are not atomic file-identity locks. Crash/reclaim,
cleanup failures and CLI-created
auxiliary files can remain until manual cleanup/uninstall. Process permission is
not a filesystem/network sandbox; environment/JVM/extensions may affect behavior.
The existing96-file staging cap permits at most24 fresh generated four-file runs
(or32 report sets) before empty-cache cleanup. Auxiliary CLI files are outside that
known-file count. No native datasource/ORM authority, verified installed CLI or new
native acceptance is claimed by source/mocked tests.

### Readonly local references (P12.2 extension)

Definition requests can open a unique literal property declaration in the current
namespaced XML, validated YAML/JSON, or formatted SQL changelog. XML/YAML/JSON
rollback references with explicit literal `changeSetId` and `changeSetAuthor`
navigate to a unique same-file tuple only when there are no includes, opaque
logical paths or unknown identities. Explicit `changeSetPath`, duplicate names,
dynamic values, unsupported scalar shapes and ambiguous scanner paths stay quiet.
Locations preserve source UTF-16 ranges and perform no process, project scan or
file reads for local references. This identifies source declarations; conditional
property precedence, inherited values and runtime usage completeness are unknown.

Direct include definitions now use bounded source parsing, reject foreign or
undeclared namespaces, validate YAML/JSON structure, and require a unique concrete
file owned by the same innermost project. Targets are statted rather than read.
No complete reference index, automatic Rename, online database/table/column
resolution or new native desktop acceptance is claimed; P12.2 remains partial.

### Explicit relative SQL and CSV source navigation

Go to Definition also opens literal `sqlFile.path`, `loadData.file` and
`loadUpdateData.file` values on direct core XML changeset children or YAML/JSON changes array entries when
`relativeToChangelogFile="true"`. The current unsaved source must be structurally valid,
namespace supported and free of `xml:base`/`logicalFilePath` ambiguity. YAML/JSON
requires a literal Boolean true and uniquely located unescaped scalar spellings;
aliases, tags and duplicate mapping keys are refused. Resolution
uses the physical changelog directory, requires an existing file in the same
innermost project, and refuses project escapes, denied reads, substitutions,
encoded aliases and URI/search-path interpretation. No SQL/CSV contents are read
or executed. This is read-only source navigation: no indexed Rename/Find Usages,
runtime database resolution or proof that the resource is valid SQL/CSV is added.
Rollback-nested resource changes are outside this addition.

The YAML/JSON resource lookup scans source keys once for the caret scalar and
once for source-path uniqueness, then walks semantic resource owners. It avoids
rescanning all keys for every resource. Source/structure limits still apply;
this is not a measured native 50 ms latency guarantee.

XML source mutation and observed generated XML require at most one declaration
at document start, with ordered XML1.0 version, optional encoding and standalone
attributes. Misplaced, duplicate or malformed declarations and the reserved
case-insensitive `xml` processing-instruction target refuse admission. Legal
leading declarations and unrelated processing instructions in edited source are
preserved; generated fragments retain their existing stricter no-PI rule.

Include-file Go to Definition validates the full current XML document before
scanner-backed navigation, including legacy unnamespaced core changelogs.
Malformed declarations/attributes/namespaces and opaque `xml:base` or
`logicalFilePath` owners stay unresolved. This read-only legacy admission does
not relax namespaced XML requirements for source mutation.

XML admission validates element and attribute qualified names with XML1.0 Fifth
Edition NCName character ranges, including namespace prefixes. Invalid digit or
combining-mark starts refuse source mutation/generated candidates; legal Unicode
names and prefixes remain supported. This is XML name validation, not full
Liquibase XSD or database-semantic validation.

Namespace-aware XML admission also rejects attributes whose different prefixes
resolve to the same namespace URI and local name. Distinct namespaces and
unqualified attributes with equal local names remain valid and byte-preserved.

Stored browser selections use bounded JSON with unique decoded keys, including
project ownership and source stamps. Ambiguous or oversized JSON owners are not
adopted, replaced or cleared by selector commands. Explicit recovery of broken
non-JSON legacy bytes remains available. These are browser-source ownership
checks; they establish no effective database/runtime target.
