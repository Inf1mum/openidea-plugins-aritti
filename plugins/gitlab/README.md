# GitLab

gitlab.com merge requests in OpenIDEA. List them, follow their timeline, review the changes with comments on diff lines and pending reviews, and approve, merge, rebase, close or reopen them. The plugin also shows the current branch's merge request in the status bar and its unresolved threads in the editor, checks out merge request branches, clones and shares projects, links to the current lines on GitLab and creates snippets.

MIT; see `LICENSE`.

## Setup

1. Run **GitLab: Create Access Token** (or click *Create Access Token* in any GitLab tool window). It opens gitlab.com's personal access token page with the name `OpenIDEA GitLab` and the scopes `api` and `read_user` filled in.
2. Paste the token into Settings › Plugins › GitLab (the `token` secret).

The IDE puts the token in the `Authorization` header itself; the plugin never reads it. A 401 response means the token was rejected.

The project needs a git remote on gitlab.com (`https://`, `ssh://` or `git@gitlab.com:`). When several remotes point there, `origin` wins.

The plugin only talks to gitlab.com: the REST API under `https://gitlab.com/api/v4`, and `https://gitlab.com/api/graphql` for Mark as Draft / Mark as Ready. Self-hosted servers aren't supported: every host must be listed in `plugin.toml` as `net:<host>`, so another server means changing `SERVER` / `SERVER_HOST` in `src/api.rs` and rebuilding.

Optional permissions:

| Permission | Used for | Without it |
|---|---|---|
| `clipboard` | Copy Link, the snippet URL | the link is shown in a notification |
| `project.read` | jumping to a thread's line, editor review | a thread's double-click opens its GitLab page |
| `events` | status bar item and editor review | neither is shown |
| `process:git` | Check Out Branch, creating or inspecting worktrees, clone, Share Project | those actions fail |
| `process:glab` | Follow Job Log (glab), Check Out Merge Request Source (glab) | those actions fail |

## Tool windows

One shared UI-kit window contains the GitLab, Details and Projects tabs; commands select their corresponding custom tab.

- **GitLab** (requests tab in the left GitLab window): the project's merge requests (50 per page, up to 10 pages) as `!iid title`, with author, branches and draft state. *Filter* cycles through the quick filters Open, Include my changes, Need my review, Assigned to me (the default) and Closed, or picks an author, reviewer, label or text filter. Expanding a merge request loads its timeline in date order: threads (author, reply count, resolved or not, `path:line`) with their replies, plus GitLab's system notes and state, label and milestone changes; *Hide Events* / *Show Events* toggles the system notes and changes. Selecting an item shows it in the Merge Request view (a selected thread or reply is the one you reply to). Diff threads open their local source line when available; other discussion targets keep their GitLab links. Open on GitLab remains available in Details.
- **Details** (tab in the same window): a form with the comment field, or a reply field under the note you reply to, the *Add to my review* checkbox (on by default) that keeps it as a pending review comment, the merge options and the description (bounded CommonMark — see below). The status line sums up state, draft, branches, author, merge status, pipeline and approvals. Buttons, shown when they apply: Comment, Reply or Add to Review; Submit Review (n) (publishes the pending comments, and posts the comment field's text as a summary if it isn't empty); Resolve Thread / Unresolve Thread; Approve / Revoke Approval; Merge (enabled when you can merge and GitLab reports `mergeable`, with *Delete source branch* and *Squash commits* checkboxes); Rebase (when GitLab reports `need_rebase`); Add Me as Reviewer, Mark as Draft / Mark as Ready, Close, Reopen; Check Out Branch (when the merge request isn't merged and isn't the current branch); Labels, Assignees, Reviewers, Milestone and Title (open the attribute choices directly; the combined Edit action remains available); Open Existing Worktree, Open on GitLab, Copy Link, Refresh. Your own comments (not system notes) get Edit, Delete (second click) and an award emoji from a fixed list of eight (`thumbsup`, `thumbsdown`, `heart`, `tada`, `laughing`, `confused`, `rocket`, `eyes`).
- A row of section buttons switches the window between **Overview** (the form above), **Changes**, **Commits**, **Files**, **Pipelines** and **Timeline**; the active one is marked with a dot and the others carry counts from cached data. The choice is remembered; selecting a merge request keeps the active section. A failing section keeps the section buttons and shows the error with Refresh.
  - **Changes**: the cumulative diff, one block per file with `+added −removed`, existing threads and pending comments under their lines. Click a line number to comment (*Add to Review* or *Comment Now*); threads have Reply and Resolve / Unresolve. *Viewed* collapses a file and is remembered until the file changes again (compared with the last merge request commit touching it, read from the local git history; without `process:git` or the commits locally, the head commit). Clicking a file name opens it locally; clicking the header folds it. Binary and very large files have no diff here; open them on GitLab.
  - **Commits**: the merge request's commits. A double click shows that commit's diff, read-only, in Changes; commit mode has no comments and no viewed marks.
  - **Files**: the changed files as a tree or a list (Tree/List button). Double-click a file to toggle Viewed, or use Mark All Viewed and Clear Viewed; it works without page scripts and shares the marks of Changes.
  - **Pipelines**: the newest 20 pipelines with their jobs by stage. Retry a job or pipeline, cancel a pipeline (second click) or open it on GitLab. Job traces: see Snapshots below; no live log or manual play.
  - **Timeline**: Kind, Author, When, Status and Summary columns, search/facets, and selected bounded plain-text body facts, from cached discussions and optional activity data. Opening, filtering and rendering a cached timeline make no HTTP requests; **Refresh Timeline** reads explicitly and commits only after rechecking the merge request owner and head. Presentation is capped at 500 rows and 384 KiB with 4 KiB bodies; an untruncated snapshot is still labelled bounded/historical. **Open** opens the MR/comment page on gitlab.com; **Reply in Overview** selects the cached discussion and returns to the reply form without posting or clearing an unsent draft.
- **GitLab Projects** (left): the projects you're a member of (100 per page, up to 10 pages, most recently active first). Double-clicking one runs `git clone` in a terminal, into the clone directory; open the clone with File › Open when it finishes.

The Details **Description** and diff comments render bounded CommonMark: tables, task lists, strikethrough and code blocks. Raw HTML is displayed as text; only HTTP(S) links without embedded credentials survive, and clicks must belong to the selected MR snapshot before opening externally. Images display their alternative text and are never fetched. Large inputs are visibly truncated. GitLab-specific mentions, custom emoji, math and server-side markup extensions are not rendered.

## Prompts

Create Merge Request, Create Snippet and Share Project are prompt sequences, not windows. The `commitsView` and `pipelineView` settings turn the Commits and Pipelines sections off, and `noteActions` the comment actions.

- **Create Merge Request** (command): asks for the source branch (local branches, current first), the target branch (project branches, default first), the title (prefilled with the last commit subject), a one-line description and whether it is a draft. Dismissing any of the first four prompts creates nothing. If GitLab refuses, a notification says so and you can open the new-MR page for that branch. No assignee, reviewers, labels or milestone here; GitLab: Edit Merge Request picks those from lists.
- **Create Snippet** (command): asks for the title, the visibility (a public snippet needs a confirm) and more project files, one path per prompt until an empty answer. It uses the focused file or its selection, or the files selected in the Project view. Dismissing any prompt creates nothing.
- **Share Project** (command): asks for the name and the visibility. A repository without a commit is initialised and committed after a confirm that lists the files; files that look secret (`.env*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`) are refused.

Values that go into a URL or a git command (branch and ref names, remote names, usernames, commit SHAs, repository paths, clone URLs) are checked first and refused when they fail, so a branch or path outside `[A-Za-z0-9._/@+-]` can't be pushed or cloned through the plugin. Project paths longer than 64 characters can't be cloned.

## Current branch

When the branch's upstream is on a gitlab.com remote, the status bar shows `!iid on <branch>` for its open merge request (forks are skipped). Clicking it selects the merge request in the Merge Request view. A branch other than the default branch with no open merge request shows *GitLab: Create Merge Request* instead. The lookup runs again on every repository change, and when a file opens if the commit changed or the last lookup is more than 5 minutes old.

With `editorReview` on, the merge request's unresolved threads appear in the open files as gutter markers and `Info` problems (source `GitLab`) on their line on the new side. They're hidden while the local branch is behind its upstream.

With a review thread's suggestion block (` ```suggestion:-N+M `) on a line of an open file, *GitLab: Apply suggestion* appears in the quick fixes there. It replaces the N lines above, the commented line and the M lines below with the suggestion (a plain ` ```suggestion ` replaces the commented line; an empty one deletes it). It's offered for the unresolved threads shown in the editor and isn't offered when the Changes section's cached diff shows a different text on any line of the replaced range that appears in that diff (lines outside the diff aren't checked); without a cached diff it doesn't check at all, so look at the lines before applying. It only edits the open file; nothing is posted to GitLab, and applying doesn't mark the suggestion as applied on GitLab.

## Read-only content snapshots

The Merge Request overview offers **Open MR Snapshot** and **Open Diff Snapshot**; Changes also offers the diff snapshot action. These open plugin-backed editor tabs: the overview contains the MR title, description and branch metadata; the diff tab contains the existing unified text diffs. Each is explicitly bound to the current GitLab project/root/remote, selected MR and complete head commit; a fresh ownership/head check after loading prevents publishing a snapshot when those facts change. Snapshot content never supplies write or review authority: save and unknown actions are rejected as read-only, and the review pane remains the place for comments, drafts, viewed files and review submission. There is no inline commenting in these tabs.

**Reload Cached Snapshot** reads only the in-memory snapshot; no API or process calls. **Refresh Snapshot** re-reads GitLab explicitly, retains the previous snapshot on failure, and refuses to rebind an old tab to a changed head; retry it after a transient failure, or open a new snapshot for a new head. **Return to Review** reveals the existing pane without clearing draft text. Content opens are tied to the originating packaged-view frame, so two windows with the same local request ID cannot resolve each other's replies; the frame routing is UI correlation, not an authentication boundary — the project/MR/head checks remain separate.

Snapshots are capped at 384 KiB of UTF-8 text with a visible truncation marker; NUL/control characters are replaced and Unicode boundaries preserved. At most four snapshots are cached per plugin instance (oldest evicted); eviction makes old callbacks report expiration — reopen from the current MR view. API pagination/server limits and binary files can omit diff content, so a diff snapshot does not claim to be complete. Refresh is subject to the host's content-action timeout and the request budget; a slow refresh may need to be retried.

**Open Job Trace Snapshot (API1.4)** on a selected listed pipeline job uses the GitLab REST trace route (never a process or browser request). It requires a native API 1.4 host and the declared GitLab network grant; the selected MR/pipeline/job membership and owner/head guards are re-established before fetching. The result is a historical read-only plain-text tab, not a managed/live log. Up to 1 MiB of UTF-8 response is accepted and displayed within the 384 KiB snapshot cap with a truncation marker; HTTP reads share the request budget with a bounded timeout (up to 20 s). No automatic polling or retry.

**Follow Job Log (glab)** is an optional adapter for a live log: it requires an approved `glab` 1.x executable (`process:glab`) authenticated separately from the plugin token. It verifies current MR/pipeline/job membership, checks the CLI version and confirms the exact repository/job before opening a native managed log tab running `glab ci trace <job-id> --repo <repository-url>`; the tab owns output, restart and cancellation. The CLI uses its own credentials and configuration; the plugin does not export its token. Native log tabs do not run in remote projects.

## Ownership and caching

MR caches belong to the exact repository root, GitLab project path and selected remote. A changed or unavailable owner clears list/details/thread/diff/commit/job/pipeline and branch caches; equal numeric MR IDs in different projects or worktrees cannot reuse an old cache. A nonempty unsent overview comment blocks cross-scope selection rather than exposing or discarding it; finish or clear it in the original project first. Refresh of the review pane also invalidates the list cache so the next model reloads current rows; MR data itself is never broadcast in invalidation messages.

The **GitLab.com account slot** setting selects the existing primary token or the optional secondary token. Each is a declared IDE secret; the plugin receives only secret placeholders. Switching slots expires account identity, project lists, selected-MR ownership and snapshot caches, so an old account's controls cannot act as the new account. Finish or clear an unsent comment before switching; restoring the previous slot lets you finish that draft. This covers two declared gitlab.com accounts, not arbitrary self-hosted servers.

## Commands

| Command | Does | Menus |
|---|---|---|
| GitLab: Open on GitLab | opens the file at the current commit on gitlab.com, with the selected lines (`#L3-L5`) | editor, editor tab and Project view context (first selected file or folder, linked at the current commit) |
| GitLab: Copy Link | copies the same link | none |
| GitLab: Create Snippet | asks for title, visibility and more files, then creates the snippet from the selection, or the whole file; the URL is copied and shown | editor and editor tab context |
| GitLab: Create Merge Request | opens gitlab.com's new merge request page for the current branch | Tools |
| GitLab: Edit Merge Request | opens the existing attribute choices (labels, assignees, reviewers, milestone, title) from the project's lists | Merge Request overview |
| GitLab: Share Project | asks for name and visibility, creates the project in your namespace, adds it as `origin` (or `gitlab` if `origin` exists) and runs `git push -u` in a terminal; needs a commit, a checked-out branch and no gitlab.com remote yet | none |
| GitLab: Show Current Branch's Merge Request | selects the current branch's merge request (the status bar item runs it) | none |
| GitLab: Open Merge Request in Worktree | fetches the selected merge request (else the current branch's) and runs `git worktree add --detach` in a sibling folder `<repository>-<branch>`; git refuses a folder that already holds files; the path is shown in a notification and no editor is opened | none |
| GitLab: Open Existing Merge Request Worktree | lists registered local worktrees, asks which exact-head match to open, and requests the normal project-open/trust flow in a new window; creates nothing | Merge Request overview button |
| GitLab: Update Merge Request Branch | asks GitLab to rebase the selected merge request (else the current branch's) onto its target (the same `rebase` API call as the Rebase button) and reports a conflict | none |
| GitLab: Re-request Review… | selects an assigned reviewer, confirms the request, rechecks current repository/MR/head and reviewer membership, then uses GitLab's re-review mutation; cancellation sends nothing | open MR details toolbar |
| GitLab: Check Out Merge Request Source (glab)… | selects a merge request, saves editor buffers and commits/stashes local changes, reviews the target project, source project ID/branch and complete head, then opens a native terminal running `glab mr checkout <iid> --repo https://gitlab.com/<target> --branch review/mr-<iid>-<head-prefix>` (official glab 1.130.0 checkout profile; no `--force`; it may update branch tracking/configuration and prompt about conflicts). This is source-branch checkout, not an exact-head snapshot; Native Stop cancels the process, but already fetched refs/configuration may remain | none |
| GitLab: Create Access Token | opens the token page (see Setup) | none |

Check Out Branch (Details button) fetches the source branch from the gitlab.com remote and checks it out, creating a tracking branch if needed. An existing local branch is fast-forwarded, or left alone if it has diverged. Fork merge requests are fetched from `refs/merge-requests/<iid>/head` into `fork/<author>/<branch>`.

The existing-worktree action requires selecting the merge request in the current project's list; selection is bound to the GitLab project path, repository root and remote (the project root must equal the repository root; nested project folders are rejected). Fresh GitLab reads verify the merge request's target project and current head. Only `git worktree list --porcelain -z` is run: no fetch, checkout, clone or worktree creation. A branch checkout must match the merge request's local branch (including `fork/<author>/` ownership); a detached checkout must have its exact full head commit. Locked, prunable, ambiguous and malformed records are excluded or rejected; lists are bounded to 1 MiB and 256 records. Picker dismissal and observed task cancellation open nothing. Selection, project, remote, head and worktree registration are checked again before requesting a new window; the host performs its normal trust flow. The worktree paths must belong to the UI machine, as required by `open_project`; no local-to-remote filesystem mapping is inferred.

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
| GitLab.com account slot | primary | use the primary or the optional secondary token (see Ownership and caching) |

## CI files

In the CI files below, problems appear on open and save, and Cmd+click on a job name in `needs`, `dependencies` or `extends` opens the job, and on a `stage` value opens its entry in `stages`. `include: local:` files are read (up to three levels) so their jobs and stages count; checks stay quiet when the file has a remote, project, template or component include, a glob, or a local file that can't be read, and for `needs` entries with `project:` or `pipeline:`, and for names built from variables. The reader is a line scanner, not a YAML parser: unusual shapes are skipped.

Completion in these files: top-level keywords, job keywords one level inside a job or `default`, the predefined `CI_*` variables after `$` or `${`, values for `when`, `stage` (from `stages`), `allow_failure`, `interruptible`, job names for `extends`, `needs` and `dependencies` (inline, flow or list items), and the job then the key inside `!reference [`. A `!reference` naming a job that is not in the file is reported with `needs` (setting `ciJobReferences`) and Go to Definition jumps to that job. The lists are typed from the GitLab docs as of November 2026 and may miss newer names; the popup opens on letters, not on `$`.

## File icons

The GitLab icon on `.gitlab-ci.yml`, `.gitlab-ci.yaml`, `*.gitlab-ci.yml`, `*.gitlab-ci.yaml`, `*.gitlab-ci.*.yml` and `*.gitlab-ci.*.yaml`.

## Limits

- gitlab.com only, with the two declared account slots; no OAuth login and no unbounded account manager.
- Lists are read page by page up to 10 pages, all the lists of one command or refresh sharing a 35 s budget (lists after it are partial or empty): 500 merge requests, 1,000 threads per merge request, 1,000 projects, 1,000 diff files.
- No side-by-side diff in the IDE's diff viewer, and no commenting from the editor gutter. Per-commit diffs are read-only.
- Line comments, replies, resolving and Viewed in the Changes section need scripts, which the desktop IDE currently blocks in plugin pages; comment from the Merge Request view or on GitLab instead. Viewed also works in the Files section.
- Threads in the editor don't move with unsaved edits.
- Notes have no `@mention` completion or attachments; raw HTML is shown as text.
- The prompts have no pickers (users, labels, milestone), no file chooser and no file-selection tree; extra snippet files are typed as paths.
- A file counts as viewed until a later merge request commit touches it; that needs the commits in the local checkout, else the head commit is used.
- Not implemented: usage statistics, `.gitlab-ci.yml` schema validation, completion and inspections beyond the four listed under CI files; `include: local:` files are followed, remote, project, template and component includes are not.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
