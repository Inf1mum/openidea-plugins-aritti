# Laravel leaf acceptance contract

[Machine-readable contract](laravel-parity-contract.json) freezes milestone 0 at
implementation checkpoint `077677c`, Laravel Idea public baseline **13.4**, and
OpenIDEA **SDK v0.1.40 / API 1.4**. It expands the ledger's 67 historical parents
into 207 separately owned leaves. This is a scoped acceptance contract, not a
claim that those leaves pass or that this number measures percentage parity.

Run the mechanical checks from the repository root:

```sh
python3 scripts/check-laravel-parity-contract.py
python3 scripts/check-laravel-parity-contract.py --self-test
```

Each leaf records its baseline URL/version boundary, behavior, checkpoint
status, owner role, task identifier, test/native/live gate and evidence. Source
inspection is distinct from execution. The contract currently claims no verified
leaf and applies no API exclusion. Known narrow host constraints are catalogued
as qualification facts; a fact does not waive a parent or silently exempt a leaf.

`implemented_unverified` means a scoped implementation exists, `partial` retains
open behavior, `missing` requires implementation, and `unqualified` needs a
precise public behavior reference before executable requirements are introduced.
`accepted_difference` is reserved for the user's native template-format choice;
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

Public documentation currently establishes process controls and integration
names, but does not establish every process restart/reconnect behavior or every
Cloud management operation. Git/LLM descriptor names are insufficient behavior
evidence. Generic Pest subject navigation is likewise unqualified; documented
Livewire Pest completion has its own leaf. These are research boundaries, not
invented implementation requirements or missing-host-API exclusions. Route-name
refactoring is explicitly outside the documented counterpart baseline; our
reviewed route rename is an additional capability.
