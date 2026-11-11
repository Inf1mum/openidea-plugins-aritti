# GitLab reviewed fork/source checkout profile

Implemented in plugins/gitlab/src/fork_checkout.rs; command `checkout-source-cli` declared in plugin.toml and dispatched from Plugin::run_command. Only plugins/gitlab changed. Existing Markdown rendering preserved.

The command requires an owned selected MR from the active repository/account, verified target/source project IDs, valid source branch and complete40-character head; clean local Git status/no pending Git operation and saved project editors. It presents the concrete source/target/head/local branch and effects before even probing the executable. After confirmation it qualifies exact official glab1.130.0, reloads MR/source/head and repository/selection, rechecks local commit/branch/clean state, then opens a native terminal for:

`glab mr checkout <iid> --repo https://gitlab.com/<target> --branch review/mr-<iid>-<head12>`

No force/reset option is passed. Independently configured CLI credentials/transports/config/hooks remain effective and are disclosed. Native CLI may prompt if an existing branch diverged. No checkout-success assertion is made on launch. The reviewed source branch can move after review; this is not pinned-head checkout. Native Stop may leave already fetched refs/config; desktop terminal interaction/live account acceptance remain unverified.

## Official tool qualification and actual runtime

Publisher-checksummed official glab1.130.0 binary receipt `/tmp/parity-developer-tools-cli/glab-download.json`; actual version `glab 1.130.0 (9b430a11f)`. Actual `mr checkout --help` retained `/tmp/parity-gitlab-fork/checkout-help.txt`. Official tagged source https://gitlab.com/gitlab-org/cli/-/raw/v1.130.0/internal/commands/mr/checkout/mr_checkout.go retained `/tmp/parity-gitlab-fork/glab-checkout-source.go`: obtains SourceProjectID and refs/heads/source_branch, falls back to target MR ref if source project inaccessible, fetches and configures tracking, switches branch; no-force divergence has explicit native prompt/non-TTY refusal.

Actual disposable runtime receipts dated2026-11-10UTC:

- `/tmp/parity-gitlab-fork/runtime.json`: real official CLI, disposable HTTP API mock, different source9/target5, existing local head object, creates/switches branch and exact head.
- `/tmp/parity-gitlab-fork/runtime-fetch.json`: source commit initially absent in target, separate disposable fork Git repository; official CLI queries source project9 then performs actual Git fetch through an explicit fixture-local URL rewrite and switches the expected branch/head. Source-only FORK file content verified. No live GitLab API request/account mutation occurred.
- Reproducible scripts `/tmp/parity-gitlab-fork/fixture.py` and `/tmp/parity-gitlab-fork/fixture-fetch.py` retain setup, mock API payloads, exact argv and result assertions. HTTP fixture substitutes GitLab service only; actual glab/Git execute.

299 Rust tests pass (294 prior +5 specific fork adapter tests: exact argv/no force, cancel before probe, dirty worktree, unsupported executable version and source-head drift). Release Wasm build passes. Superseding `/tmp/parity-gitlab-fork-verified/results.json` PASS:299 tests, fresh release Wasm, exact released component and staged-package checks, stable plugin/shared fingerprints. `git diff --check` clean. GitLab sources frozen.

No full original GitLab parity closure is asserted. Broader named account/selfhost lifecycle, original native diff Review Mode, richer review persistence and GFM-specific rendering remain separate contracts.
