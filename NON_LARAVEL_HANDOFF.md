# Non-Laravel parity checkpoint — updated 2026-11-11

Resumed 2026-11-11 after the pause; see "Session 2026-11-11" below. Full parity remains open.

## Scope and status

All 29 non-Laravel packages; latest public original-plugin baselines; released SDK v0.1.40 / API 1.4. Optional reviewed CLI/LSP adapters are authorized. No host/SDK/API changes, publication or version bumps. **Full parity remains open.**

The latest aggregate run (2026-11-11, released v0.1.40 CLI, results `/tmp/parity-next/results.json`) passed all 29 packages: 5,134 Rust tests, fresh release Wasm builds, released-host component linking and staged installer validation, with stable per-package source fingerprints. These are working-tree checkpoint results, not complete desktop acceptance or a claim that every optional external integration has universal coverage. See [machine receipt](docs/parity-implementation-2026-11-10-non-laravel/checkpoint-2026-11-11.json).

Real isolated host smoke tests also rendered native/custom views and exercised Properties staged edit → native FileAction → grouped Undo. Browser fixtures cover 30 custom hubs / 120 light/dark layouts. Detailed family receipts distinguish real external-tool probes, model tests and remaining unverified journeys.

## Implemented families

- Spring core: property provenance/reviewed rename, Boot runs/versioned metadata, typed repository generation, SpEL/JPQL ranges, source entity diagram and reviewed Hibernate SELECT console.
- Spring workflows: Feign requests, source DTO OpenAPI schemas, Gateway routes, GraphQL subscription drafts/transport, shell/WS/navigation, Batch/Modulith, Initializr and runtime request workflows.
- Database/infrastructure: offline Hibernate DDL adapters, Liquibase changesets/diff/grouping, CRD/Kustomize/Helm assistance and guarded Kubernetes references/rename, Devcontainer parsing, optional-capability denial handling.
- Developer tools: GitLab review/fork checkout, staged Swagger generation, npm/Maven/Gradle/sbt resolution, malicious-package provenance, official OSV Go analysis and separately identified experimental Rust Mach-O analysis.
- PHP/templates: Symfony profile decoding, Twig delimiters/macros/signatures, pinned optional formatter, reviewed source renames. Public unsafe writable Twig index anchors were disabled because released native Rename does not validate replacements. Blade changes here are matching-directive navigation only.
- Editor tools: dotenv/ignore graph handling, Properties compiler-backed references and staged locale editor, Ideolog JVM parsing/live tail, optional Qute/MicroProfile language-server broker, FogBugz and Redmine/Mantis adapters.

## Session 2026-11-11 (after pause)

Committed: `9bed009` Tasks `generic` REST/Java-regex family (wired, 11 bridge tests, `adapters/GENERIC.md`); `7fabc9c` six small residuals; `2af50f2` report reclassification.
- Closed: F14 OpenAPI generator JSON config import; F27 Dependency Audit ignore-profile export/import (safe-coordinate rows skipped); F33 Ignore Unicode unused-rule model (exact bytes; unprovable misses withheld); F42 Tasks owned Jira JQL via `jql:` prefix; F56 Initializr ZIP safe modes/mtime.
- Partial: F50 Kubernetes label graph affinity/topology-spread links done; escaped-scalar edits open (parser gives no scalar end offsets).
- Reclassified: F82 MVC template catalogue is LICENCE-BLOCKED (original is proprietary; clean-room only) and the multi-action marker popup is a HOST limit (`line-marker` has one target). F69 Modulith Kotlin descriptor and bean-completion warning are DECIDED-SKIPPED by the user (no documented target file; completion has no bean type/module).
- Bugzilla/Trac dropped by decision; no code exists.
- Spring Web batch (`08ee674`): MVC ControllerAdvice model variables and Thymeleaf argument-bearing fragment calls; Cloud unknown expanded Gateway args (20 factories); GraphQL Kotlin import aliases and stricter BatchMapping returns; WebSocket/Messaging XML false-positive removal plus enum/ref checks. Skipped: WebTestClient/RestClient URL shapes, `@SessionAttributes`, Web Flow Java-config registry, Java DSL predicates, Gateway shortcut arity, Stream gaps (type adapter). Spring Web tests 168 + module suites pass; no real-host run.
- Residual inventory: about 93 open of 98 counted (about 63 adapter, 19 host, 1 licence-blocked, 11 unverified). The counted F01–F98 table lives only in the session transcript; rebuild it from the family reports.
- Not done: real-host smoke of Generic search/close and the new commands; no live tracker account; `deny_unknown_fields` now on dependency-audit's stored `Ignore` (forward-compat risk).

## Next session

1. Pick adapter work by family from the family reports' exact residual inventories. Do not infer closure from tests/counts or historical PARITY.md ratings. Medium candidates: Dependency Audit PyPI markers/workspace graph, OpenAPI remote catalog/Add actions, Helm/Kustomize refs, GitLab GFM/drafts, Ignore template controls.
2. F69 decided 2026-11-11: keep Kotlin absent-descriptor creation refused and skip the bean-completion module warning (no approximation). Do not revisit without a documented Modulith target file or a bean-candidate model.
3. Keep genuine host limitations explicit: IDE datasource APIs, debugger/remote features, injected plugin-lexer rendering, multi-action markers, safe native indexed Rename policy. No API modifications are authorized.
4. Run fresh validation after changes. Build the released CLI once: `cargo install --locked --git https://github.com/l0ng-ai/openidea --tag v0.1.40 --root /tmp/openidea-parity-tools ide-host --bin openidea-plugin`, then `python3 scripts/check-non-laravel-parity.py --cli /tmp/openidea-parity-tools/bin/openidea-plugin --output /tmp/parity-next --jobs 2` (the global `openidea-plugin` targets API 1.3; checker notes in `scripts/api14-package-checker/README.md`). Runtime harness: `scripts/runtime-smoke/README.md`. UI sync must use `--exclude laravel`.

## Workspace and evidence

Unrelated Laravel, Blade injection/lexer, and shared localization changes (including `web/resources` and `i18n*` files in several plugins) were deliberately left uncommitted and must be preserved; session commits stage only owned source/manifest files. Shared UI localization was active concurrently; this checkpoint commits only the owned shared preview fix, selected CI validation step, scripts, and plugin changes. Existing generated resource localization remains outside this commit; the new JPA panel uses the previously committed shared assets. Therefore the aggregate receipt describes the tested working-tree cut, including its recorded shared-source hash, rather than a pristine standalone checkout of this commit.

Persisted family reports and aggregate receipt are under `docs/parity-implementation-2026-11-10-non-laravel/` (force-added because docs are ignored). Rich tool/runtime/browser logs remain local under `/tmp/non-laravel-parity-implementation/` and `/tmp/parity-*`; temporary logs/tools may not survive machine cleanup. Optional pinned tools are not bundled as binaries. Do not inspect or commit `.publish.key`.
