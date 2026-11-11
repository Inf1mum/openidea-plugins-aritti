# GitLab

One shared UI-kit window contains the GitLab, Details, Projects tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

gitlab.com merge requests in OpenIDEA. List them, follow their timeline,
review the changes with comments on diff lines and pending reviews, and
approve, merge, rebase, close or reopen them. The plugin also shows the
current branch's merge request in the status bar and its unresolved threads in
the editor, checks out merge request branches, clones and shares projects,
links to the current lines on GitLab and creates snippets.

## Setup

1. Run **GitLab: Create Access Token** (or click *Create Access Token* in any
   GitLab tool window). It opens gitlab.com's personal access token page with
   the name `OpenIDEA GitLab` and the scopes `api` and `read_user` filled in.
2. Paste the token into Settings › Plugins › GitLab (the `token` secret).

The IDE puts the token in the `Authorization` header itself; the plugin never
reads it. A 401 response means the token was rejected.

The project needs a git remote on gitlab.com (`https://`, `ssh://` or
`git@gitlab.com:`). When several remotes point there, `origin` wins.

The plugin only talks to gitlab.com: the REST API under
`https://gitlab.com/api/v4`, and `https://gitlab.com/api/graphql` for Mark as
Draft / Mark as Ready. Self-hosted servers aren't supported. Every host has to
be listed in `plugin.toml` as `net:<host>`, so another server means changing
`SERVER` / `SERVER_HOST` in `src/api.rs` and rebuilding.

Optional permissions:

| Permission | Used for | Without it |
|---|---|---|
| `clipboard` | Copy Link, the snippet URL | the link is shown in a notification |
| `project.read` | jumping to a thread's line, editor review | a thread's double-click opens its GitLab page |
| `events` | status bar item and editor review | neither is shown |
| `process:git` | Check Out Branch, creating or inspecting worktrees, clone, Share Project | those actions fail |

## Tool windows

- **GitLab** (requests tab in the left GitLab window): the project's merge requests (50 per page, up to 10 pages) as
  `!iid title`, with author, branches and draft state. *Filter* cycles through
  the original's quick filters: Open, Include my changes, Need my review,
  Assigned to me (the default) and Closed. Expanding a merge request loads its
  timeline in date order: threads (author, reply count, resolved or not,
  `path:line`) with their replies, plus GitLab's system notes and state,
  label and milestone changes. *Hide Events* / *Show Events* toggles the
  system notes and changes. Selecting an item shows it in Merge Request (a
  selected thread or reply is the one you reply to). Double-clicking a merge
  request selects the custom Details tab. Diff threads open their local source
  line when available; other discussion targets keep their GitLab links.
  Open on GitLab remains available in Details.
- **Details** (tab in the same GitLab window): a form with the comment field, or a reply field
  under the note you reply to when a thread is selected, the *Add to my review*
  checkbox (on by default) that keeps it as a pending review comment, the merge
  options and the read-only description. The status line sums up state, draft,
  branches, author, merge status, pipeline and approvals. Buttons, shown when
  they apply:
  - Comment, Reply or Add to Review;
  - Submit Review (n): publishes the pending comments, and posts the comment
    field's text as a summary if it isn't empty;
  - Resolve Thread / Unresolve Thread;
  - Approve / Revoke Approval;
  - Merge, enabled when you can merge and GitLab reports `mergeable`, with
    *Delete source branch* and *Squash commits* checkboxes;
  - Rebase, when GitLab reports `need_rebase`;
  - Add Me as Reviewer, Mark as Draft / Mark as Ready, Close, Reopen;
  - Check Out Branch, when the merge request isn't merged and isn't the
    current branch;
  - Labels, Assignees, Reviewers, Milestone and Title: open the existing attribute choices directly; the combined Edit action remains available;
  - Open Existing Worktree, Open on GitLab, Copy Link, Refresh.
  A row of section buttons switches the window between **Overview**
  (the form above), **Changes**, **Commits**, **Files**, **Pipelines** and **Timeline**; the
  active one is marked with a dot and the others carry counts from cached data.
  The choice is remembered. Selecting a merge request keeps the active section.
  - **Changes**: the cumulative diff, one block per file with `+added −removed`,
    existing threads and pending comments under their lines. Click a line number
    to comment (*Add to Review* or *Comment Now*); threads have Reply and Resolve /
    Unresolve. *Viewed* collapses a file and is remembered until the file changes
    again (compared with the last merge request commit touching it, read from the
    local git history; without `process:git` or the commits locally, with the head
    commit). Clicking a file name opens it locally; clicking the header folds it.
    Binary and very large files have no diff here; open them on GitLab.
  - **Commits**: the merge request's commits. A double click shows that commit's
    diff, read-only, in Changes; commit mode has no comments and no viewed marks.
  - **Files**: the changed files as a tree or a list (Tree/List button).
    Double-click a file to toggle Viewed, or use Mark All Viewed and Clear Viewed;
    it works without page scripts and shares the marks of Changes.
  - **Pipelines**: the newest 20 pipelines with their jobs by stage. Retry a job
    or pipeline, cancel a pipeline (second click) or open it on GitLab. An explicitly
    opened, bounded job-trace snapshot requires API1.4; no live log or manual play.
  A failing section keeps the section buttons and shows the error with Refresh.
- **GitLab Projects** (left): the projects you're a member of (100 per page, up to
  10 pages, most recently active first). Double-clicking one runs `git clone` in a
  terminal, into the clone directory. Open the clone with File › Open when it
  finishes.

## Prompts and details

Create Merge Request, Create Snippet and Share Project are prompt sequences,
not windows. The `commitsView` and `pipelineView` settings turn the Commits and
Pipelines sections off, and `noteActions` the comment actions.

- **Create Merge Request** (command): asks for the source branch (local
  branches, current first), the target branch (project branches, default
  first), the title (prefilled with the last commit subject), a one-line
  description and whether it is a draft. Dismissing any of the first four
  prompts creates nothing. If GitLab refuses, a notification says so and you can
  open the new-MR page for that branch. No assignee, reviewers, labels or
  milestone here; GitLab: Edit Merge Request picks those from lists.
- **Create Snippet** (command): asks for the title, the visibility (a public
  snippet needs a confirm) and more project files, one path per prompt until
  an empty answer. It uses the focused file or its selection, or the files
  selected in the Project view. Dismissing any prompt creates nothing.
- **Share Project** (command): asks for the name and the visibility. A
  repository without a commit is initialised and committed after a confirm
  that lists the files; files that look secret (`.env*`, `*.pem`, `*.key`,
  `id_rsa*`, `id_ed25519*`) are refused.
- In the Merge Request view, your own comments (not system notes) get Edit,
  Delete (second click) and an award emoji from a fixed list of eight
  (`thumbsup`, `thumbsdown`, `heart`, `tada`, `laughing`, `confused`, `rocket`,
  `eyes`). The edit box starts empty and Save Edit overwrites on one click.

Values that go into a URL or a git command (branch and ref names, remote names,
usernames, commit SHAs, repository paths, clone URLs) are checked first and
refused when they fail, so a branch or path outside `[A-Za-z0-9._/@+-]` can't
be pushed or cloned through the plugin. Project paths longer than 64
characters can't be cloned.

## Current branch

When the branch's upstream is on a gitlab.com remote, the status bar shows
`!iid on <branch>` for its open merge request (forks are skipped, as in the
original). Clicking it selects the merge request in the Merge Request and
Merge Request window. A branch other than the default branch
with no open merge request shows *GitLab: Create Merge Request* instead. The
lookup runs again on every repository change, and when a file opens if the
commit changed or the last lookup is more than 5 minutes old.

With `editorReview` on, the merge request's unresolved threads appear in the
open files as gutter markers and `Info` problems (source `GitLab`) on their
line on the new side. They're hidden while the local branch is behind its
upstream, as in the original.

With a review thread's suggestion block (` ```suggestion:-N+M `) on a line of an
open file, *GitLab: Apply suggestion* appears in the quick fixes there. It
replaces the N lines above, the commented line and the M lines below with the
suggestion (a plain ` ```suggestion ` replaces the commented line; an empty one
deletes it). It's offered for the unresolved threads shown in the editor and
isn't offered when the Changes section's cached diff shows a different text on any
line of the replaced range that appears in that diff (lines outside the diff
aren't checked); without a cached diff it doesn't check at all, so look at the lines before
applying. It only edits the open file; nothing is posted to GitLab.


## Read-only content snapshots

The Merge Request overview offers **Open MR Snapshot** and **Open Diff
Snapshot**; Changes also offers the diff snapshot action. These open API 1.3
plugin-backed editor tabs through the packaged view. The overview contains the
MR title, description and branch metadata; the diff tab contains the existing
API's unified text diffs. Each is explicitly bound to the current GitLab
project/root/remote, selected MR and complete head commit. A fresh ownership/head
check after loading prevents publishing a snapshot when those facts change.
The review pane remains the place for comments, drafts, viewed files and review
submission. Snapshot content never supplies write or review authority.

**Reload Cached Snapshot** reads only the in-memory snapshot; it performs no API
or process calls. **Refresh Snapshot** is an explicit window event that reads
GitLab again, retains the previous snapshot on failure, and refuses to rebind an
old tab to a changed head. Retry that action after a transient failure; for a new
head, open a new snapshot from the MR view. **Return to Review** reveals the
existing pane without clearing draft text. Save and unknown actions are rejected
as read-only. If opening a tab fails, the packaged view displays a notice and the
same Open Snapshot action can be retried.

Content opens require a valid originating packaged-view frame token. The token
is captured only for the current ui-kit event and restored afterward; native
buttons or expired pages without an origin cannot broadcast an open request.
Keys retain the complete head commit, a frame token and an owner digest within
128 bytes, with a full metadata collision guard. Open/reply envelopes carry the
frame token; only that frame forwards the host content-tab protocol. Thus two
windows with the same local request ID cannot resolve each other's replies.
Successful cached reload/refresh replies omit `saved: false` so the host updates
the read-only baseline; errors and rejected saves retain it. An expired key can
still route its error back to the validated originating frame.

The underlying host transport still broadcasts these envelopes to plugin view
listeners. Frame IDs provide UI routing correlation, not encrypted isolation or
an authentication boundary. The plugin's project/MR/head checks remain separate.

Snapshots are capped at 384 KiB of UTF-8 text, with a visible truncation marker;
NUL/control characters are replaced and Unicode boundaries are preserved. At
most four snapshots are cached per plugin instance. Eviction or instance
reclamation makes old callbacks report expiration; reopen from the current MR
view. API pagination/server limits and binary files can omit diff content, so a
diff snapshot does not claim to be complete. Refresh remains subject to the
host's content-action timeout and existing API request budget; a slow refresh may
need to be retried.

P06.3 is partial. Native tests and package checks verify snapshot ownership,
limits, cached callbacks and retry/error behavior; desktop tab opening, syntax
highlighting, timeout presentation and trust/window lifecycle are unverified.
There is no native side-by-side review editor, inline commenting in these tabs,
live/tailing job trace, terminal filter or host-managed job log process. No new
process CLI or credentials are introduced.

## Commands

| Command | Does | Menus |
|---|---|---|
| GitLab: Open on GitLab | opens the file at the current commit on gitlab.com, with the selected lines (`#L3-L5`) | editor, editor tab and Project view context (first selected file or folder, linked at the current commit) |
| GitLab: Copy Link | copies the same link | none |
| GitLab: Create Snippet | asks for title, visibility and more files, then creates the snippet from the selection, or the whole file; the URL is copied and shown | editor and editor tab context |
| GitLab: Create Merge Request | opens gitlab.com's new merge request page for the current branch | Tools |
| GitLab: Share Project | asks for name and visibility, creates the project in your namespace, adds it as `origin` (or `gitlab` if `origin` exists) and runs `git push -u` in a terminal; needs a commit, a checked-out branch and no gitlab.com remote yet | none |
| GitLab: Show Current Branch's Merge Request | selects the current branch's merge request (the status bar item runs it) | none |
| GitLab: Open Merge Request in Worktree | fetches the selected merge request (else the current branch's) and runs `git worktree add --detach` in a sibling folder `<repository>-<branch>`; git refuses a folder that already holds files; the path is shown in a notification and no editor is opened | none |
| GitLab: Open Existing Merge Request Worktree | lists registered local worktrees, asks which exact-head match to open, and requests the normal project-open/trust flow in a new window; creates nothing | Merge Request overview button |
| GitLab: Update Merge Request Branch | asks GitLab to rebase the selected merge request (else the current branch's) onto its target (`PUT .../rebase`, the same call as the Rebase button, which only appears when GitLab reports `need_rebase`) and reports a conflict | none |
| GitLab: Create Access Token | opens the token page (see Setup) | none |


The existing-worktree action requires selecting the merge request in the current
project's list. Selection is bound to the GitLab project path, repository root
and remote. The project root must equal the repository root; nested project
folders are conservatively rejected. Fresh GitLab reads verify the merge
request's target project and current head. Only `git worktree list --porcelain -z`
is run: no fetch, checkout, clone or worktree creation. A branch checkout must
match the merge request's local branch (including `fork/<author>/` ownership);
a detached checkout must have its exact full head commit. Locked, prunable,
ambiguous and malformed records are excluded or rejected. Lists are bounded to
1 MiB and 256 records. Picker dismissal and observed task cancellation open
nothing. Selection, project, remote, head and worktree registration are checked
again before requesting a new window; the host performs its normal trust flow.
The current window and unfinished review text are retained.

The direct attribute buttons operate only on the selected, loaded open merge
request. They recheck project/selection/owner/state after the value prompt and
before updating GitLab. Dismissal writes nothing and review drafts are retained.
Existing description and generated summary fields remain read-only. These guards
apply to the new direct buttons; the combined legacy Edit and other existing
mutation flows have not been fully audited for cross-project cache ownership.

P06.4 remains partial pending desktop verification of project opening, trust,
pickers and task cancellation. No local-to-remote filesystem mapping is inferred:
the worktree paths must belong to the UI machine, as required by `open_project`.
Remote development and filesystem races after the final registration check are
not covered. The existing **Open Merge Request in Worktree** command still
fetches/adds a checkout and reports its path; it is a separate operation.

Check Out Branch fetches the source branch from the gitlab.com remote and
checks it out, creating a tracking branch if needed. An existing local branch
is fast-forwarded, or left alone if it has diverged. Fork merge requests are
fetched from `refs/merge-requests/<iid>/head` into `fork/<author>/<branch>`.

## Settings

Settings › Plugins › GitLab:

| Key | Default | Effect |
|---|---|---|
| `editorReview` | `true` | show the current branch's unresolved threads in the editor |
| `noteActions` | `true` | edit, delete and react on your own comments |
| `commitsView` | `true` | the merge request's commits and each commit's changes |
| `pipelineView` | `true` | the merge request's pipelines and jobs, with retry and cancel |
| `ciJobReferences` | `true` | `.gitlab-ci.yml`: report `needs`, `dependencies` and `extends` naming a job that is not in the file |
| `ciStages` | `true` | `.gitlab-ci.yml`: report a job `stage` that is not in `stages` (or the default stages) |
| `ciDuplicateJobUsage` | `true` | `.gitlab-ci.yml`: report a job listed twice in one `needs`, `dependencies` or `extends` |
| `ciSingleQuotedVariables` | `true` | `.gitlab-ci.yml`: report `$VAR` in `echo '…'` / `printf '…'` script lines, which the shell does not expand |
| `cloneUsingSsh` | `false` | clone and share with the SSH URL instead of HTTPS |
| `cloneDirectory` | empty | where clones go; empty means next to the current project |

## CI files

In the CI files below, problems appear on open and save, and Cmd+click on a job
name in `needs`, `dependencies` or `extends` opens the job, and on a `stage`
value opens its entry in `stages`. `include: local:` files are read (up to three
levels) so their jobs and stages count; checks stay quiet when the file has a
remote, project, template or component include, a glob, or a local file that
can't be read, and for `needs` entries
with `project:` or `pipeline:`, and for names built from variables. The reader
is a line scanner, not a YAML parser: unusual shapes are skipped.

Completion in these files: top-level keywords, job keywords one level inside a
job or `default`, the predefined `CI_*` variables after `$` or `${`, values for
`when`, `stage` (from `stages`), `allow_failure`, `interruptible`, job names for
`extends`, `needs` and `dependencies` (inline, flow or list items), and the job
then the key inside `!reference [`. A `!reference` naming a job that is not in
the file is reported with `needs` (setting `ciJobReferences`) and Go to
Definition jumps to that job. The
lists are typed from the GitLab docs as of November 2026 and may miss newer
names; the popup opens on letters, not on `$`.

## File icons

The GitLab icon on `.gitlab-ci.yml`, `.gitlab-ci.yaml`, `*.gitlab-ci.yml`,
`*.gitlab-ci.yaml`, `*.gitlab-ci.*.yml` and `*.gitlab-ci.*.yaml`.

## Limits

- gitlab.com only, with one personal access token; no OAuth login and no
  multiple accounts.
- Lists are read page by page up to 10 pages, all the lists of one command or
  refresh sharing a 35 s budget (lists after it are partial or empty): 500 merge requests, 1,000 threads
  per merge request, 1,000 projects, 1,000 diff files.
- No side-by-side diff in the IDE's diff viewer, and no commenting from the
  editor gutter. Per-commit diffs are read-only.
- Line comments, replies, resolving and Viewed in the Changes section need
  scripts, which the desktop IDE currently blocks in plugin pages; comment from
  the Merge Request view or on GitLab instead. Viewed also works in the Merge
  Request Files section.
- Threads in the editor don't move with unsaved edits.
- Notes are plain text: no markdown rendering, `@mention` completion or
  attachments.
- Filtering: the five quick filters plus author, reviewer, label and text
  filters from the Filter button.
- The prompts have no pickers (users, labels, milestone), no file chooser
  and no file-selection tree; extra snippet files are typed as paths.
- A file counts as viewed until a later merge request commit touches it; that
  needs the commits in the local checkout, else the head commit is used.
- Not ported: usage statistics, `.gitlab-ci.yml` schema validation, completion and inspections beyond the four listed under CI files; `include: local:` files are followed, remote, project, template and component includes are not. Applying a suggestion doesn't mark it as applied on GitLab.

See the `aritti.gitlab` section of `LIMITATIONS.md` at the repository root
for the missing APIs behind each limit.

## Credits

Port of the GitLab plugin by JetBrains s.r.o. and contributors
(`org.jetbrains.plugins.gitlab` 263.7307.200, intellij-community tag
`idea/263.7307.200`, `plugins/gitlab`). Apache-2.0, see `LICENSE` and `NOTICE`.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Cross-pane packaged refresh and MR ownership

Changes to MR selection, section/review/details state and MR creation now retain
native view refresh and send model-free `ui-kit-refresh` only to the affected
Merge Requests or Merge Request pane. Each receiving hub obtains its own correlated
frame/project model; no MR data is broadcast in the invalidation. Review reload
also invalidates the list cache so its next model actually reloads current rows.
Stable renders emit no invalidation. Existing account/project catalogue views and
content-tab frame/snapshot correlation protocols are unchanged.

MR caches now belong to the exact repository root, GitLab project path and selected
remote. A changed or unavailable owner clears list/details/thread/diff/commit/job/
pipeline and branch caches. Details rendering and native section actions require
that the stored selection belongs to the current source before exposing cached
fields or making a request. Equal numeric MR IDs in different projects/worktrees
cannot reuse the old cache. A nonempty unsent overview comment blocks cross-scope
selection rather than exposing/discarding it; finish or clear it in the original
project first. Existing same-project MR selection and saved review draft behavior
remain unchanged. The branch-MR command refreshes its scoped branch lookup before
establishing selection ownership.

This is per-invocation source ownership, not atomic active-window or fresh GitLab
authorization proof. Same-root frames share selection; credential identity changes
without an observable scope change and existing render-time API loading remain
separate limitations. Read-only content snapshots retain their existing bounded
root/MR/head/frame guards. Mock tests and pinned package checks do not imply native
cross-pane acceptance; no real GitLab request or GUI ran for this slice.

### Explicit job-trace snapshots (P06.3 partial)

On native API1.4 hosts with the declared GitLab network grant, select a listed
pipeline job and choose **Open Job Trace Snapshot (API1.4)**. It uses Rust
`GET /projects/:id/jobs/:job_id/trace`, never a process or browser request. API1.3,
unactivated/unknown native versions and missing grants refuse this new fetch;
SDK and minimum engine remain1.3. The gate comes from activation Context, not
a stored flag or the SDK version constant. Reactivation retires job snapshots.

Before fetching and publishing, the selected MR/pipeline/job membership must be
positively re-established within the existing request/paging budget. MR owner,
source and full head guards still apply. An older MR pipeline is labelled as such
by the explicit statement that MR membership does not prove its current head;
fork/cross-project lookup is not inferred. The result is a historical read-only
plain-text tab, not a managed/live job log. Cached reload has no network; Refresh
re-reads explicitly, retains the previous body on failure and cannot rebind to a
new MR head. Return to Review returns to Pipelines. No review/write authority is
granted by the text, and save remains refused.

The host transfer cap is8MiB; this plugin accepts at most1MiB of UTF8 response
then displays within the existing384KiB snapshot cap with an explicit truncation
marker. These acceptance/display limits are not transfer limits. Control/NUL
characters are replaced; headers/raw error bodies are omitted. HTTP errors and
expired membership show bounded reasons/retry. Existing four-snapshot/frame/key
limits and content-action timeout remain; no automatic polling or retry runs.

[GitLab’s primary job-log API](https://docs.gitlab.com/api/jobs/#retrieve-a-log-file-for-a-job)
documents the trace route and unavailable-log status. The pinned ProcessLog API
requires a program/args/toggles and has no fetched-byte append interface; this
snapshot does not claim that missing managed-log transport. Deterministic tests
and package/link checks are source evidence only; native new-button/tab/redirect/
job-log acceptance remains pending.


### Typed Timeline snapshot (P06.2)

The packaged Merge Request view now has a **Timeline** section with Kind, Author,
When, Status and Summary columns, search/facets, and selected bounded plain-text
body facts. **Refresh Timeline** explicitly reads the existing discussions and
optional activity endpoints. Opening, filtering and rendering a cached timeline
make no HTTP requests. The shared reload control also invokes this explicit
refresh. Loading uses the existing paging/deadline budget and a cancellable native
task. Presentation is capped at 500 rows and 384 KiB of serialized resource data;
individual bodies are capped at 4 KiB. Oversized identity sets, invalid/duplicate
identities, duplicate resource keys and presentation truncation produce a partial
snapshot rather than invented reply targets. Even an untruncated snapshot is
labelled bounded/historical, not complete or live.

A capture belongs to the concrete root/repository/remote, selected MR and complete
observed head. Refresh commits only after rechecking that owner and head; failed
or cancelled refresh retains the same owner's last-good capture with an explicit
STALE notice and retry. Switching MR/project clears the capture. Unknown/stale row
keys are refused. **Open** rechecks the captured head and constructs a gitlab.com
MR/comment URL from validated repository and numeric identities, never note text
or an arbitrary API URL. Activity rows open the MR page and cannot become replies.
**Reply in Overview** explicitly selects the known cached discussion/note and
returns to the existing reply form without posting or clearing an unsent draft.
This stages an existing review action; it does not claim current server note text
or restore an editor context. Draft notes, posting, resolving and review submission
remain in the existing Overview/Changes workflows.

The original `GitLabMergeRequestTimelineViewModel` exposes timeline items, loading,
show-events and a new-note model; `GitLabDiscussionReplyViewModel` separates starting
a reply from submitting it. The retained original gallery `22857-0.png` places
Timeline next to Changes. This port uses the explicitly permitted typed packaged
layout rather than reproducing the original split editor or reactive live model.
Tests exercise the public packaged bridge, reply draft retention, current-owner/
head checks, refresh failure retention and bounds. Native desktop Timeline
interaction, narrow layouts and the complete authenticated review journey remain
separate acceptance work; no new native result is claimed here.

P06.3 snapshot lifecycle follow-up (GitLab259): the bounded four-snapshot cache now
evicts the oldest accepted capture, independently of opaque key lexical order.
A successful explicit refresh makes that capture newest; collision/fetch failure
does not reorder or replace last-good data. Retiring trace snapshots also removes
their eviction ownership, and expired entries cannot displace unrelated captures.
Cached Reload remains local and does not pretend to be a fresh server capture.

Explicit job-trace HTTP timeout is now limited by the remaining shared read
budget, up to20s, using that budget's existing whole-second clock. A response
arriving after the deadline is rejected before the API helper accepts its body;
this does not cancel a request already in flight or add a live/managed job log.
The existing API1.4 transport guard,1MiB acceptance/384KiB presentation caps,
membership/head checks and failure/retry behavior remain. All259 tests and pinned
API1.3 build/package check pass. No real GitLab API/account/job action or new native
acceptance was performed. P06.3 remains partial for managed/live logging.

P06.3 content-action budget follow-up (GitLab263): the pinned host expires
content-tab action replies after15s. Explicit Snapshot Refresh now scopes remote
GET reads to a monotonic10s budget, including JSON metadata/membership reads and
job trace, and clamps each HTTP timeout to the remaining scope. Expired bodies
are refused, final owner/head checks still precede cache commit, and an expired
refresh preserves last-good cached content for local Reload and explicit retry.
The guard cannot widen a tighter nested budget and restores its previous scope
on every return. Writes are refused while this read-only callback scope is active;
other command/API behavior is unchanged. Existing whole-second paging/trace
budgets still apply as additional bounds.

This leaves headroom for local ownership checks, serialization and routing; it
does not guarantee total host/UI latency or cancel an already in-flight request.
All263 tests and pinnedAPI1.3 build/package check pass. Only offline fixtures were
used; no real GitLab account/network/CI/job action or new native acceptance.
P06.3 remains partial for an actual managed/live job-log path.

Cached Changes callbacks for local Open and Viewed require an exact path in the
currently displayed MR or commit diff. Paths must be bounded literal repository
relative names; local Open additionally requires the current project/root and
deepest target owner to match. Unlisted/traversing/nested-project rows refuse
without local navigation or viewed-state mutation. This is cached source-row
validation, not managed glab execution or effective alternate-server authority.

Changes HTML pages emit a source context covering repository root/project/remote,
MR IID and SHA/diff refs, and single-commit mode. Every script callback echoes
that context; missing or changed identities refuse before effects or HTTP.
An old page cannot mark/comment against a newly selected MR or refreshed known
head. The identity is a cached source correspondence check, not a secret token,
a new remote-head fetch, account proof, or effective glab/server binding.

Changes review callbacks additionally require an exact displayed old/new path
and line pair. Reply/resolve targets must be one uniquely cached non-system
discussion attached to a displayed diff row; resolve requires a resolvable
thread. Overflowing line numbers refuse, and empty discussion rows are omitted
without a render panic. This validates cached source rows before requests;
remote permission and concurrent server changes remain GitLab's responsibility.

Unified-diff line positions are unknown until a valid hunk header supplies
bounded unsigned coordinates. Invalid/zero/out-of-range headers never default
to line1. Checked increments preserve the maximum valid u32 line and withhold
subsequent unrepresentable positions instead of wrapping or panicking. Diff text
remains visible; unknown line pairs cannot authorize a review comment. This is
bounded position admission, not full hunk-count or patch-applicability proof.

Files pane row activation and bulk Viewed/Clear controls carry the cached
repository/MR/head context in their emitted keys. A captured row or bulk button
cannot follow selection to another MR with the same file path. Unbound legacy
mutating keys are ignored; current emitted keys retain normal behavior. This
binds source viewed metadata only and adds no remote/glab authority.

Pipeline effect controls carry the rendered repository/MR/head context, exact
pipeline/job target and Cancel confirmation phase. A captured Retry/Browser
control cannot retarget another selected job, and replaying the first Cancel
control cannot substitute for the newly rendered confirmation. Current controls
retain status eligibility and API behavior. Historical trace controls use the
same source binding and existing API1.4 transport checks; no managed glab process
or alternate-server authority is introduced.

Requests, Details and Projects share one UI-kit tabbed GitLab window. Logical view IDs and project ownership guards remain unchanged; revealing a specific view selects its custom tab.

GitLab now uses one UI-kit window with GitLab, Details and Projects tabs. Logical view IDs and command workflows remain unchanged; navigation and scoped snapshot editor replies route through that one container.

## Follow a job log with GitLab CLI

Select a listed pipeline job, then **Follow Job Log (glab)**. This optional
adapter requires an approved `glab` 1.x executable (`process:glab`) authenticated
separately from the plugin token. It verifies current MR/pipeline/job membership,
checks the CLI version and confirms the exact repository/job before opening a
native managed log tab running `glab ci trace <job-id> --repo <repository-url>`.
The tab owns output, restart and cancellation; process errors stay visible there.
The CLI uses its own credentials/configuration, including host overrides; the
plugin does not export its token. The existing bounded Job Trace Snapshot remains
available without glab. Native log tabs do not currently run in remote projects.

**GitLab: Re-request Review…** (also in the open MR details toolbar) selects an
assigned reviewer, confirms the request, rechecks current repository/MR/head and
reviewer membership, then uses GitLab's documented re-review mutation. Cancellation
sends no mutation. Unsupported/rejected server responses are reported rather than
claimed as successful requests.

## Two account slots

The **GitLab.com account slot** setting selects the existing primary token or the
optional secondary token. Each is a declared IDE secret; the plugin receives only
secret placeholders. Switching slots expires account identity, project lists,
selected-MR ownership and snapshot caches, so an old account's controls cannot act
as the new account. Finish or clear an unsent comment before switching; restoring
the previous slot lets you finish that draft. The CLI log adapter continues to use
its independent glab authentication. This covers two declared gitlab.com accounts,
not arbitrary self-hosted servers or an unbounded account manager.

## Markdown descriptions and comments

The Details **Description** section renders bounded CommonMark, tables, task lists,
strikethrough and code blocks from the cached MR description/selected discussion.
Diff comments and pending notes use the same renderer and shared UI tokens. Raw
HTML is displayed as text. Only HTTP(S) links without embedded credentials survive;
clicks must still belong to the selected MR snapshot before opening externally.
Images display their alternative text and never create an automatically fetched
image element. Large inputs are visibly truncated. Refresh Description uses the
existing owner-bound MR refresh. GitLab-specific mentions, custom emoji/math and
server-side markup extensions are not claimed by this CommonMark renderer.

**Reviewed fork/source checkout:** `GitLab: Check Out Merge Request Source (glab)…`
uses the separately approved official `glab1.130.0` checkout profile. Select a
merge request in this project's list, save editor buffers and commit/stash local
changes, then review the target project, source project ID/branch and complete
head. The command rechecks ownership, selection, source identity, local Git state
and installed version before opening a native terminal running `glab mr checkout
<iid> --repo https://gitlab.com/<target> --branch review/mr-<iid>-<head-prefix>`.
The official CLI resolves a fork source project (or target MR ref fallback) and
uses its own credentials, Git transports, configuration and hooks. No `--force`
is passed. It may update branch tracking/configuration and prompts about conflicts;
inspect the terminal and resulting commit. A remote change after review can be
fetched: this is source-branch checkout, not an exact-head snapshot. Native Stop
cancels the process; already fetched refs/configuration may remain. Live account
acceptance and native terminal interaction remain separate from the disposable
local API/Git fixture qualification.
