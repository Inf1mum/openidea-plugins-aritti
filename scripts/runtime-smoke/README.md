# Real-Wasm runtime smoke

This isolated harness runs the released API 1.5 host (OpenIDEA v0.1.42, rev `e702e4da7b0333e81bc818e26be72db1d157ec5d`, pinned in `Cargo.toml`; moved from v0.1.40 / API 1.4 so `jpa-assist`, which needs `^1.5`, installs) and actual freshly built
plugin components against a synthetic project under a temporary directory.
It uses independent config/data/plugin installation/trust stores, never the
user's installation. It does not open a desktop window or publish packages.

```sh
cargo build --locked --manifest-path scripts/runtime-smoke/Cargo.toml \
  --target-dir scripts/api14-package-checker/target
scripts/api14-package-checker/target/debug/openidea-parity-runtime-smoke \
  "$PWD" /tmp/openidea-runtime-smoke
```

Optional package names select a subset. Laravel is always excluded. Required
network/process authority packages are skipped; no optional external authority
is granted. Add `--grant-project-read` to test the synthetic project's optional
read-access flow. No real account, Docker, Kubernetes, database or external
runtime is contacted. Read-only rendering cannot prove those integrations.

The harness stages exact target artifacts, checks copied Wasm SHA256, installs
into its private host, and invokes every unconditional declared view and native
custom editor. Models and statuses are saved to the selected output directory.
The Properties fixture opens a real document, enters the staged locale grid and
changes a French cell, then asserts both locale source files are unchanged. It resolves the actual lazy
FileAction with an exact fixture-only confirmation, applies it through the released
native workspace edit engine, and verifies grouped Undo restores both locale
buffers. These edits affect only the disposable synthetic project.
This tests real host/guest dispatch and data models, not desktop appearance or
complete feature parity. Editor-only plugins are recorded as having no views.

Build fresh Wasm and run the full package verifier first; the harness alone does
not establish source-to-artifact provenance. See [full validation](../api14-package-checker/README.md).

## Laravel released-host acceptance (headless)

`laravel` is a separate binary because the generic harness always excludes the
Laravel plugin. It installs the freshly built `plugins/laravel/dist/plugin.wasm`
into a private released-host instance (no desktop window, no external grants beyond
`project.write`/`storage`/`editor.read`) and runs against a disposable copy of a
Laravel application:

```sh
openidea-plugin build plugins/laravel
cargo build --locked --manifest-path scripts/runtime-smoke/Cargo.toml \
  --target-dir scripts/api14-package-checker/target --bin laravel
scripts/api14-package-checker/target/debug/laravel "$PWD" /tmp/laravel-host-smoke /path/to/laravel-app
```

Checks: install, activation within the per-call fuel budget, index built to completion
through view ticks, view/config completion, go to definition, missing-view diagnostic,
and the "create missing view" quick fix applied as a native workspace edit and removed
by native grouped Undo. The web hub is not rendered headlessly (the host returns a
fallback message), so tabs and their interactions still need the desktop.
