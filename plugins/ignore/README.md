# .ignore

Port of [.ignore](https://plugins.jetbrains.com/plugin/7495--ignore)
(`mobi.hsz.idea.gitignore` 4.5.7, [JetBrains/idea-gitignore](https://github.com/JetBrains/idea-gitignore)
commit `860a6855`, Apache-2.0) by Jakub Chrzanowski and JetBrains.

- Problems in ignore files, checked on open and save: duplicate, redundant
  (warning), relative and malformed entries, and invalid `syntax:` values
  (.hgignore). Quick fixes: remove the entry, remove the relative part, use
  glob or regexp syntax.
- Path completion in glob-syntax ignore files (.gitignore, .dockerignore…): files and folders below the
  ignore file's folder, after `/` and `!` too. The IDE hides dot-paths and ignored files from the file
  search, so those are not offered. `syntax:` completion in .hgignore. Ctrl-click on an entry that names one
  file opens it.
- _Add … to .gitignore_ quick fix (and an unignore variant with `!`) in any
  project file, when the project root has a .gitignore.
- New offers exact dotfile names for all 20 supported creation types, including
  `.gitignore`, `.dockerignore`, `.npmignore` and `.hgignore`, using API 1.3
  `fileName` rather than appending an extension. The Git template keeps the bundled
  starter; other types are comment-only starters (`.hgignore` starts in glob mode).
- _Ignore Templates_ tool window: the github/gitignore templates and your
  own (_Create User Template_ saves the focused file). Double-click a template
  to add it to the open ignore file.
- _Ignored_ in the status bar while the focused file is ignored, according to
  `.git/info/exclude` and every `.gitignore` from the root down to the file.
- A warning, once per project, for a Git repository without a .gitignore.
- The `dotignore` language, with highlighting, for ignore files the IDE
  doesn't detect (.chefignore, .p4ignore, .vercelignore…). The IDE's own
  `ignore` language (.gitignore, .dockerignore…) receives honest semantic tokens,
  but visible colouring remains limited by the pinned renderer (details below).
- Star a template with _Star_ in the Ignore Templates tool window; starred
  templates (kept in storage) are listed in a _Starred_ group.
- Settings › Plugins › .ignore: turn off the warning, the status or the unignore
  variant of the quick fix (`unignoreActions`), and insert templates at the caret.
- _New Ignore File_ (Project view, Tools) now points to native New and the
  **Create exact-name ignore file…** intention. The former shell writer is removed;
  commands cannot return atomic FileActions, so this legacy command is guidance,
  not a second file writer. _Unignore Selected Paths_ and _Add Selected Paths_
  keep their version-checked editor workflow and also appear in the editor menu.
- Adding an entry to a file with CRLF line endings keeps them CRLF.

The redundant-entry check compares patterns. The original matched them against
the files on disk, which plugins can't list once they are ignored. Only entries
that name one path, or a glob inside a literal directory (`build/*.o` under
`build/`), are checked.

`templates/` is [github/gitignore](https://github.com/github/gitignore) at
`53cf2971` (CC0-1.0, see `templates/LICENSE`), embedded in the component by
`build.rs`. `src/lib.rs` uses the API 1.3 registration macro for syntax, providers, views
and FileActions.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


### Previewed create and append intentions

From any project editor, **Create exact-name ignore file…** chooses one of the
20 supported names and an existing project-relative directory (empty means the
root), then confirms the exact destination and initial content. It returns a
non-overwriting, undoable FileAction that opens the new file; resolution writes
nothing. Existing files/buffers, outside paths, traversal and invalid directories
are refused. Native New-menu templates also use exact filenames.

For a project file, **Add … to .gitignore** and its unignore variant are now
FileActions: the root `.gitignore` must exist. Literal paths escape wildcard,
bracket, backslash and space characters; newline/control and traversal paths are
refused. In a glob-dialect ignore file, **Append ignore template…** selects up to
16 bundled/user templates and confirms the exact insertion. Existing template
headers are refused instead of silently duplicating a block. These append actions
preserve the target's existing LF/CRLF and current unsaved text, bind the source
path/version/project, and recheck target content after confirmation. Source or
target changes require invoking the action again. Each buffer/addition is limited
to 256 KiB. Unignore entries can still require unignoring a parent directory; the
plugin does not invent those rules.

The existing template tool window and selected-path commands still use
version-checked edits of the open file. Glob template/path insertion now refuses
regexp-dialect file types, including `.hgignore`, to avoid changing rule meaning.
For those types use native editing; the `.hgignore` starter itself remains valid.
No program or clipboard permission is required for creating/appending via native
intentions. There is no filesystem lock after resolution; final application,
encoding/BOM preservation, conflict detection and undo are owned by the host.
Desktop create/append/undo remains unverified for this change.

Original scope was checked against the pinned 4.5.7 `IgnoreFileAction`,
`CreateFileCommandAction` and `ContentGenerator` decompilations: type-specific file
creation and append commands remain the model. This bounded implementation does
not reproduce the original generator dialog's complete controls or claim visual
parity.


### Structured dialect preferences (P08.4 supported controls)

**Tools → Ignore: Dialect Checks and Preferences** opens an application-global
native form without requiring a project. A single exact-filename selector covers
42 shipped/built-in file names while keeping the form at12 fields. Each selected
type has independent **Editor assistance**, **Allow Create action**, and **Checks**
controls. Switching types retains all edits in the current draft; stale controls
from the previously selected type are refused.

Editor assistance gates this plugin's checks/quick fixes, completion, navigation,
registered directory markers, filename-aware semantic tokens and mutation paths
that append templates or literal entries to that ignore file. Checks can be turned
off independently while keeping other assistance. Disabling assistance/checks
clears tracked manual diagnostics on Save (up to256 retained paths); other results
update on the next editor/provider request. Status matching and explicit Git
observations retain their separate global controls and historical/source semantics.
These are exact-filename controls; unlisted/custom filenames retain their defaults.

Create choices are filtered by type and checked again during resolution and after
confirmation. Turning creation off does not disable editing an existing file, and
turning assistance off does not forbid independently enabled creation. The owned
Create action now supports all42 catalog filenames, including additional
`dotignore` files; generic scaffold content remains a comment except the existing
.gitignore/Mercurial conventions. Static native New-menu file templates stay
registered and are not hidden or unregistered by these controls.

The existing four workflow flags retain **Inherit / On / Off**. A fifth shared
colouring switch controls lexical token output for the package's additional
`dotignore` language. API1.3's syntax request has language/text but no filename,
so honest per-file lexical disablement is unavailable: the implementation never
guesses from active editor or equal text. The shared switch affects that whole
language family; built-in IDE grammars remain unchanged. Filename-aware semantic
tokens can be suppressed independently, but the pinned visible-colour renderer
gate described under P08.2 remains. No dynamic language/grammar registration or
static New-menu enablement is claimed.

Drafts require Save. Scoped controls reject stale/other-session events, invalid
values block saving, and Cancel/Reload discard edits. Save compares both preference
keys plus all five scalar baselines before one bounded16KiB write. New controls
use a separate version2 key; valid legacy v1 data is read only when v2 is absent,
then copied into a draft without rewriting it. First Save writes v2 only and leaves
legacy bytes intact for frozen98/97 builds. Later legacy-only changes do not override
an accepted v2 model, but intervening changes to either key refuse a pending Save.
Malformed v2 is preserved and blocks draft/save rather than falling back to legacy;
creation/append mutation paths refuse unavailable preference evidence. Existing
readonly/scalar fallback behavior remains. Storage has no final atomic CAS.

Original4.5.7 IgnoreSettings/IgnoreSettingsPanel establishes application persistence
and per-language ENABLE/NEW_FILE options. The current controls apply those concepts
to owned functionality where supported. Native runtime language/highlight ownership,
static New-menu gating, every original option and visual Save/Cancel/cross-window
acceptance remain separate. Cached original screenshots14958/14959 show project/editor
journeys, not this settings form; they do not prove matching visual parity. No
proprietary bodies/messages were copied.


## Optional Git observations for coverage review (partial P08.3)

**Ignore: Start Git Ignored-File Observations (Asks First)…** is an explicit opt-in
short background collection task. It needs optional `process:git` and `vcs.read`.
It verifies the project's native repository root and normal `.git` directory,
then previews exact project/CWD, HEAD, Git argv, added environment and limits before
starting one owned managed child with stdin closed. A source/repository check after
confirmation refuses changed scope. Unix project/repository roots up to 512 bytes
are supported; worktrees, submodules, bare repositories, subdirectory projects and
Windows roots remain unsupported. No ancestor/default working directory is used.

The builtin read-only request is `git --no-pager --no-optional-locks -C ROOT
--git-dir=ROOT/.git --work-tree=ROOT -c core.fsmonitor=false -c
core.untrackedCache=false -c core.excludesFile=/dev/null ls-files --others --ignored
--exclude-standard --full-name -z --`. Explicit safe Git directory/worktree/index/
common-directory/object/config/trace environment values are additive: the pinned
host refuses an inherited name collision, including an already identical value,
rather than silently replacing unsafe inherited state. System/global configuration
and global excludes are disabled; this is repository-ignore observation, not the
user's complete global Git ignore policy. No shell, alias, external helper,
fsmonitor, pager, git write command or network operation is requested. The user
still grants execution of the host-resolved Git program; this is not a process
sandbox or binary-authentication feature.

The managed child belongs to the explicit command/native progress task. Native
progress cancellation is checked between bounded reads; cancellation, deadline,
errors and source/project changes kill/drop the owned child. At most 40 collection
rounds use one native wait of up to 100 ms plus at most three nonblocking polls per
round, with a four-second cooperative deadline. Source/API/dispatch delays can
extend wall time; this is not a hard OS execution timeout. No durable watcher,
automatic retry, restart or scheduled collection is installed. The pinned manifest
minimum schedule is 60 seconds, so a fake rapid background schedule is not used.
Provider requests and cached rendering never start, wait on or poll a process.

**Review Cached Partial Git Observations** shows status, at most 256 observed paths
and up to 128 current root-`.gitignore` literal-rule sample counts. Counts use the
existing lexical matcher, not proof of which rule caused Git's decision. A count
of zero means **unknown**, never an unused/unmatched rule warning. Git reads disk
ignore files; unsaved editor rules are not applied to the command. The review may
compare those historical observations to current editor text, explicitly as sample
matches. Existing duplicate/redundant-pattern inspections, ignored-editing status,
preferences and atomic FileActions retain their separate source semantics.

States are Not scanned, Running (native progress), Partial and Stale. **Complete is
unavailable on this pinned transport.** `Proc` retains only the last 1 MiB/stream
without a loss flag, and exposes exit status independently from reader-thread EOF.
Successful exit and an empty poll therefore cannot prove complete output. To avoid
inventing paths across silent loss, each poll discards its first NUL fragment and
trailing unterminated fragment; only intact interior records become observations.
This deliberately loses genuine records too: a single short ignored path can yield
no samples, and the first path in every poll is omitted. No result is an ignored-file
total, and these observations never feed absence diagnostics or native file status.

Combined output is limited to 512 KiB, 2,048 unique paths, 64 KiB/stream/poll,
1,024 bytes/path, and UTF-8 without replacement/control/path-traversal forms. On a
bound, valid earlier samples can be retained as Partial if no prior sample set
exists; failed replacement otherwise retains earlier samples as Stale. Native
repository/head/index/root-rule/exclude changes are checked during collection;
file/VCS events mark matching cached roots stale. Explicit Review now revalidates
a successful capture's root/repository/HEAD/index/root-ignore/exclude proof and
marks changed or unavailable proof Stale while retaining historical positive
samples. It performs native VCS/source reads only in that explicit command; those
imports may perform host repository work, but Review starts no plugin Git child
and collects no new ignored paths. Rendering and summary remain cache-only.
Unobserved nested ignore changes and metadata timestamp granularity still prevent
any completeness or effective-policy freshness claim.
Caches are session-only, up to eight exact project roots, stale after 120 seconds;
reload/reclaim forgets them. **Clear This Project's Cached Git Observations** affects
only that root's transient cache and no preferences or files. There is no anonymous
or cross-project fallback.

Original evidence: `IgnoreManager` combines `ChangeListManager.isIgnoredFile` with
indexed ignore-language rules and an expiring status cache; `IgnoreFileStatusProvider`
asks that project service. The existing local implementation evaluates project
`.git/info/exclude` and ancestor `.gitignore` text. This new opt-in source supplements
that model with positive Git-output samples, without claiming original native VCS
status/complete filesystem coverage. Primary pinned host receipts:
`ide-host/src/plugins/api.rs` (`MAX_PENDING`, `Proc::read/chunk`,
`append_process_env`), `ide-extension/src/manifest.rs` (schedule minimum), and SDK
`host::Child::{wait,poll,kill}`. Deterministic fake-child and SDK MockHost tests use
no real Git, tracker, network or user repository. Native permission/environment,
progress cancellation and process-lifetime acceptance remain separate checks.


## Directory markers and built-in semantic tokens (P08.2 partial)

Glob entries explicitly ending in `/` now get a package-owned folder gutter SVG,
with light/dark variants declared through API 1.3 `markerIcons`. Negated directory
patterns get a re-inclusion tooltip. These markers describe **source syntax**;
they do not assert that a directory exists or navigate to a guessed path. Blank,
comment, empty/root-only, regexp, escaped/bracketed or overlong patterns are quiet.
Mercurial mode switches are tracked; invalid switches suppress further pattern
markers until a known switch. Includes are not traversed. No filesystem, inventory,
process, network or project discovery runs from these decoration providers.

The built-in `ignore` language receives semantic tokens for comments (headers and
sections use Documentation), glob operators, character classes, escaped literal
characters, Mercurial regexp entries and known syntax/include directives. Kinds are
honest Comment/Operator/Regexp/String/Keyword; paths are never labelled as types,
properties or variables just to obtain a colour. Filename/language ownership is
checked against the pinned built-in ignore file family. Package `dotignore` keeps
its existing lexical highlighter and receives directory markers, with no semantic
overlay. Generic file icons remain unchanged.

**Visible built-in semantic colouring is host-renderer blocked at pinned v0.1.39,
not delivered visual parity.** The SDK/host accepts these token kinds, but
`src/editor/lsp/semantic/index.ts` in upstream commit
`25424465c04c9221a92c02227c905911641d29e3` maps `TYPE_CLASS` at lines 34–63 without
comment/keyword/string/regexp/operator entries. `MODIFIER_CLASS` at lines 65–79
also lacks Documentation. Lines 146–165 derive no CSS class for these tokens and
`if (!classes.length && !local) continue` discards them. No ignore language-specific
semantic hook or identifier rainbow case supplies a class. This delivery preserves
the correct backend contract and the existing fallback rather than inventing
semantic ownership. Completing colour parity needs that renderer mapping/theme
path to support these lexical kinds.

Original comparison: `IgnoreDirectoryMarkerProvider.getLineMarkerInfo` first uses
`IgnoreEntryDirectory`, otherwise resolves a glob against the virtual filesystem
and caches directory existence. Our marker subset corresponds to explicit directory
syntax only; discovered directories without `/` remain pending a scoped cached
filesystem model. `IgnoreHighlighter` assigns separate comment/header/section,
negation/bracket/slash/syntax/value attributes; the existing `dotignore` lexical
fallback retains that richer categorisation where supported. SDK package marker
assets are validated/sanitized by pinned
`crates/ide-host/src/plugins/ui_points.rs::marker_icon` and rendered through the
native plugin marker gutter. Native light/dark/tooltip acceptance remains separate.

Limits: 128 KiB/document, first 4,096 lines, 1,024 bytes per directory pattern,
256 markers and 4,096 semantic tokens. All spans are nonempty, sorted UTF-16 ranges;
conversion walks each line once. Unsaved source replaces output immediately. The
providers do not modify checks/preferences or infer ignored-file coverage.
Regression fixtures cover marker meaning, negation, Mercurial/CRLF state, supported
filename ownership, Unicode ranges, honest token kinds, unsaved replacements,
denied unrelated grants and size/output limits. Background Git inventory and atomic
FileActions retain their prior tests and behavior.


The packaged Templates pane receives model-free invalidation after explicit Git
inventory start/completion/failure/clear and actual repository/file stale-state
changes. Each page requests its own current project model; no cached path/model
payload is broadcast. Repeated unchanged invalidation events stay quiet, and
render/summary never collects, polls or sends another invalidation. A busy short
collection may coalesce intermediate updates; no durable watcher or guaranteed
intermediate Running display is claimed. Native refresh remains alongside the
packaged notification. Immutable packaged-asset revisioning is separate.

The existing own-root VCS event guard already retires its child; this follow-up
adds notification of that established state transition, not a new scope fix.
Positive observations remain historical/ALWAYS Partial, with no transport EOF,
complete inventory, effective Git status or absence warning inferred. Existing
exact command/grant/cancellation/output bounds are unchanged. Earlier Desktop48
API14 decoration acceptance is dated for508c; this canonical SDK13 refresh change
and its rebased opt-in candidate require their own native acceptance.


## Canonical SDK13 provenance/settings follow-up (98 tests)

Successful-exit Partial Git snapshots now retain their capture scope; bounded/error
samples have no successful capture proof. The read-only Review adds an explicit
historical-provenance row and rechecks successful scope even when a file/VCS event
was missed. A changed/unavailable proof retires the cached state to Stale without
dropping positive samples. Repeated stale review stays quiet. Explicit inventory
commands require one current open normalized Unix root; anonymous, root-directory,
trailing/double-slash, backslash or dot-segment roots are refused. No review can
turn Partial into Complete, generate an absence warning, collect paths or publish
effective ignored-file decorations.

Structured application-global preferences now reject duplicate decoded JSON object
keys, including escaped duplicates. Invalid/unknown/ambiguous data is preserved and
the form refuses to create a draft or save over it. Valid escaped keys decode without
rewriting stored evidence merely by opening the form. Existing scalar/default
fallback behavior on unreadable preferences remains; ambiguous values are not
treated as accepted overrides. The bounded parser is shared original repository
code, with a cached serde dependency, and adds no SDK/API migration.

Canonical source remains v0.1.39/API1.3. Frozen opt-in API14 Ignore97 and its dated
native/probe receipts remain separate. API14 catalog/overlay/locks were not changed
by this follow-up. Current canonical input changes intentionally fail the old
staging catalog's freshness gate until a separately reviewed rebase; no old97
Wasm or Desktop48 screenshot is relabelled as acceptance of this canonical98.
Pinned SDK13 semantic-colour renderer limitations above remain unchanged.


## Canonical109 dialect-control follow-up

109 tests cover the98 baseline plus eleven new control/migration/scope fixtures.
A stored per-type disable still emitted directory markers in the pre-control98
source; the retained before-gating test now passes through actual registered output.
Backend tests also cover checks/completion/navigation/tokens, stale append/entry/Create
resolution, independent flags, filtered creation including additional filenames,
shared-language syntax without active-editor inference, compact selection/stale
fields, legacy preservation, dual-key/scalar conflicts and malformed-v2 refusal and bounds on both stored baselines.
Three older semantic fixtures now install MockHost because registered requests read
preferences; pure lexer/marker helpers remain source-only.

The opt-in API14 overlay/catalog/locks/frozen97 package remain unchanged and dated.
A future coordinated rebase must include these controls and v2 storage behavior;
no old97 acceptance or original screenshot is upgraded to this canonical109. The
current package stays on SDKv0.1.39/API1.3 and runs no real Git/network/native UI in
validation. Native colouring/menu registration and Apply/Undo acceptance remain
outside the backend/packaging proof.

Literal directory markers: after an ignore file opens or saves, up to64 safe literal paths beneath its folder are checked against project directories. Matching lines can show the directory icon for up to30seconds using the exact cached source, alongside slash-directory syntax markers. File, focus, VCS and settings events withdraw cached observations; edits and unavailable file subscriptions suppress them. No filesystem reads happen while rendering these cached markers. Wildcards, regex, escaped paths and foreign project owners are excluded. Directory admission uses host-confined stat results; the host may follow links within its root, so physical path identity is not established; this is a positive historical match, not complete directory discovery.

Ignore115 event retention follow-up: cached literal-directory observations survive irrelevant focus/status/other-file events and duplicate save→filesystem watcher echoes only after bounded current source/known-directory/innermost-owner proof. Matching source/target changes, unavailable proof, expired TTL, disabled support, oversized event batches and unknown future events still discard the sample. Watcher echoes preserve the original30s observation time; providers perform no filesystem work. API1.3 has no typed SettingsChanged support, so no new producer/native preference-invalidation claim is made. Pinned host delivery splits mixed events into ordinary callbacks before FilesChanged; public MockHost sequence reproduces112's loss, while the exact Native75 runtime order was not instrumented. NoAPI14 overlays/catalog/source were written.
