# Installed application Laravel MCP acceptance

This test is distinct from `tests/mcp-server`, which tests the IDE plugin's own MCP server.

Use an isolated copy of a disposable installed Laravel application whose basename starts with `laravel-mcp-milestone6-`. Do not use an application containing user data. In that copy, install `laravel/mcp` (the recorded acceptance version is v1.2.0), then run:

```sh
php /absolute/repo/plugins/laravel/tests/fixtures/prepare-application-mcp.php /absolute/disposable/laravel-mcp-milestone6-fixture
```

Run `php artisan package:discover --no-ansi` in that disposable app. Then, from the repository:

```sh
LARAVEL_APPLICATION_MCP_FIXTURE=/absolute/disposable/laravel-mcp-milestone6-fixture CARGO_INCREMENTAL=0 cargo test --manifest-path plugins/laravel/Cargo.toml --lib mcp::application::tests -- --include-ignored
```

The fixture installer refuses other basename patterns and requires the actual package. It writes four authored classes under `app/Mcp/Fixture`, literal local/web registrations to `routes/ai.php` and a fixture marker. The ignored tests require that marker. The local test launches `php artisan mcp:start milestone`, exercises the production stdio discovery/parser and kills the child. The HTTP test starts one PHP built-in server directly on an ephemeral `127.0.0.1` port, routes the production HTTP request/parser through a mock host transport implemented with `curl`, and kills the server after discovery. This tests actual Laravel protocol responses without claiming native host net capability/credential approval acceptance.

Both transports initialize and request only tools/resources/prompts metadata. The authored handlers write `mcp-handler-executed.marker` if called; the tests require that this file stays absent. Application bootstrap and MCP lifecycle code still execute. Tests assert typed tool arguments, the resource URI, and prompt metadata. They do not call tools, read resources or retrieve prompts, and they do not use credentials.

The ordinary tests cover alias-resolved registrations, foreign/decoy/dynamic negatives, bounded pagination, loopback origin validation and mocked HTTP. Registered OAuth clients and Sentry live access are unrelated gates and remain unprovisioned.
