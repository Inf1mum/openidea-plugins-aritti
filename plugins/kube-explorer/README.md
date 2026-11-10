# Kube Explorer

One shared UI-kit window contains the Kubernetes, Watch tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. The native Kubernetes Services tree stays separate. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

Browse a Kubernetes cluster from OpenIDEA through `kubectl`.

- **Kube Explorer** tool window (Services): double-click a context or namespace
  to switch to it, and expand a kind to list its resources. The toolbar acts on
  the selected resource: Describe (or double-click), YAML, Logs (pods; for
  Deployments, StatefulSets, DaemonSets and Jobs, the logs of all their pods),
  Shell, Attach, Debug (a pod: `kubectl debug -it` adds an ephemeral container on
  the image of the `debugImage` setting, `busybox:stable` by default, targeting the
  pod's first container), Port Forward (edit the `local:remote` ports, then Start) and
  Delete (confirmation required). Editing and terminal workflows open the existing
  Kubernetes panel or terminal; native read-only details stay beside the Services tree.
  **Custom Resources** lists the cluster's CRD kinds (`kubectl api-resources`,
  without core and built-in groups) by API group; expanding a kind lists its
  instances, which take Describe, YAML and Delete like any resource.
  **All namespaces**, the first entry of the Namespace list, shows every
  namespaced kind as `namespace/name` rows; an action on a row runs in that
  row's namespace. In that mode Create Secret, Apply Manifest, Apply
  Kustomization and Delete Manifest Resources are refused (pick a namespace
  first), and custom resources that are cluster-scoped or have a name that is not
  a DNS name are listed but take no action. If kubectl cannot be started and no
  context is stored, the tree offers a How to Install kubectl button.
- **Secrets**: Show Secret (a selected secret) decodes its values in the plugin and
  shows them only in the panel. It is off until you turn on `allowRevealSecrets` in
  the settings; while it is off, YAML on a secret is refused too (that YAML holds the
  base64 data), and Describe, which prints no values, still works. With it on, YAML
  on a secret is allowed and is not stripped: it opens in the panel, read-only, and is
  never written to a temp file (`tee` would create it readable by every local user).
  Show Secret itself never sends values to notifications, logs,
  storage, the clipboard, temp files or command arguments; a value that is not UTF-8
  is shown as base64. The SDK has no view-close event, so revealed values clear on Back
  only (and when the setting is turned off). Create Secret builds an Opaque Secret from
  a name and key/value pairs (Add Key for more than one) and runs `kubectl create -f -`
  with the manifest on stdin, so values never appear in arguments and no
  last-applied annotation is written; it fails on an existing name rather than
  overwriting. Keys are limited to `[-._a-zA-Z0-9]`. The form has no masked field in
  this plugin API, so typed values are visible on screen; it is cleared when you
  submit or click Back.
- **Edit Secret** (a selected secret, or the context menu; needs `allowRevealSecrets`):
  a form of the decoded keys. Edit values, add a key, or list keys to remove, then
  Save. The Secret is read as JSON and sent back with `kubectl replace -f -` on stdin
  with only `data` changed, so labels, type and `resourceVersion` stay and a change made
  in the cluster since you opened the form is refused (nothing is saved; open it again).
  Binary values are kept unchanged and not shown. Values are visible on screen, as in
  Create Secret.
- **Log Viewer** (a pod or workload, toolbar or context menu): one `kubectl logs --tail=N
  --timestamps` snapshot (`logViewerLines`, 1000 by default; at most the newest 2 MiB and
  5000 lines shown) in the panel. Filter… (text, or a regular expression with Regex on,
  matches highlighted), Match case, Since… (a UTC date or time, compared with the
  timestamps), Until… (the same for the end of the range), Wrap, Time and Source (show
  or hide the timestamp and the `[pod/container]` prefix of each line), Scroll to end,
  Clear (empties the view; Refresh reads the log again), Refresh, Copy (the shown lines
  on the clipboard, optional `clipboard`) and Download… (writes the shown lines, as
  displayed, through optional `project.write` to a new file inside the owning project,
  by default `<project>/<kind>-<name>.log`. A confirmation identifies the exact path,
  line/byte count and SHA-256 of the displayed snapshot; it is a digest review,
  not a truncated full-content preview. Outside/nested-project paths, existing files,
  dirty/open buffers and missing parents are refused; Cancel writes nothing. No
  `tee` or new cluster request runs for export. New project files have normal filesystem
  permissions and may contain sensitive log data. The SDK write has no exclusive-create/CAS
  flag: a final external filesystem race remains possible after collision rechecks.) Scroll to end relies on the page's
  script, which the IDE may block. Following a log stays in the terminal (Logs, Select
  Log Sources).
- **Completion in YAML**: in an empty document `kpod`, `kdeploy`, `ksts`, `kds`, `kjob`,
  `kcronjob`, `ksvc`, `king`, `kcm`, `ksecret`, `kns`, `krole`, `krb`, `kcr`, `kcrb`
  expand to the manifests of the New-file templates. In a chart's template, inside
  `{{ }}`: `.Values.` completes the keys of the chart's `values.yaml`, `include "` the
  names `define`d in the file and in `templates/_helpers.tpl`, and `incl` an include.
- **Chart.yaml quick fixes** (⌥⏎ in a v2 `Chart.yaml`): *Add chart dependency* writes a
  stanza with `NAME`, `VERSION` and `REPOSITORY` to replace. On a `repository:` line,
  *Use repository alias* and *Use repository URL* swap between a URL and `@alias`; they
  need the list that **Read Helm Repositories for Chart.yaml Quick Fixes** (`helm repo
  list`) stored, because a quick fix cannot run helm or ask you anything.
- **Several kubeconfigs**: the `kubeconfig` setting takes files separated by commas;
  **Choose Kubeconfig** picks the one in use (one file, not a merge of them) and resets
  the context and namespace. `nonKubernetesGlobs` (e.g. `*.compose.yaml, ci/*.yml`)
  lists YAML files that get no problems, `notifySuccess` turns off the notifications of
  successful commands (errors always show). **Telepresence Connect / Quit** run
  `telepresence connect` and `telepresence quit` in a terminal (optional
  `process:telepresence`).
- **Read-only source navigation and gutters**: known Pod/workload paths link
  ConfigMaps, Secrets, Services, claims and service accounts to cached source
  declarations and back to source uses. Ingress backend Services/TLS Secrets,
  explicit metadata namespaces and known HPA scale targets are supported.
  Field-name matches in arbitrary ConfigMap/custom-resource data are not references.
  API group, kind, literal namespace, known scope and source isolation remain part
  of the private model. Cold same-stream links work from current editor text;
  cross-file candidates require a warmed same-project cache. Deployment and cluster
  identity remain unresolved. The registered document indexer publishes an empty
  graph, so none of these associations enables automatic Rename/Find Usages.
  See the ownership audit below. Optional `project.read` is required; native
  Services browsing and existing resource-panel permission rules are unchanged.
- **New file templates** (File › New): Pod, Deployment, StatefulSet, DaemonSet, Job,
  CronJob, Service, Ingress, ConfigMap, Secret (`stringData` placeholder), Namespace,
  Role, RoleBinding, ClusterRole, ClusterRoleBinding and CertificateSigningRequest, on current stable
  apiVersions.
- **YAML** opens in the existing editable panel by default. With `yamlInEditor`
  enabled, supported modern built-ins open in a private SDK-storage editor copy
  with the schema modeline. Secrets and unsupported identities remain in the panel.
  **Save is local only.** Invoke **Review Private YAML Apply…**, review the exact
  target, editor revision, byte count and digest, then invoke **Apply Reviewed
  Private YAML** in the same editor within 60 seconds. The separate event rechecks
  project, local kubeconfig/context/namespace and unchanged current text/version.
  UID and resourceVersion are retained and cannot be edited; replacement sends the
  reviewed text on stdin, preserving server conflict checks. Successful apply consumes
  the lease; reopen the object before applying again. No `tee`/shared `/tmp` file or
  background FileSaved cluster mutation remains. See private-copy limits below.
- **⌥⏎ on a manifest**: *Add Kubernetes schema* (one-document files of a
  built-in kind); *Convert List to separate documents* (caret inside a
  `kind: List` with `items:`) and *Convert documents to a single List* (two or
  more manifest documents). Both edit only the open file and copy the item lines
  as written, so comments stay; neither is offered when the file has anchors,
  aliases or Helm actions.
- **Manifest problems** on open and save (optional `events` and `project.read`;
  a file is a manifest when a document has both `apiVersion:` and `kind:`; files
  with Helm `{{ }}` actions only get the Helm checks): `deprecated-api-version`
  (versions removed in v1.16 through v1.27, with the replacement),
  `duplicate-env-name`, `missing-name`, `missing-selector` (apps/v1 workloads),
  `chart-missing-key` (`Chart.yaml` without `apiVersion`, `name` or `version`),
  `chart-unknown-key` and `chart-invalid-value` (a key outside the Chart.yaml
  schema; a bad `apiVersion`, `type`, `deprecated` or `version`; one switch,
  `checkChartSchema`), `helm-whitespace-trim` (`{{-foo` and `foo-}}` in a chart's
  `templates/`; a trim marker needs a space beside the dash) and
  `helm-excessive-trim` (`{{-` or `-}}` on an output action that joins a line
  to the one before or after it; it works on the text, so a `}}` inside a string
  literal can confuse it). `unknown-key` (a top-level key a built-in kind does not have,
  and, as an Info because a newer Kubernetes may have added it, a `spec` key outside the
  bundled table), `unknown-kind` (a kind its core, apps, batch, networking.k8s.io or rbac
  group does not have; custom resources, other groups, `*List`, anchors and Helm files
  are never judged), `read-only-field` (`status`, `metadata.uid`, `resourceVersion`, ...;
  Info; also `ComponentStatus`, `Binding` and `Eviction`, which are not applied) and
  `deprecated-key` (`spec.serviceAccount`, `spec.rollbackTo`, `spec.loadBalancerIP`, the
  `ingress.class`, `storage-class`, `storage-provisioner` and pod seccomp annotations,
  `metadata.clusterName`, `Endpoints`, `ComponentStatus`, the `Recycle` reclaim policy).
  `missing-key` (an Info: `spec.containers` of a Pod, `spec.template` of a workload that
  has a selector, `spec.serviceName`, a CronJob's `schedule` or `jobTemplate`,
  `spec.podSelector`, a claim's `accessModes`, `roleRef`, ...; a document without its
  companion keys is taken for a patch and left alone; switch `checkRequiredKeys`) and
  `invalid-value` (an Info: a value outside the known set of `restartPolicy`,
  `dnsPolicy`, Service `type`, the traffic and affinity policies, update strategies,
  `concurrencyPolicy`, `completionMode`, `volumeMode`, and a non-number or non-boolean in
  the numeric and boolean `spec` fields; templated values are skipped; switch
  `checkFieldValues`). The kind tables cover the core, apps, batch, networking and rbac
  kinds plus a few of autoscaling, policy, storage, scheduling, coordination, discovery,
  node, certificates, admissionregistration and apiextensions; the other kinds of those
  groups are only checked for existing. In a chart template `helm-unknown-value` (an Info: a `.Values`
  path whose first missing segment is not in `values.yaml`; quiet for `global`, `tpl`,
  `with`/`range`/`define` bodies, aliases in `values.yaml`, charts with `dependencies`
  or `charts/`, and a missing last key next to `default`, `if`, `required`) and
  `helm-unknown-include` (`include "x"` or `template "x"` with no `define "x"` in the
  chart's `templates/`). Each has a switch in the settings.
- **Commands** on the open file: Apply, Validate (server dry run), Diff,
  Delete Manifest Resources (lists them with a server dry run, then waits for
  Confirm Delete), Reload Manifest from Cluster (replaces each document of the open
  file with `kubectl get -o yaml` of the same kind, name and namespace, keeping leading
  comments; one undo step; needs the optional `editor.write`; a document the cluster
  does not have is left), Render / Apply Kustomization (the file's directory), Preview
  Helm Template, Lint Helm Chart and Update Helm Dependencies (the chart
  around the file; optional `process:helm`). Update Helm Repositories runs
  `helm repo update` (also optional `process:helm`; if denied, a notification
  says so). How to Install kubectl shows the install instructions in a
  notification; it cannot install anything.

The chosen context and namespace are passed as `--context` / `--namespace`,
so your kubeconfig is never modified. To use another kubeconfig file (or several), set it in
Settings › Plugins › Kube Explorer; otherwise kubectl uses its default.

Not ported: cluster-backed checks (unknown resources or fields against the
live API), CRD-aware validation and `requirements.yaml` (Helm 2). The key tables cover
the kinds listed above only.

Cluster browsing/operations require the optional `process:kubectl` grant. The
existing external-file browsing workflow may use exec-based auth plugins
(`gke-gcloud-auth-plugin`, `aws`, …) installed on the IDE PATH; the separate
pasted-import subset below rejects executable authentication. `editor.read` is
optional for editor-related commands. Import/review need only plugin storage.

Icons: the Kubernetes logo and Icons Set by The Kubernetes Authors
(Apache-2.0, see `NOTICE`). Tree items can only name icons the IDE knows, so
each is declared as a `fileIcons` entry on a pattern no file has, and the tree
uses its `ext:fileIcon/aritti.kube-explorer/<id>` name.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Port forwarding and process lifetime

Port mappings are numeric (`8080:80`, `443`, or `:80` for an allocated local port),
limited to 16, with valid ranges and unique explicit local ports. Named remote
ports are outside this form's current subset. CLI flags cannot be entered as port
mappings. Both modes explicitly bind `127.0.0.1` and show the target/argv before
launch. Scope includes the selected kubeconfig/context, resolved namespace and a
fingerprint of the selected local cluster/server/user/TLS/proxy metadata. Raw
credentials are not retained; no `--raw` config read is made. Rechecking before
launch/restart rejects changed selections or context-to-cluster mappings. This
is not an immutable kubeconfig snapshot: a final filesystem/credential race at
external CLI launch remains outside the plugin's atomic control.

**Start in Terminal** preserves the existing long-lived terminal workflow. Stop
it in that terminal; managed controls do not control terminal or external sessions.

**Start Managed Session** reviews one owned `host.start` command. Approval only
prepares it; it does not spawn or replace a process. The returned **Prepared**
pane shows the concrete project/argv and a distinct **Launch Reviewed Forward**
button. Restart similarly prepares a **Launch Reviewed Restart** button. The Services
**Port Forward Status** button opens bounded output/status with Refresh, Stop and
Restart. Restart requires a stopped/exited session, a matching current selection
and scope, and confirmation. Replacing a live managed session is confirmed and
stops only that owned handle. Spawn is not a claim that a listener is ready;
exits and polling/start failures are shown. Prior output is retained on a failed
replacement/restart, within the 16 KiB presentation bound.

The managed draft and process now belong to a concrete invoking project root
(`project.read` is required); the host starts the child with that project as its
working directory. Start and Restart both show the exact project, executable,
arguments, loopback restriction and lifetime before approval. The single pending
approval expires after 60 seconds; its separate launch event must supply the
exact single-use token and a new invocation whose project matches the reviewed
root. That event also checks selection, settings, input and the local kubeconfig
fingerprint before replacing any previous child. Cancel, Back, new review, port
input/scope changes, deactivation, stale/error and consumed tokens invalidate the
approval. Queued copies of the old review button cannot launch it. There is no
second confirmation after choosing the distinct Launch button.

Native `Invocation.project()` is a snapshot for one call, including while its
modal confirmation is open; calling it again cannot prove the active project
stayed unchanged. The separate explicit Launch event obtains a new snapshot and
closes that modal gap. A project switch after that event begins (including while
local config is checked) can still race process start: API 1.3 has no atomic
expected-project/client check at spawn. Global open-project membership cannot
prove which project a window currently owns. MockHost's dynamic root accessor is
not evidence of native within-call freshness. Terminal mode retains its explicit
captured-target confirmation and remains available without a project; this split
launch flow applies to managed mode. Unsupported kinds cannot create a draft.

Observed project, resource, context, namespace or kubeconfig choice changes retire
the owned child and require a new draft. Plugin deactivation releases it too.
Status and controls refuse a different project's session; failed replacement in
another project cannot copy the previous project's output. Only the owned handle
is stopped. There is one lease per plugin instance, not a per-window registry:
API 1.3 does not expose a stable invoking window identity. Same-project windows
share the session; observing another project conservatively retires it.

Rendering polls cached process output without invoking kubectl. Explicit **Refresh
status** additionally revalidates the captured local cluster mapping and retires
it if that mapping changed or cannot be confirmed. Kubeconfig contents and
credentials are not continuously watched. Exit status no longer discards all
buffered output immediately: each refresh drains at most four bounded chunks,
retaining a draining handle for later refreshes. The displayed tail, including its
omission marker, stays within 16 KiB on UTF-8 boundaries. API 1.3 exposes no reader
EOF flag, so this is available-output draining, not guaranteed final stream
capture; explicit Stop/retirement may discard remaining buffered output.

Managed forwarding is **limited to the plugin instance's lifetime**. v0.1.39 can
reclaim an idle instance after about five minutes, ending its child process and
forgetting its in-memory status. Window close, disable/reload and failure also end
it. `on-schedule` reactivates inactive plugins; it does not keep active handles
alive. The normal terminal option remains the reliable choice for long sessions.
No unsupported keepalive loop or automatic reconnect is used. Durable background
watch/forward ownership and broader reconnect/backpressure behavior remain open
under P10.2; native cluster lifecycle acceptance has not been run for this slice.


## Pasted kubeconfig import (P10.4 static subset)

**Import Pasted Kubeconfig (Private, No Connection)** accepts YAML or JSON in a
manual multiline prompt, asks for a new label and exact context, then shows a
redacted review of profile/context/cluster/namespace/declared user alias/server and authentication
mode. A missing namespace becomes an explicit `default` in the saved selected
context. Only that context, its cluster and its optional user are normalized and
stored. **Review Imported Kubeconfigs (Redacted)** revalidates stored identities
and shows the same metadata without displaying tokens, keys or certificate data.
Labels/aliases overlapping credential material are refused rather than echoed
into metadata. Errors do not include pasted parser excerpts or credential values.

Import performs no kubectl/process, network, clipboard, installation, credential
execution or cluster validation. It never edits the user's kubeconfig/settings or
changes active context, namespace, Services selection or forwarding scope.
Imported profiles are not automatically activated, merged into configured files,
or exposed to the existing Choose Kubeconfig browser. Activation/export/delete
and richer profile lifecycle remain implementation work. Import/review are
available without a project, editor or kubectl grant; existing commands remain
gated by their own capabilities.

Supported input is one Kubernetes `apiVersion: v1`, `kind: Config` document with
unique named cluster/user/context arrays and valid references. Static bearer
tokens, paired inline client-certificate/key data, optional inline CA data and
anonymous users are supported. Unwrapped certificate/base64 fields receive bounded syntax
checks, not X.509/key-pair validation or proof that authentication works. API
servers must be HTTPS ASCII DNS names, full decimal IPv4 authorities or bracketed
IPv6 literals, with an optional decimal port1..65535. Compressed, expanded and
IPv4-mapped IPv6 keep their exact pasted server spelling; a single terminal slash
is allowed consistently with DNS/IPv4. No URI credentials, base path, query or
fragment. IPv6 zone identifiers/IPvFuture, HTTP, insecure TLS, proxy/TLS-name
overrides, impersonation, external credential
files, tokenFile, exec, auth-provider and unknown extension fields are rejected.
Even unused unsafe users/clusters are rejected rather than silently retained.
YAML aliases, anchors, tags, duplicate/complex keys and multiple documents are
rejected before building the document model.

Bounds are 128 KiB input/canonical config, depth 16, 4096 parser events, 64 entries
per collection, 32 published profiles, 192 KiB per private record and 4 KiB per
redacted metadata record. Credential-bearing normalized UTF-8 JSON is written only
under `host::fs::plugin_dir()` through `write_bytes`; that SDK directory/files are
owner-only (0700/private ACL), persistent across reload/restart and removed on
uninstall. Generic KV storage contains only redacted metadata and an opaque
private-file identity/path. These are plugin-owned profiles, not a native account
manager or encrypted OS credential vault.

Files use a SHA-256 identity of the complete canonical private record. Existing
different contents are refused; an identical unpublished record can be reused
without rewriting it. Both stored bytes and redacted metadata are revalidated
before review. There is no deliberate replacement of an existing profile, no
user-controlled filename and no filesystem traversal from imported names.
The SDK has no exclusive-create/CAS or private-file deletion API: precheck/write
is not an atomic no-clobber guarantee against a concurrent external edit;
count/label prechecks are also non-transactional across concurrent windows;
legitimate same-content concurrent imports address identical bytes. Cross-file
payload/metadata publication is not atomic. If metadata persistence fails after
the private write, an owner-only orphan can remain; retrying the identical import
can publish it. Private import files/orphans are capped at 64, and a directory
listing at the host limit refuses new imports. No automatic orphan removal or
credential rotation overwrite is performed. Corrupt identities fail closed and
existing profile records are retained.

Original/API comparison: inspected Kubernetes `KubeConfigKt` safe YAML loading,
duplicate-name/context-reference validation, and pinned SDK `fs::plugin_dir`,
private writes and storage behavior. Generic KV writes alone do not promise the
private file permissions, so credential payloads are kept in the explicitly
private directory. The original full credential/interpreter/connection lifecycle
is not reproduced. Tests use deterministic MockHost files only; no real kubectl,
cluster, credential plugin, clipboard or network operation was performed.

Native import compatibility: new private destination absence is established from a complete bounded private directory listing. The pinned host stat resolves only existing paths and may classify a missing private file as Invalid, unlike the mock. Existing destination validation and post-write byte identity checks remain; exclusive-create/atomic publication are still unavailable.

Services remains discoverable without kubectl permission: a non-resource Kubernetes status row explains the missing grant and the separate private import workflow. Selecting it shows read-only help and does not activate a cluster.


## Resource-reference closure and Rename gate

The registered `DocumentIndexer::index` now always returns empty definitions,
references, aliases and markers. This retains an empty export so earlier host
entries can be cleared. Correct token spans and group/namespace distinctions did
not establish a complete Rename set: the former graph defined a Secret while
omitting Ingress TLS `secretName`, and defined a Namespace while omitting
`metadata.namespace`. RoleBinding and other schema-known relations also remain
incomplete. A preview/version/root check cannot repair missing references.
No resource kind is now advertised as a complete automatic Rename/usage symbol.
This is a reference-model/deployment-ownership gap, not a new SDK blocker or a
claim of cross-project leakage: the host already isolates index storage by root.

The former exact association parser is retained **only** for explicit read-only
Go to Definition and line markers through a plugin-private cache. It preserves
API-group/kind/namespace scope, exact raw scalar/UTF-16 ranges, duplicate identity
rejection, known built-in scopes and same-stream nonconflicting CRD scope facts.
Unknown groups/scopes, malformed/dynamic/escaped/typed names and unproved owner
names stay quiet. Custom-kind and omitted-namespace identities remain file-local.
Explicit namespaces can propose cached cross-file source declarations, but equal
names do not prove one deployment/cluster. Known Helm/Kustomize markers keep
those source associations file-local. Their absence is not proof of all possible
transformers or runtime context. Marker tooltips explicitly label read-only
source candidates and unresolved deployment/cluster identity.

Two positively verified relationships now join that read-only model:
Ingress `spec.tls[].secretName` points to a core Secret in its source namespace;
valid namespaced-resource `metadata.namespace` points to a core Namespace.
Arbitrary `data.namespace`, custom `secretName` fields and cluster-resource
namespace fields do not gain those links. Source/namespace rules follow the
primary [Ingress API](https://kubernetes.io/docs/reference/kubernetes-api/networking/ingress-v1/),
[Ingress TLS example](https://kubernetes.io/docs/concepts/services-networking/ingress/)
and [ObjectMeta namespace contract](https://kubernetes.io/docs/reference/kubernetes-api/definitions/object-meta-v1-meta/).
Adding these paths improves navigation; it does not close every built-in,
custom, encoded, generated or deployment-specific reference.

Private navigation warms only through background document indexing and
open/save events. It retains at most 64 source graphs/8 MiB of accounted graph
storage and only normalized project-owned YAML inputs up to 128 KiB. No Secret
payload or full source text is retained in this cache; only source names,
relationship keys, positions and isolation facts are kept. Cache eviction and
all filesystem-change events discard facts conservatively. No persistence,
cluster calls, kubectl, Helm execution or runtime discovery is introduced.

Editor requests reparse the current provider text, including unsaved edits,
without project-wide scans. A current-file project read enforces the optional
read grant/ownership; source positions still come from the provider's current
text. Cached target candidates are re-read and reparsed before returning their
current range, with a per-call 16-target-file cap plus the shared resolver's 10 ms cooperative
read budget and 256-target cap. A target edit/deletion, denied permission, unknown
project, foreign root, oversized source or unknown schema yields no affected
link. Namespace/transform checks run only while warming, not inside editor
requests, and can be stale until invalidation/re-warming. These are bounded
positive source hints, not complete usage counts or effective runtime resolution.

Native Services, rich resource panels, secret handling, owned logs/forwarding and
all cluster mutation workflows are unchanged. Actual-export regressions verify
empty host Rename graphs for the audited Secret/Ingress and Namespace/ConfigMap
fixtures while explicit navigation still works. Tests also cover current edits,
UTF-16 ranges, reverse users, stale targets, root/grant/overlay separation and cache
bounds. Native Go to Definition/gutter acceptance for the new private provider
path remains unverified; no complete original Rename or screenshot parity is claimed.

## Native Services cluster grouping

The native Services tree now has one stable `kube:cluster` parent labelled
**Kubernetes — selected context**, with the selected namespace in its detail.
Context/namespace selectors, built-in kinds and Custom Resources sit below that
parent. Native presentation keys wrap the original scoped selector, kind, CRD and
resource identities; all-namespace rows retain their exact namespace targets. Root expand/collapse only changes
presentation; it does not discover resources, alter selection/scope, retire a
managed session or issue kubectl calls. Existing child loading/Refresh behavior
and the initial uncached current-context lookup remain unchanged. Permission
required remains its existing explicit non-resource row; unavailable context,
RBAC failures and contextual toolbar actions keep their prior behavior.

This follows the original plugin's Kubernetes contributor/context hierarchy and
its bundled Services screenshot showing a context/namespace parent. It is a
selected-context grouping subset, not the original multi-cluster connection tree,
Workloads/Network category hierarchy, live connection state or visual desktop
acceptance. No additional cluster request is made to invent a cluster name.

## Native Services details and actions

Selecting a resource offers native **Resource**, **Describe**, **YAML** and
**Events** tabs where supported. Describe/YAML/Events run only when opened and use
bounded request timeouts. Events target the exact built-in kind/name and row
namespace; unknown custom kinds do not guess an event kind. Secrets omit the
native YAML tab; explicit Show/Edit/YAML actions retain their existing reveal gate.

Pods and supported workloads also offer streaming **Logs**, with Timestamps;
pods add All Containers and Previous Container. Workloads aggregate their pods and
containers with source prefixes and a bounded concurrent stream count. The native
host cancels the active stream when its pane is hidden or its target changes.

Right-click menus follow the row kind: common Describe/Edit YAML/Delete, pod
Shell/Attach/Debug, workload logs, supported port forwarding and Secret actions.
Logs and Container actions use native submenus; Open Shell, Port Forward and
Secret actions stay directly accessible. Delete stays last. Inactive context
and namespace choices offer Switch; groups offer Expand/Collapse and Refresh.
The root exposes Watch and Kubeconfig submenus, and offline browsing still offers
private kubeconfig import/review. Watch preparation requires an invoking project
and a single namespace. Existing confirmation, permission and ownership checks
remain in the original workflows.

Native toolbar resource callbacks retain the exact scoped row identity across the
host's reselect-before-button sequence. Changed-row, foreign-view and stale-scope
callbacks refuse; mutable form callbacks continue to rotate independently.

See [Services validation and feature scope](../../docs/kubernetes-services-2026-11-10.md).

## Rich log snapshot ownership

The packaged **Log Viewer** now requires an invoking project with `project.read`.
Each bounded snapshot records its project, captured context/resolved namespace,
noncredential local configuration fingerprint and selected source; its status
shows project/scope and age. A different or unavailable project, changed selection,
namespace/context or kubeconfig choice drops that cached snapshot before rendering,
Copy, Download or Refresh. Deactivation drops it too. The download suggestion now
uses the snapshot's owner, never the first unrelated open project.

Refresh is a new event with no modal: it verifies the current local mapping still
matches the snapshot and reads logs with the captured explicit context/namespace
arguments. An unconfirmable or changed mapping clears the snapshot. A failed log
read retains output only after matching owner/scope verification, explicitly marked
as a failed refresh with the previous snapshot's age. Rendering itself performs no
log or configuration discovery. API 1.3 invocation identity is a per-call snapshot,
so a change after that event begins remains outside atomic host control; local
kubeconfig/credentials are not continuously watched. No readiness, live-stream or
runtime identity guarantee follows from a snapshot.

The native host-managed **Logs** detail pane and terminal following remain separate
and unchanged. Rich-view Download now uses its reviewed owning-project destination
and optional `project.write`, with collision/root/config rechecks and host dirty-buffer
protection. Atomic exclusive-create FileActions export and broader original
container/download settings remain open. Its modal uses captured-target semantics,
not a claim of post-modal active-project freshness. No cluster, process,
network or native UI acceptance ran for this ownership slice.

## Packaged panel invalidation and private-cache ownership

State changes consumed by both native Services and the packaged resource panel
now refresh both views. The panel additionally receives only the model-free
`{"channel":"ui-kit-refresh"}` signal: its hub requests a new model with its own
frame, sequence and project identity, deferring/coalescing while an action or form
field is pending. Native Services selection/retirement, draft preparation,
context/namespace changes, details/log/Secret changes and Back therefore update an
already open panel. Stable renders do not emit another invalidation. Native
`refresh_view` alone updates typed views but is not a packaged WebView refresh
contract. Rich log Scroll to end now targets the packaged panel as well.

Automatic refresh makes cache ownership essential. Cached rich details, revealed
Secret plaintext, Secret edit/draft data and pending manifest review are bound to
a concrete invoking project with `project.read`, the current context/namespace and
kubeconfig choice. Foreign, missing or revoked identity and scope/choice changes
clear them before rendering or stale write actions; anonymous plaintext/detail
preparation is refused with the identity-permission requirement. The existing
Secret reveal setting, redaction, RBAC and resource-version checks remain in force.
Same-path kubeconfig/server/credential changes are not discovered during render;
those remain unmonitored. Same-project windows still share the instance. Native
identity is a per-call snapshot, so no atomic multiwindow or modal-project guard
is inferred. The model-free signal contains no cached data or authority token.

Source/MockHost and actual packaged-JavaScript DOM-harness checks cover selection
retirement → invalidation → correlated render → non-Running model, stable-render
feedback prevention, and cross-project private-cache refusal. Shared browser
protocol tests are separate; native desktop acceptance of this final artifact is
pending and must not be inferred from the older324 Desktop47 journey.


### Explicit cached cluster namespace completion (P10.3 subset)

**Read Cluster Namespaces for Completion** explicitly queries namespace names using
the selected context and local redacted cluster identity, with `process:kubectl`
and an owning `project.read` invocation. Inspection and query both explicitly use
that project's working directory, including relative configuration resolution. The command checks local scope before/after
the read; it does not run from completion/render. Only `namespace/<DNS-label>`
output is accepted (64 KiB/512 unique names). A failed same-owner refresh retires
that completion snapshot; other process/log/forward snapshots are untouched.

For 60 seconds, exact current YAML `metadata.namespace` values in known built-in
namespaced group/kinds receive positive cached suggestions, including empty or
quoted values, with UTF-16 replacement ranges and captured context/age labels.
The source deployment target is explicitly unresolved. Cold/expired/foreign or
nested-project caches, changed context/namespace/kubeconfig choices, malformed or
ambiguous owner fields, flow metadata, Helm actions and unknown/cluster-scoped kinds
stay quiet. Documents are limited to 64 KiB; output to 200 suggestions. Alias/tag,
tab and other conservatively unsupported source contexts are suppressed. No
kubectl, network or filesystem discovery runs on the new completion path.

This is a dated metadata hint, not live authorization, namespace existence/absence
validation or a link proving this source deploys to that cluster. Same-path external
kubeconfig/credential/server changes are not continuously observed by a provider;
refresh explicitly or wait for cache expiry. Browser reload/scope changes and
plugin deactivation clear the cache. No automatic Rename references are enabled:
the existing indexer continues returning empty host edges while private source
navigation remains separate. Full resource-reference completion, deployment-owner
closure, CRD namespaces and safe complete Find Usages/Rename remain partial.


### Explicit managed resource watch (P10.2 supported subset)

**Review Managed Resource Watch…** chooses Pods, Services or Deployments in one
selected namespace, captures the local redacted kubeconfig/context/cluster mapping
and shows the exact project/argv review. Approval is inert: the native
**Kubernetes Resource Watch** supporting pane requires a separate **Launch Reviewed
Watch** event within 60 seconds. That event uses a fresh project invocation,
rechecks scope/settings/expiry, consumes the approval and starts its owned
`host.start` child. Cancel creates no child. Required optional grants are
`project.read` and `process:kubectl`; no install, credential import or configuration
mutation is performed. The command can authenticate/connect through kubectl.

The fixed command uses `get <kind> --watch --output-watch-events -o json` with pinned
context/namespace and a ten-minute request timeout. Both local scope inspection and
watch spawn explicitly use the owning project working directory; relative config
and environment paths therefore have the same base in both steps. kubectl owns LIST→WATCH and its
internal resourceVersion; no unsupported get `--resource-version` flag is used.
The stream has no trusted initial-end marker: initial completeness stays **UNKNOWN**.
The pane shows positive observed names only; it does not replace the Services tree,
validate resource absence, publish Rename edges or prove a source deployment target.

**Refresh Watch Status** explicitly revalidates local scope and drains at most four
16 KiB chunks. Rendering does not poll the child, run kubectl or discover files.
Fragmented/concatenated JSON is accepted; strict bounded structure, kind/API,
namespace/name/UID and opaque resourceVersion checks precede accepted metadata.
ADDED/MODIFIED update observations and DELETED removes only a matching UID.
BOOKMARK is accepted without an assumed interval; ERROR/410, malformed output,
unknown shapes, caps and polling failures release the owned handle and retain the
last accepted metadata as historical. Raw object bodies and stderr are not displayed.

Bounds are 64 KiB/event, 128 KiB pending JSON, 16 levels/4096 YAML-guard nodes and
128 observed resources. The view shows at most 32 current and 16 prior names with
counts/age and explicit incompleteness. Consumer limits and manual polling do not
promise loss-free host/server backpressure; overflow stops instead of claiming a
complete truncated result. Exit and available-output draining are distinct; the
SDK has no reader EOF guarantee. Stop/retirement may discard pending output.

**Stop Owned Watch** kills/releases only its stored Child. **Review Restart…**
starts a new reviewed LIST→WATCH after a separate launch; no automatic reconnect
is hidden in polling. Prior accepted names remain separately labelled history,
never merged into the new possibly incomplete observations. Failed new start
retains the previous owned session. Context/namespace/kubeconfig-choice changes
retire live state; Refresh also checks same-path local cluster identity. Foreign
renders give a safe message without stopping another open project's watch.
Observed project closure, lost project access and plugin deactivation dispose
owned process/cache state. Same-project windows share the one instance lease;
there is no durable per-window registry.

Native invocation identity is a per-call snapshot: the fresh launch closes the
review-modal gap but cannot atomically bind project/config/auth through every
later host operation. Same-path credentials/config are not continuously watched.
Idle reclamation (normally about five minutes), window/plugin stop or failure can
end the process before its CLI timeout; there is no schedule keepalive, background
job durability, readiness or real-cluster acceptance claim. Existing host-owned
log panes and forwarding controls remain separate. Whole P10.2 parity, automatic
Services reconciliation, broader kinds/CRDs and reconnect policy remain partial.


### Private resource YAML copies (SDK 1.3)

The optional editor workflow supports stable built-in core/apps/batch/networking/rbac
identities listed in `yaml_editor.rs`; no Secrets, CRDs or guessed legacy schemas.
Reads and replacements use the owning project working directory, explicit captured
context/namespace and redacted local config identity. This is a local source guard,
not proof of effective remote credentials or a transactional configuration lock.
`kubectl replace` receives the current reviewed source on stdin; no private path is
reread at execution. It retains UID/resourceVersion, so server conflicts fail and
user text stays available. The command reports exit success, not deployment health.
There is no cluster Undo/rollback or autosave behavior. Existing panel apply behavior
is separate and unchanged.

Each copy is bounded to 64 KiB, one alias/tag/duplicate-free document, depth 16 and
4096 parser nodes. At most 16 live leases and 64 persistent backing files are allowed.
Review cancellation, expiry, changed source, missing/foreign project, changed local
scope and replay do not replace a resource. Failed apply consumes the approval but
retains the editable copy. Activation/reload cannot recover persistent old autosave
records as mutation authority. Plugin deactivation drops leases and retains files.

**Retire Private YAML Editor** removes apply authority and may blank only a generated
copy whose current editor text and backing bytes still match their original digest;
edited copies stay. Host private writes reject dirty buffers. SDK 1.3 has no private
file-delete API or conditional write/CAS: files remain, external filesystem races are
not excluded, and storage is not advertised as encrypted or automatically erased.
Backups/editor histories can retain contents. Closing an editor is not an observed
lease-close event; use Retire or reload/deactivate to release authority. Private-editor
behavior is mocked/source-validated; no new native acceptance is claimed.

### Project cwd consistency for owned forwarding

Project-owned forwarding now uses the same explicit project working directory for
local scope inspection, port discovery, review/revalidation and managed Child
launch. A relative kubeconfig setting is therefore interpreted from one base across
those operations. Owned port discovery refuses a closed project rather than falling
back to a global process cwd, and its raw tool error text is withheld. The separate
legacy unbound terminal inspection path remains unchanged; these checks do not
establish atomic config/credential identity after the final host operation or real
listener readiness. No real cluster command ran for the MockHost validation.

### Log-view UTC time-prefix safety

Since/Until now accept bounded ASCII UTC date/time prefixes with valid calendar
fields (date, hour, minute, second or up to9 fractional digits; optional trailing
Z). A date or minute prefix includes that whole precision unit. Timezone offsets
and leap seconds are outside this client-side filter subset. Invalid, Unicode or
reversed-range input is rejected before replacing the last accepted filter; the
owned log snapshot and prior display remain available without a new cluster read.

Timestamp recognition uses complete UTC-Z stamps. Fractional comparison pads an
absent fraction with zero, avoiding ordering errors across timestamp precision.
Malformed/unsupported timestamp-like payloads remain ordinary continuation/body
text and are not stripped by Hide Time. Defensive rendering of an invalid internal
bound shows a problem and no lines, retaining underlying snapshot data. This fixes
a reproduced Unicode byte-slice panic; no real cluster or native UI run is claimed.

### Project-owned rich log reads

The rich Log Viewer now explicitly uses its invoking project working directory
for current-context resolution, redacted config inspection, initial snapshot read
and Refresh scope/read. Relative kubeconfig settings use that one base. Owner
closure/change still clears the cached view before another read; there is no global
fallback for this already project-bound tool. Raw read errors/output are withheld;
a failed read retains the prior snapshot only after same-owner/scope verification.
The shared request-scope path below also covers Services/browser commands. Neither
path establishes continuous credential watching, atomic config identity, native
following, reconnect or loss-free buffering. The source tests use MockHost only.

### Shared request scope and opaque resource keys

Project-capable browser/resource/Secret and source-command process calls now use
one explicit invocation project as their working directory. The binding is local
to that call and restores its prior state on return/error/unwind, including nested
calls. It is never reused as a last-project fallback; a captured project that closes
or loses access cannot silently become a global process. Source manifest, Kustomize,
Helm chart and Reload operations reject an editor owned by another/innermost nested
project before running a tool or applying a reload edit.

Resource rows in project-bound views carry opaque owner/settings-epoch correlation
proofs. Selection, activation, context-menu commands and native detail requests reject
old proofs after project/context/namespace/kubeconfig-choice changes, even when a new
project has the same literal kind/name. Browser context/children/selection are reset
on these changes. The proofs do not grant process permission, identify a resource UID,
prove runtime deployment ownership or provide stable per-window identity.

A known invoking project whose identity lookup is denied fails before process or editor actions.
Explicit global compatibility remains for browsing with no invoking project,
standalone Helm repository operations and existing unbound forwarding inspection.
The tree labels Project or Global CLI mode. Disconnected kubeconfig import/review,
static help/preferences and external global utility commands do not require a project
execution binding. Global compatibility does not become a managed project lease.
Owned watch, private YAML, cached namespace reads and rich logs retain their separate
explicit Project handles; editor providers still do not query kubectl.

Discovered resource types are admitted only as literal DNS plural/group names, never
CLI options; lookup validates again before execution. API-resource discovery refuses
malformed/oversized batches (256KiB/4096 types) as an error rather than publishing a
false empty inventory. This is spelling/scope safety, not full CRD type or namespace
semantics. Same-path config/credential changes, post-invocation races, real readiness,
backpressure and native cross-project acceptance remain separately unverified.

Single-namespace resource callbacks reject slash/percent paths, comma target lists,
whitespace/control characters, dot segments and leading options before CLI or native
log construction. Literal RBAC names containing colons remain supported.

Helm repository cache decoding uses bounded JSON strings; alias/URL conversions
read one YAML scalar and escape the replacement scalar. Quotes, backslashes and
quoted hash characters retain their literal value; trailing comments stay outside
the edit. Providers still use only the local cache and never invoke Helm.

Editable YAML panels correlate Apply messages with the exact page instance. Opening
a fresh page retires the previous callback even for the same resource; a failed Apply
keeps its token for retry, while a successful refresh retires it. Unproved legacy
Apply messages refuse before process execution. This is callback correlation, not
manifest metadata validation or Kubernetes resource UID locking.

Secret JSON reads are bounded to 2 MiB and strict JSON/object/depth validation before
plaintext or forms are published. Data keys must be unique supported Secret keys
(up to 4096); repeated data members refuse. The edit path retains the original
serialized object and resourceVersion rather than reserializing unrelated fields.

Secret Show/Edit responses must identify a v1 Secret with the selected name and a
concrete valid namespace matching the selected namespace when explicit. Edit also
requires a nonempty resourceVersion before building replacement authority. This
validates the returned target; it does not freeze cluster identity after the read.

Helm Template revalidates its release argument when consumed, including the fallback
chart directory basename and persisted values, so a leading option or control value
cannot become Helm flags. An explicit safe release still permits such chart paths;
Lint and Dependency Update do not consume the release argument.

Secret data decoding requires canonical padded standard base64 (CR/LF wrapping is
allowed). Truncated groups, misplaced/excess padding, nonzero trailing bits and
other whitespace refuse instead of silently producing changed editable bytes.

Secret Show/Edit/Create form commands withhold raw failed-command diagnostics,
including process-level errors, rather than trying to redact transformed text or
binary data. Safe Conflict/AlreadyExists messages and numeric status remain; failed
Edit retains its form. Stale/cancelled/permission classifications stay explicit.

On the next exported render/action after allowRevealSecrets is disabled, cached
Secret YAML is cleared along with revealed/edit forms, retiring its page Apply
token. Secret YAML failed read/Apply diagnostics are also withheld; failed Apply
keeps its permitted page for retry. Nonsecret YAML error behavior is unchanged.

Mutable rich controls now carry an opaque instance token in their emitted field and
button IDs. New Secret Create/Edit forms, YAML/details previews and selections retire
old control callbacks, even for the same target after a project switch. Manifest
delete confirmations and selected-resource toolbar actions use the same check; the
first resource Delete click rotates its proof before confirmation. Raw legacy mutable
control events refuse. Context-menu commands retain their captured row proof, and
failed form/page submissions retain current tokens for retry. These correlate UI
state and do not grant Kubernetes permissions or freeze post-invocation identity.

Log snapshot control IDs also correlate with the exact loaded snapshot. Reopening
or successfully refreshing retires old Copy/Refresh/filter/export controls; a failed
matching refresh retains the accepted snapshot and its current control tokens.
The existing project/scope/selection guards still run before log operations.

Forwarding draft fields/terminal-review controls, prepared-review cancellation and
managed-session status/Stop/Restart controls also carry instance proofs. Fresh
drafts/reviews/replacement sessions retire old callbacks, so an old Stop cannot
stop a newly approved session under the same owner. Existing exact reviewed launch
tokens, scope/selection ownership, loopback limits and child-handle checks remain.
Stale queued draft edits/prepares refuse without consuming a current reviewed ticket.

Helm aliases whose cached name maps to conflicting URLs do not offer conversions;
identical rows deduplicate and independent unique aliases remain available. Both
fresh repository JSON and older private cache rows receive this conservative check,
so URL-to-alias-to-URL conversion cannot silently choose a conflicting first target.

Back and Create Secret entry controls also correlate with the current view/form.
Initial browser render issues the proof; project/settings changes retire it before
new controls can act. Delayed old controls cannot discard or replace a fresh form's
unsaved values. New revealed Secret caches renew the proof, and current Back still
dismisses its own view without touching an owned managed child.

Verified resource keys remove their proof envelope exactly once. Bare resource
parsing preserves the original admitted name, including embedded/repeated '~'
characters, instead of silently redirecting actions to a prefix name. This is
exact-target preservation, not a claim that every admitted synthetic name is valid
for every Kubernetes kind; the API may still reject an unsupported literal name.

Project-bound context/namespace child choices carry owner/settings-epoch proofs for
both selection and activation, so old same-name choices cannot retarget a different
project. Explicit unbound browsing retains raw legacy choices, including context
names containing slashes or tildes; owned-choice IDs never downgrade into that mode.

Protected mutable fields use short issued IDs mapped to their exact original names,
within the pinned native host's128-byte field-key bound. Native clipping cannot turn
a long Secret key into its valid shorter prefix. Maps retire with instance/scope
changes; old full-name control encodings refuse. Revealed-value field IDs are bounded
indexes while labels/values stay unchanged. Tree identifiers above the host1024-byte
bound become explicit non-actionable rows, avoiding clipped target identifiers.
These host-converter checks are source/MockHost validation, not native acceptance.

Structural Explorer nodes/parent links now carry owner-bound tree IDs as well as
resource and choice leaves. Preparing/committing a managed forward retires earlier
tree callback epochs, so a late old group/row selection cannot indirectly stop the
replacement child through selection retirement. Fresh selection retains the existing
retirement policy. Raw structural IDs remain explicit unbound compatibility only;
cached facts are not reclassified as fresh by issuing new callback IDs.

Successful panel navigation replaces the prior visible mode (Secret reveal/edit/create,
YAML/describe, log snapshot or forwarding review) instead of leaving a higher-priority
cached form on screen. Failed reads keep the prior mode. Known unsaved Secret form
values refuse replacement until Save or explicit Back; clean forms may be replaced.
This guard covers guest-held Secret form state, not unsent edits in a YAML web page.
Panel navigation keeps an independently owned forwarding child/watch/private editor;
its hidden uncommitted forwarding review and prior rich callbacks are retired.

Helm repository alias/URL edits preserve trailing YAML comments even when a plain
URL contains literal apostrophes or double quotes; only a scalar-opening quote
starts quoted-value scanning. CRLF/source outside the scalar remains exact.

Chart source actions require one bounded YAML mapping document without duplicate
keys, aliases, anchors, explicit tags or complex keys (8192 nodes, depth32).
Repository conversions target actual scalar mapping keys at the chart root or in
dependency items; description text and unrelated nested mappings are excluded.
Add dependency validates the resulting document and withholds unsupported quoted-key
or flow-sequence insertion shapes instead of returning duplicate or malformed YAML.

List↔documents conversions compare bounded parsed item values before offering
line-copy edits (8192 nodes/depth32/256 documents, no ambiguous duplicates, aliases
or explicit tags). Whitespace/chomping changes that alter ConfigMap scalar bytes
withhold the action; equivalent keep-chomp values still convert. Splitting a List
with nonempty wrapper metadata also refuses rather than dropping those fields.
Comments and CRLF retain the existing copy behavior for accepted conversions.

The schema modeline action shares bounded parsed source-document checks with
conversion guards. Quoted API/kind values and trailing comments decode correctly;
one unambiguous document and identifier-shaped schema path components are required.
The inserted modeline follows CRLF when the source uses CRLF. This local mapping
does not fetch schemas or prove remote schema availability.

Revealed Secret values larger than the pinned native multiline field's1MiB limit
are shown as numbered Unicode-safe parts with explicit join-without-separators
instructions. A large binary value's base64 can exceed this limit even when decoded
bytes fit1MiB. Ordinary values keep their existing display; no partial field is
silently presented as the whole value. Converter-emulated checks are separate from
native UI acceptance; no value is logged or persisted by this presentation.

Read Cluster Resource Names for Completion explicitly captures names for one chosen
Secret, ConfigMap, Service, PersistentVolumeClaim or ServiceAccount kind in the
selected single namespace/project. The command may connect/authenticate through
kubectl; it requests names only and retains no object bodies or Secret values.
Snapshots expire after60seconds (512names/64KiB per kind); failed same-owner refresh
retires that kind while a failed foreign read preserves its prior owner’s cache.

YAML completion uses those positive names only at existing schema-known reference
paths, with an explicit matching source metadata.namespace and innermost project
owner. Cold, expired, foreign, changed-choice and unsupported source contexts stay
quiet. Up to200 candidates replace current UTF16 scalar ranges, including blank or
quoted values; other documents in a valid stream do not supply target ownership.
Providers never run kubectl. Suggestions show capture age/scope and unresolved
deployment target; same-path configuration/auth changes require refresh or expiry.
No runtime identity, authoritative inventory, graph edges or automatic Rename is
created by this completion feature. Native acceptance remains separately dated.

The resource panel’s context menu follows the clicked row: contexts and namespaces offer Switch when inactive; groups offer Expand/Collapse and Refresh; Secrets also offers Create Secret in a single namespace. Resource menus expose Describe/Edit YAML/Delete plus pod, workload, Secret and port-forward actions only for supported kinds. Menu actions use the current scoped row key, and command validation rejects stale keys and mismatched kinds. Delete retains its confirmation.

The top Services toolbar shows applicable Describe, Edit YAML, Follow Logs, Open
Shell, Secret actions, Port Forward and Delete as native icon buttons with
tooltips. Secondary pod/log actions use the trailing More Resource Actions picker.
Resource callbacks use fixed-size action/scope proofs within the host's 64-byte
button-ID bound, including long resource names; host reselect does not invalidate
them, while changed targets/scopes and altered actions refuse.
