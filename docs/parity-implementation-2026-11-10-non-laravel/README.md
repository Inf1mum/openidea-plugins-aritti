> Paused 2026-11-11 at user request. Latest aggregate checkpoint: all 29 packages pass, 5,123 tests; full parity remains open. Read [the committed handoff](../../NON_LARAVEL_HANDOFF.md) and [machine receipt](checkpoint-2026-11-11.json). Family reports named `parity-*.md` below are the latest copied agent inventories; older paragraphs remain historical.

# Non-Laravel parity implementation — in progress

Scope: all29 non-Laravel packages, released SDKv0.1.40/API1.4, latest public original baselines and optional local adapters. Laravel and host/API source are excluded. Existing shared-kit/Blade work was preserved. No package publication or release-version bumps.

**Full feasible parity is still open.** The linked family inventories retain concrete plugin and adapter work; source-model limits are not relabeled API blockers. Historical task statuses have not been promoted. This is an evolving progress receipt, not completion acceptance.

## Reproducible validation

[Released-checker and runner instructions](../../scripts/api14-package-checker/README.md). `check-non-laravel-parity.py` checks tests, exact release Wasm, actual released-host linking and full staged-package installation validation, then refuses source drift. It does not copy into canonical dist or install plugins. `sync-plugin-ui.py --exclude laravel` now supports explicit package scope; its exclusion safety regression passes.

Fresh public metadata captures36 original counterparts (merged Spring counterparts included, Laravel originals excluded, DDEV added). OpenAPI has distinct latest-publication and latest-build compatibility branches; preserve both rather than treating version strings as a universal ordering. Raw metadata is in latest-originals-branches.json; it is not a feature completion score.

## Central implementation

- Added an independently pinned released-host component checker and full staged-installer validation runner; older installed API1.3 CLI is not used as a package failure signal.
- Added explicit UI synchronization selection/exclusion and non-Laravel browser scope. Corrected DDEV fixture to its actual six-action command policy.
- Repaired stale shared catalog preview markup to include the grouped-view navigation element required by production hub code.
- Added Twig3.15+ physically resolved macro named-argument completion: literal imports/from aliases/current `_self`, existing positional/named exclusion, exact Unicode ranges, duplicate/dynamic-owner refusal and known older installed-version gating. [Official version contract](https://twig.symfony.com/doc/3.x/templates.html#named-arguments).

## Family implementation and residual inventories

- [spring-core](spring-core.md)
- [spring-workflows](spring-workflows.md)
- [spring-web-feature-inventory](spring-web-feature-inventory.md)
- [shell-ws](shell-ws.md)
- [integration](integration.md)
- [initializr-runtime](initializr-runtime.md)
- [php-templates](php-templates.md)
- [infrastructure](infrastructure.md)
- [database](database.md)
- [developer-tools](developer-tools.md)
- [editor-tools](editor-tools.md)
- [env-ignore](env-ignore.md)
- [properties-ideolog](properties-ideolog.md)
- [quarkus-qute](quarkus-qute.md)

Each report identifies authored files, tested behavior, original baseline and remaining work. Reports can lag active implementation; final exact-artifact validation must follow the final source state. Current local receipts are under `/tmp/non-laravel-parity-implementation/`; earlier checkpoints include deliberately rejected drift results and must not be presented as final passes.
