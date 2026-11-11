# OSV-Scanner optional call-analysis adapter

Implemented only `plugins/dependency-audit/src/call_analysis.rs`, two normalized real-result fixtures and appended README. Parent integrated the module/State/command/view/capability in lib.rs/plugin.toml.

Official baseline: OSV-Scanner2.7.1 released2026-11-09. Official docs https://google.github.io/osv-scanner/usage/scan-source#scanning-with-call-analysis expose Go govulncheck and experimental Rust compilation/DWARF, with Rust build-script execution. Exact official source schema inspected at https://github.com/google/osv-scanner/blob/v2.7.1/pkg/models/results.go: groups.experimental_analysis is advisory-ID keyed; `called` is an explicit bool. No absent-entry clean inference.

Downloaded Darwin arm64 CLI from official GitHub release and verified publisher SHA256SUMS. Receipt `/tmp/parity-osv-scanner-tools/download.json`. Official Go1.27.4 archive SHA256 verified against go.dev release metadata: `/tmp/parity-osv-scanner-tools/go-download.json`. Disposable runtime qualification exact dated args/status/logs `/tmp/parity-osv-scanner-tools/qualification.json`; executable reproduction `/tmp/parity-osv-scanner-tools/qualify.py`.

- Go called: safe `language.ParseAcceptLanguage("en-US")` against golang.org/x/text0.3.7 yields GO-2022-1059 called:true.
- Go uncalled: `language.English.String()` yields same advisory called:false. Both exit1 (known vulnerability), valid structured explicit facts. No exploit input used.
- Rust: dependency-free Cargo fixture compiles with rustc1.99.1; scanner fails127 because its DWARF reader rejects Darwin Mach-O (`bad magic number`). It nevertheless writes partial JSON. No successful Rust platform qualification claimed. Docker CLI exists but daemon absent; Linux qualification unverified.
- A deliberate incomplete Go module failed govulncheck but also exited1 and wrote advisory JSON without call analysis. Adapter therefore requires explicit per-advisory called bool facts before replacing evidence, plus retained cross-poll independent stream failure detection. Missing facts stay Unknown. Failed partial outputs retain prior evidence.

Managed start requires explicit review before version probe; exact2.7.1; root go.mod/Cargo.lock; owned bounded source snapshots; saved project editor buffers; fresh owned output. Poll/cancel remain project-bound. Cancellation retains Child ownership if kill fails. Source drift prevents acceptance; process failures/unknown-only results don't replace previous evidence. Native form caps displayed facts200; structured evidence storage512KiB; snapshot500 files/512KiB. Persisted root/snapshot/fact paths validated before display. Existing OSV reports untouched. Editor providers do not invoke scanner/network/build.

7 focused tests pass: exact version, actual called/uncalled fixture parsing and unknown alias, split-error phrase across poll boundaries with independent streams, no probe on cancelled review, foreign persisted evidence rejection, managed completion/persistence, source drift and prior evidence retention on failed scan. `git diff --check` clean. Parent owns combined full Rust/Wasm/component/package acceptance.

Remaining: actual native OpenIDEA managed interaction unverified; successful Rust platform runtime fixture unverified; all other languages Unknown; no Mend-equivalence claim; external build/toolchain/dependency inputs outside bounded captured project sources not asserted unchanged.
