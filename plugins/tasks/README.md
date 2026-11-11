# Tasks

Port of the IntelliJ **Issue Trackers** plugin (`com.intellij.tasks` 263.7307.108, intellij-community tag
`idea/263.7307.112`: `plugins/tasks`, `platform/tasks-platform-impl`, the GitHub tracker from `plugins/github`
and `GitTaskHandler` from `plugins/git4idea`). Apache-2.0, © JetBrains s.r.o. (see `LICENSE`, `NOTICE`).

- **Native Tasks** tool window: grouped task/closed/issue trees with counts, active-task and elapsed-time details. Open Task uses a native searchable picker; Search Issues keeps server queries separate. Task Actions offers description, edit, switch, close, browser and work-time actions for the selection; Contexts offers save/load/remove. The panel has no packaged web frame.
  IntelliJ's dialogs become the New, Open and Close forms, which remember their choices.
- **Git** (`process:git`): each new task gets a branch (`branchNameFormat`), checked out when you switch to it.
  Changes are shelved as stashes. Closing a task can merge its branch into the base branch and delete it. The commit
  message can also go to `commit.template`.
- **Context** (`events`): switching back to a task reopens the last 10 files opened while it was active.
- Status bar: the active task. Click it to copy its commit message (`commitMessageFormat`, default `{id} {summary}`).
- Commands: Close Active Task (⌥⇧W, with the Close form's last choices), Open Active Task in Browser (⌥⇧B),
  Copy Commit Message, Update Issues.
- Trackers: GitHub, gitlab.com, codeberg.org, YouTrack Cloud, Jira Cloud, Trello and Lighthouse Cloud. Each one is used once its token
  is set in Settings › Plugins › Tasks, and issues are updated every 20 minutes. Closing a task can resolve GitHub,
  GitLab, Codeberg, YouTrack and Jira issues; work time can be posted to YouTrack and Jira.

## Search Issues

**Tasks: Search Issues** asks for a query, then a configured server/repository when more than
one is available. Each request searches that selection only. GitHub, gitlab.com and Codeberg
use one repository at a time (GitLab also supports its existing all-visible-issues selection);
YouTrack/Jira use a configured Cloud tenant, and Trello uses the configured account. Unsupported
or missing Cloud hosts are excluded. Repository paths, returned issue hosts and issue API
endpoints are validated before a result can be selected.

The picker offers **Next page**, **Previous page**, **Retry this page from server**, **New search
query** and, when relevant, **Choose another server / repository**. Empty results and request
errors have separate states. Retry is explicit; there is no automatic retry or rate-limit loop.
Cancel preserves the active task and its context. Picking a hit keeps the existing Open Task
branch review, or activates it directly when Git integration is off. An existing task with the
same ID can be reused only when tracker and issue API identity match.

GitHub/GitLab/Codeberg use `page`; YouTrack uses `$skip`/`$top`. Each page contains at most 50
issues, with a five-page limit per query/selection. Next is offered after a full page, so the
following page may be empty; results are not claimed to be exhaustive. Jira enhanced-JQL
search follows its opaque `nextPageToken` rather than offsets or full-page guesses,
including short/empty pages with a continuation. Tokens are session-only, bounded
to4096bytes and query/tenant/root/page scoped; malformed/repeated/cyclic or
inconsistent terminal metadata is refused without caching that page. Previous
uses the exact page/cursor cache; query/server changes reset the cursor, and
Retry invalidates that query/selection lineage. Selection rechecks live cached
membership/expiry. Trello query search uses zero-based `cards_page` with the same
50-card/five-page picker bound. It does not claim pagination of the separate
member-card Refresh listing; only an explicit search query can use these pages.
The tracker protocol references are [GitHub pagination](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api),
[GitLab pagination](https://docs.gitlab.com/api/rest/#pagination),
[Forgejo pagination](https://forgejo.org/docs/latest/user/api/usage/), and
[YouTrack pagination](https://www.jetbrains.com/help/youtrack/devportal/api-concept-pagination.html), and
[Jira enhanced JQL search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/#api-rest-api-3-search-jql-get), and
[Trello search](https://developer.atlassian.com/cloud/trello/rest/api-group-search/).
Jira search errors show a bounded constant reason, without raw server bodies or
continuation tokens. Five pages is a deliberate partial-results bound, not complete
tracker search or snapshot consistency. No provider/render fetch is introduced.

Previous pages use a memory cache keyed by project, relevant server/filter configuration,
repository, query and page. Cached pages are labelled, expire after 120 seconds, and the cache
holds at most 20 pages / 1000 issues. Retry invalidates that query/selection's pages. A new
Search Issues invocation clears the cache, and caches are never persisted. The host exposes
secret presence, not account/credential identity: a same-name credential rotation while a picker
is open cannot be detected; Retry or restart search after changing accounts. Project/configuration
membership is checked after each request and prompt, including immediately before accepting a hit.
Queries are limited to 256 bytes, selections to 16, picker actions to 20 per invocation, and tracker
response bodies to 1 MiB. Network timeouts/cancellation remain the host's responsibility.

Original comparison: `TaskSearchSupport.getRepositoriesTasks` (pinned Issue Trackers source /
decompiled `tasks-core/.../actions/TaskSearchSupport.java`) distinguishes local/cache hits from
server requests and accepts offset/limit/force-request. This slice adapts that separation to native
input/pick controls; it does not add the original live choose-by-name popup. Closed GitHub/GitLab/Codeberg search is supported through `searchClosedIssues`; see below.
No live tracker requests or desktop acceptance are claimed by the deterministic tests.

## Permissions

| Permission | Used for | Without it |
|---|---|---|
| `project.read`, `storage` (required) | Read the project, keep task contexts and settings | The plugin does not load |
| `secrets` (optional) | Store each tracker's API token (`github-token`, `gitlab-token`, `forgejo-token`, `youtrack-token` and so on) in the IDE's secrets store. The plugin never logs a token; the host puts it in the request header | Trackers that need a token cannot be used |
| `net:api.github.com`, `net:gitlab.com`, `net:codeberg.org`, `net:*.youtrack.cloud`, `net:*.atlassian.net`, `net:api.trello.com` (optional) | Fetch and update issues on the tracker you configured. Only the hosts of the trackers you turn on are contacted | That tracker is unavailable; the others still work |
| `process:git` (optional) | A branch per task, shelving, merge on close, commit template | Tasks work without VCS integration |
| `vcs.read`, `events`, `clipboard` (optional) | Detect the project's remote, record the files of each task, Copy Commit Message | Those features are off |

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Apply task text to the current commit draft (API 1.3)

The explicit **Apply Active Task to Commit Message** command is also available above
Commit's message field. Optional `vcs.read` and `vcs.write` grants are required. It
uses the active task's configured message format and previews the current/replacement
text, task, project and draft revision. Cancel preserves the draft. Changed task text
or a changed commit revision refuses the write without retrying. It stages or commits
nothing. The host may save included dirty files when reading the pending draft, as
its Commit workflow does.

The API has no separate draft repository identity: this action requires included
changes, all within the task project. Empty drafts and mixed-project selection are
refused. This is message-field integration, not the original task-close automatic
commit workflow. Existing clipboard and commit.template flows remain available.

Original comparison: pinned `/tmp/port/11545/dec/tasks-core/.../VcsCloseTaskPanel.java`
exposes commit/merge choices and commits a task changelist; this implementation uses
the supported host draft API for an explicitly reviewed message operation only.
Native Cancel/Apply message-field acceptance previously passed; this state/callback
slice still needs separate desktop acceptance.

## Cached issue references (P28.3 supported subset)

After an explicit **Tasks: Search Issues**, hovering an exact known issue ID in
source shows its escaped summary, tracker and page URL, clearly labelled as a
cached search snapshot. The provider uses only validated in-memory search pages
and the document's verified project identity: no storage/file reads, source
searches, git commands, tracker requests or credential lookups occur on hover.
Exact browser/API/source identity proof supports GitHub, Codeberg, Jira Cloud and
YouTrack Cloud directly. GitLab additionally requires the search response
`references.full`, numeric project ID and IID to agree with the selected repository,
exact API path and browser path. Trello requires a24hex card ID plus an8character
shortLink agreeing with its response URL; contextual browser links use the canonical
`https://trello.com/c/<shortLink>` form. Missing/conflicting mappings remain quiet.
These typed reference facts are ephemeral and explicitly excluded from task
serialization/deserialization, so local/stored tasks cannot mint this authority.
Only this instance's last explicit search session is eligible; persisted/local
tasks and ordinary background refresh results are not inferred into this cache.

**Open Cached Issue at Caret in Browser** and **Activate Cached Issue at Caret
Locally (No VCS Changes)** are available from the editor context menu/command
palette with optional `editor.read`. They require one caret or one exact ID
selection, current source text/version, the same owning project, an unexpired
unambiguous search hit and unchanged server/repository/query configuration.
Local activation previews the exact root, issue, tracker, web/API identity and
scope, then rechecks ownership after confirmation. It updates local task/time
tracking only; it bypasses the existing git/remote-state/context-reopening flow.
Conflicting existing task identities are refused before review and before write.
Cancellation and failures preserve the current task. Opening uses the previously
validated HTTPS issue page, with no new plugin HTTP request.

The source document is limited to 512 KiB and the exact ASCII issue token to
128 bytes; UTF-16 ranges include only that token. Embedded Unicode identifiers,
unknown IDs, expired results and same-ID/different-server/repository collisions
are quiet. Search's existing 120-second lifetime, 20 cached pages, 50 hits/page
and five-page request budget remain unchanged. Duplicate observations of the same
identity coalesce. Starting a new explicit search discards old session metadata;
authenticated account identity is not available, so snapshots are never presented
as current authorization or live status. Explicit actions revalidate configuration;
hover labels historical facts rather than fetching for account/configuration changes.

Original comparison: decompiled `AbstractTaskSymbol`/`TaskSymbol` provide task
summary/description/state documentation and browser navigation; `OpenTaskDialog`
uses a task manager and VCS extension panels. The reviewed documentation screenshot
`managing-tasks-and-context-0.png` shows explicit server URL and query ownership.
This subset provides cached summaries and explicit actions, without the original
full documentation/state resolution. **Review Cached Issue Activation and Branch Options
at Caret** now opens the existing task/VCS options form for a not-yet-local cached
issue when `process:git` is enabled. Preparing or cancelling this form changes no
task state and runs no Git. Its retained ticket binds the exact source text/version,
project, issue/server/API identity, search expiry/configuration and branch-name
format; Open Task checks these again before importing/activating the issue or
running the existing opt-in Git workflow. The form shows the derived branch name;
users can disable branch creation or select shelving explicitly. Existing branches
are switched to; otherwise Git creates from HEAD. This is a command-palette entry,
not native inline links/gutter actions. Git failures retain the existing workflow's
warning/local-activation behavior; it is not a transaction across Git and task storage.
No new native branch API, tracker mutation, automatic branch creation or issue
reference scanning/network provider is claimed.


## Project task and recorded-context state (P28.4 source subset)

Task lists, active task/clock fields, choices, repository overrides and named file
contexts persist under the exact concrete project-root key. New writes use a
schema/owner/revision envelope; malformed/foreign-owner/unsupported records never
fall back silently to a new default task. Valid legacy records under that root's
existing key are validated (including owned file paths) and upgraded on the next
successful write. Empty task arrays, unknown active/default identity, duplicate IDs,
foreign/prefix-sibling/traversal file paths and unsupported sizes fail without
replacing the stored bytes. Storage denial does not create anonymous or volatile
replacement state. Different project roots retain separate task counters and data;
a renamed/moved root does not silently adopt another root's stored state.

**Reset This Project’s Task State (Asks First)…** is explicit recovery for corrupt
or unwanted records. It previews the exact project/key, byte count and deletion
scope, checks the original bytes again after confirmation, then replaces only that
key with a fresh owned default state. Cancel preserves it. Recovery changes no git,
tracker, source files or other project's key. There is no implicit reset/migration
when validation fails.

Read and write share a 2 MiB serialized bound; record limits are 512 tasks, 1,000
cached issues, 64 named contexts, ten recorded files per task/context and bounded
names/descriptions/repository overrides. A rejected oversized write retains the
last valid record. Writes recheck the original raw record before storage_set and
advance a checked revision; stale writers fail instead of retrying. The SDK has no
atomic storage compare-and-set, so a final read/write race remains. This is not a
transaction across tracker, git and plugin storage operations; existing explicit
optional git/tracker workflows retain their separate failure semantics.

Rendered tree keys, field keys and button IDs carry opaque project/snapshot/form
identity. Rendered data and its guard come from one storage read. Only an exact
local write receipt can advance a callback binding; a non-writing field/selection
callback cannot adopt an unrelated concurrent storage change. Backend task/issue
IDs are unchanged. Old native or packaged callbacks
cannot submit a previous project's form, target a replaced storage snapshot or
reuse a previous form's buttons after another project renders. Project-free,
denied and corrupt views discard their selection/form binding. UI drafts and
selection are reset across projects; only submitted state is persistent. Multiple
open projects/project-free contexts do not reuse a global active-task status from
whichever project happened to load first/last. Active status uses the API 1.4 window override from commands and focus events, so opening another project no longer hides the invoking window's active task; each view reports its own root's active task separately.

File-open events record only exact owning-project paths (including a longest-root
check for nested projects), never raw string-prefix siblings. Save/Remove Context
revalidate the source TaskList after prompts; reopening a saved context checks all
recorded paths against the current owner before opening any. Changing context
state during a picker or creating a nested project under recorded paths refuses
reopening. Reopen errors report how many files opened rather than silently claiming
restoration. Switching back to a task applies the same file-owner preflight.

Named contexts now capture **up to ten currently open project editors in the invoking window** using API 1.4. Saving requires optional `editor.read`; foreign/nested-project editors are excluded and more than ten owned tabs refuse rather than truncate. The first selection of the active editor is saved and restored only while its coordinates still fit the current file. Legacy named contexts and automatic task-switch file history remain file-only and reopen at line/column zero. Tab/split order, folds, dirty buffer contents, bookmarks and debugger breakpoints are not captured/restored. No editor is closed,
rewritten or silently saved by context reopening. Active clock fields survive
restart as elapsed wall-clock task time (including time while the task/IDE is
inactive), not measured focused coding time. No unavailable activity API is inferred.

Original audit: `tasks-core/META-INF/plugin.xml` describes editor tabs, bookmarks
and breakpoints as working context and registers `BreakpointsContextProvider`;
that decompiled provider serializes and reloads debugger state and can clear
breakpoints. This implementation persists owned file references only and performs
no debugger/bookmark restoration. Native commit-message integration, scoped issue
search/caret actions and existing task/git workflows remain separately implemented;
source tests cover restart, legacy migration, corruption, denial, limits, stale
writers/views/forms, project switches and nested-project context ownership.

## External packaged-view invalidation

External close/switch/reset commands and committed task/context/issue updates now
retain native refresh and send only `{"channel":"ui-kit-refresh"}` to the Tasks
packaged view. Its hub requests its own frame/project-correlated model. Even a
cancelled switch refreshes the action tokens rotated by that command. Own hub
requests keep their existing response path; stable direct and ui-kit renders emit
no further invalidation, and no tracker or git request was added to rendering.

Existing project-owned storage snapshots, schema/raw revision checks, task/server
identity comparisons and opaque per-render action generations remain authoritative.
A different project clears the previous form/selection; missing or revoked
project.read also clears cached form data before render fails. Other projects'
updates may notify the view, but the receiving page loads only its invoking
project and old source tokens stay invalid. HTTP capability/auth and explicit
network action contracts are unchanged. No atomic active-window, credential
revocation, original tracker-runtime or native multiwindow acceptance is implied;
validation uses mocks and pinned build/check only.


API 1.4 migration validation: source MockHost tests cover current-tab capture replacing closed-file history, active selection restoration, and status with multiple open projects. Exact native window routing and multi-tab UI acceptance remain to be exercised.

## Native UI parity checkpoint

The native tree, native search/action pickers and lazy native New/Open/Close forms preserve
the supported Issue Trackers workflow described in JetBrains’
[Manage tasks documentation](https://www.jetbrains.com/help/idea/managing-tasks-and-context.html):
local versus tracker tasks, explicit activation/branch review, closed-task grouping,
task descriptions, and a distinct context workflow. Task descriptions open read-only native
content tabs. Native toolbar actions retain project/storage/action identities, and obsolete
packaged messages are rejected rather than served alongside native controls.

This is supported interaction parity, not a pixel-for-pixel copy of IntelliJ's popup/dialogs.
Generic server connectors, changelists, automatic context-on-commit, complete layout/bookmark/
breakpoint restoration and original full time-tracking UI remain outside this panel.
Mock tests and release Wasm checks are separate from actual native light/dark, narrow-dock,
keyboard and multi-window acceptance. Native text fields commit on blur/Enter.

The native toolbar follows the official Tasks & Contexts flow: **New Task…**,
selection-aware **Open Task… / Switch Task / Reopen Task**, **Close Active Task…**,
**Search Issues…**, **Contexts…**, selected **Task Actions…**, and **Update Issues**.
Open operates directly on an existing row selection; it only opens a task picker when
no task is selected. The active task disables Open, while Close Active always reviews
the active task regardless of another row selection. Selected-task close remains explicitly
labelled in Task Actions. Open/Close forms show the reviewed task as a read-only value
above their branch/issue choices. This preserves the documented meaning of active versus
selected tasks without adding a second picker before an already-known task.

## Closed tracker issue search

Enable **Include closed GitHub, GitLab and Codeberg issues in Search Issues**
(`searchClosedIssues`) to include resolved issues in explicit server searches.
The default is off. Automatic Refresh/Update Issues remains open-only. Changing
this setting invalidates the active search ownership signature, so cached pages
cannot be reused under another closed-issue policy. YouTrack and Jira retain their
configured native query semantics; this setting does not rewrite JQL or YouTrack
query expressions. Selecting a closed server hit creates/activates a local task;
it does not reopen the remote issue.

## Issue descriptions

Tracker results retain plain descriptions from GitHub, GitLab, Codeberg,
YouTrack and Trello. Jira Cloud descriptions are flattened from Atlassian
Document Format to plaintext (text nodes, line breaks and paragraph boundaries);
formatting marks, links and embedded resources are not executed or fetched.
Descriptions are bounded to32KiB, with explicit truncation markers. Existing
Show Task Description tabs and cached issue hover use this data; hover escapes
Markdown and shows a bounded excerpt. Refresh updates task descriptions together
with summaries. These are cached server snapshots, not live status guarantees.

## Lighthouse Cloud

Configure `lighthouseHost` (`acme.lighthouseapp.com`), one positive numeric
`lighthouseProject` and the `lighthouse-token` secret. Grant the optional
`net:*.lighthouseapp.com` permission. The adapter sends the token only through
`X-LighthouseToken`, uses documented ticket XML endpoints and keeps every request
bound to that tenant/project. Explicit searches use the standard page controls;
automatic refresh is open-only. `searchClosedIssues` also includes resolved
Lighthouse tickets in explicit search.

IDs use `LH-<tenant>-<project>-<number>` to avoid confusing the same ticket number
in another tracker, project or account. Cached descriptions/hover and browser
navigation retain the exact ticket identity. Close Task can set the documented
`resolved` state; it rechecks the current tenant/project and exact API/web paths
before sending that update. Responses are capped at1MiB/50 tickets; malformed,
foreign-project and incomplete XML pages are refused, including DTDs. No external
XML entities are resolved and raw response error bodies are not logged.

Protocol references: [Lighthouse API basics](https://help.lighthouseapp.com/kb/api/api-basics)
and [ticket search/update](https://help.lighthouseapp.com/kb/api/tickets).
The public login endpoint was reachable on2026-11-10; implementation acceptance
uses deterministic fixtures and the actual released host capability matcher,
not an authenticated customer account. The support site's expired TLS certificate
was observed during documentation retrieval; runtime TLS verification is retained.

FogBugz Cloud is available in **Search Issues…** through an optional approved
Python3 transport. Configure `fogbugzHost` as an exact tenant such as
`acme.fogbugz.com` and `fogbugzProfile` as the absolute path of an externally
provisioned private JSON profile. The helper requires an owned regular0600 file
without symlinks and uses the official XML form-POST API. The token stays inside
the helper; native settings/storage receive only an opaque profile fingerprint,
issue number and latest-event identity. See [profile setup and transport
contract](adapters/README.md).

Each explicit search reviews the tenant/profile/query and admits at most50
validated results. The XML API has no supported offset; refine a larger query.
Startup and periodic refresh never launch the credential helper. FogBugz results
participate in the native task/branch/context workflow and cached source issue
references. Close Task offers remote state update only for a validated closable
issue. Its final review binds the tenant/issue/event/profile; the helper freshly
checks permissions and event identity, submits optimistic `ixBugEvent`, and
verifies closed state. TLS verification is mandatory and every redirect is
refused. A timed-out mutation requires checking the remote issue before retrying.
Live authenticated customer-tenant acceptance remains separate from the actual
local HTTP/XML fixture tests.

Self-hosted **Redmine** and **MantisBT** servers are available through the same
approved Python3 permission. Add up to16 nonsecret `selfHostedProfiles`
descriptors containing `family`, exact HTTPS `baseUrl` and absolute
`profilePath`. The external owned0600 profile contains the API key and optional
explicit close-status ID; no API key enters native settings/storage. Multiple
installations under different URL prefixes retain distinct task identities.
[Setup, supported query parameters and server contracts](adapters/SELF_HOSTED.md)
include the exact profile format.

The `generic` family covers other REST trackers through a user-declared JSON-pointer or
Java-regex contract: see [adapters/GENERIC.md](adapters/GENERIC.md).

Search Issues offers those servers with actual documented filter parameters and
up to five50-result pages. Redmine uses server status metadata and offset/limit;
Mantis requires a project and reads that project's status/workflow metadata.
Neither bridge invents free-text query support or closed-state semantics when
required metadata is unavailable. Startup/periodic refresh never runs either
helper, and retains imported rows.

Close Task reviews the exact installation/project/issue/target status and stored
profile/revision. Redmine freshly checks allowed closed statuses before PUT and
verifies the result; its API offers no documented compare-and-swap guarantee
between that last read and write. Mantis verifies a strong ETag and uses
`If-Match` for PATCH. Both recheck credentials and current revision, refuse
foreign identities and verify the returned closed state. Failed/uncertain writes
are never retried automatically. These protocols have actual HTTP fixture tests;
live authenticated installation acceptance remains separate.
