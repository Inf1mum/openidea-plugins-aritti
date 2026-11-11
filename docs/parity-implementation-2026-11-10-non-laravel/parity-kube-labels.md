# Kubernetes label/key source graph — 2026-11-10

Implemented in NEW src/label_refs.rs only. Existing lib.rs/indexer.rs integration is owned by infrastructure parent.

Cache API:
- `Cache::warm(path, source)`, `warm_file(path)`, `clear()`
- `definition(doc, at)` -> exact current source locations
- `occurrences(doc, at)` -> read-only source occurrence locations
- `show_occurrences(ctx)` -> explicit pick-and-open workflow with source version/text and fresh current target revalidation after the picker
- `markers(doc)` -> one-target source-candidate gutters
- `completion(doc, at)` -> ConfigMap/Secret data-key completion at supported keyRef values

Integrate with KubeExplorer field `label_refs: label_refs::Cache`; warm on indexing/open/save; clear on FilesChanged/deactivate; append definition/completion/marker providers. Route a manifest-scoped command (`source-occurrences`) to show_occurrences before generic cluster-action execution. Command needs existing editor.read/project.read grants and does not write. No indexed references/Rename are exported: generic DocumentIndexer would also offer automatic Rename without reference closure.

Behavior:
- Parses bounded YAML/JSON source through the existing duplicate/alias/tag guard and a marked AST reader. Supports block and flow maps/sequences.
- Source ranges represent exact physical spelling only; escaped/multiline spellings are withheld. Scanner character offsets convert through a precomputed byte-offset table, then UTF16 positions. No scalar-length guessing after Unicode.
- Pod labels and workload pod-template labels associate with Service, NetworkPolicy and PodDisruptionBudget simple selectors. Deployment/ReplicaSet/DaemonSet/StatefulSet/Job matchLabels resolve only to their own file/document template labels. Full simple selector map must match, not just one shared key. matchExpressions are currently withheld.
- Exact explicit namespace, kind/API group and resource identity are required. Missing namespace/runtime defaults are not assumed. Source providers use caller text for unsaved ranges and fresh target reads/digests; nested/foreign project ownership and target edits withdraw authority.
- ConfigMap data/binaryData and Secret data/stringData keys resolve from pod/workload env keyRefs, volume items and projected-volume items, scoped by core kind+resource name+namespace. Duplicate candidate declarations withhold authority. Key completion and read-only usages share this model.
- Secret values are not retained: the cache keeps SHA256 source fingerprints and structural label/key facts, never secret plaintext/source text.
- Maximum128KiB source/64 cached files; existing parser guard bounds nesting/nodes. No cluster processes, network operations or persistent storage in these providers.

Validation: nine focused label_refs tests passed in /tmp/parity-kube-label-tests.log. They cover full-selector matching, reverse label occurrences, exact key/kind/name/ns scope, stale targets, nested projects, duplicate resources, aliases/tags/generated content, quoted/escaped ranges, Unicode and own-template isolation. Full package suite has concurrent sibling changes; use parent's final combined run. Strict Clippy currently reports numerous pre-existing/concurrent files (54 errors), not claimed passing.

Remaining plugin work: complete matchExpressions semantics; namespaceSelector behavior; generated/Helm/Kustomize deployment identity; complete project reference closure and safe Rename; key references inside all extension schema forms; provider support for incomplete/escaped key scalar edits. Current implementation is an explicit source-candidate workflow, not runtime Kubernetes semantic identity.

## Match-expression and exact traversal follow-up

The earlier `matchExpressions` residual is now implemented for the supported source selector family. `matchLabels` and every expression are evaluated as a conjunction. `In`, `NotIn`, `Exists` and `DoesNotExist` use Kubernetes documented set semantics, including NotIn matching an absent key. Expression shape, label syntax, value typing and operator-specific cardinality are validated; any unsupported/invalid clause withholds the whole selector instead of ignoring a condition. Reference: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/ .

The model retains known Pod/template owners even for valid empty or absent label maps, provided the PodSpec source mapping exists. Negative conditions navigate to the owning resource declaration rather than inventing a missing label definition. Read-only resource occurrences expose matching negative selectors. In-expression values navigate only to a matching concrete label value; NotIn values identify matching owner candidates rather than pretending they are equal value definitions. Own-template workload selectors remain isolated to the exact file/document resource. ReplicationController's literal selector/template form is also modeled.

Additional exact key paths:
- Direct configMap/secret volume items and projected source configMap/secret items preserve name/kind/namespace identity.
- Downward API `metadata.labels['key']` references in container env and direct/projected downwardAPI volume items navigate to the current Pod/template label key. Completion replaces only the exact key subscript; reverse occurrences are read-only.

Sixteen focused tests now pass. New cases cover all four expression operators, conjunctive failure, absent keys/empty label sets, invalid expressions, owner declaration occurrences, full own-template isolation and projected/downward key paths. Full package tests/build logs: /tmp/parity-kube-label-full-tests.log and /tmp/parity-kube-label-build.log.

Remaining graph boundaries now concern namespaceSelector/affinity/node-label models, generated/Helm/Kustomize effective deployment identity, incomplete/escaped scalar edits, and safe automatic Rename/reference closure. None of these are relabeled as host API blockers by this implementation.

## Namespace and Node scope follow-up

NetworkPolicy ingress/from and egress/to peers now model explicit source Namespace labels. Same-peer podSelector+namespaceSelector conditions are AND; separate peers remain independent OR alternatives. Pod-only peers remain in the policy's explicit namespace. Namespace-only peers navigate Namespace label sources directly; combined namespace/pod references require matching source-owned Pods. Empty/negative namespace selectors require a unique explicit Namespace declaration. Missing/stale/duplicate Namespace sources withdraw authority. ipBlock mixed with selector fields and unknown/malformed selector clauses are refused rather than weakened.

Pod and workload-template nodeSelector maps now resolve only to core Node label declarations. Every key/value must match; similarly named Pod/Namespace labels cannot supply Node identity. Node declarations are cluster-scoped; duplicate Node resource names withhold authority. Reverse source occurrences share these rules. This does not infer API-generated Namespace name labels or scheduler/runtime state.

## Reviewed exact source Rename

Implemented Cache.file_actions(doc, selection) and resolve_file_action(doc, action), using data marker `kube-source-rename`. Parent integrates these in crd_export.rs's FileActions dispatch.

Data-key action:
- Named ConfigMap/Secret data/binaryData/stringData declarations and supported explicit Pod/workload keyRefs, including direct/projected volume items.
- Requires exactly one explicit namespace/kind/resource declaration and one old-key definition; new-key collisions and duplicate source identities are refused.
- YAML numeric/boolean/null-like replacement names are quoted where needed; JSON and existing quoted scalars retain valid string encoding.

Label-key action:
- Starts at an exact Pod/template, Namespace or Node label key declaration.
- Computes a connected component through selector **scope**, independently of the current value predicate. This includes currently nonmatching values, which is necessary to preserve NotIn/DoesNotExist semantics.
- Updates component label definitions, connected equality/expression condition keys and exact DownwardAPI subscripts. Own-template scope stays isolated when no broader selector connects it.
- Rejects duplicate identities and replacement-key definitions/conditions anywhere in the target family. Namespace-dependent scopes require unique explicit Namespace sources.
- Reparses every rewritten source and requires an identical before/after matrix of every modeled selector-to-source association before offering edits. This includes positive, negative, cross-namespace and Node relationships.

Both actions:
- Resolve-time full admitted project inventory: at most128 YAML/JSON files,128KiB per file,2MiB total, plus open YAML/JSON buffers. No cached subset is accepted as reference closure.
- Refuse file-cap sentinels, unreadable/symlink/foreign/nested-project sources, generated/template markers, Helm/Kustomize sources, unsupported List wrappers, malformed/duplicate/alias/tag/escaped/multiline input, and supported namespaced resources without explicit namespaces.
- Show a complete exact before/after **key-range** preview with path/line/column. Secret values are never included in that preview. Visible exclusions cover closed ignored files, external/cluster state, custom-controller fields and opaque string references; there is no universal runtime guarantee.
- Re-enumerate membership and re-read every admitted source after confirmation. Any source/membership/ownership drift cancels authority. Return only native atomic FileAction.edits; no direct writes or cluster operations.
- Bounds additionally limit label graphs to4096 fields/conditions,128 closure iterations and65536 association comparisons; over-limit cases return no partial edit. The final resolver-to-host application race remains governed by the native workspace-edit contract, not an invented plugin CAS guarantee.

Validation now passes **34 focused graph/refactor tests and508 full Kubernetes package tests**. Latest logs: /tmp/parity-kube-label-tests.log, /tmp/parity-kube-label-full-tests.log, /tmp/parity-kube-label-build.log. Release Wasm build passes. Added tests cover complete peer AND/OR semantics, Namespace/Node scope and freshness, full inventory guards, cancellation/collision/duplicate handling, YAML/JSON replacement encoding, before/after selector preservation, scope-connected nonmatching values, own-template isolation, DownwardAPI edits and source/membership revalidation. No new desktop apply/undo acceptance was performed in this worker.

Remaining graph implementation scope: affinity/topology spread selector variants, arbitrary custom-controller schemas, label-value Rename, and dynamic/generated or escaped/incomplete source models. These remain explicit plugin/model work where achievable, not host API blockers. Generic automatic indexed Rename remains intentionally unused; the new reviewed source-owned action is functional without it.
