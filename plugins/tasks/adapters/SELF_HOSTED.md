# Redmine and MantisBT REST adapters

`self_hosted_bridge.py` provides actual, optional HTTPS transports for these two
specific REST APIs. It is not a generic URL/regular-expression scraper. Native
Tasks materializes it beside the reviewed `fogbugz_bridge.py` utility and runs:

```text
python3 -I -S self_hosted_bridge.py PROFILE REQUEST OUTPUT
```

The utility is loaded by its explicit sibling path; no project Python module,
site customization, package installation or project bootstrap is used. Only
Python 3 stdlib is required.

## External credential profile

The user provisions an external regular single-link profile owned by that user,
mode **0600**, at a normalized absolute path without symlinks (maximum 16 KiB):

```json
{"version":1,"family":"redmine","baseUrl":"https://tracker.example/redmine","apiKey":"USER_PROVISIONED_API_KEY","closeStatusId":"5"}
```

For MantisBT set `family` to `mantis` and use the installation root, for example
`https://tracker.example/mantisbt`. `closeStatusId` is optional: without it the
adapter is read-only. It is a user-configured positive decimal string, never a
hardcoded assumed status. Redmine uses `X-Redmine-API-Key`; current MantisBT uses
`Authorization: Bearer …`. Keys never enter command arguments, native settings,
URLs, logs or output. Tokens/profile files must not be committed.

Only exact HTTPS DNS hostnames and an optional slash-separated installation
prefix are accepted. Explicit ports, userinfo, IP literals, percent-encoded or
traversing prefixes, query/fragment and trailing slash are refused. Requests are
constructed from known API paths beneath that root. Redirects, proxy environment
routing and server-supplied URLs are never followed; TLS verification stays on.

## Search contract

```json
{"version":1,"operation":"search","expectedFamily":"redmine","expectedBaseUrl":"https://tracker.example/redmine","query":"project_id=3&status_id=open&assigned_to_id=me","page":1}
```

The query is an explicit allowlisted parameter string, **not free text**:

- Redmine: `project_id`, `tracker_id`, `status_id` (`open`, `closed`, `*` or numeric),
  `assigned_to_id` (`me` or numeric), `issue_id` (up to 20 comma-separated numbers).
  Defaults are `status_id=open&assigned_to_id=me`. Pages use documented
  `offset`, `limit=50` and stable `id:asc` sorting; returned pagination is checked.
- MantisBT: a required positive `project_id`, and optional positive saved
  `filter_id`. Pages use documented `page` and `page_size=50`. A full page signals
  possible further results. Saved filters carry search semantics on the server.

Unknown, repeated, malformed or free-text parameters are refused before network
access. Search results contain bounded plain summary/description, locally built
issue URLs, decimal `number`/`projectId`, `closed`, opaque `revision`, `canClose`,
`closeStatusId` and `guard` (`fresh-read`, `etag` or `none`). The response echoes
`operation`, `family`, `host`, `baseUrl`, `page`, `hasMore` and opaque `profileId`.
Revision hashes bind the issue identity, project, status, source summary,
description and update timestamp; fingerprints contain no raw credential.

`canClose` is a configured candidate action, not a promise that a subsequent
server mutation will be permitted. Fresh server workflow/access checks still
apply. Redmine obtains actual `is_closed` metadata from `/issue_statuses.json`.
MantisBT obtains project-specific `bug_closed_status_threshold`,
`status_enum_string` and `status_enum_workflow` via `/api/rest/config`.
Unavailable closed-status metadata causes refusal, rather than guessing from
status names/numbers. Missing/unsupported Mantis workflow metadata makes close
unavailable. Empty workflow configuration means unrestricted enum transitions;
configured transitions must explicitly admit the target status.

## Reviewed status update

```json
{"version":1,"operation":"close","expectedFamily":"redmine","expectedBaseUrl":"https://tracker.example/redmine","expectedId":"12","expectedProjectId":"3","expectedRevision":"64_lowercase_hex_sha256","expectedProfileId":"64_lowercase_hex_sha256"}
```

Native review must display the exact configured status and the actual guard.
The adapter rejects changed family/root/profile before remote access. It fetches
the current issue, checks exact project/revision and current status metadata,
then rechecks unchanged credentials before mutation:

- Redmine requests `include=allowed_statuses` and requires the configured target
  to be both closed and presently allowed. It sends one
  `PUT /issues/{id}.json` with `issue.status_id`. Its documented API supplies no
  conditional update guard used here: **a race remains between fresh read and
  write**. Output `guard=fresh-read` explicitly preserves that limitation.
- MantisBT requires a fresh strong ETag from `GET /api/rest/issues/{id}`, and
  sends one `PATCH` with `status.id` and that exact `If-Match`. The actual server
  REST implementation checks this precondition. Missing/weak ETag refuses close;
  HTTP 412 is reported as `revision_precondition_failed`, with no retry.

Both families re-fetch status metadata and the same issue after mutation and
require the exact configured status to be closed. Uncertain outcomes return
`close_state_unknown`; inspect the remote issue before another attempt. No
resolution, project change, note, time entry or arbitrary field update is sent.
No mutation is retried automatically.

All I/O is bounded (8-second operation deadline, 1 MiB response/output, 50 issues,
200 statuses, 4096-byte query; summary 1024 bytes and description 16 KiB). Native
must additionally enforce its external process deadline. Credentials echoed in
returned text are redacted and control characters removed. JSON duplicate keys,
excessive nesting/counts, inconsistent IDs/projects or invalid update timestamps
are refused. Errors are fixed safe codes, never raw HTTP bodies/exceptions.
Private result files are atomically replaced with mode0600.

## Official contracts checked 2026-11-11

- [Redmine issues: filters, pagination, show/allowed statuses and update](https://www.redmine.org/projects/redmine/wiki/Rest_Issues)
- [Redmine REST authentication and JSON](https://www.redmine.org/projects/redmine/wiki/Rest_api)
- [Redmine closed-status metadata](https://www.redmine.org/projects/redmine/wiki/Rest_IssueStatuses)
- [MantisBT actual REST routes, pagination/filtering, PATCH and If-Match check](https://github.com/mantisbt/mantisbt/blob/master/api/rest/restcore/issues_rest.php)
- [MantisBT REST token authentication](https://github.com/mantisbt/mantisbt/blob/master/api/rest/restcore/AuthMiddleware.php)
- [MantisBT project-scoped config endpoint](https://github.com/mantisbt/mantisbt/blob/master/api/rest/restcore/config_rest.php)
- [MantisBT config response and enum conversion](https://github.com/mantisbt/mantisbt/blob/master/core/commands/ConfigsGetCommand.php)
- [MantisBT workflow semantics/defaults](https://github.com/mantisbt/mantisbt/blob/master/config_defaults_inc.php)
- [MantisBT server ETag tests](https://github.com/mantisbt/mantisbt/blob/master/tests/rest/RestIssueEtagTest.php)

No MantisBT implementation code/assets were copied; this adapter independently
implements the documented/source-defined protocol using Python stdlib.

## Verification

```sh
python3 plugins/tasks/adapters/test_self_hosted_bridge.py
```

Real loopback HTTP fixtures exercise methods, API paths/prefixes, authentication
headers, filter/page encoding, Redmine allowed-status review, Mantis If-Match,
post-mutation verification, rejected stale/profile/metadata states, sanitized
errors, strict profile and isolated atomic output. Loopback transport is an
internal test injection only; production has no HTTP/TLS-bypass flag. No real
Redmine/Mantis account was contacted or changed during tests. These receipts do
not replace live service and desktop acceptance.
