# Laravel leaf acceptance contract

[Machine-readable contract](laravel-parity-contract.json) freezes milestone 0 at
implementation checkpoint `077677c`, Laravel Idea public baseline **13.4**, and
OpenIDEA **SDK v0.1.40 / API 1.4**. It expands the ledger's 67 historical parents
initially into 207 separately owned leaves; the current contract has **212 leaves / 67 parents**. This is a scoped acceptance contract, not a
claim that those leaves pass or that this number measures percentage parity.

Run the mechanical checks from the repository root:

```sh
python3 scripts/check-laravel-parity-contract.py
python3 scripts/check-laravel-parity-contract.py --self-test
```

Each leaf records its baseline URL/version boundary, behavior, current status with preserved checkpoint evidence, owner role, task identifier, test/native/live gate and evidence. Source
inspection is distinct from execution. The contract currently claims no verified leaf. Interactive terminal completion and the precise LLM agent-finished trigger have exact API exclusions; native template format and user-approved plugin-only/Qodana delivery are the two accepted differences. Known narrow host constraints are catalogued
as qualification facts; a fact does not waive a parent or silently exempt a leaf.

`implemented_unverified` means a scoped implementation exists, `partial` retains
open behavior, `missing` requires implementation, and `unqualified` needs a
precise public behavior reference before executable requirements are introduced.
`accepted_difference` records only the user's native template-format choice and explicit plugin-only/Qodana exception;
capability gaps cannot be hidden under that decision. `api_excluded` requires an
exact blocked behavior, SDK reference, supported alternative and confirmed proof.

To promote a leaf to `verified`, attach individually scoped passing evidence for
every required test/native/live gate, explicitly covering that leaf with an
implementation revision or artifact hash. Aggregate suite/build receipts alone
cannot close leaves. The checker validates receipt structure and coverage, not
whether the stated execution actually happened; reviewers must inspect/authenticate
the referenced results. Baseline/safety leaves cannot evade this by disabling
their test/native gates.

Do not edit the frozen historical matrix to match current implementation. Update
leaf statuses/evidence for subsequent work while retaining checkpoint receipts
and the applicable version boundary. A runtime source change, package-version
change or changed interaction may require requalification rather than retaining
an old broad pass.

Public process-tab announcements do not establish every restart persistence behavior. The [remaining primary-source qualification](laravel-baseline-qualification-20261111.md) now establishes exact metadata transformers, read-only Cloud workflows and Git/LLM hooks. Generic Pest subject navigation is withdrawn as an invented requirement; documented Pest templates and Livewire completions remain required in G02/B03. Broad Cloud mutation supersets are not inferred from an integration heading. Route-name refactoring remains explicitly outside the documented counterpart baseline; our reviewed route rename is an additional capability.

## Baseline extension after the original freeze

The original 207-leaf milestone remains historical. A primary-source recheck added `M01.php85_pipe` for the explicitly documented PHP8.5 pipe-completion behavior in comparison version 13.2, producing **208 leaves / 67 parents**. At that historical extension it was missing; it now has scoped PHP8.5 metadata/Eloquent implementation and fixtures, with the actual native gate still pending. The [PHP manual](https://www.php.net/operators.functional) qualifies language syntax; inferred Laravel-specific fixture targets are labeled separately from published comparison examples. Current statuses are reconciled in the machine-readable contract; historical extension receipts are retained.

Four further explicit diagnostic/marker leaves were added after source review: `M04.config_inspection`, `R03.resource_method_inspection`, `E07.mass_assignment_inspection` and `M04.parent_config_marker`. The current contract contains **212 leaves / 67 parents**; earlier207/208 counts remain historical. Each new leaf has its own status and gates.

## Current acceptance blocker and scope reconciliation

accepted_difference=2, api_excluded=2, implemented_unverified=207, missing=0, partial=1. Zero verified leaves reflects the unmet actual native-host gates: actual desktop acceptance has not run; the earlier activation fuel trap is cleared in a compiled-Wasm probe against the pinned runtime (see the pending audit). It does not erase scoped MockHost, actual PHP/framework, browser or custom-Wasm receipts. The [pending audit](laravel-parity-pending-audit.md) lists only concrete qualified remaining behavior; arbitrary dynamic/remote/version shapes are separately recorded implementation limits. Qodana is explicitly excluded by the user’s plugin-only decision on2026-11-11, not left as implementation work.

### Resolved qualification reshaping

The current212-leaf shape withdraws invented generic Pest navigation and replaces broad other-hooks with exact Git Cloud refresh and LLM helper-finished leaves. Git freshness is implementable through visible native VcsChanged/scheduled refresh; the authoritative agent-finished batch is absent in the pinned ABI and narrowly API-excluded. The two accepted user differences remain unchanged. Native verification remains0.
