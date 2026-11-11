# Ideolog optional JVM adapter — final 2026-11-10

## Delivered supported contract

The new **Ideolog: Capture JVM Regex and Date Observations** command supplies a faithful Java custom-format parsing path outside editor callbacks. It selects an enabled raw imported `LogParsingPattern` (including constructs the local fancy-regex/numeric timestamp implementation rejects), asks for explicit BCP47 locale and Java timezone identity or actual JVM defaults, reviews the source/regex/date pattern/tool scope, then runs one approved Java17+ source-launcher process for the entire bounded snapshot.

The independently implemented embedded helper uses actual Java `Pattern` and `SimpleDateFormat`. It follows the original parser's lineStart.find, first-line regex.find, nonoverlapping capturing-group order and multiline last-message extension. Actual Java locale names, regional timezone rules, all supported SimpleDateFormat fields and its default lenient/partial parse semantics are available in this path. Epoch milliseconds and time gaps are computed from these observations, rather than approximating them using the local timestamp parser.

The new **native JVM Observations** view shows source lines, gap seconds, epoch milliseconds, timestamps, severity/category/message captures and exact runtime provenance (Java version, effective locale/timezone, selected format/date pattern). Activating a row validates current project ownership, exact source contents, version where available and current custom-format configuration before source navigation. A project-scoped discard control clears the in-memory snapshot.

### Bounds/lifecycle

- Input: nonempty owned project log,64KiB/512lines,4096bytes/line; regexes4096bytes, date pattern256bytes, locale/timezone128bytes, capture columns-1..128.
- One child per explicit invocation, no project/application bootstrap or project classpath. Only optional `process:java` is requested. The approved executable is resolved by the host; no automatic installation/tool download.
- Helper source is embedded with include_str and materialized into plugin-private storage after review. A bounded private input file avoids blocked child-stdin writes before deadline setup. Its contents are cleared via RAII on normal/error exits; empty filename metadata may remain until uninstall. The host's process environment is inherited.
- Java heap64MiB,15second Rust deadline, cancellable Task, child termination on timeout/cancel/drop,256KiB stdout/8KiB stderr. The helper independently caps input/output. Display fields are truncated512UTF16 units; truncation does not create source edit ranges.
- Missing/denied Java, older/unsupported versions, invalid regex/date/locale/timezone, failed child, invalid output, cancellation, limits and stale source retain the last successful snapshot.
- No process launches from syntax/semantic providers, no persistent parsing cache, no automatic capture while typing.

## Verification

- `cargo test --locked --manifest-path plugins/ideolog/Cargo.toml`: **104 passed**, zero failures.
- Fresh `cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/ideolog/Cargo.toml`: pass.
- Actual disposable helper test, installed **Java26.0.1**, verifies French weekday/month names and CET parsing with Java-only Unicode class + possessive regex, multiline message behavior, source lines0/2, absolute epoch1704161045000ms and1.000second gap. `/tmp/ideolog-jvm-real.log` records the actual runtime metadata and successful run.
- Unit regressions cover malformed protocol/hex/UTF8/output bounds, prelaunch cancellation/denied Java and oversized document, exact-source navigation staleness and foreign-project discard refusal.
- Manifest parses as TOML; `process:java` optional, `capture-jvm` command, activation for native `jvm-observations` view, no web entry on that view, engine^1.4. Root performs actual package component validation and generated-resource sync.
- `git diff --check -- plugins/ideolog`: pass.
- Fresh Wasm SHA256: ac0d923144296eedb2561b9ac2ab69bcfddb60a3b9cc9fd414699915daf91466.

Logs: `/tmp/ideolog-jvm-tests.log`, `/tmp/ideolog-jvm-build.log`, `/tmp/ideolog-jvm-real.log`.

## Precisely qualified closure/residual

**Supported Java custom parsing observations and faithful Java date normalization are implemented**, with full underlying JVM regex/date engines for admitted inputs. This closes the absence of an available faithful engine for explicit bounded custom-format parsing; it does not replace the low-latency editor parser or claim live syntax-renderer parity.

Still separate: unsupported Java regex/date formats remain unavailable in automatic local editor highlighting; Java custom-highlighting-rule rendering, arbitrary exact colors/fonts/backgrounds/stripes, native live-tail/hide-lines, extended selection and heatmap are not delivered by this adapter. Some could use further owned custom UI; existing host surfaces do not supply all native editor behaviors. No new desktop/real-host successful command acceptance has been performed. Actual Java execution evidence is a disposable local integration test plus production protocol/ownership tests, not an installed-host receipt.

Sources: fresh Marketplace metadata still reports Ideolog263.7307.101; supplementary MIT public source contracts read from `JetBrains/ideolog` master `lex/LogFileFormats.kt`, `lex/LogFileLexer.kt`, `util/IdeologDocumentContext.kt`. No original code/artifacts were copied into the helper.

## Final main-editor integration (supersedes104-test observation-only checkpoint)

Actual JVM observations now feed the built-in Log semantic provider for the same captured document. Timestamp fields (including actual JVM-derived time-gap category), recognized error/warning severity and category captures are located **uniquely** in the original first physical line. Ambiguous occurrences, including overlapping textual repeats (`ana` in `banana`), multiline values, possibly truncated display cells and overlapping ranges are withheld. UTF16 ranges preserve astral-character prefixes and never cross CR/LF.

Provider admission checks exact path/full text/version, deepest project root, warmed custom-format source and current customPatterns setting. It reads no files or pattern storage and starts no process. Delivered lifecycle events withdraw live eligibility but retain the explicit observation table. Mark Current Log Source first subscribes to lifecycle and warms current stored patterns outside the provider, then may re-admit an unchanged snapshot and publish the same JVM-derived native source marks. Cross-instance stored-format changes become visible after refresh/delivered lifecycle; independent immediate cross-instance storage observation is not claimed.

The real Java integration test now feeds its actual French localized-date/Java-regex result into the production semantic-cache path, without Java permission in that provider host, and verifies two timestamp tokens plus WARN styling. Additional tests cover exact version/text/path/current-setting/warmed-config guards, nested project reassignment, lifecycle withdrawal/re-admission, ambiguous/truncated/overlapping cell refusal and source-mark reuse without child execution.

**Final Ideolog107 tests pass, fresh release Wasm build passes, manifest TOML/optionalJava/native-view assertions pass and scoped diff check passes.** Latest Wasm SHA256:064c032efa205342e19384b1b3649a2e7846cf43cba917237a2d10bb6fc7d023. Final logs retain the same `/tmp/ideolog-jvm-*` locations.

This closes the supported explicit Java custom-format engine **and exact-source cached main-editor field-highlighting** contract. Automatic changed-text JVM parsing, Java custom-highlighting-rule/full-line style reproduction, native renderer/color parity and custom ideolog-language syntax integration remain distinct; this batch does not claim them completed. No new installed-host successful capture receipt is claimed.

## Further delivery — 2026-11-11

See `/tmp/parity-ideolog-live.md`: actual JVM-backed owned live tail/literal-filter UI, observed rotation/truncation, source confinement/invalidation/cancel/expiry, generic built-in Log folding (including exact-source cached JVM groups), and nested selection provider/command are implemented.113 package tests and fresh Wasm build pass, plus actual file/runtime and headless browser fixtures. These supersede prior generic “no tail or selection/folding API” statements; native editor line visibility, heatmap, arbitrary styles and unsupported platform profiles remain qualified.
