# GraphQL subscription transport implementation — 2026-11-11

## Delivered workflow

Owned Spring Web commands `graphql.subscription-start`, `graphql.subscription-poll`, `graphql.subscription-cancel` use optional `process:node`. Parent wired the Runtime state/commands into Endpoints and the manifest. No host/API changes, application deployment, Actuator access, arbitrary native HTTP-file syntax or server endpoint publication.

Start requires a current project-owned `.graphql`/`.gql` editor document with one explicit `subscription` operation and optional explicit fragment declarations. Balanced lexical admission rejects extra query/mutation/subscription operations, incomplete strings/nesting and oversized input. This is document admission, not full GraphQL grammar/type/schema validation. The original current text, version/path/project, selected endpoint, variables, initialization object and Node runtime are reviewed. Source/config/runtime rechecks occur after review; current source checks run after the possibly blocking Node proof. No server-schema/runtime resolver association is claimed.

The endpoint is an exact selected canonical absolute ws(s) URL. A bounded complete, unique literal local `spring.graphql.websocket.path` + servlet context path supplies a suggestion only; conflicting/incomplete/dynamic settings supply none. The selected full URL remains user authority. HTTP endpoints, userinfo, fragments, ambiguous authorities, unresolved placeholders and whitespace/control characters are refused. The Node helper applies its own URL parsing/canonical-equality guard before opening any socket.

Variables and connection initialization are separate explicit JSON objects, each <=4 KiB. Query <=8 KiB. The exact objects are displayed in the final size-checked review, then passed only over owned stdin. The helper/plugin does not write them to disk, argv or direct client logs, read an application environment/credential source, copy Actuator authentication, set custom HTTP headers/cookies or install an alternative dispatcher. Like other approved local process adapters, the selected Node runtime/environment is trusted and inherited; this is not a sandbox for a hostile local runtime.

## Protocol and lifecycle

`runtime/graphql-subscription.cjs` independently implements one `graphql-transport-ws` operation using Node's built-in WebSocket (Node major22+ plus runtime feature probe). ConnectionInit is sent once; Subscribe waits for ConnectionAck. It answers protocol ping/pong, validates operation identity/result/error/completion shapes, rejects duplicate acknowledgement/pre-ack operation messages/invalid JSON/binary or oversized protocol messages, and handles terminal error/complete/close separately. Unknown operation IDs are ignored rather than conflated with this owned operation.

Connection acknowledgement timeout10 seconds, session timeout120 seconds, protocol message check64 KiB, client input24 KiB, cumulative output256 KiB with terminal-status reserve. Oversized message checking occurs after the built-in WebSocket receives/materializes it; this is not a claimed transport-allocation limit. Server result output is JSON-encoded. Close reason and acknowledgement/init payloads are not echoed as client logs. Output-bound termination requests Complete and closes.

One session per Runtime. Cancel sends a stdin control; the helper sends protocol Complete for an active subscription and closes with a500 ms exit grace. The plugin waits up to1500 ms before a kill fallback. Cancellation results explicitly do not prove independent remote application cleanup. Known project change/missing project ends the previous owned process without forwarding its output. Disposal of the owning plugin/window also ends it. Manual polling reads the owned output; this is finite process ownership, not durable background subscription support.

Native `Child.status` is process exit, not stdout-reader EOF. Polling therefore does not discard a just-ended reader before receiving the helper's final protocol record. Startup failures without that record retain the ended reader for another poll or explicit Cancel/release. A dedicated regression covers a late final record.

## Exact released native contracts

Inspected released checkout `37c2a60` (SDK v0.1.40):
- `crates/ide-plugin/src/wit_host.rs`: command argv <=256 entries, each <=4096 bytes; stdin write <=1 MiB; Child write/wait/kill/drop dispatch.
- `sdk/rust/openidea-plugin/src/host.rs`: Child stays owned behind a handle; write, close_stdin, wait, poll and drop semantics.
- `crates/ide-host/src/plugins/api.rs`: start pipes stdin and asynchronously drains stdout/stderr; write flushes the pipe; release kills the owned process group; Proc::chunk exposes status without requiring reader EOF.

The6783-byte helper cannot be one `--eval` argument on the released native host. Production uses a short bootstrap followed by exact Unicode-preserving source fragments, each <=4000 bytes; reconstructed source is evaluated in the approved Node process. Only static client code appears in argv. A Rust contract test checks source reconstruction/limits. The OS fixture executes this identical bootstrap, not a larger OS-only invocation. No private helper materialization or additional filesystem capability is required.

## Public source contracts

- https://nodejs.org/api/globals.html#class-websocket : built-in browser-compatible WebSocket is available without an enabling flag from22.0 and no longer experimental from22.4. Feature-disabled runtimes fail the probe.
- https://raw.githubusercontent.com/enisdenjo/graphql-ws/master/PROTOCOL.md : graphql-transport-ws message ordering, operation identities, Next/Error/Complete and bidirectional heartbeat/cancellation contracts.

Independent implementation; no third-party runtime dependency or proprietary source/assets copied.

## Evidence

Rust behavioral tests: **12 pass** in `/tmp/parity-graphql-subscription-rust.log`, covering lexical/authority/payload admission, current source+oversized exact review/cancel/stale checks, source fragments and native argv bounds, denied Node capability, explicit stdin/no credential logging, graceful/fallback cancellation, result polling and late reader output, exact local path suggestion conflicts.

Actual OS fixture: `python3 plugins/spring-web/tests/graphql-subscriptions/qualify.py --receipt /tmp/parity-graphql-subscriptions-receipt.json`. Python3.14.6 standard-library disposable loopback RFC6455 server, actual approved Node24.18.0 built-in WebSocket. **8 scenarios pass**: Next/server Complete; verified client Complete frame on cancellation; operation error; pre-ack result refusal with no Subscribe; duplicate Ack; invalid JSON; output-limit Complete; cross-authority HTTP302 not followed. Receipt records actual runtime, source/bootstrap hashes, native-compatible fragment sizes, client-frame sequence and result/output bounds. No application/remote network/credential/database used. Initial fixture needed concurrent output draining to avoid its own pipe backpressure; final fixture follows the actual host's independent reader contract.

Full parent Web tests and fresh release wasm32-wasip2 build logs `/tmp/parity-graphql-subscription-all.log`, `/tmp/parity-graphql-subscription-wasm.log` are refreshed after final edits. Scoped diff check passes. Parent handles final combined artifact/package check and translations.

## Precise residuals

- No native OpenIDEA desktop prompt/output/cancellation acceptance in this worker.
- Actual fixture runtime is24.18.0 on macOS; no executed Node22, Windows or TLS-certificate fixture is claimed. Built-in availability/version is explicitly probed, not assumed.
- The120-second timeout and10-second acknowledgement timeout are implemented timer paths, not long-duration acceptance scenarios in the8-case fixture.
- Parent separately implements a controller/schema-to-subscription-document bridge; this transport consumes the resulting current document and does not infer an effective server model. Full document validation/completion, active profile/runtime schema identity, multi-operation multiplexing, persisted history, reconnection/resumption and credential selection UI remain plugin/adaptor work, not generic missing host APIs.
- No legacy `graphql-ws` protocol or SSE/RSocket transport; those need separate explicit adapters. No arbitrary custom WebSocket HTTP header option (the browser-compatible built-in API provides no such request setting).
- This finite manual transport workflow is not whole GraphQL/original-plugin parity and does not imply that an observed client exit/socket handshake proves subscription execution success.

## Files

New owned files: `src/graphql_subscriptions.rs`, `runtime/graphql-subscription.cjs`, `tests/graphql-subscriptions/qualify.py`, `tests/graphql-subscriptions/README.md`. Parent owns main mod/Endpoints/manifest wiring. No unrelated/shared/Laravel edits.

Integration checkpoint note: a full Web run recorded164 passing and one existing custom-root source-lens test failure after parent intentionally changed complete-SDL ownership filtering. The old test attempted to unwrap a now correctly absent wrong-root shortcut lens. Parent/Feign owner is updating that assertion while preserving explicit SchemaMapping custom-root generation/global-command refusal. This adapter's12 tests and8 real OS cases remain passing. Final whole-Web acceptance belongs to the parent's refreshed combined checkpoint.
