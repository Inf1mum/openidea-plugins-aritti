# Editor/tooling parity delivery — checked implementation progress, 2026-11-11

Scope: dotenv, ignore, properties, ideolog, tasks and quarkus. No Laravel, host/API, shared crate or shared-script edits. Root separately synchronizes generated shared UI. Existing source work was preserved. This record supersedes the earlier finite checkpoint descriptions in the child reports; it does NOT claim that every feasible original feature is closed.

## Latest baseline and original contracts

Official Marketplace refresh retains dotenv263.7307.209, Ignore4.5.7, Properties263.7307.108, Ideolog263.7307.101; root records Tasks/Quarkus metadata centrally. The original inventory in docs/audit-gaps-2026-11-06/D-tooling.md covers160 feature groups for these six packages (199 including GitLab, which is assigned elsewhere). Those old Done/Partial/Missing totals are historical, not promoted by current test counts. Primary public source references are in the child reports and adapter docs.

| Original contract family | Current implementation | Remaining distinction |
|---|---|---|
| dotenv declaration/completion/inspection/navigation | Existing source providers plus Compose flow/multiline inputs, environment anchors and merge precedence | Runtime loader ownership is not inferred from lexical equality |
| dotenv env_file discovery | Full admitted local YAML alias/nested merge graph, optional-file handling and confined source reads | Environment physical-range closure remains unknown where expanded keys cannot be mapped; runtime precedence is separate |
| Ignore pattern matching, templates and creation | Git-correct whitespace/excluded parents; native atomic multi-template creation and preferences retained | Native ignored-tree filtering/arbitrary editor closure absent |
| Ignore unused entries | Opt-in complete ordinary-directory capture, native cached snapshot diagnostics and reviewed source removal | ASCII glob model only; unsupported patterns withheld; snapshot presence is not effective winning-rule/future-file semantics |
| Properties locale/bundle editing | Multi-locale create/rename/delete; Unicode conversion; staged editable all-locale grid with atomic Apply intention; native comment/continuation/prefix-group folds | Native desktop grid/Undo acceptance remains separate |
| Properties source inspections/navigation | Optional actual JDK compiler graph for canonical ResourceBundle/@PropertyKey contracts, approved JAR dependency profiles, complete physical inventories/current unsaved snapshots and exact reviewed atomic Rename | Kotlin, custom loader/constant-key/inherited contracts and automatic semantic graph remain concrete work; no unchecked Rename index |
| Ideolog custom regex/date formats | Actual JVM parser/semantic cache, owned secure finite live-tail/filter/follow/stop UI, generic stacktrace folds and nested selection expansion | Native arbitrary style/hide/heatmap/terminal hooks remain distinct; alternate-tail native visibility/Windows profile acceptance remains |
| Tasks search/documentation/state | Native six trackers plus Lighthouse; actual approved FogBugz credential-profile XML bridge with reviewed optimistic close; bounded descriptions/ADF and cached source hover | Bugzilla/Trac/Redmine/Mantis/Generic and self-hosted transports remain feasible adapter work; Pivotal service EOL distinct |
| Tasks VCS/context/commit | Existing branches/stashes/native file context and reviewed commit draft integration retained | Native changelists/full breakpoint-bookmark-layout restore and post-successful-commit hook absent |
| Quarkus config/source facts | Gradle catalogue/source facts plus optional official MicroProfile LS/JDT broker, typed config validation/conversion and effective Java hover/inlays with unsaved source overlay | New/YAML/external source overlays, multmodule/Gradle and runtime source-lens journeys remain |
| Quarkus Qute typed editor | Hash-pinned official Qute↔JDT broker with native opted-in project setup and actual type freshness tests | Native desktop journeys and additional editor/model protocol journeys remain separately unverified |

## Implemented changes

### dotenv

- Multiline Compose environment lists/maps and env_file paths preserve physical key ranges where available; comments, quotes/CRLF and nested source ambiguities are handled explicitly.
- Environment anchors/aliases and merge precedence preserve original declaration locations. Explicit keys override inherited keys; merge sequences use first-source precedence.
- env_file discovery uses pure serde-saphyr2.1.1 with filesystem includes/property interpolation disabled. Local alias and nested service-merge graphs resolve scalar/list/long-form paths. Missing required:false paths are admitted only when no other source requires that path. Paths remain confined to project scope.
- Explicit limits:256KiB source; depth32;20,000 nodes;100,000 events;128 anchors/aliases;1MiB scalar/retained anchor bytes;20,000 replay events;128 expansions/anchor;128 distinct file paths. Undefined/cyclic aliases, duplicate mapping keys, unsupported tags and exceeded budgets keep discovery incomplete. Expanded environment names must match the physical-range model before completeness is admitted.
- Actual parser tests verify node/depth/input/alias bombs and scalar-byte charging during alias replay, before JSON-tree expansion.

### Ignore

- Pattern whitespace and excluded-parent negation match Git behavior; a child cannot restore an entry beneath an excluded parent.
- Atomic Create ignore file from templates intention chooses an owned directory, supported filename and up to16 templates, previews exact combined content and refuses collisions/cancel/stale sources.
- New Find Unused Entries from Complete Directory Snapshot command runs an optional approved python3 -I -S helper. It imports no project application, scans all names including hidden/ignored/Git metadata twice, follows no symlinks, propagates entry/read errors and requires identical metadata manifests. Limits4096 entries/512 directories/depth24/8seconds/1MiB result; output is atomically completed in private storage, not inferred from lossy stdout.
- Admitted unmatched ASCII glob rules yield source/version/root-bound native snapshot hints and exact-reviewed undoable removal actions. Matching conservatively folds case; unsupported patterns are counted and withheld. Providers perform no inventory/process work. Delivered filesystem/source/settings events invalidate observations, and expired cache cannot authorize an action. Results explicitly describe capture time rather than future-file guarantees.

### Properties

- Atomic creation of up to32 locales; reviewed current-locale rename/delete; lossless native/Unicode source conversion including non-BMP surrogate escapes and structural delimiters.
- Shared discovery/create/rename/reference locale grammar covers ISO639-2 codes, scripts, regions and Java variants with retained source/license attribution.
- Native Edit All Locales staged table edits values across every visible locale, missing translations, bundle-only key rename and insert/delete rows. Source remains unchanged until an exact-reviewed multi-file FileAction is applied. Continue to Apply opens the source for its explicit native Apply staged bundle changes intention; direct view workspace-apply is not exposed by API1.4.
- Draft generation/source/root/version checks preserve inputs on cancel/stale changes, stale grids become read-only, and retirement requires observing every proposed source text. Project Discard Staged Locale Grid also recovers drafts after the original source was renamed/deleted.
- Root independently exercised the real released host+Wasm native structure/bundle and French cell staging with real temporary documents: /tmp/non-laravel-parity-implementation/runtime-properties-grid/results.json . This is backend runtime/model proof, not desktop visual acceptance.

### Ideolog

- Numeric timestamp parser additions:12-hour clocks, variable fields, alternate hour conventions, milliseconds, RFC822 offsets and explicit UTC/GMT zones.
- Optional actual Java17+ helper implements Java Pattern and SimpleDateFormat with explicit/default JVM locale/timezone; no application bootstrap. Reviewed capture uses private helper/input files,64KiB/512line input,256KiB output,15second deadline/cancellation and input clearing.
- Native observations show source provenance, epoch/time gaps and guarded navigation. Exact-source cached observations feed built-in Log semantic tokens and existing explicit Mark path. Only uniquely located, nontruncated first-line fields with exact UTF16 physical spans are admitted.
- Providers never launch processes or read source/pattern storage. Path/text/version/deepest owner/current setting/warmed configuration and lifecycle eligibility must match. Cross-instance stored-format edits become visible after refresh/lifecycle; no independent storage revision is claimed.

### Tasks

- Optional closed GitHub/GitLab/Codeberg/Lighthouse explicit search; automatic listings stay open-only. Search-policy changes invalidate cached page ownership.
- Descriptions retained from all supported trackers, with bounded Jira ADF text/paragraph/break flattening, no link/media evaluation,32KiB cap/truncation marker and escaped cached hover excerpts. Refresh follows summaries/descriptions together.
- Lighthouse Cloud adapter uses documented XML search/page and resolved-state update with X-LighthouseToken. Strict tenant/numeric project/API/web checks and account/project-qualified IDs prevent cross-target updates. Malformed/foreign/incomplete/DTD XML pages refuse, including entity and nested-version handling.
- Actual unchanged released v0.1.40 capability parser/matcher compiled independently proves net:*.lighthouseapp.com permits constrained subdomains and rejects apex/lookalikes/net:*. See /tmp/lighthouse-host-permission.log . API behavior is fixture-tested, not authenticated-customer acceptance.
- Current official Lighthouse login is live; FogBugz documentation is live but native header-only token substitution cannot satisfy its required token argument/body. A separate authenticated bridge remains feasible. Pivotal's original EOL URL now redirects to an offering sunset record; no fake live adapter was added.

### Quarkus

- Conventional Gradle plugin catalogue resolution uses exact io.quarkus IDs, real top-level plugin alias calls, normalized unique accessors and complete TOML parsing. Deferred apply-false/custom/dynamic catalogues do not prove module ownership. Catalogue/settings changes invalidate cached module proof before warming.
- Static Qute includes, implicit Template injection, extensionless Location, CheckedTemplate methods/name strategies and TemplateInstance records navigate to admitted exact templates. Cached section/include completion and current-text source include Info notes preserve ownership/absence limits.
- Exact canonical JAX-RS/scheduler source facts drive lenses/document symbols with revalidated read-only source inspection. Java ConfigProperty field/parameter/record defaults and ConfigMapping WithDefault/WithConverter facts drive source-only inlays. Corrected scheduler contract: cron takes precedence over every.
- Actual official Qute0.25/Eclipse JDTLS1.62/official extension bundles are brokered through documented callbacks and JDT commands. Runtime/profile/artifact hashes, whole-JDT plugin distribution hashes, fresh private per-session Java workspaces, native project TOML examples, profile builder and repeatable integration tests are shipped under plugins/quarkus/adapters.
- Native setup shares one opted-in broker across Java/qute-html; only exact qute.html suffix is contributed, ordinary templates use explicit file overrides. Generic HTML/Java ownership is not replaced by activation.
- Two actual Maven Quarkus3.40.1 fixtures (default and explicit empty CheckedTemplate base) prove inferred-model Java completion and five-phase freshness: original member→unsaved rename/old member absent→restore+close→watched disk rename→disk restore. Both exit cleanly. No native desktop acceptance claim.

## Verification

ALL SIX pass /tmp/non-laravel-parity-implementation/editor-complete-20261111/results.json : Rust tests, fresh release Wasm, actual API1.4 component dispatch, full installer schema/assets validation and source/shared stability. Counts: dotenv59 +Ignore135 +Properties137 +Ideolog113 +Tasks96 +Quarkus180 =720 Rust tests. Additional adapters:23 actual Properties JDK tests,11 actual FogBugz HTTP/XML tests,28 Quarkus protocol/real-tool tests (zero skips), including two actual Qute five-phase lifecycles and11-phase actual MicroProfile unsaved Java/config/disk restore. Ideolog actual Java secure-tail and Chromium UI fixtures pass. Source-only companion cleanup is separately rechecked at /tmp/non-laravel-parity-implementation/editor-final-clean-20261111/results.json.

All six manifest TOMLs parse; scoped git diff --check passes. Generated Python bytecode caches were removed from plugin resources. No package was installed/published and no external tracker or real database was mutated.

Detailed evidence:
- /tmp/parity-env-ignore.md (earlier parser/template checkpoints; above supersedes old env_file/unused residuals)
- /tmp/parity-properties-ideolog.md
- /tmp/parity-ideolog-jvm.md
- /tmp/parity-quarkus-qute.md
- /tmp/parity-qute-broker-delivery/live-qute-default-receipt.json
- /tmp/parity-qute-broker-delivery/live-qute-empty-base-receipt.json
- /tmp/lighthouse-host-permission.log
- /tmp/properties-grid-{test,build}.log
- /tmp/parity-dotenv-graph-{test,build}.log
- /tmp/parity-ignore-complete-{test,build}.log
- /tmp/parity-tasks-lighthouse-{test,build}.log

## Remaining implementation vs host acceptance

Still actionable plugin/adapter work: Properties Kotlin/directory-modulepath/custom loader/constant-key/inherited contracts and automatic live graph; Quarkus new/YAML/external config-source overlays and multi-module/Gradle/model refactoring journeys; Bugzilla/Trac/Redmine/Mantis/Generic and self-hosted tracker bridges; full environment physical-range/runtime-loader ownership; Unicode/unsupported-pattern unused-rule models. Owned live tail, declared-Java/compiler JAR graph and official MicroProfile typed unsaved-properties assistance are now implemented and exercised. Detailed concrete residuals: /tmp/parity-properties-compiler-profile.md, /tmp/parity-quarkus-qute.md, /tmp/parity-ideolog-live.md, /tmp/parity-fogbugz-bridge.md. These are not blanket API blockers.

Host-only original surfaces remain distinct: native bundle synthetic grouping/encoding chooser, arbitrary original editor background/font/stripe rendering and line hiding/heatmap, arbitrary terminal filters, complete native task changelist/bookmark/breakpoint/layout/after-commit integration. Root must retain current native desktop acceptance and authenticated service acceptance as separate evidence requirements. None of the720 test passes or supported adapter workflows establishes unconditional full original-feature parity.

## Latest semantic/runtime continuation (supersedes earlier partial implementation bullets)

Properties compiler dependency/profile/source details and exact revalidation proof: /tmp/parity-properties-compiler-profile.md. Ideolog actual owned-tail/filter/fold/selection journey: /tmp/parity-ideolog-live.md. FogBugz external credential-profile transport/native integration: /tmp/parity-fogbugz-bridge.md and plugins/tasks/src/fogbugz.rs. Official MicroProfile0.18/Quarkus0.25/JDT1.62 broker plus public config-source overlay companion: /tmp/parity-quarkus-qute.md and /tmp/parity-microprofile-final-delivery/live-microprofile-receipt.json. Generated companion JAR is retained only under /tmp/parity-qute-adapter-tools/overlay-built; plugin ships source/builder/profile instructions. All current source behavior stays within six package directories.
