# Deferred MCP server acceptance

Prepared on 2026-11-10, **not built or executed** because the user requested tests
be kept for later. This harness is distinct from the existing Wasm smoke test:
it instantiates the released `ide-host::mcp::Registry`, installs the real package,
starts the real `ide-mcp::McpServer` on a loopback ephemeral port and calls it over
MCP Streamable HTTP. It uses a disposable profile/project and a tiny authored PHP
Artisan fixture, and leaves the user's IDE profile and projects alone.

When the user resumes testing:

```sh
openidea-plugin build plugins/laravel
LARAVEL_RUN_MCP_ACCEPTANCE=1 cargo run \
  --manifest-path plugins/laravel/tests/mcp-server/Cargo.toml \
  -- plugins/laravel
```

Requires a host `php` executable and API 1.4 package/checker. The `Cargo.toml`
pins host/plugin/MCP dependencies to released `v0.1.40`. The Cargo lockfile has
not been generated because this harness has not been built.

Prepared coverage:

- Real JSON-RPC initialize and HTTP tools/list for namespaced plugin declarations.
- Source inventory calls, schema rejection and live withdrawal after disable.
- Restricted Mode filtering and mutating-tool call rejection.
- Missing project-window confirmation, explicit denial and wrong-window answers.
- Separate process capability denial after MCP approval; approved execution only
  in the newly created fixture.
- Bearer discovery credentials remain inside a temporary private profile and are
  neither printed nor saved as evidence.

The fixture validates the registry/transport/approval path rather than Laravel's
framework command implementations. The separate `tests/wasm-smoke` harness covers
compiled guest behavior against a disposable installed Laravel application.

## Source-confirmed host boundary

Released source `v0.1.40`, commit `37c2a60`, has the following behavior:

- `crates/ide-host/src/plugins/mcp.rs` validates schemas, names and live declarations
  before dispatching through the ordinary plugin actor.
- `crates/ide-host/src/mcp/mod.rs` filters non-read-only declarations in Restricted
  Mode both when listing and calling. Destructive calls ask the owning window.
- The same module calls `call.confirm(&metadata.description, None)`. The native
  MCP confirmation therefore uses the tool description with no argument detail;
  it does **not** provide an exact command/argv preview. Declaring a destructive
  tool is not evidence that its runtime arguments appear in that confirmation.
- `mcp/requests.rs` binds approvals to their project window and rejects missing,
  expired, rejected or wrong-window answers.

No upstream files or SDK interfaces were changed here. Desktop server discovery,
native approval presentation and this new harness remain unverified until the
user resumes testing.
