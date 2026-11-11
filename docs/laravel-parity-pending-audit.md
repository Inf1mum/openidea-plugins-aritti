# Laravel current remaining work audit

Reconciled on 2026-11-11 against the [212-leaf / 67-parent contract](laravel-parity-contract.json). Current counts: accepted_difference=2, api_excluded=2, implemented_unverified=207, missing=0, partial=1. Earlier 207/208 expansions and checkpoint receipts remain historical. No leaf is `verified`.

Actual **aritti.laravel activation** no longer exceeds OpenIDEA 0.1.40’s 5,000,000,000-fuel callback budget on the 30,904-file `baby-bob` project (2026-11-11 compiled-Wasm probe: all 175 indexing callbacks complete, longest ≈250 ms). The trap was `web_config` rebuilding the full model overview (`model_overview::rows`) just to list action keys; it now uses `model_overview::source_keys`. This is a Wasm probe against the pinned runtime, not native desktop acceptance: no native apply/undo or other native gate is claimed. The initial bottom-panel observation was from a different installed Laravel plugin and has been retracted.

## Qualified implementation or scope still pending

| Leaf | Current remaining scope |
| --- | --- |
| `T06.oauth` | Registered-client device authorization, explicit polling/expiry/keychain/disconnect are implemented with mocked transport tests; configured registered-client live acceptance is pending. Persisted refresh has the separately evidenced API1.4 stored-secret-to-form-body transport limitation; device authorization itself is not API-blocked. |

These rows distinguish a confirmed remaining implementation from a behavior qualification or narrower SDK transport constraint. Owners are still working; source completion does not satisfy the actual native-host gate.

## Qualification and accepted boundaries

- The user explicitly confirmed **“Exclude Qodana; keep plugin-only”** on 2026-11-11. Qodana is an accepted external IntelliJ/JVM-runtime exception, not an OpenIDEA API exclusion. No companion is required. Native generator template format is the other accepted difference.
- Interactive terminal completion (`R09.terminal`) and the exact LLM agent-finished changed-file trigger (`T11.llm_helper_finished`) have narrow, separately evidenced API1.4 exclusions. Workspace/keyed terminals and reviewed/manual/on-save helper generation remain implemented and required.
- OAuth device authorization is implemented. Persisted Sentry refresh has the narrower stored-secret/form-body transport limitation described in the [SDK qualification](laravel-runtime-sdk-qualification.md); a registered client/account is a connection prerequisite and live gate, not a missing API.
- The pinned artifact now establishes exact eight-name metadata completion-transformer grammar and inverse navigation; scoped implementation and inverse navigation are present; final fresh aggregate guards/native gate remain separate. Native generator mutators are not equivalent. Enum config-key wording is now qualified as array-key `Enum::Case->name/value` resolution and remains implementation work.
- The [primary-source qualification](laravel-baseline-qualification-20261111.md) resolves all three former unqualified leaves: Cloud has exact read-only details/log/variable/database workflows; Git refreshes the visible Cloud view; the precise LLM callback is API-excluded. Generic Pest navigation was an invented requirement and is withdrawn; documented Pest templates/Livewire completion remain in G02/B03. Arbitrary Cloud write supersets and process restart persistence are not invented.
- Arbitrary dynamic PHP, remote icon disks, SCSS breadth, composite Eloquent primary keys and unspecified old/custom package variants are conservative implementation limits unless exact primary baseline behavior establishes them. They no longer inflate a list of required missing features.
- SDK declaration/settings localization is supported through automatically discovered `locales/<tag>.json` source-text catalogs; enum/default IDs remain data.

## Scoped evidence and current implementation

- The [complete JSON audit](laravel-parity-pending-audit.json) and [contract](laravel-parity-contract.json) retain all 212 leaves, owner tasks, gates, implementation limits and source receipts. `implemented_unverified` means the documented scoped implementation exists; it does not waive any native/live gate.
- Metadata domains: independently rerun 19/19 focused tests, including Unicode/hex CSS, UTF16 navigation, empty-directory grants/fallback, declared provider icons and stale/ownership negatives.
- Parameterless Run: 13/13 workspace tests cover Main explicit entry, kept jobs/keyed terminals, runner/token/permission/replay/staleness/busy guards and passive no-execution.
- Config defaults: 14/14 focused tests including two explicitly run installed-source acceptances, Laravel10/11/12/13 merge boundaries, PackageTools declarations, shallow app override and integer renumbering. Actual Laravel13.40/PHP8.4 differential passed. [Reproduction](../plugins/laravel/tests/config-inventory/README.md).
- Native catalog: all36 core kinds and applicable package variants have installed framework/package fixtures; [version matrix](../plugins/laravel/tests/native-catalog/README.md). Actual Filament APIs, Vue/React/Svelte compilers and Chromium/WebKit wizard fixtures have narrower receipts; licensed Nova live acceptance remains separate.
- Semantic owners report current actual Laravel/PHP fixtures for modern Attribute accessors, query/optional/eager/map flows, source-bound relation adapters and closure/contextual container bindings. E06 [PHP reflection/provider fixtures](../plugins/laravel/tests/eloquent-forwarding/README.md) and the official installer [structured receipt](../plugins/laravel/tests/project-wizard/installer-5.28.1-acceptance.json) retain exact bounds.
- Browser/UI-kit, native Rust MockHost, actual PHP/framework, custom compiled-Wasm and platform MCP harness evidence are distinct tiers. None is silently relabeled actual pinned desktop acceptance.

Qualification reshaping retains212 leaves/67parents: withdraw `V09.pest`, replace broad `T11.other_hooks` with `T11.cloud_git_refresh` and `T11.llm_helper_finished`. This is not native verification or a new user scope exception.
