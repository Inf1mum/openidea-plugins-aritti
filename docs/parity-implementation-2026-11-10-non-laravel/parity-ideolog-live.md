# Ideolog live tail / filters / editor features — 2026-11-11

## Delivered

1. **Owned Live Log Tail**: shared-kit standalone panel, approved Java17+ runtime helper, actual directory-relative secure read profile, rolling complete-line display, literal case-insensitive text filter, inverse Hide matches, Follow scrolling, Stop and explicit Stop Live Log Tail recovery command.
2. **Actual streaming lifecycle**: last64KiB initial tail, appended bytes, observed truncation/prefix change and file-identity rotation reset,2second grace for missing rotation replacement, source/project/unsaved invalidation, stale frame/nonce/sequence rejection, cancellation, plugin-shutdown termination and helper-side lease expiry. It follows real files through FileChannel/SeekableByteChannel; it does not pretend repeated MockHost snapshots are tailing.
3. **Generic API1.4 folding**: built-in Log current-buffer multiline events/stacktrace folds, warmed configuration only,512KiB/8192line/1024fold/15ms cooperative bounds. Exact-source JVM cached observation groups can supply folds when their Java-only custom parser is not represented locally.
4. **Selection gap requalification and implementation**: SDK1.4 provides selection-ranges and TextEditor::set_selections. New nested field→physical line→event→document provider and Extend Log Selection command use those APIs. UTF16 carets, surrogate interiors and source version/text are validated. Previous README “plugins cannot set selection/fold” blanket blocker statements were corrected.
5. All contributed views now share native `group="ideolog"`: Patterns (retaining its existing internal Snapshot navigation), JVM Observations, Live Log Tail. New standalone page imports shared tokens/components/icons/widgets/controls; no independent palette/button system.

## Runtime bounds and exact supported profile

- Java source helper is embedded in Wasm, materialized only in plugin-private storage and executed through the optional approved process:java grant; no tool installation, project/application bootstrap or project code loading.
- Profile requires canonical project paths, regular UTF8 source, usable file identity and **SecureDirectoryStream**. Directory-relative NOFOLLOW_LINKS opens reject symlinks; canonical owner checks reject outside-root replacement. Tested on **Darwin25.5.0 / Java26.0.1**. Windows/unsupported-filesystem profiles refuse; this is not a cross-platform tail claim.
-5minute/1MiB input lease,4096byte reads,2MiB total helper output,64MiB heap. Visible view drains bounded32KiB chunks at1second intervals with a500ms backend minimum.512complete lines/128KiB retained; max4096bytes/line; incomplete UTF8 lines are held until newline and invalid UTF8 refuses the profile.
- Output protocol verifies READY owner/path, generation and byte offsets; rotation resets retained rows rather than mixing identities. Source changes that truncate/regrow between polls without observable identity/size/prefix changes are not guaranteed detectable.
- Hiding/pagehide requests Stop, including an in-flight Start. Browser-visible controller messages are correlated; expired/foreign controller messages cannot stop a newer lease. Native dock-to-DOM visibility propagation still needs actual-host acceptance. Helper self-expiry bounds a lost controller.
- Native source editor line hiding, native scrollbar heatmap, arbitrary exact background/font/stripe styling and Java HIDE custom-rule behavior are not claimed by the owned literal filter UI.

## Evidence

- Full cargo tests: **113 passed, zero failures**.
- Fresh `cargo build --locked --release --target wasm32-wasip2`: pass.
- Fresh Wasm SHA256:25a18f4292bc24eef8aff682d003a7fe257878ee2d34ca7b07cda7a95c24c0b4.
- Actual Java runtime integration (not just lifecycle mocks): append, copy-truncate, delayed rename/replacement rotation, outside-root symlink escape rejection, actual OS process cancellation and1second helper lease expiry pass. Real protocol frames are consumed by the production Rust tail decoder.
- Actual JVM localized-date/Java-regex fixture also validates cached generic fold grouping; retained JVM observation/semantic functionality still passes.
- Native API command tests prove field→line→event selection and surrogate-interior refusal with no process or file writes. Generic folding follows current unsaved text and clears after fixing/removing stacktrace continuations.
- Real headless Chromium fixture passes safe text rendering (log HTML never executes), text filter/invert, visible polling, hidden-stop/no subsequent polling,380px layout and no page errors.
- Manifest TOML assertions: native grouped views, standalone tail entry exists, optional Java, folding/selection-ranges provider kinds, engine^1.4. Root must run actual package/component validation and shared asset synchronization after this manifest change.
- `git diff --check -- plugins/ideolog`: pass.

Reproduce:

```
cargo test --locked --manifest-path plugins/ideolog/Cargo.toml
cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/ideolog/Cargo.toml
PLAYWRIGHT_MODULE=<installed @playwright/test module> node plugins/ideolog/tests/live-tail-ui.mjs
```

Logs: `/tmp/ideolog-live-tests.log`, `/tmp/ideolog-live-build.log`, `/tmp/ideolog-live-real.log`, `/tmp/ideolog-live-browser.log`.

## Qualification

Supported owned live-tail/filter and generic folding/selection contracts are implemented and have concrete helper/browser runtime evidence. No installed OpenIDEA desktop session, actual host process-grant/visibility roundtrip or native rendering acceptance is claimed. Original unlimited/native-editor behavior and unsupported platform profiles remain distinct. Full original Ideolog parity is not established by these113 tests.

This worker changed Ideolog only in this continuation. Parent owns Properties. Root owns shared-kit/generated asset sync and component acceptance.
