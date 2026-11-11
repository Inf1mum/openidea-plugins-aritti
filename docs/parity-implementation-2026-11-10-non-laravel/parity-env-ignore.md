# dotenv and ignore implementation — 2026-11-10

## Latest baseline refresh

Queried official Marketplace JSON APIs directly on 2026-11-10:
- https://plugins.jetbrains.com/api/plugins/9525/updates?size=1 → dotenv 263.7307.209, file `9525/1212895/dotenv-263.7307.209.zip`.
- https://plugins.jetbrains.com/api/plugins/7495/updates?size=1 → ignore 4.5.7, file `7495/936603/ignore-4.5.7.zip`.
These latest versions match the existing pinned audit version numbers. Official source behavior references: https://github.com/JetBrains/idea-php-dotenv-plugin and https://github.com/JetBrains/idea-gitignore . Git pattern contract: https://git-scm.com/docs/gitignore . No proprietary code/assets were copied.

## Implemented

- dotenv: multiline Compose `environment` flow lists/maps with CRLF, comments, quoted commas/closing delimiters, escaped double quotes and doubled single quotes. Physical key byte ranges are retained. Unclosed/nested collections stay quiet rather than inventing scalar declarations. Existing interpolated-key rejection preserved. Existing README limit revised.
- ignore: preserve meaningful leading pattern spaces and escaped trailing spaces, strip only unescaped trailing spaces. Use identical meaningful-byte policy in duplicate/redundancy inspections for non-switchable glob dialects. Updated existing duplicate fixture to test trailing-only ignored whitespace, and added distinct leading/escaped cases.
- ignore: source fallback now checks every ancestor before the leaf. Child negations and nested ignore files cannot restore a file while a parent directory remains excluded. Explicit parent re-inclusion remains supported.

## Validation

- dotenv cargo test --locked: 49 passing (was 47).
- ignore cargo test --locked: 129 passing (was 126).
- Both cargo build --locked --release --target wasm32-wasip2 pass.
- git diff --check restricted to these plugins passes.
- Disposable real Git `check-ignore --no-index` confirms parent exclusion, leading spaces and escaped trailing spaces all classify as expected.
- Logs: /tmp/parity-dotenv-test.log, /tmp/parity-dotenv-build.log, /tmp/parity-ignore-test.log, /tmp/parity-ignore-build.log.
- No desktop acceptance or package checker claim from this pass.

## Remaining parity

This is implementation progress, not full feasible-parity closure. dotenv still lacks YAML anchor/alias resolution and runtime-aware loader/Compose-service/build-stage ownership needed for complete usages/Rename. Native value folding/secret hiding and custom completion triggers remain host integration constraints. Exact current-file source update remains the safe bounded action, not semantic Rename. Docker stage interpolation and general usage-language AST fidelity remain work beyond this parser pass.

ignore still lacks native tree filtering/closing editors, scrollbar/host decoration fixes and arbitrary dynamic language associations where API1.4 does not expose them. Complete original template-generator UI and filesystem-aware unused-entry inspection must still be checked against native workflows; bounded cached Git observations do not establish complete file inventory. No task ledger status was promoted.

Only plugins/dotenv and plugins/ignore were edited; no shared files, scripts, Laravel, manifests, dependency versions or historical ledgers were changed.

## Follow-up implementation (supersedes first-pass counts/residuals)

- dotenv now resolves bounded local YAML anchors used as Compose environments: scalar mapping/list block anchors, inline mapping/list anchors, plain `environment: *name`, environment-declared inline anchors, and block `<<: *name` / `<<: [*first, *second]` merges. Anchor definitions must precede use; later definitions shadow earlier ones. The first merge anchor wins duplicate keys; explicit environment keys override inherited entries. Source ranges refer to physical declaration keys, and duplicate source tuples are deduplicated. Unsupported nested/alias-dependent/cyclic definitions do not produce partial expansion. Limits: 256 KiB, 4096 lines, 128 names. This is positive declaration lookup, not runtime environment or rename ownership.
- dotenv multi-line `env_file` flow lists now also support comments and quoted commas; existing project-confinement path validation remains in the index.
- Ignore gained **Create ignore file from templates…** atomic intention. Existing glob-dialect filename + directory + up to 16 template choices + exact confirmation result in a non-overwriting Create FileAction. Shared selector now serves append too. Tests cover combined Rust/Node content, no writes during resolution, duplicates/unknown/cancel, and refusing regexp dialect `.hgignore`. Existing source/path/version/collision/dirty-buffer checks remain in force.
- Final tests: dotenv **53 passed**, Ignore **131 passed**. Final release wasm32-wasip2 builds pass for both. Scoped diff whitespace check passes.

Remaining feasible plugin gaps are not silently classified as host blockers: dotenv `env_file` aliases, inherited/nested anchor graphs, complete language-aware loader ownership and Docker interpolation/usages remain absent. Full original template-generator controls/preview persistence and filesystem-aware unused-rule analysis remain unimplemented. Existing managed Git inventory explicitly cannot prove complete stdout transport, and project walk omits hidden/ignored files; unused-rule absence must therefore remain unknown unless a complete inventory adapter is established. Core editor close/filter/typed trigger hooks remain genuine host boundaries. No ledger statuses changed; no full parity claim.
