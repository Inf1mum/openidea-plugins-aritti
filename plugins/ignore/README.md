# .ignore

Support for ignore files (.gitignore, .dockerignore, .npmignore, .hgignore and many more) in OpenIDEA. MIT; see `LICENSE`.

## Checks

Problems are checked when an ignore file is opened or saved:

- Duplicate entries.
- Redundant entries (warning). The check compares patterns against each other, never against files on disk (ignored files cannot be listed). Only entries that name one path, or a glob inside a literal directory (`build/*.o` under `build/`), are checked.
- Relative and malformed entries, and invalid `syntax:` values in .hgignore.

Quick fixes: remove the entry, remove the relative part, switch to glob or regexp syntax.

The lexical matcher follows Git's whitespace rules: leading spaces and escaped trailing spaces are part of the pattern; unescaped trailing spaces are ignored, and duplicate/redundancy checks use the same spelling policy for glob dialects. A child negation cannot restore a file beneath an excluded directory (every parent must first be included, as in Git), and nested ignore files cannot override an excluded ancestor.

## Completion and navigation

- Path completion in glob-syntax ignore files (.gitignore, .dockerignore…): files and folders below the ignore file's folder, after `/` and `!` too. Dot-paths and ignored files are hidden by the IDE's file search, so those are not offered.
- `syntax:` completion in .hgignore.
- Ctrl-click on an entry that names one file opens it.

## Quick fixes and intentions

- **Add … to .gitignore** (and an unignore variant with `!`) in any project file, when the project root has a .gitignore. These are non-overwriting, undoable FileActions: literal paths escape wildcard, bracket, backslash and space characters; newline/control and traversal paths are refused. The target's existing LF/CRLF and current unsaved text are preserved, the source path/version/project is bound, and target content is rechecked after confirmation; each buffer/addition is limited to 256 KiB. Unignore entries can still require unignoring a parent directory; the plugin does not invent those rules. No program or clipboard permission is required.
- **Create exact-name ignore file…** (from any project editor): choose one of the 20 supported names and an existing project-relative directory (empty means the root), confirm the exact destination and initial content, and get a non-overwriting, undoable FileAction that opens the new file. Existing files/buffers, outside paths, traversal and invalid directories are refused.
- **Create ignore file from templates…**: an atomic intention combining up to 16 bundled/user templates into one new glob-dialect ignore file; the exact combined content and destination are reviewed first. Dismissal, duplicate or unknown selections, existing files/buffers and changed source context return no operation. Regexp dialects (including `.hgignore`) are excluded from template workflows.
- **Append ignore template…** (glob-dialect files): select up to 16 bundled/user templates and confirm the exact insertion; existing template headers are refused instead of silently duplicating a block. Template pickers list the up-to-16 most recently chosen templates first (stored locally, bounds-checked).

_New Ignore File_ (Project view, Tools) points to native New and the **Create exact-name ignore file…** intention; it is guidance, not a second file writer. _Unignore Selected Paths_ and _Add Selected Paths_ keep their version-checked editor workflow and also appear in the editor menu. Adding an entry to a file with CRLF line endings keeps them CRLF.

Native New offers exact dotfile names for all 20 supported creation types (`.gitignore`, `.dockerignore`, `.npmignore`, `.hgignore`…) using the `fileName` field rather than an appended extension. The Git template keeps the bundled starter; other types are comment-only starters (`.hgignore` starts in glob mode).

## Ignore Templates tool window

The github/gitignore templates and your own: _Create User Template_ (a native input modal prefilled with the focused filename; empty or duplicate names are rejected, and the document/version/editor identity is rechecked before saving) saves the focused file. Double-click a template to add it to the open ignore file. Star a template with _Star_; starred templates are kept in storage and listed in a _Starred_ group. Deleting a user template uses a native confirmation modal and rechecks the selected template and its stored content first; Cancel changes nothing. A right-aligned Preferences gear opens the preferences draft in the same window, with Back to Templates; the Tools Preferences command opens the same form.

## Status bar, warnings and highlighting

- _Ignored_ in the status bar while the focused file is ignored, according to `.git/info/exclude` and every `.gitignore` from the root down to the file.
- A warning, once per project, for a Git repository without a .gitignore.
- The `dotignore` language, with highlighting, for ignore files the IDE doesn't detect (.chefignore, .p4ignore, .vercelignore…). The IDE's own `ignore` language receives honest semantic tokens (comments — headers and sections use Documentation — glob operators, character classes, escaped literals, Mercurial regexp entries, and known syntax/include directives, with Comment/Operator/Regexp/String/Keyword kinds; paths are never mislabelled just to obtain a colour). Visible colouring of the built-in language is currently blocked by the pinned host renderer, which maps no CSS class for these token kinds; the backend contract and the `dotignore` lexical fallback remain in place.

Settings › Plugins › .ignore: turn off the warning, the status or the unignore variant of the quick fix (`unignoreActions`), and insert templates at the caret.

## Dialect Checks and Preferences

**Tools → Ignore: Dialect Checks and Preferences** opens an application-global form without requiring a project. One exact-filename selector covers 42 shipped/built-in file names; each selected type has independent **Editor assistance**, **Allow Create action** and **Checks** controls, and switching types retains the current draft (stale controls from the previously selected type are refused).

Editor assistance gates this plugin's checks/quick fixes, completion, navigation, registered directory markers, filename-aware semantic tokens and mutation paths that append templates or literal entries to that ignore file; checks can be turned off independently while keeping other assistance. Disabling assistance/checks clears tracked manual diagnostics on Save (up to 256 retained paths); other results update on the next editor/provider request. These are exact-filename controls: unlisted/custom filenames keep their defaults. Turning creation off does not disable editing an existing file, and turning assistance off does not forbid independently enabled creation; the Create action supports all 42 catalog filenames, including additional `dotignore` files. Static native New-menu file templates stay registered and are not hidden by these controls. Create choices are filtered by type and checked again during resolution and after confirmation.

The four workflow flags retain **Inherit / On / Off**; a fifth shared colouring switch controls lexical token output for the `dotignore` language family (per-file lexical disablement is unavailable because the syntax request carries language/text but no filename; the switch never guesses from the active editor or equal text). Filename-aware semantic tokens can be suppressed independently.

Drafts require Save; invalid values block saving and Cancel/Reload discard edits. One bounded 16 KiB write compares both preference keys plus all five scalar baselines. Controls use a separate version-2 storage key; valid legacy v1 data is read only when v2 is absent and copied into a draft without rewriting it. Intervening changes to either key refuse a pending Save; malformed v2 data is preserved and blocks draft/save rather than falling back to legacy, and duplicate decoded JSON object keys (including escaped duplicates) are refused and preserved. Creation/append mutation paths refuse unavailable preference evidence.

## Directory markers

Glob entries explicitly ending in `/` get a package-owned folder gutter icon (light/dark variants), and negated directory patterns get a re-inclusion tooltip. These markers describe source syntax; they do not assert that a directory exists or navigate to a guessed path. Blank, comment, empty/root-only, regexp, escaped/bracketed or overlong patterns are quiet; Mercurial mode switches are tracked, and invalid switches suppress further pattern markers until a known switch. Includes are not traversed, and no filesystem, inventory, process, network or project discovery runs from the decoration providers.

After an ignore file opens or saves, up to 64 safe literal paths beneath its folder are checked against project directories: matching lines can show the directory icon for up to 30 seconds using the exact cached source, alongside the slash-directory syntax markers. These cached observations survive irrelevant focus/status/other-file events and duplicate save→filesystem-watcher echoes only after bounded current-source/known-directory/innermost-owner proof; matching source/target changes, unavailable proof, expired TTL, disabled support, oversized event batches and unknown future events still discard the sample. Watcher echoes preserve the established 30-second observation time. No filesystem reads happen while rendering; wildcards, regex, escaped paths and foreign project owners are excluded. Directory admission uses host-confined stat results (the host may follow links within its root, so physical path identity is not established); this is a positive historical match, not complete directory discovery.

Limits: 128 KiB per document, the first 4,096 lines, 1,024 bytes per directory pattern, 256 markers and 4,096 semantic tokens. All spans are nonempty, sorted UTF-16 ranges; unsaved source replaces output immediately. The providers do not modify checks/preferences or infer ignored-file coverage.

## Optional Git observations

**Ignore: Start Git Ignored-File Observations (Asks First)…** is an explicit opt-in short background collection task (optional `process:git` and `vcs.read`). It verifies the project's native repository root and normal `.git` directory, then previews the exact project/CWD, HEAD, Git argv, added environment and limits before starting one owned managed child with stdin closed; a source/repository check after confirmation refuses a changed scope. Unix project/repository roots up to 512 bytes are supported; worktrees, submodules, bare repositories, subdirectory projects and Windows roots are not. No ancestor/default working directory is used.

The builtin read-only request is `git --no-pager --no-optional-locks -C ROOT --git-dir=ROOT/.git --work-tree=ROOT -c core.fsmonitor=false -c core.untrackedCache=false -c core.excludesFile=/dev/null ls-files --others --ignored --exclude-standard --full-name -z --`. Explicit safe Git directory/worktree/index/common-directory/object/config/trace environment values are additive; an inherited name collision is refused rather than silently replaced. System/global configuration and global excludes are disabled: this is repository-ignore observation, not the user's complete global Git ignore policy. No shell, alias, external helper, fsmonitor, pager, git write command or network operation is requested; the user still grants execution of the host-resolved Git program.

The managed child belongs to the explicit command/native progress task. Native progress cancellation is checked between bounded reads; cancellation, deadline, errors and source/project changes kill/drop the owned child. At most 40 collection rounds use one native wait of up to 100 ms plus at most three nonblocking polls per round, with a four-second cooperative deadline (source/API/dispatch delays can extend wall time; this is not a hard OS execution timeout). No durable watcher, automatic retry, restart or scheduled collection is installed; provider requests and cached rendering never start, wait on or poll a process.

Because the process transport retains only the last 1 MiB per stream without a loss flag and cannot prove complete output, each poll discards its first NUL fragment and trailing unterminated fragment; only intact interior records become observations. A single short ignored path can therefore yield no samples, and the first path in every poll is omitted — results are always at most **Partial**, never an ignored-file total, and never feed absence diagnostics or native file status. Combined output is limited to 512 KiB, 2,048 unique paths, 64 KiB per stream/poll, 1,024 bytes per path, and UTF-8 without replacement/control/path-traversal forms; on a bound, valid earlier samples can be retained as Partial or Stale. Native repository/head/index/root-rule/exclude changes are checked during collection, and file/VCS events mark matching cached roots stale.

**Ignore: Review Cached Partial Git Observations** shows status, at most 256 observed paths and up to 128 current root-`.gitignore` literal-rule sample counts (counts use the lexical matcher, not proof of which rule caused Git's decision; zero means **unknown**, never an unused-rule warning; Git reads disk ignore files, so unsaved editor rules are not applied). Successful-exit Partial snapshots retain their capture scope; the review adds a historical-provenance row and rechecks that successful scope even when a file/VCS event was missed, retiring a changed or unavailable proof to Stale while retaining positive samples. It starts no plugin Git child and collects no new paths; rendering and summary are cache-only. States are Not scanned, Running, Partial and Stale. Caches are session-only, up to eight exact project roots, stale after 120 seconds; reload/reclaim forgets them. **Ignore: Clear This Project's Cached Git Observations** clears only that root's transient cache, no preferences or files. Explicit inventory commands require one current open normalized Unix root; anonymous, root-directory, trailing/double-slash, backslash or dot-segment roots are refused. No review can turn Partial into Complete, generate an absence warning, collect paths or publish effective ignored-file decorations.

**Ignore: Publish Cached Partial Git Sample Hints (API 1.4)** decorates at most 16 existing owned files with a muted I badge and a historical tooltip, from a successful Partial capture younger than 120 seconds that passes current scope checks; it starts no new Git child. **Ignore: Clear Partial Git Sample Hints** and observed source/project/preferences/lifecycle events withdraw the owned set (the host supplies no automatic decoration expiry).

## Unused entries from a complete directory snapshot

**Ignore: Find Unused Entries from Complete Directory Snapshot…** explicitly reviews and runs an optional `python3` helper in isolated mode (`-I -S`). It scans the focused `.gitignore` directory twice, including hidden/ignored paths and Git metadata. It follows no symlinks and refuses errors, unsupported filesystem names or kinds, concurrent changes and limits (4,096 entries, 512 directories, depth 24, 8 seconds, 1 MiB result). The helper writes its final complete result atomically into plugin-private storage, so managed stdout loss cannot turn a partial scan into a complete inventory; it imports no project application and changes no project file.

For admitted complete inventories, unmatched entries appear as native snapshot hints and offer an exact reviewed, undoable removal action. ASCII glob matching conservatively ignores case to avoid false absence across case-sensitive and case-insensitive filesystems; non-ASCII literal/glob patterns are evaluated by exact bytes, misses are trusted only when no inventory path is non-ASCII, and non-ASCII patterns with `?` or `[` stay withheld. Unsupported pattern forms are counted and withheld. This checks whether names match any existing path, not whether a rule is currently Git's winning rule or whether future generated files will need it. Providers use only source/version/owner-bound cached results, never filesystem scans or processes; eligibility lasts 120 seconds and observed source/filesystem changes withdraw it. Results describe capture time, so rerun to refresh.

## How it works

`templates/` is [github/gitignore](https://github.com/github/gitignore) at `53cf2971` (CC0-1.0, see `templates/LICENSE`), embedded in the component by `build.rs`. The plugin registers syntax, providers, views and FileActions through the plugin API. Required capabilities: `project.read`, `editor.read`, `editor.write`, `events`, `storage`; optional: `clipboard`, `process:git`, `process:python3`, `vcs.read`. Degraded optional grants simply disable the corresponding opt-in commands.

Native-intention creation/append holds no filesystem lock after resolution; final application, encoding/BOM preservation, conflict detection and undo are owned by the host.

## Not implemented

- Visible built-in `ignore` semantic colouring: the pinned host renderer maps no CSS class for comment/keyword/string/regexp/operator token kinds, so honest tokens are emitted but not coloured.
- Complete ignored-file inventories or effective Git policy from the observation commands (positive samples only, always Partial).
- Template insertion into regexp-dialect files (including `.hgignore`).
- Directory markers for patterns not literally ending in `/` (discovered directories without `/` have no cached filesystem model).
- A visual generator dialog matching other IDEs' ignore support.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
