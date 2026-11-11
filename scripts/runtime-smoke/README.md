# Real-Wasm runtime smoke

This isolated harness runs the released API1.4 host and actual freshly built
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
