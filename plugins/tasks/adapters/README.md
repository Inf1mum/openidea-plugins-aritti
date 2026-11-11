# FogBugz XML API bridge

`fogbugz_bridge.py` is an optional Python 3 stdlib adapter for one explicitly
configured FogBugz Cloud tenant. It performs real HTTPS XML API form POSTs.
It does not emulate FogBugz, log in using passwords, create profiles, install
packages, start project code, read repository configuration or follow redirects.

The native Tasks plugin invokes:

```text
python3 -I -S fogbugz_bridge.py /absolute/profile.json /absolute/request.json /absolute/output.json
```

The profile is externally provisioned, owned by the invoking user, a regular
single-link file with mode **0600**, at a normalized absolute path without
symlinks. Maximum size is 16 KiB. Its strict JSON shape is:

```json
{"version":1,"endpoint":"https://acme.fogbugz.com/api.asp","token":"USER_PROVISIONED_API_TOKEN"}
```

Use a token created through the FogBugz UI. No password/token entry is routed
through plugin UI or command arguments. Only one direct `*.fogbugz.com` tenant
label, HTTPS, the exact `/api.asp` path and implicit port 443 are accepted.
Userinfo, explicit ports, query/fragment, alternate/self-hosted endpoints and
symlink paths are refused. Do not commit this profile.

Search request:

```json
{"version":1,"operation":"search","query":"assignedto:me status:active","page":1,"expectedHost":"acme.fogbugz.com"}
```

Search uses `cmd=search`, `q`, `max=50` and columns `sTitle`,
`sLatestTextSummary`, `fOpen`, `ixBugEventLatest`. Descriptions are the server's
latest plain-text summary, not the complete case event history. Search is a
single bounded result set: the documented XML command has no offset parameter,
so pages after 1 are refused. `hasMore=true` when all 50 slots are used; refine
the query rather than assuming the result is a complete inventory.

Success response:

```json
{"version":1,"ok":true,"host":"acme.fogbugz.com","profileId":"64_lowercase_hex_sha256","operation":"search","page":1,"hasMore":false,"issues":[{"number":"123","summary":"Fix issue","description":"Latest comment","url":"https://acme.fogbugz.com/default.asp?123","closed":false,"eventId":"10","canClose":true}]}
```

Numbers and event IDs are canonical positive decimal strings. The opaque
`profileId` binds the normalized profile path, exact endpoint and profile bytes;
it contains no raw token. Native review must retain this fingerprint and the
selected case identity/event before requesting a close:

```json
{"version":1,"operation":"close","expectedHost":"acme.fogbugz.com","expectedId":"123","expectedEventId":"10","expectedProfileId":"64_lowercase_hex_sha256"}
```

Close first fetches the exact case and verifies its identity, event and server
`operations` permission. It rechecks unchanged profile credentials before
sending **one** `cmd=close` request with `ixBug` and optimistic `ixBugEvent`.
A subsequent fresh search must prove that same case closed. It does not resolve
first, change statuses, retry mutations or silently change trackers/tenants.
Uncertain mutation outcomes return `close_state_unknown`; inspect the remote
case before trying again. An already closed case with matching reviewed event
returns `closed=true,alreadyClosed=true` without a mutation.

Failures are atomic JSON `{ "version":1,"ok":false,"error":"fixed_code" }`.
No raw network/server/profile exception, token or XML payload is logged or
returned. Returned summaries/descriptions redact the exact credential and drop
control characters. URLs are constructed locally without authentication data.
TLS certificate verification remains enabled; all redirects and compressed
responses are refused. Response bodies are capped at 1 MiB, cases at 50, query at
4096 UTF-8 bytes, summary at 1024 bytes and description at 16 KiB. DTD/entity
XML, non-UTF-8, excessive depth/structure and malformed identities are refused.
The operation has an eight-second deadline; native process control should also
enforce an external deadline and cancellation. Input/output files use bounded
reads and output is written 0600 via atomic replacement in the same directory.

## Official contract checked 2026-11-11

- [API general rules: UTF-8, token, GET/POST, XML response/errors](https://support.fogbugz.com/article/55732-fogbugz-xml-api-general-rules-for-api-requests)
- [Listing/searching/viewing cases: search, q, cols, max](https://support.fogbugz.com/article/55766-fogbugz-api-listing-searching-and-viewing-cases)
- [Editing cases: close and optimistic ixBugEvent](https://support.fogbugz.com/article/55748-fogbugz-xml-api-editing-cases)
- [Sample payload: fOpen, latest summary, event ID and operations](https://support.fogbugz.com/article/55785-fogbugz-xml-api-sample-xml-payloads)
- [Token guidance: create token in UI instead of storing password](https://support.fogbugz.com/article/55717-get-an-api-token-using-fogbugz-api-commands)

## Verification

```sh
python3 plugins/tasks/adapters/test_fogbugz_bridge.py
```

The tests use a real loopback HTTP server behind an internal injected connection
factory. Production CLI has no HTTP, alternate-host, disabled-TLS or test switch.
They exercise actual POST bytes and authentication fields, UTF-8 form encoding,
strict TLS context construction, redirect refusal, fresh/stale close guards,
profile replacement, permission refusal, sanitized errors, XML bounds, atomic
0600 output and isolated Python invocation. No live tenant/account was accessed
or modified by these tests. They verify the adapter contract, not live desktop
or FogBugz-service acceptance.
