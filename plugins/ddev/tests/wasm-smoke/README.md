# Compiled Wasm smoke test

From the repository root, using the v0.1.40 or newer plugin CLI:

```sh
openidea-plugin build plugins/ddev
cargo run --manifest-path plugins/ddev/tests/wasm-smoke/Cargo.toml -- plugins/ddev/dist/plugin.wasm
```

Runs the actual component in the pinned OpenIDEA runtime with its default fuel
and memory limits. Activation must succeed; nested/conditional/reference-based
completion (including colon boundaries and Unicode), diagnostics with duplicate
key limits, and hover must succeed with a **50 ms** timeout per
provider. Unexpected host callbacks panic. No process, network, storage or events
grant is present; the fake host reports no open projects.

This checks Wasm exports, sandbox/import behavior and the tested editing calls.
Reported timings are observations of that machine/run, not a performance
benchmark or a promise for every configuration. It does not exercise desktop
Services, terminal UI, Database credentials, Docker or a live DDEV project.


## Command/view workflow harness

```sh
cargo run --manifest-path plugins/ddev/tests/wasm-smoke/Cargo.toml --bin workflows -- plugins/ddev/dist/plugin.wasm
```

Uses a disposable on-disk project and a fixture-backed implementation of the
v0.1.40 host API. It loads the actual Wasm component, renders Services and detail
panes, dispatches view events, invokes all 27 current manifest commands, saves/
resets preferences, checks stopped-state guards, verifies cached context updates
after background refresh/editor focus without another CLI read, and verifies the configuration/
Setup fallback without a process grant. It also verifies native Database source/
secure-credential/connection callback separation, window status, keyed terminal
completion, configuration removal and real host run-target argv/path preparation.
The fake Database disconnects on upsert, like the real host: unchanged manual/
background refresh must preserve the connection and avoid settings/credential
writes. A one-shot secure-store failure tests retry/reconnection and no fallback. Data-source writes must not contain the
fixture password. All files are deleted when the harness exits normally or
unwinds; no DDEV process, real network request, OS clipboard or native terminal
is used. This validates compiled ABI and workflow behavior, not live desktop or
container operation. Additional probes cover moved native metadata reuse,
Stop/Restart recovery after describe failure, exact operation IDs for delayed
Configure completion, and Windows/UNC editor paths. `default-run` preserves the original editor-only probe.

## Optional live target check

With `DDEV_LIVE_PROJECT` set to the running disposable fixture created by the
[live smoke runner](../LIVE-SMOKE.md), workflows additionally execute the target
registered by the compiled component through the actual host wrapper and real
DDEV PHP/Node containers. Without that variable this harness retains its normal
fixture-only behavior. Native desktop/Database callbacks remain fixture-backed.
