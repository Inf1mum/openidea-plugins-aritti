# Spring Initializr Launcher

Starts Spring Boot projects from OpenIDEA through a Spring Initializr server:

- Spring Initializr tool window (right): server URL (default `https://start.spring.io`), build (Maven, Gradle - Groovy, Gradle - Kotlin), language, Spring Boot version (blank for the server's default), group, artifact, name and package name (both follow the artifact until you change them; the package is `group.artifact` lower-cased without `-`), description, packaging, Java version and dependencies (comma-separated ids such as `web,data-jpa`). The form is kept per project and server in plugin storage; Reset restores form defaults in the current scope.
- Generate Project opens `<server>/starter.zip?…` in the browser, which downloads the project zip. Explore in Browser opens the server's page (`<server>/#!…`) with the same options. Blank options are left out. The status line says what's missing (an http(s) server URL, a group or an artifact).
- Generate with Spring Boot CLI opens an IDE terminal running `spring init` with the form's explicit service (`--target`), build, type, language, Boot and Java versions, packaging, group, artifact, name, description, package name and dependencies, plus `--extract` into a directory named after the artifact (in the terminal's working directory). It needs the optional `process:spring` and `project.read` capabilities, an identifiable open project, and the Spring Boot CLI on the PATH; without the capability, a notification says so and the browser download runs instead. Group, artifact, name, versions and package name are checked against letters, digits, `.`, `_` and `-`, and the button reports a refusal otherwise. Values reach `spring init` as separate program arguments, never through a shell line, so none can inject a command. Keyed terminal completion and separate managed CLI controls track the result. After a successful exit, Review and Open Generated Folder independently inspects the physical tree, presents its hash/file manifest, rechecks it and requests normal project trust/open. The separate source-only ZIP import does not execute this CLI.
- The Spring Initializr: Open start.spring.io command opens the site.

- Spring Boot version is a select in the Project form, populated from the server catalogue. Server default shows the advertised default; saved versions remain visible if no longer advertised and still undergo compatibility validation.
- The Dependencies section (toolbar button "Dependencies (N)" next to "Project") browses the server's catalogue as a tree. The first node, Selected (N), lists the chosen ids (name and description from the catalogue, the raw id when unknown); below it come the catalogue categories, collapsed, with a check on chosen dependencies. Double-click or Enter on a dependency adds or removes it; the chosen ids stay in the form's `dependencies` field, which you can still edit in the Project section. Search... filters by name and description (case-insensitive, categories open, "N matches" in the status line) until Clear search; Clear all asks first; Refresh reloads the catalogue. Dependencies whose `compatibilityRange` excludes the current Spring Boot version (the form's version, or the server's default when blank) are hidden; a chosen one stays under Selected marked "(not compatible with <version>)". The Generate buttons work in both sections.
- The catalogue comes from `https://start.spring.io/metadata/client`, needs the optional `net:start.spring.io` capability and is loaded from a saved snapshot when available, otherwise fetched once on first opening of the section; Refresh requests new metadata. For any other server, or without the capability, the tree shows a message and Selected keeps working; type the ids in the Project section.
- Spring Initializr: Prepare Add Spring Boot Starters (Tools menu) prepares a version-bound metadata cache for a supported current Maven POM; its **Preview Add Spring Boot Starters…** source intention then picks and previews an atomic edit. The former immediate Maven/Gradle writer is removed; see the supported subset below.

The form, dependency choices and browser download work without project access; CLI generation is a project-bound action.

Generation is available through browser download, Spring CLI, or the reviewed complete ZIP adapter. Plugin requests target start.spring.io only.

Not implemented: native New Project integration, arbitrary ZIP timestamp/nonstandard permission preservation beyond normalized644/755, and catalogue/Boot choices for servers other than start.spring.io. The complete ZIP adapter supports outside-project destinations; the browser path remains manual download/extraction.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


P19.1 now persists changed fields (including selected dependencies) and the server
metadata snapshot under an exact project-root + normalized server identity. A
project-free browser form has its own explicit scope. Switching projects or
servers loads that scope's form/catalogue; the last server is remembered per
project. A saved catalogue is reusable offline. Refresh failures, malformed or
duplicate choices, oversized responses, and storage failures retain the previous
saved snapshot and report the failure. The obsolete global `form` value is left
untouched and is not assigned to an unknown project. Without storage the active
form/catalogue works for the session; switching scopes cannot restore unsaved
state. Search, expansion and the section choice are not part of this scoped
form snapshot.

The form displays the server's declared build, language, packaging and Java
choices once metadata is available. Generation and CLI arguments reject choices
absent from nonempty server lists, and reject selected dependencies/options whose
explicit `compatibilityRange` excludes the chosen Boot version (or the declared
server default). Supported ranges are numeric versions with M/RC/SNAPSHOT
qualifiers, lower bounds, and open/closed intervals. Missing defaults/lists,
unknown range syntax and unknown manually entered dependency ids remain
unverified; the server remains authoritative. No Boot-to-JDK/build table is
invented when the metadata does not supply a relationship. Java here is the
requested project language level, not verification of an installed JDK. Metadata
is a saved snapshot with explicit Refresh, not a freshness guarantee.

State has a shared 2 MiB read/write bound and a 32 KiB changed-fields bound;
fields are limited to 8192 bytes (server URLs to 2048), options to 512 per list,
dependency categories to 128, and dependency ids to 4096. Full scope identities
are checked inside hashed storage keys before reuse/replacement. Reset changes
only the current server's form fields and retains its catalogue. Server catalogue
network access still targets start.spring.io only; custom-server values remain
manual and compatibility-unverified. Browser generation works project-free;
CLI extraction requires a verified project and otherwise falls back to the
browser. This does not establish full native project-wizard parity; the later source-only import is a separate bounded path.

Original comparison: inspected cached 20212 `SpringInitializrSettingsProvider`
(project-scoped settings) and `SpringInitializrCommandProvider` (command
registration), plus Marketplace screenshot `20212-0.png` showing server/build,
JDK and Java as separate controls. This delivery adopts scoped state and declared
choices without claiming its native JDK selector or New Project wizard lifecycle.

When a loaded snapshot declares a default Boot version, leaving Boot blank pins
that version in the outgoing browser/CLI request. This keeps the generated
request aligned with the version used for compatibility checks even if the
server changes its default later. Without that fact, blank is left to the server.


### Preview Add Starters (P19.3 partial)

Open a current `pom.xml`, run **Prepare Add Spring Boot Starters** to prepare metadata,
then invoke **Preview Add Spring Boot Starters…** from its source actions. The
command performs the existing explicit server requests and changes no source;
a command cannot return a FileAction. Listing and resolution use the prepared
15-minute, project/path/Boot-version-bound memory cache without network. A
reclaimed plugin instance needs preparation again. Generation form fields,
browser/CLI workflows, selected dependencies and persisted metadata are preserved.

This slice supports only a literal three-number GA
`org.springframework.boot:spring-boot-starter-parent` with explicitly empty
`relativePath`, or a parentless standalone POM whose sole management entry is a
literal GA `org.springframework.boot:spring-boot-dependencies` BOM with `type=pom`
and `scope=import`. The latter requires literal own group/version and refuses
any competing parent, additional managed dependency or BOM. Both require Maven
model4.0.0 and a top-level dependencies block
with a line-separated closing tag, an empty `<dependencies/>` block, or no
dependencies block. The action expands the empty block or creates one before
an existing `<build>` block (otherwise before the line-separated project close). Default/no Maven namespace is accepted;
foreign/prefixed or nested namespace forms are refused. Profiles, other dependency
management/BOM forms, repositories, modules, extensions, nonstandard properties and
competing `.mvn/maven.config`, `jvm.config` or `extensions.xml` along the bounded
module ancestry are refused. Only `java.version` and
`project.build.sourceEncoding` properties are admitted. Files are capped at
12 KiB, 512 XML nodes, 32 XML levels / ancestry directories. Gradle, competing/custom
BOMs, custom/local parents, dynamic/pre-release versions, custom servers and
other build-model forms remain unsupported; no effective Maven model is inferred.

The actual POM parent/BOM version must appear in the saved server metadata. The
`/dependencies?bootVersion=…` response must echo that exact version. Pickers
include only declared-compatible plain `org.springframework.boot` starter
coordinates with no explicit version, BOM or repository requirement and a known
compile/runtime/provided/test scope. Unknown ranges/configurations and other
coordinates are omitted. At most eight distinct advertised IDs can be chosen;
existing group/artifact pairs are not inserted again. No dependency version,
repository, BOM, scope migration or Java/runtime compatibility is guessed.

The intention binds the exact project, path, language, document version/text,
server selection and metadata snapshot. It checks current visible source and
competing project configuration before selection and after confirmation, then
returns one current-file insertion as a host-managed atomic, undoable edit.
The preview contains the **complete exact resulting file**, preserves LF/CRLF
and existing bytes, and refuses previews beyond the host confirmation limit.
Cancel, expired/forged/replayed actions, changed ownership/metadata/source and
collisions return no edit. `Project::read_file` prefers unsaved editor text, so
these are visible-source freshness checks, not saved-on-disk proof; unsaved
current text is supported. The final read/host-prepare gap and external/global
Maven settings remain outside this static model's authority.

Original `SpringInitializrModuleBuilder` / `SpringBootDependency` evidence uses
server dependency IDs and framework-version ranges and delegates project
creation to shared starter infrastructure. This bounded existing-file action
does not reproduce that full semantic/build-model infrastructure. Native
picker/complete-preview/apply/Undo acceptance, Gradle insertion and general
BOM/repository migration remain gaps. Recognizing an existing sole Boot BOM does
not add plugin management, migrate a BOM or evaluate an effective Maven model.
[Spring Boot’s primary documentation](https://docs.spring.io/spring-boot/maven-plugin/using.html#using.import)
distinguishes imported dependency management from plugin management. No proprietary implementation was copied.


### Source-only Maven ZIP import (P19.2 partial)

**Prepare source-only import…** (or Tools → **Prepare Maven Source-only Import**)
uses the current project/server form and saved metadata. It requires an open
project and advertised Maven / Java / Jar / Boot / Java-version choices and
known dependency compatibility. Custom servers and unknown ranges are refused.
Choose an existing ordinary parent directory **inside that project**; the target
is its vacant artifact-name child. A first confirmation shows the exact URL and
target and states that no process or disk extraction runs. The ZIP is downloaded
only by this explicit command, then decoded and validated entirely in memory.
No source file is written during preparation.

Open any small existing project editor and invoke **Preview Import Maven
Sources…**. The approval contains the complete exact source manifest, every
file's UTF-8 content and byte count, the new folder, request and explicit known
wrapper exclusions. The result is one host-managed atomic FileAction of at most
32 non-overwriting creates. A collision, changed owner/form/server/metadata,
expired 15-minute plan, stale anchor document, cancellation or oversized full
preview returns no operation. Source content is capped at 12 KiB and the complete
confirmation at 16 KiB; larger projects are refused rather than summarized or
partially imported. LF/CRLF and all admitted file contents are preserved.

Only these known wrapper files are omitted, and their presence is explicitly
listed in approval: `mvnw`, `mvnw.cmd`, `.mvn/wrapper/maven-wrapper.properties` and
`.mvn/wrapper/maven-wrapper.jar`. This is deliberately a **source-only Maven
folder**, requiring separately installed Maven for a later manual build; it is
not complete archive extraction. The importer rejects other binary/non-UTF-8
files, executable/nonstandard modes, symlinks/devices, unknown `.mvn` files,
unsafe/reserved/traversing/absolute paths, duplicate names, case or parent/file
ambiguities, encryption and unsupported compression. Only ordinary single-disk
classic ZIP with stored/deflated entries is supported; ZIP64/comments/multidisk
forms are refused. Limits before/while decoding: 1 MiB archive, 64 entries,
128 KiB each, 512 KiB declared total and bounded compression ratio. CRC/decoded
sizes are checked. A root POM must match the chosen group/artifact/Boot parent
and supported static Maven form; a source path under the chosen Java package
must exist. This is not a compilation or semantic Java validation.

After applying, **Open reviewed source folder…** is a separate command/button.
It independently checks that all expected paths physically exist as ordinary
project files/directories and their **current project-visible text** matches the
reviewed manifest. Missing, partial or changed files refuse opening. Confirmation
explicitly explains that unsaved buffers can mask disk contents: disk bytes,
extra files, a successful build and effective runtime state are not certified.
The command rechecks after confirmation and calls `open_project(..., true)` only
then, requesting the normal trust/open flow in a new window. It does not mark
anything trusted or assume the host applied the earlier action. Host apply/open
requests have no completion callback; a cancelled apply simply fails the later
verification. Final read/prepare/open gaps remain.

API 1.3 boundaries were checked in `host.rs`, `wit_host.rs` and `file_actions.rs`:
`pick_directory` grants no filesystem permission; `fs.*` needs declared literal
path grants outside plugin-private storage; it offers no directory-create API;
FileActions Create is text-only and stays inside the current project. No dynamic
arbitrary grant is invented. Browser generation and the legacy CLI terminal path
remain separate; this source import runs no CLI, shell or wrapper. Native
preview/create/Undo/open/trust acceptance remains unverified. Prepared manifests
are session-memory only and disappear on reclaim, project/server changes or
metadata replacement; existing generated folders can still be opened manually.


P19.3 format follow-up: Add Starters now handles an absent or self-closing Maven
`dependencies` container without manual preparatory edits. It retains the exact
existing source bytes outside one returned edit, preserves LF/CRLF and existing
build configuration, derives indentation from source, and revalidates the complete
result with the same static POM proof before confirmation. All current metadata,
Boot version, owner, source/revision, expiry, cancellation and replay guards remain.
The resolver performs no fetch; source changes after preparation still refuse.
This does not add BOM/repository migration, arbitrary editable XML, Gradle insertion,
project model resolution or dependency version inference. Those remain outside
the proved static parent form.

Original comparison for this follow-up: retained decompiled
`SpringInitializrModuleBuilder` parses dependency IDs/ranges and delegates to shared
starter infrastructure; `SpringBootDependency` carries framework version ranges
and coordinates. The retained gallery dependency screenshot shows Boot selection,
searchable categories/descriptions and selected dependencies, not an existing-POM
BOM migration contract. Existing native picker/full-preview/atomic-edit interaction
is preserved; the new empty/absent container cases have focused tests and matching
package verification, with desktop acceptance still separate.

### Strict compatibility range proof (P19.1 follow-up)

Numeric version components and M/RC qualifier numbers must parse completely within
the supported integer model. Overflow, malformed/trailing components, unsupported
qualifiers and contradictory interval bounds remain unknown; they are not compared
using a partial numeric prefix or a substituted qualifier number. A dependency
with such a declared range stays visible with **compatibility range unverified**
in both Selected and catalogue rows, plus a status explanation. Known incompatible
ranges remain filtered. Project status states that only advertised choices and
supported ranges are checked; unspecified requirements are unverified. This does
not infer installed JDK compatibility or a build resolver, block manual unknown
choices as if they were proven incompatible, or perform any new request.

### Scoped CLI target preflight (P19.2 follow-up)

CLI arguments pin the selected literal service with `--target` (including the
default), instead of silently using the CLI default/configuration. This form
refuses credential/query/fragment/escaped/whitespace URLs, unsupported host/port/
path spellings and traversal. Custom-service catalogue compatibility remains
unverified; this is an external CLI request, not a plugin `fetch` host-grant claim.

Before opening the terminal, the fresh invoking project must match the form owner
and both root/output-child namespace owners. The non-hidden artifact child is
bounded to128bytes and must not already exist as a file or directory. Pinned
host `open_terminal` uses the Invocation workspace root as cwd. No shell is used,
no force flag is added, and a refused preflight launches nothing.

These checks are preflight, not exclusive directory creation: another process
can race them. The terminal launcher does not track CLI completion, validate
partial output, clean up failures or automatically open a generated project.
Managed CLI staging/completion and broader extraction remain P19.2 work; the
separate reviewed source-only ZIP path retains its stronger atomic guarantees.


### Current source-import owner checks (P19.2 follow-up)

A lexical child path is insufficient when another open project owns a nested
folder. Source-only import now checks the current innermost project owner of the
selected parent and its existing ancestry before archive fetch, and the proposed
target plus every planned source path before issuing creates. These checks repeat
around the final review. The separate reviewed-folder open path also refuses an
expected file reassigned to a nested project, even if its bytes still match.
Unknown or denied ownership fails without returning operations or opening a
folder; sibling and same-owner imports retain their existing behavior.

Three regressions cover a nested selected parent before fetch, owner changes
after preparation (including a deeper planned-file namespace), and a nested
expected file before reviewed open. The first two failures were reproduced on
67 before the fix. All70 tests and the matching pinned build/check pass in the
receipt. These are metadata-owner checks, not canonical filesystem alias proof
or a final read-to-host-apply transaction. No real network, CLI or native GUI
was run. Managed CLI completion, arbitrary outside-project extraction and full
native project-wizard acceptance remain separate P19.2 work.


### Unambiguous metadata and saved state (P19.1 follow-up)

Server metadata/coordinates and saved form/selector envelopes now use bounded
JSON admission with unique decoded object keys. A key written through a Unicode
escape is the same key for this check; duplicate owner/server/default fields
cannot silently select a last value. Limits are64 nesting levels,50,000 values
and the existing2MiB response/state cap (selectors are now consistently8KiB). Ambiguous or
overbudget stored data is refused and retained instead of adopted or overwritten.
The same admission is used before starter/import selector ownership checks.

Present catalogue option/dependency containers must have the supported object/
array shape; present names, defaults and descriptions must be bounded strings
or null. Missing facts remain unverified, rather than becoming incompatibility
assertions. A failed response preserves the prior accepted catalogue and saved
bytes. Three public regression groups reproduce the former duplicate-server,
malformed-container and duplicate-owner-state failures; two parser tests cover
decoded keys, depth/value/byte bounds and trailing input. All75 tests and the
pinned API1.3 build/check pass.

The direct serde dependency was already resolved/cached transitively; only the
root dependency edge changes in Cargo.lock, with no new package/version or SDK
change. The helper is reused from this repository's MIT implementation. This
does not add custom-service metadata fetching, inferred JDK/build compatibility,
managed CLI completion or native state/reload acceptance.


### Maven XML context admission (P19.3 follow-up)

Add Starters now refuses duplicate/misplaced XML declarations, unsupported
declaration attributes/order/values, malformed comments and reserved-case XML
processing-instruction targets before metadata fetch. A root xsi:schemaLocation
requires the exact corresponding schema-instance namespace declaration. Valid
leading declarations, optional encoding/standalone attributes and ordinary m2e
processing instructions remain source-preserved through the previewed edit.
The source-only ZIP POM check reuses this strengthened proof.

Four new tests cover these refusals and valid complete resulting previews.
Actual declaration-placement and malformed-comment failures were captured on75;
the declaration-attribute guard was separately reproduced after the first
placement fix. All79 tests and pinned API1.3 build/check pass. This adds specific
XML context guards within the existing12KiB/512node/32depth static POM subset,
not full XML/XSD conformance, effective Maven compatibility, Gradle support or
native edit/Undo acceptance. No schema or dependency is fetched by these guards.


### Failed server transitions retain the form (P19.1 follow-up)

Server choice persistence now uses one8KiB envelope cap in readers, writers and
source-action selector checks, with the existing2048-byte server field bound.
Supported-length project/server identities no longer write a selection that
the reader silently discards. Larger generated envelopes are refused before
storage; no previous selector is overwritten on that failure.

An explicit server change first saves the current form, then persists the new
selector, and only then changes in-memory server/form ownership. Failure of either
write keeps the current form model's server, fields and metadata, with a status
explaining the refused change. Retry after storage recovery saves the retained
draft before switching. This is an ordered in-instance transition, not an atomic
storage CAS; process reclaim and unrelated project/window switching remain
separate from this guarded server-change operation.

Three new tests reproduce the long-selector reload mismatch and both failure
transitions, including exact prior bytes, retained unsaved fields and recovery.
All82 tests and pinned API1.3 build/check pass; no service or CLI runs. Browser/
CLI/source-import/AddStarters choices remain otherwise unchanged.

The unused optional guest editor.write grant is removed. Source changes are
prepared FileActions applied by the host after the explicit review; no Initializr
production path directly applies editor edits or writes project files. This is
not a new native acceptance claim for Initializr's source actions.

### Declared CLI project facts

The CLI adapter supports `maven-project`, `gradle-project`, and
`gradle-project-kotlin` explicitly. Other types require the browser generation
action. When the saved catalogue declares a type's `tags.build` or `tags.format`,
they must agree with the adapter's build and complete-project extraction.
Missing tags remain unknown. Refresh rejects malformed tag containers or
non-string/oversized build and format facts while retaining the prior snapshot.
These checks do not establish installed JDK compatibility, effective build tool
versions, or successful CLI extraction.

Source import applies its4096byte normalized absolute-path bound to the joined
output directory and every joined archive filename, in addition to validating
the selected parent and relative archive names. An overlong directory is refused
before archive fetch; overlong source paths are refused before preview/actions.

The source-only Maven importer requires the returned POM to echo the selected
literal `java.version` as well as coordinates and Boot parent. Packaging must be
explicit `jar` or omitted (Maven's default). A different or absent Java property
and explicit WAR packaging refuse preparation. This checks returned source
facts; it does not resolve inherited compiler settings or verify an installed JDK.

With selected dependency IDs, source-import preparation also requests
`/dependencies?bootVersion=<selected version>` and requires an exact version echo.
The returned POM must contain every selected plain Boot-managed starter with its
declared scope. Up to64 selected starters are supported; coordinate aliases,
version/BOM/repository mappings and unproved IDs refuse this bounded import.
Additional generated dependencies are preserved in the complete preview; no
transitive dependency graph or resolver is inferred.

Form and catalogue persistence now compares the raw state and server-selector snapshot captured when the draft loaded. A changed saved baseline refuses writes and retains the local draft. Reload saved form requires confirmation before discarding that draft and loading current saved data; cancelled, denied or unreadable reloads retain it. This is optimistic conflict detection with ordinary storage get/set, not atomic cross-window compare-and-set.

The separate CLI run section offers reviewed managed Start, explicit Check status/output, and Cancel. One child is retained; the exact project/CWD, destination and argument array stay attached to its bounded32KiB observed output. Output overflow requests stop; failed stop retains the control handle and blocks duplicate starts. Scope changes and deactivation request stop. Status checks run only on request: there is no automatic idle deadline; a child continues until exit, Cancel or window close. The inherited Java/environment configuration applies and the CLI can access its selected service and create partial output. No exclusive target reservation, automatic cleanup, generated-tree validity, build success or project opening follows from exit status. The existing terminal launcher remains separate; source-only ZIP import retains its distinct reviewed atomic safeguards.

Conflict or storage-denied drafts are retained in memory for at most two other project/server scopes while switching projects. Returning to the same scope restores the draft and its original save baseline, so stale data still cannot be overwritten. A further scope change refuses without discarding the current draft; the message identifies retained scopes for Reload recovery. These unsaved drafts are not durable across plugin/window restart.

### Add Starters: literal Gradle source

The existing Prepare Add Starters command also supports the owning project's exact `build.gradle` (Groovy) or `build.gradle.kts` (Kotlin) file. The supported declaration has the Java plugin, a literal three-part numeric Boot plugin version advertised by the saved catalogue, and literal `io.spring.dependency-management` version `1.1.7`. A dependencies block is required. Optional supported source is limited to literal group/version/description, a numeric Java toolchain, `mavenCentral()`, and the conventional JUnit Platform test task. An optional matching-dialect settings file may contain only literal `rootProject.name`. Source is bounded to12KiB; line comments/UTF8 text and LF/CRLF bytes are preserved outside the insertion.

Preparation fetches coordinates for that exact source Boot version. Only advertised compatible plain Boot-managed starter coordinates are offered. Compile/runtime/test map to `implementation`/`runtimeOnly`/`testImplementation`; `provided` is refused rather than guessed. Existing exact group/artifact collisions are skipped. Selection produces a complete resulting-file review and one source-bound undoable FileAction insertion; Cancel, stale source/owner/settings/selector and replay refuse. Provider discovery does not fetch or execute Gradle.

Dynamic expressions, escaped/interpolated strings, block comments, extra plugins, applied scripts, modules, version catalogs, properties/build logic, competing build files, custom repositories and additional BOM/platform declarations are outside this adapter. Server coordinates requiring a repository/BOM/explicit version remain refused. This is declared static source management, not an effective Gradle model, installed toolchain check or build/runtime certificate. Maven parent/sole-BOM support and source-only ZIP import safeguards remain unchanged.

### API 1.4 terminal lifecycle

Generate with Spring Boot CLI now uses a keyed native terminal operation, with
an owned completion event and project-scoped completion notice. An active
operation prevents a second generation or managed CLI start. Completion never
validates generated files or opens/removes a generated project. The terminal's
native controls own cancellation; the separate bounded managed CLI path retains
its explicit Check/Cancel controls.

A reusable native Run configuration is deliberately not published for `spring
init`: Rerun would repeat extraction after the destination exists without the
plugin's destination preflight. Every new generation continues through the
current form and destination checks.

### Compact tool-window actions

Project and Dependencies keep Generate Project as the primary toolbar action.
More Actions… opens the native action picker for the existing CLI launcher,
browser exploration, source-only import preparation, reviewed-folder opening,
Boot version selection, reset/reload, and catalogue refresh where applicable.
Only currently enabled actions appear; each retains its existing review and
capability checks. A changed form, project scope or saved-state snapshot while
the picker is open refuses the selection. Project, Dependencies and CLI run
remain separate sections; dependency search/selection and managed CLI controls
stay directly accessible.


### Current Initializr client metadata

Dependency compatibility reads the current `versionRange` field as well as
legacy `compatibilityRange` snapshots. Conflicting aliases or malformed range
values reject metadata refresh and preserve the previous catalogue. The same
range drives dependency visibility, generation validation and strict source-only
import checks. Search also matches dependency IDs, case-insensitively, so an ID
such as `data-jpa` remains discoverable when its display name uses different
wording.

### Generate and open the complete ZIP

**More Actions → Generate and Open…** (also the **Generate and Open Complete
Project** command) downloads the official server ZIP and uses an optional approved
`python3` executable, Python3.8 or newer, in isolated mode. Choose an existing
parent anywhere the approved process can access, including outside the current
project. A project-free invocation is supported. After download/inspection, review
the canonical destination, archive SHA-256 and complete file/directory manifest
before any destination writes occur.

The adapter retains binary wrappers and empty directories. Executable files get
mode755 and other files644; special modes, links, traversal, case-colliding names,
encrypted archives and file/directory conflicts are refused. It supports up to
512 entries, 8MiB compressed and 32MiB expanded, with at most4MiB per file; the
complete review must fit the host's16KiB confirmation limit. No generated wrapper,
build or project script is executed. The downloaded ZIP occupies one bounded
private cache file; a competing generation changes its digest and invalidates the
review.

Extraction goes into a newly allocated private staging directory in the selected
parent. A kernel-exclusive rename commits the whole tree without merging or
replacing an occupied destination. Supported profiles are macOS `renamex_np`,
Linux libc `renameat2`, and Windows's non-replacing `os.rename`; an unavailable
profile fails visibly. Failures leave staging for explicit inspection and never
open a partial project. Successful commit requests the IDE's normal project
trust/open flow in a new window. This does not grant trust, configure an installed
JDK or automatically import/execute a build. The original source-only atomic
FileAction import remains available independently.


After a successful managed or keyed-terminal Spring CLI exit, **CLI run → Review
and Open Generated Folder…** uses the same approved Python3 profile to inspect
the current physical folder. It rejects links, special files/permissions,
metadata directories and oversized trees, shows a complete path/size/mode/hash
manifest and verifies the tree again after approval before requesting the normal
trust/open flow. CLI exit0 alone never opens a folder or certifies a successful
build. Failed/cancelled or foreign-project runs cannot use this action.

Native task cancellation is checked between acquisition, inspection, review,
extraction and opening. Cancellation during the adapter can leave a
`.initializr-*` staging folder in the chosen parent, or a fully committed project
if commit already occurred; neither is deleted automatically and no cancelled
operation opens a project.

Starting a new managed or keyed-terminal generation retires the preceding
successful-run candidate. A later failed/closed terminal cannot accidentally
reopen the previous operation's output. Keyed completion still requires its exact
operation key and native terminal ID, and source-tree review is separate from
exit status.
