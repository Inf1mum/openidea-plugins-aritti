# Infrastructure plugin implementation — 2026-11-10

## Frozen current original baseline

Fresh Marketplace API checks:
- Kubernetes: Marketplace10485, latest263.7307.240, update1215871 (previous audit263.7307.101). Public feature contract: https://www.jetbrains.com/help/idea/kubernetes.html . New documentation describes YAML/JSON assistance, CRD schemas, Kustomize fields/local references and Services workflows. This implementation did not inspect/decompile proprietary plugin code.
- Dev Containers: Marketplace21962,263.7307.115, update1207753; unchanged from earlier audit. Standard contract: https://containers.dev/implementors/json_reference/ and https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainerjson-reference.md . Native remote IDE backend remains absent; CLI and owned mapped targets are separate existing workflows.
- DDEV: Marketplace18813, XML de.php_perfect.intellij.ddev, latest1.2.10+261, update1213829. Source https://github.com/ddev/ddev-intellij-plugin : stable tag1.2.10 =7291f3ea58cf937ae8c6ac7973ed24876915e308; main HEAD1c5443127c38e392c3c4fc859f381dc758692160. These match existing SOURCE-AUDIT.md and FROM-ZERO-RECHECK.md. Current changelog concerns platform compatibility, failure notifications, terminal APIs, executable fallback and JSON version output; existing port audit covers corresponding behaviors.

## Implemented

Devcontainer source checks.rs/lib.rs:
- Schema-backed nested property completion in build, hostRequirements, otherPortsAttributes and per-port portsAttributes objects.
- Excludes already-present properties while allowing the active edited property.
- Supplies string enum choices for protocol/onAutoForward and boolean choices for requireLocalPort/elevateIfNeeded.
- Uses existing tolerant JSONC scanner; stays quiet in comments, arrays, arbitrary customizations/env/build argument keys and unsupported nested schemas.
- Preserves existing colon/value when editing an existing nested key; emits snippets only when inserting a new property.
- Maintains UTF16 replacement positions with multibyte text and unfinished-object completion.
- README now describes actual nested completion instead of declaring all nested objects unsupported.

Kubernetes source complete.rs:
- Root Kustomization field completion for kustomization.yaml, kustomization.yml and Kustomization.
- Documented SIG CLI fields from https://kubectl.docs.kubernetes.io/references/kustomize/kustomization/ . No deprecated bases/vars/patchesStrategicMerge added as new suggestions.
- Stable sorted suggestions, excludes present keys, replaces the whole partially typed key including suffix after caret.
- Quiet for nested mappings/list entries, values, comments, flow-style/multiple documents. Completion text limited to256KiB.
- README records scope explicitly.

DDEV plugin unchanged: actual source intentionally has six Setup toolbar actions including open-docs and no report-issue (tests.rs:351–352). ui_config policy already matches. Root notified to repair synthetic fixture rather than changing real plugin to match stale fixture.

## Validation

- Devcontainer153 tests pass; Kubernetes448 tests pass; DDEV110 tests pass,1 live test ignored.
- All three release Wasm builds pass against locked v0.1.40 SDK.
- git diff --check passes for owned plugin dirs.
- Fresh logs: /tmp/parity-devcontainer-tests.log, /tmp/parity-kube-tests.log, /tmp/parity-ddev-tests.log and matching *-build.log.
- No real Docker/cluster/DDEV/Database operations and no new desktop acceptance performed.

## Remaining gaps — no full-parity claim

These additions close bounded editor gaps, not whole original plugins. Kubernetes still needs comprehensive schema/CRD assistance, Kustomize local-file references, Helm semantics and complete ownership models for usages/Rename; these are plugin/adapter implementation work, not blanket API blockers. Current cluster cache/navigation source models intentionally do not equate literal text with symbol identity. Devcontainer broader template/feature metadata/configuration assistance can still be extended; native remote IDE backend and per-launch target introspection require host support. DDEV generic Composer root/hooks/cwd and target pre-launch state validation remain host issues2407/2408; actual native Database/IDE debugger, remote/Windows and latest desktop acceptance remain unverified. Existing port source audit details additional terminal profile/interception, background targeting and language-level hooks unavailable in1.4.

Laravel and shared crates/scripts were not edited by this worker. Existing dirty changes were preserved.

## Expanded implementation pass

The initial bounded pass above is superseded by these additional changes:

- `kustomize.rs`: exact current-source local resources/components/patch-file references; decoded block YAML strings (including quoted/escaped paths), same-project relative completion, navigation and gutters. Remote/outside/nested-project paths are refused. Directory references require exactly one Kustomization file. Source and target are read fresh; aliases/tags/duplicate keys do not produce guessed ownership.
- `crd_schema.rs`: project-local indexed/open CRD cache keyed by exact source and owner; unique group/version/kind schema identity. Nested property/enum/bool completion and live structural validation cover type, required/enum, explicit additionalProperties, numeric bounds/integer multipleOf, supported regex/string bounds, collection size/uniqueness, readOnly/deprecated. Definitions are parsed at warm time; providers only re-read relevant matching sources and compare equality. Unsupported composition/ref/CEL paths stay quiet.
- `crd_export.rs`: reviewed native FileAction creates a nonoverwriting sibling schema and relative YAML language-server directive as one undoable workspace edit. Resource version/text and schema digests are rebound at resolution and after confirmation. Existing/open destinations, stale schemas and canceled reviews return no operations. Relative filenames are percent encoded and CRLF retained. JSON Schema conversion handles nullable and int-or-string OpenAPI forms. Optional installed YAML language server supplies broader schema support.
- `json_assistance.rs`: strict JSON physical object/key/value scanner with exact UTF16 diagnostics, duplicate key checks, root/spec/metadata key completion, known Pod/Service enum completion and cached local CRD schema assistance. Conservative kind tables are shared with YAML instead of diverging. JSON never gets a YAML modeline refactor. Comments/trailing-comma JSONC is intentionally quiet, not called invalid JSON.
- `label_refs.rs` (child worker): explicit namespace/resource/group ownership for label and selector graphs; full conjunction of matchLabels and In/NotIn/Exists/DoesNotExist expressions; Pod/workload own-template associations; exact ConfigMap/Secret env, direct/projected-volume key references; Downward API label-key subscript associations. Definitions/gutters/completion and a source-revalidated read-only occurrences picker are integrated. Cache keeps parsed facts+SHA256 fingerprints, not Secret values. No unsafe automatic indexed Rename is exported.
- `helm_assistance.rs` (database sibling worker): same-project fresh-source Chart/values/schema models; local-ref/allOf/core values schema completion/validation, dependency shape/alias/condition assistance, optional local YAML-LS schema action. New explicit local Helm repository catalog refresh is bounded, rechecks repository signature/chart identity and offers dependency version completion from a dated10min cache. It does not run Helm in providers or automatically update network repositories. Real official Helm4.2.3 isolated schema/alias/catalog fixtures passed.
- Kubernetes `DiagnosticsProvider` registration now serves current unsaved documents. Existing Helm cross-file discovery remains event-based instead of running during live diagnostic calls. JSON and JSONC language declarations were added with the strict-subset caveat above.
- Devcontainer now adds cached feature-option editor completion bound to the current feature index; known root type/enum and port attribute diagnostics with an independent setting; feature proposals and explicit free-form string creation options with cancel/default omission behavior and safe JSON serialization.

### Fresh validation

- Kubernetes latest combined suite:489 tests passed; Devcontainer156 passed; DDEV110 passed with1 live ignored.
- Reusable full runner passed tests, fresh release Wasm, released API1.4 component linking and full package checks for all three packages: `/tmp/parity-infrastructure-expanded-final/results.json` and sibling logs.
- `git diff --check` passed for owned plugin directories. Strict Clippy is not claimed: it reports numerous existing/concurrent style lints outside the new modules.
- Child evidence: `/tmp/parity-kube-labels.md`; Helm evidence and runtime probes: `/tmp/parity-helm-assistance.md`, `/tmp/parity-helm-assistance/`.

### Latest feature-by-feature inventory

`/tmp/parity-infrastructure-features.json` maps60 public behavior rows across the three original baselines to source files, current implementation status and concrete residuals. This is more precise than treating the previous16 ledger tasks as the entire feature universe.

Remaining feasible model/adapter work is explicitly retained: complete JSONC and full builtin OpenAPI schemas, all Kustomize transformed/flow/remote reference contexts, generated deployment reference closure and safe Rename, complete Go-template scope/dependency/image registry intelligence, comprehensive Dev Container JSON Schema/Compose YAML semantics. These are not relabeled host blockers. Optional language-server/Helm adapters cover declared subsets only; runtime server admission/CEL, generated defaults, native remote IDE/debugger/terminal APIs and full desktop acceptance remain distinct. No unconditional full-parity claim is made.

## Third implementation increment

- JSONC now has source-preserving comment/trailing-comma normalization with no byte-offset drift; completion respects comment ownership, original UTF16 positions are retained, URL/comment-like string contents stay intact. Malformed inputs remain quiet rather than emitting guessed structural errors.
- Label graph now adds source-local Namespace peer selectors and Node selector associations (child evidence details exact scope). Read-only graph interfaces are unchanged.
- Devcontainer feature forms now admit typed string proposals and explicit custom values; cancellation/omission/default behavior is tested, no descriptor/network/process fetch occurs at selection.
- Devcontainer Compose forms now admit literal `.yaml`/`.yml` with yaml-rust2 0.13 (new locked OSS dependency), while retaining strict JSON behavior for JSON sources. Structural/event guard runs before recursive conversion:8KiB bytes,8192 nodes,32 frames; aliases/tags/duplicates/multidoc/nonstring keys/nonfinite values refuse. Tests cover128-deep input, cyclic aliases and oversize source.159 tests pass.
- Latest combined Kubernetes498 tests pass; latest full releasedAPI1.4 verifier receipts: `/tmp/parity-infrastructure-latest/results.json`. Reviewed source Rename work is still in progress; earlier no-Rename residual remains until that increment is validated.

## Reviewed Rename and final source stability

- ConfigMap/Secret data-key and connected label-key Rename are implemented as explicit native FileActions, not generic indexed automatic Rename. Complete admitted static inventory includes project-discovered YAML/JSON plus open buffers, capped128 files/128KiB each/2MiB total. Generated Helm/Kustomize/List, ambiguous/unparseable/escaped sources, unknown explicit namespaces, duplicate resource identities, collisions and incomplete inventory refuse the entire operation. Review shows source paths/ranges and exact before/after keys, names excluded runtime/cluster/custom/ignored scopes, and returns only atomic edits after re-enumeration and byte revalidation. Label-key edits must preserve the full modeled selector-association matrix before any operation is returned.
- Namespace/Node selector families are implemented. Combined NetworkPolicy peer pod/namespace predicates use AND; separate peers remain independent. Node selectors match cluster-scoped Node label sources, not similarly named Pod labels. Negative predicate conditions can navigate to owning resource declarations instead of invented label definitions.
-508 Kubernetes tests pass, including34 focused graph/Rename cases.159 Devcontainer tests pass,110 DDEV tests pass with1 live ignored.
- `/tmp/parity-infrastructure-complete-check/results.json`: all test/build/component/full-installer execution steps passed for all three; Devcontainer source-stable PASS. Kubernetes and DDEV own source fingerprints remained unchanged, but their shared-crate fingerprints changed during verification due concurrent shared UI edits, so the runner correctly records their final source-stability gate as changed. Do not report those two current-tree receipts as unconditional PASS until a stable shared snapshot is rechecked.
- Current feature classifications are `/tmp/parity-infrastructure-features.json`; this remains a feature-level inventory with actual plugin/model/adapter work retained, not an unconditional full-parity assertion.

## Real-Wasm denied project-read rendering fix — 2026-11-11

Root's released-host smoke found both Kubernetes views returned PermissionDenied with only required consent. The guest Views wrapper called with_process_scope, whose concrete invoking Project::root correctly required optional project.read but propagated that denial as a failed render. Fixed only the rendering boundary to return an inert explanatory Message/status with empty toolbar for project.read denial. Commands, events and detail authority still refuse; no automatic global CLI downgrade is introduced. Existing rich cache expiry still clears revealed Secret plaintext.

Regression covers both views after grant revocation, no process execution and strict event rejection. The older secret-revocation test now verifies the inert view, cleared plaintext, empty toolbar and unchanged process count.509 package tests pass, fresh release Wasm builds.

Actual released-host/guest smoke with required-only consent now installs and renders both kube/resource-details: `/tmp/non-laravel-parity-implementation/runtime-kube-denied-fixed/results.json`; both rendered JSON models contain only the no-access message/status. Artifact SHA256 c0a29cb76f1286dd30364146742e19b8e12b745db83a1c1c0408d26042222b22. This is actual compiled-Wasm render evidence, not GUI/cluster acceptance. No additional capabilities were required or silently granted.
