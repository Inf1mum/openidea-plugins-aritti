# Non-Laravel parity checkpoint — paused 2026-11-11

Paused at the user's request. Resume implementation only in the next session.

## Scope and status

All 29 non-Laravel packages; latest public original-plugin baselines; released SDK v0.1.40 / API 1.4. Optional reviewed CLI/LSP adapters are authorized. No host/SDK/API changes, publication or version bumps. **Full parity remains open.**

The latest aggregate checkpoint passed all 29 packages: 5,123 Rust tests, fresh release Wasm builds, released-host component linking and staged installer validation, with stable per-package source fingerprints. These are working-tree checkpoint results, not complete desktop acceptance or a claim that every optional external integration has universal coverage. See [machine receipt](docs/parity-implementation-2026-11-10-non-laravel/checkpoint-2026-11-11.json).

Real isolated host smoke tests also rendered native/custom views and exercised Properties staged edit → native FileAction → grouped Undo. Browser fixtures cover 30 custom hubs / 120 light/dark layouts. Detailed family receipts distinguish real external-tool probes, model tests and remaining unverified journeys.

## Implemented families

- Spring core: property provenance/reviewed rename, Boot runs/versioned metadata, typed repository generation, SpEL/JPQL ranges, source entity diagram and reviewed Hibernate SELECT console.
- Spring workflows: Feign requests, source DTO OpenAPI schemas, Gateway routes, GraphQL subscription drafts/transport, shell/WS/navigation, Batch/Modulith, Initializr and runtime request workflows.
- Database/infrastructure: offline Hibernate DDL adapters, Liquibase changesets/diff/grouping, CRD/Kustomize/Helm assistance and guarded Kubernetes references/rename, Devcontainer parsing, optional-capability denial handling.
- Developer tools: GitLab review/fork checkout, staged Swagger generation, npm/Maven/Gradle/sbt resolution, malicious-package provenance, official OSV Go analysis and separately identified experimental Rust Mach-O analysis.
- PHP/templates: Symfony profile decoding, Twig delimiters/macros/signatures, pinned optional formatter, reviewed source renames. Public unsafe writable Twig index anchors were disabled because released native Rename does not validate replacements. Blade changes here are matching-directive navigation only.
- Editor tools: dotenv/ignore graph handling, Properties compiler-backed references and staged locale editor, Ideolog JVM parsing/live tail, optional Qute/MicroProfile language-server broker, FogBugz and Redmine/Mantis adapters.

## Next session

1. Read [current family reports](docs/parity-implementation-2026-11-10-non-laravel/README.md), especially their exact residual inventories. Do not infer closure from tests/counts or historical PARITY.md ratings.
2. Tasks Generic adapter is wired (2026-11-11): `generic` family in `selfHostedProfiles`, native search/close beside Redmine/Mantis, docs in `plugins/tasks/adapters/GENERIC.md`. Evidence: 101 Rust tests, 11 bridge tests (real HTTP/JVM fixtures), release Wasm build and pinned API 1.4 checker pass. Not run: full installer CLI, real-host smoke, live tracker. Bugzilla/Trac were dropped by decision; no code exists.
3. Continue concrete remaining plugin work: tracker families/configurable adapters, broader semantic PHP/Java/template cases, JPA query/entity/driver coverage, and the family-specific residuals. Distinguish feasible adapter work from genuine host boundaries.
4. Keep genuine host limitations explicit: IDE datasource APIs, debugger/remote features, injected plugin-lexer rendering, and safe native indexed Rename policy. No API modifications are authorized.
5. Run fresh validation after changes. Older globally installed `openidea-plugin` targets API 1.3; use the pinned released checker/CLI instructions in `scripts/api14-package-checker/README.md`. Runner: `python3 scripts/check-non-laravel-parity.py --output /tmp/parity-next --jobs 2`. Runtime harness: `scripts/runtime-smoke/README.md`. UI sync must use `--exclude laravel`.

## Workspace and evidence

Unrelated Laravel, Blade injection/lexer, and shared localization changes were deliberately left uncommitted and must be preserved. Shared UI localization was active concurrently; this checkpoint commits only the owned shared preview fix, selected CI validation step, scripts, and plugin changes. Existing generated resource localization remains outside this commit; the new JPA panel uses the previously committed shared assets. Therefore the aggregate receipt describes the tested working-tree cut, including its recorded shared-source hash, rather than a pristine standalone checkout of this commit.

Persisted family reports and aggregate receipt are under `docs/parity-implementation-2026-11-10-non-laravel/` (force-added because docs are ignored). Rich tool/runtime/browser logs remain local under `/tmp/non-laravel-parity-implementation/` and `/tmp/parity-*`; temporary logs/tools may not survive machine cleanup. Optional pinned tools are not bundled as binaries. Do not inspect or commit `.publish.key`.
