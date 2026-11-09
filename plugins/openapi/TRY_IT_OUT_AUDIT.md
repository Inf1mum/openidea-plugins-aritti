# P13.4 redirect-safe execution audit

## Current guarded delivery

The current SDK1.3 package implements reviewed, Rust-owned GET/HEAD requests on
native API1.4 hosts with per-hop redirect-grant enforcement. Activation Context
(version plus exact loopback grant) gates execution; default/unknown/API1.3
contexts refuse before prompts or fetch. No SDK or minimum-engine retarget is
required because the HTTP request shape is unchanged.

`try_http.rs` prepares an immutable method/URL/source preview; only its native
Send-once event has a fresh Invocation. It revalidates root/current editor/source
text/version and a single-use120-second ticket, then calls `host::fetch` directly
without another modal. Responses and bounded constant failures remain source
scoped. The supported contract is literal127.0.0.1 with explicit port, no-auth
JSON OpenAPI3.0/3.1, GET/HEAD, scalar declared path/query parameters, five-second
timeout,256KiB acceptance/64KiB display. The host8MiB transfer cap is distinct.
Other targets/auth/headers/bodies/Swagger/YAML remain unsupported; same-host
redirects can change port/path, disclosed in the concrete preview.

Source tests cover11TryHTTPcases including nofetch preparation/cancel, API guard,
fresh event scope, stale/replay/expiry, exact encoding, bounds and safe failures.
The latest full165test run includes them. Prior actual native acceptance is dated
separately in `docs/parity-2026-11-08-refresh/implementation/desktop-43-api14-openapi/`.
No new native request or acceptance is asserted by this audit.

## Historical API1.3 preflight (before guarded implementation)


At this historical checkpoint P13.4 was not implemented. No Try-it-out request command, network capability or
send button is introduced by this audit. Existing request-file/curl export and
preview workflows are preserved. No real network request or local service was
used for this investigation.

Pinned host commit: `25424465c04c9221a92c02227c905911641d29e3` (v0.1.39).

The pinned v0.1.39 host exposes `HttpRequest` with method, URL, headers, body and
timeout fields. The plugin can validate a localhost GET/HEAD URL, show an exact
confirmation and request `net:localhost` / `net:127.0.0.1`, but it cannot disable
redirects or inspect a redirect before the host follows it.

Inspected source chain in `/private/tmp/openidea-api-review-20261108`:

1. `crates/ide-plugin/src/wit_host.rs:955`, `impl net::Host::fetch`, checks the initial
   URL's `Capability::Net(host)` before calling the host transport.
2. `crates/ide-http/src/client.rs:45`, `build`, creates the shared reqwest client
   without a redirect policy override.
3. `crates/ide-host/src/plugins/api.rs:1423`, `fetch`, explicitly records that the
   shared client follows redirects. It awaits `builder.send()` and then compares
   `response.url()` with the initial host. A different final host is denied only
   after that transport request has happened.
4. `sdk/rust/openidea-plugin/src/types.rs:430`, `HttpRequest`, has no redirect-policy
   field; the SDK response does not expose the final URL either.

Consequently a plugin-side localhost-only URL check cannot guarantee
localhost-only traffic. A timeout, response limit, GET/HEAD restriction or
confirmation of the initial URL cannot undo a redirect request. Suppressing the
response after a Denied result does not enforce destination authorization before
network access. No wildcard grants, browser-network fallback or curl execution
would repair that boundary.

The original `SwOpenInHttpClientLineMarkerProvider` resolves a detected source
endpoint into an HTTP Client URL-path context; its lifecycle does not establish
that the current plugin fetch transport is redirect-safe. Embedded Swagger UI's
browser requests are not an acceptable replacement for Rust-owned/granted-host
execution here.

Required host change: disable redirects for plugin fetch, or enforce the plugin's
allowed destinations before every redirect hop, then expose enough redirect
behavior to maintain an honest concrete request preview. This is a specific host
transport limitation, distinct from the remaining feasible plugin work below.
A source-contract receipt at `/tmp/openapi-try-out-contract.log` checks the
initial-grant/send/final-host ordering and records fragment hashes. It proves the
source ordering, not a live network security test.

Once that boundary is available, the bounded plugin implementation still needs:
verified project/current-source operation membership and stale revalidation;
strict loopback authority/port parsing; encoded path/query inputs without header
or body injection; exact method/URL confirmation; response and timeout bounds;
cancellation/error handling; and a plain-text response viewer. Arbitrary hosts,
write methods, authentication, multipart/body execution and the original full
HTTP Client workflow remain outside the proposed localhost GET/HEAD subset.

A focused MockHost regression grants loopback plus a named remote test host, then
renders a spec and runs unsaved providers with a localhost server declaration.
It asserts that no requests/terminals are created and that an unsupported send
command remains unavailable. This covers the shipped no-fetch boundary; the
redirect finding itself is a source audit, not a mock simulation of live redirects.
No standalone planner/UI is added merely to imply execution progress.
