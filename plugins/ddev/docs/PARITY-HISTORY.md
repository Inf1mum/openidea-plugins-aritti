# Historical parity passes

Implementation notes retained for traceability. The current assessment is in
[SOURCE-AUDIT.md](../SOURCE-AUDIT.md). Counts and schema versions below describe
the state at each historical pass.

## Second parity pass — 2026-11-08

Rechecked original HEAD (`7291f3ea`) and OpenIDEA HEAD
`b57fba8b4314c0d2c963ca115a176f6c575661da`. No SDK/WIT changes from the pinned
v0.1.39 affect these integrations; #2133, #2134, #2135, #2139 and #2140 remain
open without responses at this check. The original source remains 1.2.10.

Additional implemented corrections:

- Explicit runtime commands re-read current state, including when watching is
  disabled. A stale running/stopped snapshot cannot authorize a now-invalid
  command or prevent Start after an external stop. Only short status/installation
  reads block; lifecycle processes still run in native terminals.
- Configuration existence is checked before cached runtime reuse. Removing
  `.ddev/config.yaml` immediately removes cached running controls on the next
  render/command, without depending on a watch event.
- Saves of `config.*.yaml`, `docker-compose*.yaml` and project preferences refresh
  runtime status, including with scheduled watching disabled. Filesystem watching
  includes the preference file; unrelated YAML saves are ignored.
- Delete and Power Off remain discoverable if DDEV is installed/configured but
  `describe` fails, matching their original availability predicates. Installation
  and execution grants are checked again before launch; confirmations remain.
- The project Browser action falls back to the web service's HTTPS-first URL if
  the top-level URL is absent, consistent with original web-service actions.
- Cached Services renders republish invoking-window context keys. Dynamic status
  text uses a neutral `DDEV` label when publishing with multiple projects open,
  because the host broadcasts status items to every window. Per-project state
  remains visible inside Services. This is a newly documented API gap, requested
  in [#2146](https://github.com/l0ng-ai/openidea/issues/2146); it is not full parity
  with the original per-project status widget/visibility.

Six added regression tests exercise these corrections (55 native tests total).
The checked-in standalone Wasm smoke harness tests actual activation, nested
keys, conditional database values, referenced hook tasks, diagnostics and hover.
With default runtime limits, observed activation was about 3.3 ms and provider
calls about 0.04–5.2 ms on this machine; each call was given a 50 ms deadline.
These observations cover selected fixtures, not a worst-case performance claim.
The fake host exposes no project files and grants no process/network/storage
access. This verifies exports and actual sandbox execution independently of the
native MockHost suite; desktop/container end-to-end coverage is still pending.


## Third parity pass — 2026-11-08

Original source HEAD remains `7291f3ea`. OpenIDEA HEAD was rechecked at
`9a4ef162bbb4da321253a108284daceb98c8f0e3`; the SDK/WIT interfaces affecting
this port remain unchanged. All six filed API requests remain open. No existing
blocker was reclassified as implemented simply because another pass was made.

This pass concentrates on the inherited YAML assistance and project-owned
resource behavior:

- Completion now works immediately after a mapping colon, before whitespace or
  an existing value, and before closing flow delimiters. Its edit adds the
  required separating space and preserves valid YAML; previously `php_version:`
  at that cursor yielded no suggestions.
- The diagnostic cap is enforced before schema validation if duplicate-key
  errors already filled it. Previously 100 duplicate errors plus a schema type
  warning could produce 101 diagnostics.
- Standalone log editor keys now include stable project and service identity,
  and titles include the DDEV project name. The host keys these editors by
  plugin/window/key, without project identity, so the old generic `ddev-project`
  key could focus another project's stream after switching roots in one window.
  Reopening the same project/service still focuses one tab. In addition, each
  log command now explicitly supplies the reported DDEV project name. The host
  resolves the current window's working directory again when spawning/restarting
  a log PTY, so a unique editor key alone cannot keep an old tab pointed at its
  original project after a root switch. DDEV's
  [`logs [projectname]`](https://github.com/ddev/ddev/blob/v1.25.3/cmd/ddev/cmd/logs.go)
  supports the explicit target (also checked in the installed CLI help). Invalid
  project names are rejected rather than interpreted as options.
- Connection-file maintenance rejects multiple entries with the plugin-owned
  ID before writing. The original IDE model has unique data-source IDs; the
  file-based adaptation must explicitly reject this ambiguous input. Both
  entries and the entire file are left unchanged for review.

Five additional regression tests bring the total to **60**. Besides targeted
cases, a deterministic partial-document test checks completion, diagnostic and
hover ranges at Unicode character boundaries through prefixes of quoted/flow,
block-scalar, anchor/alias and incomplete YAML samples. This is regression
coverage, not a claim of exhaustive YAML language-server parity.

The actual Wasm harness now includes immediate-colon completion, Unicode ranges
and the duplicate-diagnostic bound. All tested calls pass the 50 ms runtime
deadline; observed provider times were about 0.04–5.9 ms in this run. Strict
Clippy, the release Wasm build and package check pass. The development link is
still active. Live IDE/container limits were rechecked: OpenIDEA is 0.1.38 and
Docker remains stopped, so desktop end-to-end verification is still pending.


## Fourth parity pass — 2026-11-08

Original HEAD remains `7291f3ea`. OpenIDEA HEAD was compared at
`4d5de201c462817d948ad9019b2f96bf4d88b74d`; there are no intervening SDK/WIT
additions affecting the documented integration blockers. This pass compares
project lifecycle and notification behavior with `DdevNotifierImpl`, the
project-owned state/listeners and `InstallationInstructionsAction`.

Implemented corrections:

- Failure-notification history is keyed by project. Without the optional storage
  grant, visiting a different failing project or recovering another project no
  longer causes an already-reported failure to notify again.
- Successful windowless scheduled reads reset the recovered project's failure
  history, including persisted history. A subsequent failure can notify again;
  background recovery does not publish another project's UI/context globally.
- Pending Configure follow-ups coexist per project in memory, even without
  storage. Configuring project B no longer replaces A's pending follow-up. Each
  changed file opens once in its own invoking context. Idle-reclaim persistence
  still requires the optional storage grant.
- Unsaved Setup drafts coexist per project in the live instance. Switching
  projects preserves reviewed-but-unsaved field values independently and does
  not write preferences. Disabling checks invalidates only that project's draft.
- Project-specific update/recovery/tour notification actions carry an originating
  root. The dispatcher rejects a click from another project's window before any
  mutation or runtime work. Update/error bodies identify their originating root.
  This closes the wrong-project preference-write risk; it does not provide host
  project-targeted presentation. That missing API is requested in
  [#2157](https://github.com/l0ng-ai/openidea/issues/2157), separate from implemented
  action buttons (#1955) and per-window status text (#2146).
- Installation/release/issue links and revealing Services work without an open
  project or a process grant. The original installation action is also global.
- A cached snapshot installed by the scheduler revalidates the selected service
  on reuse. Removing a service returns selection to the project instead of
  retaining a missing service's controls.

Seven additional native regression tests bring the total to **67**. Tests cover
project switching without storage, independent draft/follow-up state, quiet
windowless recovery, origin-checked notification actions, global help and removed
service selection. Strict Clippy, release Wasm build/check and the existing
compiled-Wasm provider smoke suite pass. No desktop multi-window, Docker or live
Database success is inferred from these tests; those end-to-end checks remain
pending on a compatible running host/container runtime.


## Fifth parity pass — 2026-11-08

Upstream source heads were rechecked and remain `7291f3ea` (original plugin)
and `4d5de201` (OpenIDEA). This pass compares `InitPluginActivity`,
`DdevSettingsConfigurable.apply/reset`, the original Composer provider boundary
and DDEV's own CLI argument handling.

Supported corrections implemented:

- PHP and Node terminal commands use `ddev exec --raw -- php …` / `node …`.
  The [DDEV v1.25.3 exec implementation](https://github.com/ddev/ddev/blob/v1.25.3/cmd/ddev/cmd/exec.go)
  assigns `RawCmd` only when the raw flag is explicitly supplied. Without that
  flag, reconstruction through `quoteArgs`/a container shell can expand dollar
  expressions/backticks and lose empty arguments despite separate host argv.
  The explicit flag preserves the intended argument boundaries. Installed CLI
  help was checked too. Composer retains `ddev composer` and its own workflow;
  this does not implement the missing built-in Composer provider.
- Startup initializes every open configured project, including when periodic
  watching is disabled. Previously multiple open projects skipped immediate
  initialization. The multi-project path reads status/maintains integrations
  without publishing a project's context or status globally; existing update
  preferences/throttles still apply.
- Setup exposes **Reset Changes**, matching the original settings reset flow.
  It reloads the current project's saved preferences without writing files,
  invoking DDEV or resetting another project's unsaved draft.
- Preference writes accept standard and inline TOML tables at both the plugin
  container and DDEV section. Valid inline settings previously produced a
  table-type error. Unrelated keys and comments remain intact.
- Save Preferences immediately applies enabled database maintenance through a
  fresh quiet status read, matching the original apply/reinitialize behavior.
  This also works when scheduled watching is off. Missing optional runtime
  permission does not prevent saving settings.

Five added regression tests bring the total to **72**. Tests cover literal PHP/
Node arguments (including dollar signs, backticks, empty strings and comments),
windowless multi-project startup, independent reset, three inline-table layouts
and immediate database maintenance after Save. Strict Clippy, release Wasm
build/package check and the actual-Wasm provider suite pass (50 ms per tested
provider call). Docker execution of PHP/Node, native desktop UI interaction and
full remote-interpreter/Database integration remain unverified or API-blocked;
these source/native tests do not establish full runtime parity.


## Sixth parity pass — 2026-11-08

Original HEAD remains `7291f3ea`. OpenIDEA HEAD was rechecked at
`99100e48bd6a939a27435b65c2eec2a3cb4c9fd7`; no SDK/WIT additions affect the
remaining integration blockers. This pass follows original action predicates
(`DdevAwareAction`, lifecycle action subclasses), the host's path-event delivery
and the host's actual command-condition evaluator.

Additional implemented corrections:

- Services events synchronize the invoking project's stored selection before
  capturing a toolbar target. A button arriving before the first render after a
  project switch can no longer inherit the previous project's selected container.
  Synchronizing selection itself does not require executing DDEV.
- Saved/changed paths are matched against the invoking project's root. Nested
  `.ddev` folders, another project's files, similarly prefixed sibling roots and
  empty filesystem batches no longer run the root project's CLI. Windows drive
  paths handle separators and ASCII casing without matching another drive/root.
  This preserves the original root-only configuration discovery scope.
- Filesystem subscriptions include the `.ddev` and `.openidea` directories
  themselves, so parent-only directory deletion/creation events can invalidate
  configuration/preferences. Removing `.ddev` clears configured/running controls
  even with periodic watching disabled.
- Command conditions distinguish an available CLI from a running project and
  from reported database information. Delete/Power Off remain available while
  starting or when description fails but the CLI is installed; PHP/Node/Composer
  require running state, database actions require reported DB info, logs require
  an available runtime, and opening configuration remains runtime-independent.
  Existing dispatch-time checks still handle state changes after presentation.

Five added native regressions bring the total to **77**. In addition,
[the manifest-condition smoke harness](../tests/WHEN-SMOKE.md) runs 105 checks against
OpenIDEA's actual TypeScript evaluator, with only its Zustand store stubbed.
This tests the real manifest across seven state scenarios, including hyphenated
context keys; it is not a desktop menu interaction test. Native tests verify the
published contexts, current-project toolbar selection and filesystem behavior.
Strict Clippy, release Wasm build/check and the compiled-Wasm provider smoke suite
pass. The development link remains active. Full IDE/container/remote integration
coverage remains pending and is not inferred from these checks.


## API 1.3 conclusion archived on 2026-11-09

## Verification and remaining work

**83 native tests** cover the workflows above, including original JSON fixtures,
version/error cases and configuration-assistance regression cases. Strict Clippy
and the v0.1.39 CLI Wasm build/package check pass. The compiled Wasm editing
providers also pass the repeatable [runtime smoke test](../tests/wasm-smoke/README.md)
with default fuel/memory limits and a 50 ms timeout per call. The development folder is linked.

Live end-to-end testing remains open: the installed OpenIDEA is v0.1.38 (the port
requires v0.1.39) and Docker is stopped. Thus this audit does **not** declare full
runtime parity. Host-blocked integrations and deliberate presentation/platform
adaptations are explicitly listed above and in [LIMITATIONS.md](../../../LIMITATIONS.md#ddev-integration-arittiddev).


## Consolidated conclusion — 2026-11-08

Latest comparison: original commit `7291f3ea58cf937ae8c6ac7973ed24876915e308`,
OpenIDEA HEAD `24bee8eef27e87e068ca7da4e54e56eef19e3809`. No newer SDK/WIT
interface closes the remaining blockers. All seven requests below were checked
and remain open. The original descriptor inventory contains no additional
plugin-owned inspection, YAML provider, keyboard shortcut or run-configuration
contribution hidden outside the mapped families above.

| Functional family | Current assessment | Evidence / remaining difference |
|---|---|---|
| Lifecycle actions and recovery | Implemented through native terminals | All declared commands invoked through the compiled Wasm component; native state/confirmation regressions. Exact post-exit refresh remains partial |
| Services, browser, shells, logs and details | Implemented as native contributions | Actual Wasm render/detail/view-event/command exports exercised with fixtures; desktop rendering/streaming remains unverified |
| DDEV project/global configuration assistance | Implemented with pinned official v1.25.5 schemas | Completion, hover, full schema validation, bounded YAML parsing; native and 50 ms Wasm probes. No effective override-file merge or general YAML-server replacement |
| Preferences, setup and multi-project startup | Implemented | Save/reset/inline TOML, independent drafts, startup and project isolation tested; setup uses native forms |
| Version/update checks | Implemented with deliberate daily throttling | Upstream numeric version semantics, structured errors, permissions and project-origin actions tested |
| Database connection setup | Partial | Project-owned file maintenance tested; secure credentials, live Database refresh, introspection and remote/tunnel behavior are not covered by this workaround: [#2134](https://github.com/l0ng-ai/openidea/issues/2134) |
| Remote PHP/Node SDKs, built-in Composer, debugger server/mappings | Partial | Explicit container tools and local PHP hints implemented; IDE registrations/global language-level integration need [#2133](https://github.com/l0ng-ai/openidea/issues/2133) |
| Terminal completion/environment | Partial | Native launch/log specs tested; exit callbacks and command environment need [#2135](https://github.com/l0ng-ai/openidea/issues/2135), [#2139](https://github.com/l0ng-ai/openidea/issues/2139). Original terminal preset/typed-command interception/PTY ownership are not reproduced |
| Executable/platform routing | Partial | Approved DDEV on PATH and host routing; custom executable selection needs [#2140](https://github.com/l0ng-ai/openidea/issues/2140). Original local Windows UNC/WSL patch and remote log tabs remain absent |
| Status and notification presentation | Partial | Root-scoped contexts and origin-checked actions implemented; per-window status and project-targeted background notifications need [#2146](https://github.com/l0ng-ai/openidea/issues/2146), [#2157](https://github.com/l0ng-ai/openidea/issues/2157) |
| Tutorials, reporting and JetBrains extension points | Adapted/omitted explicitly | Native Setup/tour and port issue form implemented; exact Got-It overlays, automatic Sentry reporting, JetBrains dependency dialogs/settings migration and public argument-provider extension are not reproduced |

This final review found and corrected a concrete schema-version gap: the previous
v1.25.3 snapshots lacked project `dbimage`, `nodejs_root`, `skip_global_hooks`,
new describe/delete hooks, PHP 8.6, MySQL 9.7 and the Maho/MODX project types.
Global hooks, `override_config` and `ssh_agent_upstream` were also missing.
Both unmodified schemas now come from the current stable v1.25.5 release, with
updated checksums and license provenance. Empty string enum suggestions now have
a visible `""` label instead of a blank completion row.

The review also corrected an overly strict service-name filter. The official
[Compose schema](https://github.com/compose-spec/compose-spec/blob/main/schema/compose-spec.json)
allows leading `.`, `_` and `-` in service identifiers. Such reported services
are now retained. Shell/log commands bind the name as `--service=<name>` so even
a leading dash remains one unambiguous option value. Invalid names outside the
Compose character set remain rejected; project-name validation stays separate.

State reuse was also corrected during the consolidated review. Cached action
lookups now republish the invoking window's contexts, and editor-focus events
reuse the scheduler's fresh snapshot to update command enablement without another
CLI read. Live service selections remain separate per project even without the
optional storage grant; that grant is only needed across idle reclaim. The Wasm
workflow harness verifies the background-refresh/focus context round trip too.

Verification completed:

- **83 native tests**, strict Clippy, format and whitespace checks.
- **105 manifest-condition checks** using the actual host TypeScript evaluator.
- Actual release **Wasm component** loaded in the v0.1.39 runtime under default
  fuel/memory limits; editing providers pass the 50 ms test deadlines, including
  new project/global schema cases.
- A separate fixture-backed **Wasm workflow harness** covers activation, Services
  rendering/detail tabs, selection/shell/log/browser actions, all **27 declared
  commands**, settings save/reset, file events, stopped-state rejection and
  denied-runtime fallbacks. All filesystem writes are confined to a disposable
  project. Processes, network, clipboard and terminal UI are host fixtures, so
  this is an ABI/sandbox/workflow test, not a real Docker/desktop test.
- Release build/package check and development link confirmed.

Environment rechecked: installed OpenIDEA **0.1.38**, installed DDEV **1.25.3**,
Docker daemon unavailable. The plugin requires OpenIDEA **0.1.39+**. Live native
UI, real container commands, secure Database interaction, remote/Windows behavior
and side-by-side original-plugin execution remain unverified. **Full runtime
parity is not claimed.** The source inventory is accounted for; passing these
checks is not a claim of 100% line or branch coverage.

Reproduction commands are in [tests/wasm-smoke](../tests/wasm-smoke/README.md) and
[tests/WHEN-SMOKE.md](../tests/WHEN-SMOKE.md). Earlier incremental implementation
notes are retained in [PARITY-HISTORY.md](PARITY-HISTORY.md).
