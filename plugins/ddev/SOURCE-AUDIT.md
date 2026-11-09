# DDEV source parity audit — 2026-11-08

Compared against upstream **1.2.10**, commit
[`7291f3ea58cf937ae8c6ac7973ed24876915e308`](https://github.com/ddev/ddev-intellij-plugin/tree/7291f3ea58cf937ae8c6ac7973ed24876915e308),
Stable source baseline. Also rechecked main `1c5443127c38e392c3c4fc859f381dc758692160`
(1.2.11-beta1 development): the production changes since the baseline are
testability/annotation changes, with no new feature/UI family. Marketplace build:
**1.2.10+261**. This is a source/API audit, with port regression tests; it is not
a side-by-side live IDE test.

The audit inventories 126 Java files and 12 resources: the main descriptor and
all five optional integration descriptors, actions and their enablement predicates,
state/listener flow, execution, settings, interpreters, database, terminal,
notifications, tutorials, icons and version handling. Original test cases and
JSON fixtures were compared where they describe host-independent behavior.

Paths below are relative to upstream
[`src/main/java/de/php_perfect/intellij/ddev/`](https://github.com/ddev/ddev-intellij-plugin/tree/7291f3ea58cf937ae8c6ac7973ed24876915e308/src/main/java/de/php_perfect/intellij/ddev).
The port is in `src/` beside this file.

| Original implementation | Port implementation and evidence | Assessment |
|---|---|---|
| `actions/Ddev{Start,Restart,Stop,PowerOff,Delete,Share,Config}Action.java`, `cmd/DdevRunnerImpl.java` | `lib.rs`, `runtime.rs`; terminal commands, confirmation, configuration/PHP arguments; command/state regression tests | Lifecycle actions implemented. Configure also permits confirmed reconfiguration; the original enables it only before configuration |
| `Ddev{Start,Restart,Stop}Action.isActive` permits absent description if installed/configured | `Main::can_start`, Restart/Stop command guards and recovery context; installed CLI fallback | Failed `describe` permits Start/Restart/Stop with installed CLI and root configuration; known stopped/starting states retain the original stricter predicates |
| `cmd/RunnerImpl.java`, `DdevRunnerImpl` completion callbacks | `execution.rs`, API 1.4 keyed terminals and completion events | Implemented: exit-triggered refresh even without watching, successful unchanged config opens, failure/close clears pending follow-up; reclaim/project-isolation tests |
| `DdevRunnerImpl.createCommandLine` sets `DDEV_NONINTERACTIVE=true` | API 1.4 `Command` in native terminal sessions and read processes | `DDEV_NONINTERACTIVE=true` implemented; append-only environment cannot replace a pre-existing variable |
| `cmd/BinaryLocatorImpl`, `state/DdevStateManagerImpl`, `settings/DdevSettingsState.ddevBinary` | Approved `process:ddev` with API 1.4 host executable choice | Native global/project chooser, path/hash pinning; all port execution paths retain the declared alias |
| `cmd/wsl/WslAware.java`, `WhichProvider`, `DockerImpl` | Host local/remote routing; DDEV version/environment diagnostics | Original local Windows UNC/WSL patch not reproduced; remote log tabs currently unsupported by the host |
| `cmd/parser/JsonParserImpl`, `cmd/Description`, `Service`, `DatabaseInfo` | `model.rs`, all four original describe/version JSON fixtures | JSON envelopes/debug lines, project status, service URLs and DB fields covered. Port tolerates non-JSON preambles and restricts browser links to HTTP(S) |
| `cmd/DdevErrorOutput` and its tests | `model::error_message`, `run_json` | Fixed last-error precedence and suppression of informational JSON; original fatal/plain/empty/info cases covered |
| `state/DdevConfigLoaderImpl` | `CONFIG` and project read APIs | Root `.ddev/config.yaml` matches original; nested project discovery is not an original feature |
| `InitPluginActivity`, `state/*Listener`, `StateWatcherImpl` | Activation/events/scheduler, snapshots, runtime guards | Implemented with host minimum 60-second cadence, compared with original 10 seconds. File/manual refresh remains available; windowless work avoids publishing a project's context into unrelated windows |
| `service_actions/ServiceActionManagerImpl`, `actions/OpenServiceAction` | `model.rs`, `services.rs`; native Services and browser picker | HTTPS-first URLs, web and Mailpit/Mailhog, service-specific links covered. Logs/Inspect/Configuration panes are port additions |
| `status_bar/DdevStatusBarWidgetImpl`, `ServicesActionGroup` | Status item and state-filtered action picker | API 1.4 window status and direct service URLs in the picker implemented; exact popup layout and per-window visibility remain different |
| `version/Version.java`, `version/util/VersionCompare`, `VersionCheckerImpl`, `GithubClient`, listeners/actions | `version.rs`, `check_updates`, `runtime::automatic_updates`; upstream comparison cases | Fixed numeric comparison including HEAD/RC suffixes and missing zero components. Automatic checks throttled daily, errors quiet; explicit checks still work when automatic checks are disabled (deliberate port behavior) |
| `settings/DdevSettings{State,Component,Configurable}` and `ChangeSettingsAction` | Plugin settings plus native Setup form, reviewed project preference save | Watch/updates/database switches, a unified execution-target switch/container path and PHP-version override implemented; PHP/Node selections remain independently configurable in native host settings |
| `docker_compose/*`, `php/ConfigurationProviderImpl`, `PhpInterpreterProviderImpl` | `execution.rs` generic DDEV run target, raw argv, mapped project/cwd; explicit PHP command and config hints | Host-selected runs/tests and mapping; no per-launch plugin validation (#2408), JetBrains SDK/Compose metadata or automatic local version-probe replacement |
| `php/PhpVersionArgumentProvider` | `config_php_version`: explicit override, local IDE PHP level, exact Composer platform version | Partial: effective global IDE PHP language level cannot be queried. No constraint guessing |
| `php/composer/*` | Explicit `ddev composer` command with parsed arguments | Selected host run target also wraps built-in Composer runs; generic target uses container Composer and misses DDEV Composer hooks/root/env/sync (#2407), while the explicit command uses ddev composer. Target lacks the original running-state preflight (#2408) |
| `php/server/{ConfigureServerListener,ServerConfigManagerImpl}` | API 1.4 target container root (`/var/www/html` default) | Host debugger mapping inspected in source; live DBGp connect-back/source URI passed; full IDE debugger unverified; no JetBrains server object |
| `node/{AutoConfigureNodeInterpreterListener,NodeInterpreterProviderImpl}` | Explicit Node command plus selected API 1.4 target | Host Node/test/script runs can use DDEV; no JetBrains helper/SDK object, tool discovery remains host-local |
| `database/{AutoConfigureDataSourceListener,DdevDataSourceManagerImpl,DataSourceProviderImpl}`, `index/ManagedConfigurationIndexImpl` | `database.rs`: stable project-owned connection ID, update/no-op detection, unrelated TOML preservation; MySQL/MariaDB/Postgres tests | Native API 1.4 source upsert, secure credential, loaded Database update and interactive connection request implemented; legacy file fallback retained. No JetBrains IDs/JDBC driver metadata migration |
| `terminal/Ddev{PredefinedTerminalActionProvider,TerminalRunner}` | Open Shell command and container-targeted Services actions | Native terminal available; no DDEV entry in the host terminal-profile menu, nor plugin-owned PTY lifecycle |
| `cmd/DdevShellCommandHandlerImpl` | Actions/terminal launch plus normal status watching | Does not intercept manually typed terminal commands; no corresponding host extension point |
| `notification/DdevNotifierImpl`, `state/UnknownStateListener` | `support.rs`, recovery/update/install actions, deduplication until recovery | Native recovery/update actions and per-project deduplication implemented; background project targeting is API 1.5 (#2157), outside the target; the original secondary 30-second unknown-state warning is not reproduced |
| `tutorial/GotItTutorialImpl`, `terminal/TutorialListener` | Native guided Setup/checklist/environment diagnostics, one-time tour | Workflow adapted to native forms; exact anchored Got-It overlays not reproduced |
| `ReportIssueAction`, `InstallationInstructionsAction`, `ReloadPluginAction`, `SyncStateAction` | Port issue form/install guide, Refresh, Setup | Implemented equivalents. Issue link targets this port; no automatic submission |
| `ManagePluginsAction`, `util/{FeatureRequiredPlugins,PluginChecker,PluginDisplayNameMapper}` | Setup describes permissions and unavailable integrations | No JetBrains dependency-manager dialog; those platform dependencies do not apply to OpenIDEA. Host plugin/settings screens remain accessible normally |
| `error_reporting/*` | Explicit issue form and host error logs | Automatic Sentry collection intentionally omitted |
| `DdevConfigArgumentProvider` extension point | Built-in PHP argument handling | No public third-party argument-provider extension API in this port |
| `icons/DdevIntegrationIcons`, packaged icons | Official DDEV mark, package tile and transparent Services icon, attribution/licenses | Technology identity retained; uses the repository icon convention |

## Inherited DDEV configuration assistance

No plugin-owned YAML completion, hover, inspection or schema registration appears
in the six original descriptors/source. However, the original
[Features wiki](https://github.com/ddev/ddev-intellij-plugin/wiki/Features)
explicitly describes IntelliJ automatically detecting DDEV's JSON schema and
providing smart completion. Therefore absence of a provider in Java is **not**
evidence that the original user experience lacks configuration assistance.

The port now supplies this experience itself in `src/configuration/`:

- Complete, unmodified DDEV **v1.25.5** project and global schemas, bundled offline;
  [provenance and checksums](schemas/README.md).
- Top-level and nested property completion, hook/daemon/exposed-port array items,
  enum/boolean/default values and conditional database versions.
- Key/value hover documentation from schema descriptions and types, and
  completion documentation.
- YAML syntax/duplicate-key diagnostics and complete Draft 7 schema validation,
  including nested constraints and unknown properties.
- Correct quoting for numeric strings, colon-preserving edits, Unicode/UTF-16
  ranges, YAML anchors/merge keys, block values and flow collections; basic
  unclosed-flow recovery while typing.
- `.ddev/config.yaml`, `config.*.yaml`, `global_config.yaml` and
  `global_config.*.yaml`; other YAML files are ignored. No DDEV process or network
  grant is needed for editing assistance.

Limits: single-document YAML, 256 KiB, depth/expanded-alias bounds, at most 100
schema diagnostics. Suggestions require surrounding parseable YAML apart from
the edited token and recoverable unclosed flow collections. No filesystem path
completion or general YAML language server replacement is claimed. Schemas are
pinned; future DDEV fields can cause version-labelled warnings until updated.
Override files are validated individually, without merging the effective DDEV
project configuration. Generic YAML highlighting/formatting remains the host's.

## API 1.4 parity update — 2026-11-09

Pinned host/SDK: **v0.1.40**, `37c2a6048eea89d4f6db645622612358b643862b`.
Current upstream HEAD `b4e2d4493bf5299954cd4883b8b302499e39ce4b` was also inspected:
it introduces **API 1.5** project-targeted notifications. The seven requests filed before this recheck
are closed upstream; closure does not mean every one shipped in API 1.4.

| Area | Current implementation / qualification |
|---|---|
| Terminal completion (#2135) | Keyed native sessions, operation exit events across reclaim; immediate state refresh, success-only Configure opening even when unchanged, failure warning. Project/argument-scoped keys |
| Terminal environment (#2139) | `DDEV_NONINTERACTIVE=true` in native sessions and finite CLI reads. Host environment is append-only: an inherited same-name variable causes a launch error |
| Executable choice (#2140) | Host Settings > Plugins chooser with global/project scope and normal approval/pinning; no arbitrary wrapper scripts |
| Execution profiles (#2133) | A cached DDEV target offers raw argv and configurable project/cwd path mapping; host-selected PHP/Node/Composer runs/tests. Native host owns first-use prompt, per-language selections, LSP target and supported debugging placement |
| Database (#2134) | Native owned upsert and secure credential. Explicit setup and the first interactive automatic refresh/focus request normal connection/introspection; windowless startup only maintains settings/credential. Non-secret file fallback and guarded legacy migration |
| Status (#2146) | Actual status text in the invoking window; no project-specific global fallback. Per-window visibility is still not exposed |
| Background notifications (#2157) | Implemented by upstream in API 1.5, not imported by this API 1.4 component. Existing root-labelled messages and action-origin guards remain |
| UI | Native Services with Overview/Inspect/Logs/Database/Configuration/Help; direct logs and service links; preferences above help text in Setup. Root inherits the provider SVG (raw package paths are not resolved for row icons) |

All original feature families remain inventoried above. Remaining differences:
10-second polling versus host minimum 60 seconds; terminal profile-menu/preset
and typed-command interception; exact JetBrains SDK/Compose/server metadata and
global PHP-level access; local UNC/WSL patch; remote log tabs; exact Got-It
presentation, Sentry and public argument-provider extension. These are explicit
platform adaptations/limits, not missing Database/terminal/run-target work.

Verification:

- **107 native tests**, strict Clippy, formatting and release Wasm/package check.
- **105 conditions** through the actual host evaluator.
- Actual API 1.4 Wasm editing providers under default sandbox limits and 50 ms deadlines.
- All **27 commands** and Services/setup/detail callbacks through the actual Wasm
  component with a fixture host. Secure credentials travel only through the
  credential callback; target wrapping/path mapping also uses the real host code.
  Includes terminal exit, window status and withdrawal on configuration removal.
- Native desktop check in installed **OpenIDEA 0.1.40**, using a disposable stopped
  DDEV project: Services, Overview, Setup, disabled controls, per-window stopped
  label and real root logo inspected. DDEV **1.25.3**, Docker daemon reachable. A real native Configure command also
updated the disposable project and opened its YAML after terminal completion.

The [live smoke checks](tests/LIVE-SMOKE.md) now cover real PHP/Node execution,
Composer root/hooks, MariaDB SQL and Xdebug DBGp transport/source URI. A native
secure Database connection, full IDE debugging, remote/Windows and side-by-side
original-IDE execution remain unverified. Full runtime
parity or 100% line/branch coverage is not claimed. Older notes are archived in
[PARITY-HISTORY.md](docs/PARITY-HISTORY.md). Reproduction:
[Wasm probes](tests/wasm-smoke/README.md), [conditions](tests/WHEN-SMOKE.md).

## API 1.4 regression recheck — 2026-11-09

Rechecked original main `1c54431` and OpenIDEA HEAD `84cfac9` (API 1.5);
the port remains pinned to v0.1.40/API 1.4. Concrete corrections:

- **Native Database idempotence:** the pinned host's
  [`database_upsert`](https://github.com/l0ng-ai/openidea/blob/37c2a6048eea89d4f6db645622612358b643862b/crates/ide-host/src/plugins/api.rs)
  disconnects a source on every call, even with identical settings. Automatic
  maintenance now compares exact desired settings/credential in memory, verifies
  source ownership/existence, and skips unchanged writes. Rotation, port changes,
  deletion and failed partial credential setup invalidate the relevant cache and
  permit recovery/reconnection. Explicit setup still forces reapplication.
- **Target preference cleanup:** external preference edits withdraw a disabled,
  changed or invalid target even when DDEV fails to describe the project. Failed
  background reads withdraw stale targets; recovery registers again.
- **Action parity:** the status picker omits Database setup without reported DB
  information, matching the manifest's availability condition. Expected reload
  cancellations no longer produce execution-target warnings.

The original Composer source and DDEV's own implementation reveal two remaining
static-target limits. The generic target does not use DDEV's Composer root/hooks/
environment/sync semantics, and cannot run the original provider's running-state
validation before a built-in launch. These are now filed as
[#2407](https://github.com/l0ng-ai/openidea/issues/2407) (per-command cwd arguments)
and [#2408](https://github.com/l0ng-ai/openidea/issues/2408) (pre-launch validation).
Explicit Composer/PHP/Node commands remain state-guarded; Composer uses
`ddev composer`. Generic target runs may auto-start DDEV, unlike the original
Composer validation. This is partial parity, not a fully equivalent interpreter
provider.

Verification: **106 native tests**, 105 conditions, strict Clippy, Wasm build/check
and editor probes. The actual-Wasm fixture host now models the native disconnect
on upsert and verifies repeated/manual/background refresh preserves an unchanged
connection without rewriting settings/credentials. It also injects a credential
store failure after upsert and verifies the next refresh retries/reconnects
without switching to the file fallback. Live-container/debugger/remote
qualifications above remain unchanged.

## Three complete inventory passes — 2026-11-09

Original main remains `1c54431` (126 Java files, all six XML descriptors);
current host HEAD `bdb8ca55` was checked in addition to the pinned API 1.4 SDK.
The feature/action/state/settings/terminal/interpreter/database/version/tutorial/
icon/configuration families above were cross-checked across the three passes.

| Pass | Cross-check and findings | Applied correction / evidence |
|---|---|---|
| 1 — original source and lifecycle | Compared action predicates and managed-configuration identity with the original Java/index code | Stop/Restart now recover from unavailable description with an installed CLI/configuration, matching the original. Native and marked legacy source IDs are reused after a project move; ambiguous candidates are refused |
| 2 — execution, state and integrations | Replayed command/view/events, denied runtime, source maintenance, project switches and reclaim | Configure follow-up matches both session key and host operation ID. Changed arguments cannot overwrite a known active request; late exits cannot consume a newer identical run. Native metadata reuse reconnects through its existing private ID |
| 3 — configuration, UI and permissions | Checked manifest conditions, native view/detail payloads, schema completion/hover/diagnostics, Windows/UNC vs Unix scoping and independent grants | Stop toolbar/menu availability matches recovery predicates. Windows/UNC filename case is handled in editor assistance. Provider-logo inheritance and independent configuration/help remain intact |

**106 native tests** pass, including movement/ambiguity/
operation-identity/path regressions; the earlier recovery-action test now includes
Stop/Restart. **105 manifest-condition checks** use the actual v0.1.40 evaluator.
Compiled-Wasm workflows still exercise all **27 commands**, now including
unavailable-description Stop/Restart, moved native metadata reuse and an old
completion arriving after a new run with the same session key. Wasm editor
probes include Windows drive/UNC paths and retain the 50 ms deadlines. Strict
Clippy, formatting and the release package check pass.

The previous native desktop Services/Setup/logo and finite Configure evidence
remains valid for that inspected run; these additional paths were verified in
native tests and the actual component/fixture host. No additional live Windows,
container, secure Database, debugger or original-IDE session is claimed.
Static target Composer and pre-launch limits **#2407/#2408** remain open (rechecked);
project-targeted notifications require API 1.5. The platform/adaptation limits
listed above remain explicit.

## Additional operation-deadline pass — 2026-11-09

Rechecked original main (`1c54431`) and current host HEAD (`bdb8ca55`, API 1.5)
against the pinned API 1.4 implementation. The static run-target interface still
lacks per-command workdir arguments and pre-launch validation; #2407/#2408 remain
open. No SDK upgrade was made.

Configure now starts a fresh 30-minute follow-up deadline only when the host
returns a new operation ID. Reusing the same live terminal preserves its original
deadline, including after idle reclaim and expiry. Retaining same-key metadata
until the returned operation ID is compared prevents an expired operation from
silently renewing its deadline. Four native cases cover live/replaced operations
before/after expiry. All 106 native tests, 105 manifest conditions, strict Clippy,
release build/package check and compiled-Wasm command/editor probes pass. No new
live desktop or container validation is claimed by this pass.

## Confirmation pass after Services scope review — 2026-11-09

Fetched original main again: still `1c5443127c38e392c3c4fc859f381dc758692160`.
Rechecked the production diff against stable 1.2.10 and compared action dispatch/
availability, project settings, dynamic service URL actions, database listeners,
watcher cadence and version handling with the port. Reviewed the newer upstream
action/service-group tests as behavioral evidence; they were read, not executed
in an IntelliJ instance. No additional implementation gap was identified in
these paths beyond the adaptations already recorded above.

Services intentionally remains scoped to the invoking OpenIDEA project. Each
project retains separate selections, terminal keys and Database state; there is
no global `ddev list` dashboard. Multi-project aggregation was explicitly left
out of this pass at the user's request. The original service actions are also
project-scoped; a global dashboard is not required to reproduce them.

Fresh verification passed: 106 native tests, strict Clippy, formatting, release
Wasm/package check, all 27 command workflows and 105 manifest conditions through
the pinned host evaluator. Compiled-Wasm completion, hover and diagnostics probes
remain within their 50 ms budgets, including nested/global schema assistance and
bounded duplicate diagnostics. Only this audit record changed; no implementation
change or new live desktop/container validation was needed for this pass.

## Fresh source inventory and behavioral review — 2026-11-09

Rebuilt the source inventory without accepting the earlier assessment as proof:
126 main Java files, two additional platform terminal adapters, 12 resources
including all six descriptors. Original stable remains `7291f3e`; a fresh fetch
confirms main `1c54431`. See the [new review matrix](docs/FROM-ZERO-RECHECK.md)
for the reviewed families, source counts and remaining qualifications.

Found and fixed a notification mismatch: upstream's `reportFailure` uses a
per-command latch until success, while the port used the exact error fingerprint.
Changing failure text could therefore emit another alert during the same outage.
The port now latches the project's runtime failure until a successful status read,
including across idle reclaim when storage is granted. Older fingerprint markers
are recognized. Explicit failed commands still return their current error; this
only suppresses repeated automatic recovery notifications.

The regression failed before the correction (three alerts instead of one) and now
passes with/without storage, after reclaim, with old storage markers and after
recovery/re-failure. Fresh verification: **107 native tests**, strict Clippy,
formatting, Wasm/package check, all **27 compiled-Wasm command workflows**, editor
probes below 50 ms and **105 host-evaluator conditions**. Bundled schema checksums
also match their provenance. No new desktop, live container, Database connection,
debugger, remote or Windows execution is claimed. Services scope is unchanged.

## Live execution and debugger transport — 2026-11-09

The remaining source/API limitations were rechecked: #2407/#2408 are still open,
and current host main `b4e2d449` retains the static run-target interface. Latest
published numeric tag is still v0.1.40; API 1.5 notification support is on main.
The port keeps its existing API 1.4 compatibility.

Added a reproducible [live smoke runner](tests/LIVE-SMOKE.md) and an ignored native
integration test. Real DDEV 1.25.3 containers passed literal/empty PHP and Node
argv, explicit Composer root and hook checks, actual describe/Services-model/
Database-metadata parsing, and MariaDB SQL. The target registered by compiled
Wasm was also executed through the real host wrapper: file paths and a working
directory containing a space mapped correctly. A real Xdebug DBGp connection
reported the expected source URI and completed a `run` exchange.

The full runner passed, including deletion of its own project; cleanup disables
DDEV's unrelated-container cleanup. The normal suite passes 107 tests with one
live test ignored; the live test was run separately and passed. These checks
reduce runtime uncertainty, not the known generic Composer or target preflight
feature gaps. Secure native Database integration and IDE debugger workflows still
use fixtures; Windows/remote and original-IDE comparison remain unverified.
