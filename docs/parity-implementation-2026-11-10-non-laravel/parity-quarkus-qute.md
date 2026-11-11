# Qute and optional typed-language-server implementation — 2026-11-10/11

**Current status:** the optional Qute↔JDT broker has now been implemented and tested against real official tools, including two actual typed-model lifecycle fixtures. The older source-only/broker-unimplemented statements below describe earlier checkpoints and are superseded by the final continuation. Native desktop acceptance and full original-plugin parity remain separate.

Scope: only `plugins/quarkus/src/qute.rs`; parent owns manifest/Providers/diagnostics wiring. No host/API/Laravel changes. Public behavior verified from https://quarkus.io/guides/qute-reference/ (include section, injection and type-safe templates) and https://www.jetbrains.com/help/idea/quarkus.html . No source copied from proprietary originals.

## Implemented

- Static `{#include template /}` and paired include source navigation in conventional module `.html`/`.txt` Qute templates. Exact physical ID caret only; explicit suffixes and extensionless unique variants resolve from the bounded fresh cache. Dynamic `_id` parameters, fragments, unsafe paths, comments, raw blocks and escaped open braces do not become source references. Multiple target variants remain ambiguous and quiet.
- Exact owned `@Inject Template field` declaration navigation for jakarta/javax injection with Qute Template imports or FQNs. Field name derives the default template ID; no library/member inference or Rename anchors introduced.
- Extensionless `@Location("id")` read-only navigation. This deliberately does not loosen the existing explicit-extension empty-file creation contract. Existing known-namespace, static-field, dynamic-string and source scope restrictions remain.
- `@CheckedTemplate` static native method declaration navigation: top-level empty base path, nested static-class enclosing simple-name base path, explicit literal basePath. Exact TemplateInstance result ownership, non-overloaded method and non-generic known source required. Known owned ELEMENT_NAME/HYPHENATED_ELEMENT_NAME/UNDERSCORED_ELEMENT_NAME strategies are supported. Unknown naming expressions, fragment methods and unknown ownership stay quiet.
- Module-scoped Qute section snippets (`if`, `for`, `each`, `let`, `with`, `include`, `insert`, `fragment`, `when`) plus known include-path completion. No template language registration replaces native HTML ownership. Completion skips comments/raw/escaped regions, replaces an existing closing brace, and never scans files/processes/HTTP.
- Current-text source include Info diagnostics against a complete fresh conventional-template inventory. The message explicitly allows custom runtime locators. Cold/incomplete/expired inventory remains quiet; an unsaved fix clears the note immediately. These are source facts, not runtime missing-template assertions.

## Verification

`cargo test --locked --manifest-path plugins/quarkus/Cargo.toml` and fresh release Wasm build pass (final count supplied by parent; this file's delivery added seven tests + existing test adjustments and was green at 170 before parent added additional catalog regression). Logs `/tmp/parity-quarkus-qute-test.log`, `/tmp/parity-quarkus-qute-build.log`. No native desktop acceptance claimed.

Tests cover UTF-16 caret mapping, unique/ambiguous variants, dynamic `_id`, comments/raw/escaped tags, exact identity, wrong namespace/generic shadows/static fields, extensionless navigation without creation widening, source-local @CheckedTemplate mappings, template-only completion, source diagnostics clearing and stale-cache silence.

## Concrete remaining feasible features

- Qute fragment declaration/call navigation and method call-site resolution remain. Owned @CheckedTemplate defaultName strategies and TemplateInstance record IDs are now implemented (see continuation below).
- Qute section closing-name completions and full section/else/optional-end validation; template parameter declarations, local lexical expression completion/usages, inserted named-block navigation.
- Java-to-Qute inferred data models, checked expression member/overload resolution and converter/default-value semantics require an optional semantic compiler/LSP adapter or substantial independent source model. They are not closed by source navigation.
- Supporting custom template roots, configured suffix lists and TemplateLocator implementations requires explicit source/runtime model; current model intentionally states conventional module sources only.
- Quarkus/MicroProfile language-server registration, Java code lenses, runtime/dependency config values and broader JAX-RS symbols remain separate missing adapter/features. Parent owns metadata/source model work outside this file.

Host-side surfaces that no plugin source change supplies: arbitrary debugger auto-attach/evaluation and native framework-facet/run-dashboard integration. Those are distinct from the plugin-side work listed above. This delivery does not establish full feasible parity.

## Continuation: native Qute LS feasibility and additional typed source IDs

Implemented exact owned CheckedTemplate naming constants (ELEMENT_NAME, HYPHENATED_ELEMENT_NAME, UNDERSCORED_ELEMENT_NAME), including static imports and conflict checks. Added TemplateInstance record-derived IDs for top-level/nested records with default or literal basePath/name strategies. Tests cover positive paths plus foreign return types, constant owners, class fields shadowing constants and unknown annotation expressions. No Java runtime type/member model is inferred.

Final package test count: **172 passed**, zero failed. Fresh release Wasm build passes. Current test/build logs above include this continuation.

### Actual server experiment

Fetched official EPL-2.0 source `redhat-developer/quarkus-ls` at commit `2add98b0c605cb300b10e5e298de0012138586e0` and Apache-2.0 official VSCode client source at `e9aed7004d4748470ded0a0b760af272ea94fa38`. Read their licenses before analysis. Built actual Qute LS using `mvn -f /tmp/parity-qute-ls-source/qute.ls/com.redhat.qute.ls/pom.xml -DskipTests package`; build succeeds, yielding actual `com.redhat.qute.ls-uber.jar` (0.26.0-SNAPSHOT). This is a source-feasibility probe, not the frozen latest original plugin version or a shipped adapter dependency.

Launched actual server using Java stdio framing, initialized it, opened a fixture Qute HTML document with a Java parameter declaration, and requested completion. Probe answered custom requests using the exact API1.4 native LSP unknown-method policy. The server initialized, then requested `qute/template/projects` and `qute/template/project`. Native-style METHOD_NOT_FOUND replies left the completion result `items:[]`. Evidence: `/tmp/parity-qute-native-client-probe.py`, `/tmp/parity-qute-native-client-probe.json`, `/tmp/parity-qute-ls-build.log`. No native OpenIDEA app was started by this probe; its client response policy is reproduced, not claimed as desktop integration.

### Concrete transport and semantic boundary

Released OpenIDEA commit37c2a60 `crates/ide-lsp/src/handler.rs:310-347` handles standard server-to-client requests and rejects unknown methods. The API1.4 Wasm plugin SDK does not expose custom server-to-client LSP request callbacks or queries into the host's Java LSP session.

Official Qute LS `QuteLanguageClientAPI` requires additional callbacks for project/projectDataModel/javaTypes/resolvedJavaType/javaDefinition/javadoc/generatedJavaMember plus Java diagnostics/lenses/links. The official VSCode client `src/qute/languageServer/client.ts:65-99` forwards these to JDT `workspace/executeCommand`; `qute.jdt/com.redhat.qute.jdt/plugin.xml:20-39` supplies the corresponding JDT extension commands.

Therefore a static Qute language-server launch registration alone would not implement full typed Qute intelligence. It was deliberately not shipped as a pretend working adapter. There are two concrete integration paths:
1. A host custom-LSP-request bridge (outside user-authorized host/API scope).
2. An optional external **dual-process Qute+JDT bridge** that loads qute.jdt bundles into its own JDT-LS session, routes the documented callbacks to workspace/executeCommand, propagates Java project import/model changes and handles editor snapshots, cancellation, file changes and shutdown. This path is feasible plugin-adapter work, **not an API-proven impossibility**; it remains unimplemented and unvalidated. Startup or mocked callbacks cannot close it.

No complete typed-Qute model or full feasible parity is claimed by these source-local additions.

## Further continuation: JAX-RS/scheduler lenses, symbols and Java annotation inlays

Added `plugins/quarkus/src/source_features.rs` and parent wired API1.4 language features/manifest contributions. Features:

- Canonically owned `jakarta.ws.rs` / `javax.ws.rs` method annotations and exact literal class/method `@Path` produce declared JAX-RS route lenses and document symbols. Class root ownership must be known. Missing/dynamic paths and foreign annotation names stay quiet. The lens shows declared source path, not a constructed runtime URL.
- Canonical `io.quarkus.scheduler.Scheduled` literal cron/every values produce scheduler declaration lenses and symbols. Expressions referencing configuration remain unknown. Inspection displays exactly declared scheduling facts, with runtime timezone/state/config disclaimers.
- Lens action opens a read-only source-evidence content tab. Dispatch rechecks focused editor path/language/version, project ownership, source method span and matching reconstructed fact. Stale/foreign documents refuse; no HTTP/process/job request occurs.
- Java annotation inlays show literal `@ConfigProperty(defaultValue=...)` on fields, method/constructor parameters and record components. On direct `@ConfigMapping` methods, exact `@WithDefault` and declared `@WithConverter(Foo.class)` expose source facts. Converter class spelling is labeled as a declaration, not resolved implementation or execution. No configured/runtime/secret values are read.
- Corrected an existing scheduler semantic mismatch: official current Quarkus scheduler reference says cron takes precedence when both cron and every are specified. Removed the false both-triggers diagnostic and suppress irrelevant every validation when cron wins. Explicit empty cron allows the declared interval. New tests verify this precedence rather than preserve a false positive.

Official behavior references: https://quarkus.io/guides/scheduler-reference/ and https://smallrye.io/smallrye-config/Latest/config/mappings/ . No original implementation source was copied.

Final current-source package tests: **177 passed**, zero failed. Fresh release Wasm build passes. Logs: `/tmp/parity-quarkus-source-test.log`, `/tmp/parity-quarkus-source-build.log`; `git diff --check` passes for owned files. New module formatted with rustfmt. No desktop rendering/acceptance claim.

Remaining concrete limitations: JAX-RS runtime application roots, custom HTTP method annotations, resource inheritance and programmatic registration need semantic/runtime models. Scheduler expression resolution, effective timezone/cron type, next-fire-time computation and runtime job management remain separate work. Java nested mapping-group ownership, complete annotation escape decoding and converter resolution/execution remain unimplemented. Declared source facts and passing tests do not close full typed language-server parity.


## Final continuation — actual optional Qute/JDT broker (2026-11-11)

### Shipped implementation

New files live entirely in `plugins/quarkus/adapters/` plus the new explicit-suffix `plugins/quarkus/languages/qute-html.toml`. Parent wired manifest activation/provider languages. The broker launches official Qute LS and an isolated private JDT LS session with official Qute/LSP4MP JDT extension bundles. It translates the exact documented Qute Java model callbacks to `workspace/executeCommand`, forwards Java unsaved document changes and watched events, and routes JDT model-change commands back to Qute.

A Java document-symbol synchronization barrier precedes clearing Qute Java model/type caches on open/change/save/close and watched Java changes. Template requests wait behind this barrier; failed synchronization produces errors rather than stale typed responses. The bridge waits for real JDT `ServiceReady`, namespaces endpoint/client IDs, handles cancellation, validates bounded framing, limits queues/requests, times out stalled initialization/model requests, and shuts down both owned backends. Private session data/configuration avoids shared JDT workspace locks and is removed on shutdown. Source text is not logged by the broker.

`create_profile.py` checks reviewed tool versions, pins the selected Java executable/runtime version, complete JDT plugin JAR inventory/configuration and Qute/MicroProfile extension artifacts by SHA256. It writes a new explicit profile only. It neither downloads nor executes a shell. `qualified-tools.json` records official distribution URLs and exact acquired archive hashes. No third-party binaries/source were committed.

`openidea-settings.example.toml` documents the actual released-host custom-LSP settings interface: one shared server name serves Java and Qute HTML; the opted-in project's builtin jdtls is disabled because the broker already owns a private JDT. Generic Java/HTML language ownership remains. Only `*.qute.html` automatically selects the new language; conventional `.html` templates use explicit project-relative `files.fileTypeOverrides`. The example is merged into `.openidea/settings.toml`; no user's settings were changed in this implementation.

### Actual integration results

Reviewed official tools:
- vscode-quarkus **1.24.0-923**, Qute LS/JDT **0.25.0**.
- vscode-microprofile **0.18.0-844**, LSP4MP JDT **0.18.0**.
- Eclipse JDT LS **1.62.0-202610131109**.
- Java **26.0.1**, Python **3.14**, macOS aarch64.

Actual checked-in integration tests use a disposable Maven **Quarkus3.40.1** project and real server binaries. There is no explicit template-side parameter type declaration; Java `@CheckedTemplate` supplies the model. Each fixture checks:
1. Qute completion sees Java `name` and `age`.
2. An **unsaved** Java field change replaces `name` with `displayName`, with the old field absent while disk still contains `name`.
3. Restore/discard and close returns the disk `name` model.
4. A watched external saved-file edit replaces `name` with `diskTitle`.
5. Disk restoration returns `name`, with `diskTitle` absent.
6. Clean shutdown returns broker exit0.

Two real fixtures pass: normal nested @CheckedTemplate base path and explicitly empty basePath. Investigation exposed an upstream0.25 empty-base data-model path containing `templates//hello`; the bridge canonicalizes repeated separators only in conventional physical-source model paths, and never rewrites custom locator IDs or traversal paths. Both variants now pass all5 freshness phases.

Final adapter test run: **14 tests passed** (12 protocol/unit tests plus2 actual tool integration tests), in **18.450s**. Actual receipts: `/tmp/parity-qute-broker-delivery/live-qute-default-receipt.json` and `live-qute-empty-base-receipt.json`, each5 phases and exit0. Test log: `/tmp/parity-qute-broker-final-tests.log`. Generated fully pinned tested profile: `/tmp/parity-qute-adapter-tools/generated-profile-v2.json`. Standard tool-free test invocation skips the2 explicitly opt-in actual-tool tests.

Current Quarkus Rust: **177 tests pass**, fresh release Wasm build passes. Logs `/tmp/parity-quarkus-final-tests.log`, `/tmp/parity-quarkus-final-build.log`. New language/settings TOML parses; `git diff --check` passes.

### Precise remaining boundaries

The transport blocker is now addressed by the optional external bridge; it must not be retained as an API-only impossibility or described as unimplemented. Complete original-plugin parity is still not established: native desktop custom-server/file-type setup needs acceptance; other OS/runtime/tool profiles, multi-module/Gradle imports, changed dependency/classpaths, the complete inherited/generic/custom Qute data model, all refactoring/client-only command paths, Java-embedded template routing, and full source/library navigation need dedicated journeys. Quarkus/MicroProfile Java custom language-server extensions beyond Qute callbacks are not silently supplied by this adapter. Existing source-local features remain independently available without the adapter.

## Latest continuation — official MicroProfile routing and live unsaved configuration model

This section supersedes the earlier Qute-only limitation regarding MicroProfile custom callbacks and all previous test counts.

### Implemented and exercised

- Added `microprofile_broker.py`, an optional official third language server sharing the existing private JDT model. It maps all15 exact LSP4MP0.18 custom project/Java callback names to the actual JDT commands, waits for ServiceReady, and resets MP model caches following Java barriers.
- Properties configuration completion, resolve, validation, hover and navigation route to official LSP4MP0.18 plus Quarkus0.25. Java completion/hover/definition/actions/lenses/inlays combine JDT and MP results with explicit backend ownership on opaque resolve data. JDT class source virtual URIs and workspace-symbol requests are routed to JDT.
- Added optional reviewed `jdt-overlay` companion source built against actual JDTLS1.62/LSP4MP0.18 public APIs. It replaces existing official PropertiesConfigSource entries using the project's public mutable source list, preserving source name/URI/ordinal/profile/order. Exact replacement permits deleted keys to disappear instead of being resurrected from the disk source. No user project content is written.
- The broker keeps bounded private UTF16-aware unsaved buffer snapshots. Before each opaque MPJDT callback it synchronizes the exact generation, requires receipt coverage of every dirty URI, rejects incomplete/unsupported receipts, and rejects responses after configuration changes during analysis. Java merged responses drop the MP portion when generation changes. Cancellation and timeout apply to snapshot barriers. Clean snapshots restore official sources after buffer close/save.
- Only existing official local properties config sources are supported by the companion; invalid/unrecognized/ambiguous/oversized snapshots produce unknown/errors, never guessed effective Java facts. Without the companion, Java MP semantic assistance is withheld while config buffers are dirty. MPJava diagnostics are always suppressed while config dirty because unsolicited results carry no snapshot-generation token.
- `create_profile.py --with-microprofile --config-overlay-bundle ...` pins official MP/Quarkus LS and companion artifacts, plus the entire reviewed runtime/JDT distribution. Actual companion final reproducible JAR hash: `b621bdebdd315f72c1e4f2b7af40a7880b4398c3bb3f290f9c009135d200e4b1` (build output ignored, not shipped as binary).
- Children inherit a restricted runtime/OS environment. Additional names require explicit profile flags; this prevents automatic MP environment completion from enumerating the agent's credentials/environment. Receipts retain only selected test-project keys/values.
- Parent wired explicit `quarkus-properties` language alias with no suffix ownership and native local provider fallbacks. A new Rust test confirms alias local hover/diagnostics/inlays/completion. Parent wired Qute folding for html/plaintext/qute-html. Settings example documents project-only shared Java/Qute/configuration broker activation with fileTypeOverrides and `microprofile.tools` settings.

### Actual acceptance evidence

All actual tools are those version/hash-pinned earlier; profile `/tmp/parity-qute-adapter-tools/generated-profile-overlay.json`. `python3 -m unittest discover -s plugins/quarkus/adapters -p 'test_*.py'` with both actual profile environment variables runs **28tests, all pass, zero skips**, in28.204s. Log `/tmp/parity-quarkus-overlay-all-tests.log`.

Actual MP fixture has **11phases** in9.36s, exit0:
1. Java ConfigMapping metadata produces `greeting.message` and `greeting.count`.
2. Unsaved Java mapping rename produces `greeting.welcome`, removes old key, disk unchanged.
3. Restoring/closing Java restores saved mapping keys.
4. Unsaved properties integer conversion failure emits real SRCFG00029.
5. Unsaved properties correction clears it.
6. Java ConfigProperty hover reports configured disk value `hello`.
7. Java inlays report effective configured default `hello` and BuiltInConverter.
8. Unsaved config changes Java inlay to `unsaved-current`, with disk unchanged.
9. Deleting that key in unsaved config removes both unsaved and saved values from Java hover.
10. Closing the config buffer restores disk effective Java inlay `hello`.
11. MP completion resolution returns to the originating backend.

Receipt and backend stderr: `/tmp/parity-microprofile-overlay-delivery/live-microprofile-receipt.json`, `live-microprofile.stderr`. Same all-tests run also repeats both actual Qute5-phase Java-unsaved/disk-watch lifecycle fixtures against the three-server+companion profile. Receipts in the same directory.

Current Rust **180tests pass**, zero failed: `/tmp/parity-quarkus-overlay-rust-tests.log`. Fresh release Wasm build log `/tmp/parity-quarkus-overlay-wasm-build.log`. New TOML aliases/settings parse. Owned-source diff whitespace checks pass. Companion real-tool compilation and Java source assertions documented in `/tmp/parity-jdt-config-overlay.md`.

### Concrete remaining journeys and implementation boundaries

1. Native desktop acceptance of the project settings/custom server and actual HTML/properties editor behavior, including conflicts with independently configured LSPs, has not been run. Integration is explicitly user-installed opt-in; no user settings changed automatically.
2. Actual fixtures cover a single Maven project. Multi-module and Gradle workspace imports, dependency/classpath changes, inherited/generic/custom Qute extensions and custom locators need dedicated actual-tool acceptance, with fixes if those journeys fail.
3. Overlay only replaces official recognized existing PropertiesConfigSource entries. Supporting new unsaved source files, YAML, external sources and third-party ConfigSource providers requires explicit provider identity/precedence contracts and new source adapters. They are currently refused, not an API-host impossibility.
4. Upstream background cache eviction/import could race after a successful synchronization command; the public command barrier is not an atomic transaction across arbitrary JDT background work. Current real fixtures establish serialized editor journeys only.
5. Unsolicited MPJava diagnostics cannot prove which configuration generation produced them; they remain suppressed while any config is dirty. Native requested hover/inlays have guarded generation coverage.
6. Qute Java-embedded template routing, client-only commands, complete source/library refactoring/navigation, and all original native UI journeys remain unqualified. Workspace symbol/virtual source routing is implemented, but not an exhaustive navigation acceptance suite.
7. Source-local JAX-RS runtime application roots/custom HTTP annotations/inheritance and scheduler effective expressions/timezones/next-fire/runtime management still require additional semantic/runtime behavior. Existing exact source lenses and declarations remain available without tool installation.
8. Other OS/tool/runtime profiles remain unqualified. Reviewed profile generation enforces current versions rather than silently claiming arbitrary future tool compatibility.

The original native custom-callback transport gap is concretely addressed by the optional broker and companion, without host or API changes. These results close the exercised typed-model and unsaved-properties journeys; they do not establish exhaustive original-plugin parity.
