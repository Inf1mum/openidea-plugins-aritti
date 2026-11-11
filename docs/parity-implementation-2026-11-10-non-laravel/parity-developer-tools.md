# Developer tools parity implementation — 2026-11-10

Scope: GitLab, OpenAPI and Dependency Audit; released API1.4/SDKv0.1.40. No Laravel, host, SDK or shared component edits. Concurrent generated packaged-UI changes belong to root's synchronization work.

## Latest baseline freeze

- GitLab Marketplace22857: latest263.7307.200, same version as prior inventory.
- Package Checker18337: latest263.7307.101, same version as prior inventory.
- OpenAPI14394 has two active compatibility branches: latest-published262.12573.12/update1216942 (published2026-11-10; since262.12573/until262.*); newer-host branch263.7307.209/update1212896 (published2026-11-05; since263.7307/until263.*). A lexical version maximum or /updates?size=1 alone does not identify both. Metadata verified against /api/plugins/14394/updates?size=20. Root has latest metadata receipt.
- Current public behavior baseline: JetBrains IntelliJ IDEA2026.3 Help (pages read2026-11-10). No proprietary JARs or implementation copied.

Primary sources:
https://www.jetbrains.com/help/idea/openapi.html
https://www.jetbrains.com/help/idea/create-and-merge-gitlab-merge-requests.html
https://www.jetbrains.com/help/idea/package-analysis.html
https://docs.gitlab.com/cli/ci/trace/
https://docs.astral.sh/uv/concepts/projects/layout/
https://swagger.io/docs/open-source-tools/swagger-codegen/codegen-v3/about/

## Implemented

### OpenAPI

- Fixed three endpoint assertions to preserve tag plus operation identity (`Pets@listPets`); added activation assertion proving the same tagged row opens its source operation. Kept existing route labels and handler identity rather than removing the identity to satisfy historical tests.
- New `generate-code` command and optional `process:swagger-codegen` permission. Supports approved Swagger Codegen stable3.x, Swagger2.0/OpenAPI3.0, Java/Python clients, Spring server and HTML docs. Version probe precedes generation review. Destination is a new project-relative directory; traversal/options/control/shell characters and existing paths refused. Requires saved project inputs, active source version/path/project revalidation after review, and opens a keyed native terminal with separate argv. User controls cancellation/output in terminal. No install, silent invocation or overwrite.
- New `export-comparison` command creates `<spec>.comparison.md` through existing reviewed save/clipboard fallback. Preserves baseline/current provenance and structural rows. Refuses stale current source and changed baseline contents.
- Tests cover exact codegen argv construction, actual managed terminal registration, cancellation, invalid/occupied destinations, unsupported CLI and dirty source, exported content and stale current/baseline source.

### GitLab

- New selection-bound `Follow Job Log (glab)` pipeline toolbar action and optional `process:glab`. Requires approved glab1.x, independent CLI authentication, current repository/MR/head and fresh MR/pipeline/job membership before and after confirmation.
- Opens native managed ProcessLog running `glab ci trace <numeric-id> --repo https://gitlab.com/<project>`. Native host owns output/restart/cancel. Existing token-backed bounded trace snapshot stays available. No plugin token reaches argv/environment.
- Tests verify selected numeric job/repository argv, membership removal refusal before probing, and cancellation without opening a tab.
- Limits: glab credentials/configuration/host overrides are external process authority, explicitly described in review/README. Native log tabs do not run in remote projects. No live account operation performed.

### Dependency Audit

- uv.lock added to parser discovery and event subscription. Admits source-backed name/version records only for canonical PyPI registry URLs; local/editable/Git/URL/private-registry sources stay unknown. Exact source ranges and no lockfile text edits. Records do not assert platform-marker activation or installed reachability.
- npm package-lock v1 nested dependency tree now scanned, including multiple physical instances/versions of the same name and scoped packages. Structured source-member offsets avoid first-string-match locations. Traversal capped at32 nested levels/10000 entries; duplicate/malformed objects withhold results.
- package.json sibling resolution now reads v1 direct locked versions too. Existing v2/v3 behavior retained.
- Tests verify nested duplicate-name source locations, version sets, sibling manifest resolution, uv source classes and subtable isolation.

## Feature inventory and remaining implementation work

These are concrete remaining plugin gaps; none is relabeled host-blocked solely because implementation is substantial. Existing source inventory is not exhaustive latest method-level parity.

| Area | Current implementation / delivered subset | Exact next missing work | Classification |
|---|---|---|---|
| OpenAPI create/preview/templates | YAML/JSON templates, Swagger/Redoc custom preview | Complete original gutter Add-section actions and preview orientation controls | plugin work |
| OpenAPI local refs | Strict pointer completion/definition and read-only current-document component occurrences | Cross-file incoming declaration/reference closure and indexed safe usages/Rename (P13.2) | plugin model work |
| OpenAPI URL mapping draft | Own spec EndpointsProvider exports | Generate draft from framework mappings; current API has no generic cross-framework endpoint query; can integrate explicitly supported framework source adapters | adapter/plugin work; generic native query unavailable |
| OpenAPI remote specs | Local/declared-localhost workflows | Persist declared-host remote catalog, SwaggerHub adapter/private key workflow within declared net grants | plugin work for declared hosts; arbitrary hosts require manifest/capability changes |
| OpenAPI generation | Four reviewed SwaggerCodegen3 adapters | Generator discovery, configuration/options/custom template mapping, additional CLI dialect/version support | adapter work |
| OpenAPI comparison | Existing structural diff plus new Markdown export | Full changed parameter/response/section compatibility model, original quick-navigation report links | plugin work |
| GitLab lifecycle | MR create/edit/review/approve/merge/rebase/close, typed filters, branch/worktree/clone, suggestions, viewed files | Full multiple-account and fork-repository creation/checkout parity; richer form persistence and Markdown description/timeline rendering | plugin work with declared host/account grants |
| GitLab logs | Bounded token-backed snapshot plus approved glab managed following | Exact installed-version/platform native lifecycle acceptance and reconnect/output behavior | adapter acceptance; no live account fixture available |
| GitLab native review | Source navigation, cached diff positions/thread diagnostics | Original native editor Review Mode/diff/comment decorations and native git log actions | separate host integration plus possible owned approximations |
| Dependency ecosystems | Maven/Gradle literal/model subsets; npm/package managers; PyPI locks incl uv; Composer/Cargo | sbt model; full Maven parent/BOM/profile and Gradle configuration resolution; complete package-manager workspace/importer/marker/reachability paths | plugin/adapters, not closed |
| Dependency declared vulnerability actions | OSV, preview upgrade, scoped ignores, report details, before-commit cache | Shared ignore-profile import/export, direct safe-dependency list, richer root/module selection | plugin work |
| Vulnerable API/malware | OSV package advisory evidence only | Vulnerable-call facts and malware intelligence equivalent to original Mend-powered service | unavailable data/semantic adapter; OSV alone cannot supply this |
| Security tab | Owned advisory report | Register into original-equivalent native Problems Security tab | host integration unavailable |

## Validation

Logs: /tmp/parity-gitlab-tests.log, /tmp/parity-gitlab-build.log; /tmp/parity-dependency-audit-tests.log, /tmp/parity-dependency-audit-build.log; /tmp/parity-openapi-tests.log, /tmp/parity-openapi-build.log.
GitLab286 tests; Dependency Audit129 tests; OpenAPI228 tests expected on final run. Release Wasm build commands run for all three with --locked and wasm32-wasip2. Root's API1.4 component checker required for fresh exact-artifact package linking. No desktop/live installed CLI acceptance, account mutation, package install or publication performed.

Full feasible parity remains open for the concrete plugin-work rows above. These deliveries do not close entire P06.3/P13.2, nor substitute passing tests for complete latest-feature inventory.

## Exact adapter and package acceptance update

- Fresh reusable runner `/tmp/parity-developer-tools-final/results.json` initially passed all three plugins: Rust tests, fresh Wasm builds, released-host component compilation/linking and isolated staged installer validation. Repeating in `/tmp/parity-developer-tools-verified` after real-CLI probe correction.
- Downloaded official SwaggerCodegen3.0.85 from Maven Central, verified published SHA1 and recorded SHA256 in `/tmp/parity-developer-tools-cli/swagger-download.json`. An isolated executable wrapper ran the exact adapter generation argv against a disposable OpenAPI3.0.3 fixture. Java client35 files, Python client20, Spring server22, HTML docs3; all exits0. Receipt and exact args: `/tmp/parity-developer-tools-cli/codegen-results.json`. No existing destination reused and no project output touched.
- Downloaded official glab1.130.0 Darwin-arm64 release, verified publisher SHA256. `/tmp/parity-developer-tools-cli/glab-download.json` records source/checksum. CLI `ci trace --help` confirms numeric job argument, full repository URL and real-time output contract. Actual version prints `glab 1.130.0 (9b430a11f)`, so adapter probe was corrected to accept this current format as well as older `glab version 1.x`; regression uses exact observed version text. No authenticated GitLab request performed.

Final stable results: `/tmp/parity-developer-tools-verified/results.json` proves stable Dependency Audit129 and GitLab286 test/build/component/package checks. OpenAPI228 passed every execution check there but concurrent source drift prevented that receipt's stability gate; isolated rerun `/tmp/parity-openapi-stable-final/results.json` passes all gates including unchanged package/shared source fingerprints. `git diff --check` clean for the three plugin directories.

## Continuation tranche2 (implemented after initial report)

- OpenAPI Swagger Codegen reads the installed CLI `langs` catalog, validates its bounded source format and exposes all installed generator IDs instead of four hard-coded choices. Actual3.0.85 output includes39 generator IDs. `--skip-overwrite` is now always in generation argv. Actual CLI race fixture retained sentinel README, Java source and generator VERSION files unchanged; receipt `/tmp/parity-developer-tools-cli/skip-overwrite.json`.
- OpenAPI physical project component usages command discovers bounded JSON/YAML files, resolves exact same-project file/pointer identity, includes admitted array references and declaration navigation, validates source before opening, and explicitly warns about partial/unsupported discovery. This is read-only.
- OpenAPI reviewed visible-source component rename quick action returns one multi-file workspace edit only after complete admitted discovery, semantic/ref count agreement, exact physical declaration/reference spans, source revalidation and collision review. Unknown/dynamic/discriminator/security-scheme/encoded models refuse all edits. It is explicitly visible-source refactoring, not native global indexed Rename. Tests cover UTF16, cross-file same-name isolation, unchanged unrelated strings, collision/dynamic/unknown rejection and cancellation.
- OpenAPI root info/paths/components section additions are code actions on the version key, preserve source and refuse duplicate owners. Literal native gutter Add controls remain impossible through released LineMarker: SDK `providers.rs:147` has only line/icon/tooltip/target Location, no command callback. Existing code-action workflow is implemented; original exact native gutter action is not claimed.
- GitLab re-request assigned review command/toolbar uses the documented `mergeRequestReviewerRereview(projectPath,iid,userId)` GraphQL mutation with assigned reviewer choice, confirmation and fresh repository/head/reviewer checks. Server acknowledgement required. No live mutation was sent during testing.
- GitLab two declared account slots (primary existing token, optional secondary-token) use IDE secret placeholders, account-bound selection identity and cache invalidation. Unsent comments block slot switching until completed/cleared, preventing a previous account's draft being posted under another account. Current slot count is2; no claim of unbounded/self-hosted account management.
- Dependency Audit adds Pipfile.lock default/develop exact versions on canonical verified PyPI indexes. Private/ambiguous/local/Git records stay unknown.
- Dependency Audit new native npm dependency graph inspects v2/v3 physical package records, normal/optional/peer/root-dev edges, nearest nested module lookup, source activation, explicit unresolved linked workspaces and stale-source rejection. Expanded/collapsed native nodes survive refresh. Source navigation uses exact structured package-member spans and rejects duplicate owners; equal nested metadata keys cannot steal navigation. Input512KiB/2000-record/8000-edge bounds, optional/peer/platform activation is not evaluated.

Stable tranche2 receipt `/tmp/parity-developer-tools-tranche2-final/results.json`: GitLab290, OpenAPI236, DependencyAudit132 tests; all fresh Wasm/component/staged-package/source-stability gates pass. Dependency Audit has two subsequent correctness tests for exact navigation and native expansion (134 total); superseding receipt `/tmp/parity-dependency-graph-final/results.json`.

### Exact next public plugin-side contracts still open

1. OpenAPI: generator-specific configuration/options/custom template selection (`config-help -l <catalog-id>`), richer operation/response/path-context Add actions, remote specification catalog restricted to explicitly declared hosts, typed client URL completion from registered specs, full semantic compatibility reporting, and indexed Rename beyond the reviewed admitted-source transaction.
2. GitLab: unbounded named accounts and configured declared self-hosted server adapters; fork-repository source selection/checkout; richer Markdown descriptions/review drafts; original native editor/diff review controls. The first two have substantial plugin work within declared server permissions. Arbitrary unknown hosts cannot be admitted by a universal `net:*`: released capability parser allows exact hosts and `*.suffix`, rejects bare `*` (`ide-extension/src/capability.rs:250–270`), so configured server support must retain explicit manifest grants.
3. Dependency Audit: sbt declared/effective dependency model; Maven parent/BOM/profile and Gradle configuration resolution adapters; remaining PyPI/modern lock model support (pylock, lock marker/group activation); package-manager workspace/link/alias dependency resolution; shared ignore-profile import/export and safe-coordinate report rows. These remain plugin/adapter work, not host-blocked.
4. Original vulnerable-call/malware equivalence still needs a licensed/public supported intelligence source; the currently available OSV package records are insufficient. Native Problems Security tab registration and native generic semantic member queries remain actual host integration gaps.

No total original-feature closure is asserted from this tranche.


## Continuation tranche3 — verified current sources

Superseding stable receipt: `/tmp/parity-developer-tools-tranche3-verified/results.json`. GitLab294 + OpenAPI239 + Dependency Audit142 =675 Rust tests; fresh release Wasm, exact released component checks, isolated staged installer validation and unchanged plugin/shared fingerprints all PASS. Sources frozen for root integration; no generated UI assets synchronized independently.

- GitLab now renders the MR description and review discussion/pending bodies using the same bounded CommonMark renderer. Raw HTML is escaped, remote images become inert alt text, URL parser rejects non-HTTP(S), credentials and ambiguous/control-bearing input. Intercepted links must belong to the currently selected cached source before the host opens them. Shared UI tokens drive styles. All32 introduced registry dependency licenses/notices retained under plugins/gitlab/licenses/markdown. Production renderer browser fixture `/tmp/parity-markdown-browser/browser-results.json`: Chromium light/dark at320/960px, zero requests, zero image nodes, zero page errors, working safe-link bridge and no horizontal overflow. Dark320 screenshot visually inspected. This is renderer acceptance, not full OpenIDEA desktop acceptance.
- OpenAPI configured generation uses the installed bounded `config-help -l <generator>` option catalog, explicit property selection/values, reviewed owned template directory and supported template engine. Real3.0.85 configured Java generation used custom templates, apiPackage and library and produced expected source/template output: `/tmp/parity-developer-tools-cli/configured-java.json`.
- Important correction to earlier overwrite claims: SwaggerCodegen3.0.85 needs `--skip-overwrite true`; bare flag exits0 after usage error, so the earlier sentinel-only receipt did not demonstrate generation. Correct flag still rewrites `.swagger-codegen/VERSION`. The earlier tranche2 overwrite conclusion is withdrawn. Generation now targets a fresh stage only; successful terminal completion enables a reviewed native directory Rename to the still-absent final destination. The final directory is never sent to the CLI. Existing/stale destination/source/marker checks refuse apply. Real staging race fixture `/tmp/parity-developer-tools-cli/staged-race.json` proves successful generation while existing final README/VERSION/pom remain byte-identical. Host source confirms directory workspace Rename support; actual desktop apply/Undo remains unverified. Completed stages can be forgotten without deleting files; running terminals require native stop because API1.4 has no close-terminal call.
- Dependency Audit now has explicitly reviewed managed effective-build adapters: exact Maven3.9.16 with dependency-plugin3.8.1 JSON tree, and exact Gradle9.9.1 with owned init script/configuration selection. Inputs and dirty buffers checked, process output bounded, native poll/cancel, explicit completed-export audit, strict export ownership/schema/module checks, source revalidation around OSV and atomic old-report retention on failure. Editor providers remain cache-only. Real official distributions verified against publisher checksums in `/tmp/dependency-build-resolution-tools/download.json`; three-module disposable fixtures with imported JUnit BOM5.10.2, parent Guava32.1.3-jre property, internal project dependency and transitive failureaccess1.0.1 succeeded. Exact results `/tmp/dependency-build-resolution-tools/fixtures/results.json`; normalized real exports are Rust test fixtures. Root/module source provenance is honest build-file provenance, not an invented exact declaration span.

### Remaining concrete feasible contracts after tranche3

OpenAPI: arbitrary generator JSON config import for complex property values; source-context path/operation/response additions; declared-host remote specification catalog and typed client URL completion; richer semantic comparison; draft specification export from explicitly supported framework source adapters. Native global indexed Rename remains separate from current reviewed admitted-source transaction.

GitLab: GFM mentions/custom emoji/math, richer persistent review drafts; fork-source selection/checkout; configured declared self-host adapters; broader named account lifecycle beyond the two protected declared secret slots. Original native diff/editor Review Mode is a separate host integration gap.

Dependency Audit: sbt model; qualification of other Maven/Gradle versions and profile/config/module selection; effective-coordinate to exact declaration binding; modern PyPI marker/group/pylock semantics; package workspace/link/alias graph semantics; shared ignore-profile import/export; safe-coordinate rows.

Correction to older malware/call-analysis inventory: OpenSSF already publishes known-malicious package reports in OSV format (https://github.com/ossf/malicious-packages), so known-malicious classification is feasible public-data work and must not be dismissed wholesale as unavailable intelligence. Official OSV-Scanner docs (https://google.github.io/osv-scanner/usage/scan-source#scanning-with-call-analysis) expose Go call analysis via govulncheck and experimental Rust via compilation/DWARF; a reviewed, version-qualified optional adapter is feasible for these languages. This does not establish original Mend data equivalence or Java/Kotlin/Python call-analysis support. Those are next investigation/implementation tasks, not completed contracts.

No full parity closure is asserted. Live account GitLab acceptance, OpenIDEA desktop generation apply/Undo and full native managed-process interaction remain unverified.


## Continuation tranche4 — security and fork checkout (integration in progress)

GitLab reviewed official glab1.130.0 fork-source checkout is complete/frozen. Receipt `/tmp/parity-gitlab-fork-verified/results.json` PASS299 tests plus release Wasm/component/staged package/stable fingerprints. `/tmp/parity-gitlab-fork/runtime-fetch.json` verifies separate-fork fetch of a source-only commit and actual local branch/head checkout using disposable service fixtures and an explicit local URL rewrite. No live account mutation. Details `/tmp/parity-gitlab-fork.md`.

Known-malicious OpenSSF MAL evidence is implemented, not deferred. Actual querybatch PyPI0-8@0.0.0.1 returned MAL-2025-2925; official unchanged detail fixture and provenance/license retained. `/tmp/parity-known-malicious-receipt.json` records query, response, retrieval time, URL/checksum. Resource classes/reporters/dates, package-only diagnostic, suppressed automatic upgrades including historical retained reports, exact namespace and withdrawn-history behavior tested. `/tmp/parity-known-malicious.md` describes contract.

Optional official OSV-Scanner2.7.1 Go/Rust adapter integrated into command/native view, explicit pre-probe review, managed poll/cancel, bounded sources/output, source/dirty-buffer checks, separate dated evidence, prior evidence retained on failure. Actual official Go1.27.4 fixture observed GO-2022-1059 called:true for golang.org/x/text0.3.7 language.ParseAcceptLanguage and called:false for language.English.String. Rust Darwin actual runtime fails127 on Mach-O DWARF reader; Docker daemon unavailable. Experimental Rust failure remains Unknown; no false closure. Final fixture receipts/tests and combined runner pending.


Tranche4 final stable receipt: `/tmp/parity-developer-tools-tranche4-verified/results.json` PASS all gates for all3 packages: GitLab299 + OpenAPI239 + Dependency Audit152 =690 tests; fresh release Wasm, exact released component, staged installer package and unchanged source/shared fingerprints. Runtime scanner receipt `/tmp/parity-osv-scanner-tools/qualification.json` reproduces actual called/uncalled Go and actual Darwin Rust failure. Updated implementation requires explicit called facts to replace evidence; unknown-only/analysis-failed output cannot erase previous evidence. Seven scanner-specific tests include independent stdout/stderr split-error detection, managed lifecycle, prior evidence/source drift and foreign persistence rejection. Native call view now requires central generated adaptation synchronization; no owned generated assets were independently synced.

Sources frozen after final receipt. These receipts do not establish full original feature parity, live account GitLab acceptance, original native diff review/security tabs, successful Rust platform call analysis or OpenIDEA desktop managed-process/apply/Undo acceptance. Next feasible plugin work remains earlier OpenAPI source-context/catalog/config/framework contracts, GitLab declared self-host/richer-account/draft contracts and Dependency Audit sbt/modern lock/precise resolution binding/shared ignore profiles. Data equivalence beyond supported public evidence remains unclaimed.


## Continuation tranche5 — native Rust and sbt (final verification pending)

Official Rust prerequisite investigation is recorded in `/tmp/parity-rust-native-prerequisites.md`: pinned source directly uses debug/elf.NewFile; Cargo builds successfully and Xcode reads emitted dSYM, but official tool still127 with Xcode PATH. This is not a missing LLVM/binutils/cargo-auditable prerequisite. Original failed official result remains Unknown.

Separately named/versioned source-built `osv-scanner-macho` profile `2.7.1-openidea-macho.1` now exposes qualified native Darwin Rust evidence. `/tmp/parity-osv-scanner-macho/qualification.json`: actual safe smallvec0.6.9 grow fixture yields RUSTSEC-2019-0009 called:true and the same dependency without grow called:false. Advisory without function facts and absent alias facts remain Unknown. Implementation additionally fixes a discovered current Rustv0 receiver/generic normalization defect that otherwise falsely said uncalled. Current Cargo JSON artifacts replace stale .d guessing; exact owned UUID-matching bounded adjacent dSYM validation, missing/malformed debug rejection and no partial facts. Reproducible pinned-source adapter subtree/license/tests being finalized. Backend version-isolation/source-drift and normalized real-output tests pass; no unchanged official-binary success or exploitability inference is claimed.

Official sbt2.0.12 effective native resolution implemented/frozen. `/tmp/parity-sbt-tools/qualification.json` actual3-module export:317 module/configuration records,62 external coordinates; Guava property32.1.3-jre/transitive failureaccess1.0.1/Test JUnit5.10.2 validated. Internal Scala-cross-version project identities removed by exporter and independently filtered by parser. Owned session command runs UpdateReport without build-file edits; complete counts/header/root, CREATE_NEW output, saved-source and ownership checks, managed poll/cancel, explicit source-revalidated OSV audit retains old report on failure. Root build.sbt provenance does not pretend exact declaration binding. Evidence `/tmp/parity-sbt-resolution.md`; sbt1.x/platform/cross-build selection remain next concrete qualifications.

Current full Dependency Audit160 tests pass (`/tmp/parity-sbt-rust-combined-tests.log`); combined fresh Wasm/component/package/source-stability receipt pending final native helper subtree freeze. No total parity closure asserted.


Tranche5 final superseding stable receipt: `/tmp/parity-developer-tools-tranche5-final/results.json` PASS all3: DependencyAudit161 + GitLab299 + OpenAPI239 =699 tests; fresh release Wasm/component/staged installer/source+shared stability all PASS. Source frozen after final native builder version-probe environment isolation fix. Runtime exact isolated-built native helper receipt `/tmp/parity-osv-scanner-macho-final-runtime/qualification.json` PASS called/uncalled/unknown on final CGO0 binary. Pinned fresh source patch/build/reader tests/modules verification receipt `/tmp/parity-osv-scanner-macho/final-isolated-build.json`; exact patched-file hashes proof `/tmp/parity-osv-scanner-macho/final-patch-proof.json`. Source-helper evidence summary `/tmp/parity-osv-scanner-macho.md`. Missing/malformed/duplicate-UUID/truncated-after-valid-symbol negative readers pass; structured demangler avoids valid test-harness fn-arrow/higher-ranked-lifetime false refusals. Source patch/license/build/reproduction artifacts retained in owned adapters/osv-scanner-macho subtree; no global tool install or host/API edits. Other toolchains/platforms remain unqualified; original official Darwin failure still Unknown. Native desktop interaction remains unverified. Next feasible plugin contracts remain sbt1.x/explicit scope selection/precise declaration binding, newer lock semantics and shared ignore profiles, plus earlier concrete OpenAPI/GitLab contracts. No whole-parity closure asserted.
