# DDEV Integration

An OpenIDEA source port of [DDEV Integration for IntelliJ](https://github.com/ddev/ddev-intellij-plugin),
version **1.2.10+261**, source tag **1.2.10**, revision
`7291f3ea58cf937ae8c6ac7973ed24876915e308`. Original authors: Nico Löber and
the DDEV contributors. BSD-3-Clause; see [LICENSE](LICENSE) and [NOTICE](NOTICE).
Official DDEV logo assets and bundled schemas use Apache-2.0; see
[icons/LICENSE-DDEV](icons/LICENSE-DDEV) and [schema provenance](schemas/README.md).

Requires **OpenIDEA v0.1.40 or newer (API 1.4)**. Runtime features require DDEV
on PATH and a working container runtime. Open the folder containing
`.ddev/config.yaml` as the IDE project, as the original plugin does.

## Services

Open **Services > DDEV**. The project node shows its status; children are the
services reported by `ddev describe --json-output`, plus Mailpit/Mailhog links.

- **Start / Stop**, **Shell** and **Browser** reflect project state; unavailable
  actions are disabled. Actions are checked again when dispatched.
- **Actions…** and the **DDEV status-bar item** open a context-aware project menu
  for Restart, Share, Delete, Power Off, container tools and setup.
- **Shell** uses the selected container (`ddev ssh --service …`); the project
  node and the status menu use `web`. Browser prefers HTTPS; double-click opens
  the node's URL. A service without a URL reports that it has none.
- **Overview** summarizes the project/service. **Inspect** shows DDEV JSON with credential fields redacted; **Configuration** shows the project
  YAML and the toolbar opens it in an editor; **Help** explains setup and recovery.
- **Logs** uses the host's native streaming pane with
  `ddev logs <project-name> --follow --tail=200 --service …`. Timestamps adds `--time`. The project
  node uses `web`; synthetic Mailpit/Mailhog links have no separate shell/log.
  Standalone log tabs have project/service-specific identities and project names
  in their titles; reopening the same stream focuses its existing tab. The explicit
  project argument keeps a restarted stream targeted after a project switch.
- **Database** shows the published local connection details when DDEV reports them.

Configuration and Help remain available when Docker is stopped, DDEV is missing,
or `process:ddev` is denied. Runtime errors appear as an unavailable state rather
than hiding the project. This uses native Services views and detail panes, with
no custom web UI. **DDEV: Guided Setup** opens a native form with a setup checklist,
environment check, project preferences, installation guide and plugin issue link.
Preferences are saved only when you press Save Preferences; unrelated settings
are preserved, including valid inline TOML tables. **Reset Changes** reloads the
current project's saved values without writing or clearing other projects' drafts.
Save applies enabled database maintenance immediately when runtime access is
available, including with periodic watching disabled. The setup form remains useful without a DDEV execution grant.

Every open configured project is initialized at startup. Status refreshes on a
one-minute schedule when enabled, on base/override/Compose
configuration and project preference changes, and when changing projects. File
events are restricted to the project root; nested/other-project configuration
files do not trigger DDEV reads. Removing the parent `.ddev` directory also
invalidates controls. **Refresh** remains available manually. Snapshot and
failure caching avoid repeated DDEV processes on node selection. Selections stay separate per project during the live session. The optional
storage grant preserves them across idle reclaim. Cached state is republished
on editor focus and action lookup without forcing another CLI read. Explicit runtime commands revalidate current CLI state even when
watching is disabled. Removing the configuration invalidates cached controls.
Each invoking window receives its own status text. Long commands use keyed native
terminals with `DDEV_NONINTERACTIVE=true`; finishing or closing one refreshes
status even with periodic watching disabled. Failed exits show a warning; terminal output retention follows the host. Repeating the same live operation focuses
its existing terminal; different projects/arguments use distinct keys.
Configure opens the YAML only after a successful terminal exit, including when
its content is unchanged. File changes alone do not imply command success.
Completion survives idle instance reclaim; the pending Configure follow-up uses
optional storage across reclaim and expires after 30 minutes. Follow-ups match
the launched terminal operation, including when an earlier completion is delayed.
Different Configure arguments wait for the active operation to finish; repeating
the same active operation focuses its terminal. A new host operation gets a fresh
30-minute follow-up deadline; focusing the existing operation does not extend it.

## Commands

All commands appear under **DDEV** in Find Action. Installation, release and issue
links, and revealing Services, also work without an open project. Common commands appear in Tools
and Services context menus. The status menu and command conditions reflect project/CLI/database state.
Delete and Power Off remain available during startup and recoverable description
failures. Like the original, Stop and Restart can recover when description is
unavailable but DDEV is installed and the project is configured; container tools require running state. Valid Compose service identifiers,
including leading punctuation, are supported with bound `--service=<name>` values. Toolbar targets restore the
current project's selection before dispatch.

| Command | Behavior |
|---|---|
| Start / Restart / Stop | `ddev start` / `restart` / `stop` in a terminal |
| Power Off All Projects | Confirms, then `ddev poweroff`; affects all DDEV projects |
| Delete Project | Confirms, then `ddev delete` with `DDEV_NONINTERACTIVE=true` |
| Share Project | `ddev share`; requires DDEV's sharing configuration |
| Configure Project | Confirms, then `ddev config --auto`; optionally passes `--php-version` |
| Open Configuration | Opens `.ddev/config.yaml` in an editor |
| Open Shell | `ddev ssh --service …`; context menu targets its node |
| Open Service in Browser | Picks from reported URLs; context menu targets its node |
| Open Logs | Opens a native log editor tab for the selected container (default `web`) |
| Run Composer / PHP / Node.js | Prompts for arguments, then runs `ddev composer …`, `ddev exec --raw -- php …` or `ddev exec --raw -- node …` |
| Configure Database Data Source | Creates/updates a plugin-owned project connection; see below |
| Copy Database Password | Copies DDEV's database password on explicit request |
| Project Actions | Opens the status/action picker |
| Help / Guided Setup | Opens the native setup checklist and preferences form |
| Check Environment | Shows selected DDEV/Docker version fields and failure guidance in Setup |
| Show Services | Reveals the DDEV Services provider |
| Open Installation Guide / Release Notes / Plugin Issue Form | Opens the corresponding page; never submits an issue |
| Disable Automatic Update Checks for Project | Persists the project preference with `project.write`; explicit checks remain available |
| Refresh Status | Reloads status and maintains the data source when enabled |
| Check for Updates | Compares `ddev --version` with GitHub's latest stable release; no installation |

Arguments are passed separately, without building a shell command string.
The tool-argument prompt accepts quoted arguments and cancellation. PHP/Node
explicitly request raw execution, preserving literal dollar signs, backticks and
empty arguments instead of asking DDEV to reconstruct a container shell command. PHP/Node/Composer
commands require a running project. The separate DDEV execution target integrates with the IDE's Run/Debug and interpreter settings; see below.

Configure Project chooses PHP from the plugin's explicit **PHP version** setting,
then `[languages.php].languageLevel` in `.openidea/settings.toml`, then an exact
`config.platform.php` version in `composer.json`. Composer patch versions are
reduced to major.minor; version constraints are not guessed. Otherwise DDEV chooses.
The plugin cannot read an effective global IDE language level.

## DDEV configuration assistance

Project and global DDEV YAML use the complete official **v1.25.5** schemas,
bundled for offline use. Completion covers nested properties (database, hooks,
daemons and exposed ports), enum/boolean values and engine-specific database
versions. Key/value hover and completion show schema documentation. Diagnostics
report YAML syntax, duplicate keys, unknown properties and invalid schema values.
Numeric version strings are inserted with quotes; existing colons are preserved.
Completion also works immediately after a colon, adding the required space.

Windows drive and UNC paths match configuration filenames without ASCII case
sensitivity; Unix paths remain case-sensitive. This is checked in native logic
and actual-Wasm providers, not a live Windows desktop test.

Assistance is scoped to `.ddev/config.yaml`, `config.*.yaml`,
`global_config.yaml` and `global_config.*.yaml`. No DDEV process/network grant is
needed. Schemas are pinned, so future fields may trigger version-labelled warnings.
Files are checked individually (not merged with overrides); single-document YAML
up to 256 KiB is supported, with bounded depth/alias expansion. Completion recovers
the edited token and basic unclosed flow collections; unrelated syntax errors can
prevent suggestions. General YAML highlighting/formatting remains the host's.
See [schema provenance](schemas/README.md) and the [source audit](SOURCE-AUDIT.md).

## Database setup

With **Maintain a DDEV database data source** enabled and `database.sources`
granted, startup and refresh maintain a plugin-owned native Database connection.
DDEV's password goes directly to the secure Database credential store and is
never included in connection settings or project files. MySQL, MariaDB and
PostgreSQL use the published port on `127.0.0.1` (the project server for remote
projects). The explicit command requests normal Database connection/introspection;
automatic setup also requests it at the first interactive refresh/focus of the
session. Windowless startup maintains the source without attempting a window-only
connection request. `project.write` is not required for native Database setup.

Without `database.sources`, the previous non-secret connection-file workflow
remains available with `project.write`: it preserves unrelated entries/comments,
rejects duplicate IDs and requires manual password entry/connection. After native
setup succeeds, a uniquely identified, explicitly marked legacy entry is removed
when project writes are permitted. Unrelated or ambiguous entries remain intact.
Copy Database Password is an optional explicit convenience. Existing uniquely identified managed sources are
reused after a project move, including the marked legacy file entry. Ambiguous
managed identities are left unchanged. Native setup repopulates the credential
through the secure API for the moved project; no password is migrated through
project files.

Automatic native maintenance compares the desired connection and credential in
memory, checks that the owned source still exists, and skips identical upserts
and credential writes. The host disconnects on every upsert, so this keeps a
manual/live connection intact during normal status polling. Changed ports or
credentials clear the connection marker and allow a later interactive reconnect;
a removed source is recreated. A partial setup failure clears the cache so a
subsequent refresh retries. The comparison is never logged or persisted. Explicit
setup can force reapplication of user-edited owned settings.

## PHP, Node.js, Composer and Run/Debug

The plugin registers a project-owned **DDEV: <project>** run target. The host asks
whether to use it; registration never changes the user's selected target. Choose
it in Settings > Run Targets, PHP/Node interpreter settings or a run
configuration's **Run on** selector. Existing PHP/Node runs, Composer/npm scripts
and PHPUnit/Node tests can then use it through the host's usual execution flow.

The target uses approved `ddev exec --raw --dir=<mapped-cwd> -- <tool> <args>`.
Arguments stay separate; project paths and working directories map under
`/var/www/html`, configurable as **Project path inside the web container**.
This generic target invokes the container's Composer binary; the explicit
Composer command retains `ddev composer`. The generic target does not apply
DDEV's `composer_root`, pre/post-composer hooks or Composer-specific environment/
sync behavior. Shared target cwd arguments prevent a correct `ddev composer`
override while preserving PHP/Node cwd mapping ([#2407](https://github.com/l0ng-ai/openidea/issues/2407)).
Unlike the guarded explicit tools, a built-in run through the static target has
no plugin pre-launch state check; DDEV may start a stopped project
([#2408](https://github.com/l0ng-ai/openidea/issues/2408)). Environment forwarding into the
container follows DDEV's own conventions. Choosing a custom executable in
Settings > Plugins > DDEV Integration > `process:ddev` applies to commands,
terminals, logs and targets; the host approves and pins it globally/per project.

**Offer DDEV for project runs** controls registration. Invalid/changed mapping preferences, disabled registration, removal of
`.ddev/config.yaml` or a failed runtime refresh withdraw the target. Successful
recovery registers it again. Unchanged registrations are cached to
avoid restarting target language servers on every status read. The host can also
run a configured language server in the target when it is installed in the
container; this remains a separate explicit host selection. PHP connect-back
Xdebug mapping and supported debugger placement use the host's target machinery;
Xdebug must be enabled/configured in DDEV. Live checks now cover PHP/Node target
execution and Xdebug DBGp connect-back/source URI; full IDE debugger and language
server sessions remain unverified.

## Settings and permissions

Settings > Plugins > DDEV Integration provides:

- Automatic status refresh (default on; every minute).
- Automatic update checks (default on; at most once per day, including failed
  attempts, with network and storage granted). Current versions stay quiet;
  newer versions notify with release-note, installation-guide and disable-check
  actions. The explicit command always reports its result.
- Automatic native data source maintenance (default on; `database.sources`).
- Offer a project execution target (default on) and its container path.
- Explicit PHP version for project configuration (empty by default).

A successful first project detection offers a one-time setup/Services tour when
storage is granted. Runtime failures offer Retry, Setup and Installation Guide
actions; failures stay quiet per project even when the error wording changes.
A successful status read, including background recovery, allows the next failure
to notify again. Project-specific notification actions
reject clicks from another project's window. Windowless notifications can still
appear in every window; their bodies identify the originating project.

| Capability | Use |
|---|---|
| `project.read` (required) | Project configuration, connection file and PHP-version hints |
| `process:ddev` (optional) | Status/version reads, terminal commands and log streams |
| `events` (optional) | Configuration filesystem/save events and project-focus refresh |
| `storage` (optional) | Saved selection and daily update-check throttle |
| `net:api.github.com` (optional) | Automatic/explicit version checks |
| `project.write` (optional) | Save project preferences; non-secret legacy source fallback/migration |
| `database.sources` (optional) | Owned native sources, secure credentials and connection/introspection |
| `clipboard` (optional) | Explicit Copy Database Password |

Denied optional grants leave independent features usable. DDEV itself uses its
normal configuration, hooks, network and credentials.

## Source audit and remaining differences

The [source-by-source audit](SOURCE-AUDIT.md) includes all six plugin descriptors and the original action, state,
execution, interpreter, database, Composer, terminal and version-check code.

| Original source | Port status |
|---|---|
| `DdevRunnerImpl` lifecycle commands, `ddev ssh`, HTTPS-first service links | Implemented through native terminals and Services |
| `StateWatcherImpl` and action `isActive` checks | Automatic refresh, context keys and runtime guards implemented; **60 seconds rather than 10**, due to the host's minimum schedule |
| `DdevStatusBarWidgetImpl`, version settings | Native status/action picker and automatic/explicit checks implemented |
| `PhpVersionArgumentProvider` | Explicit/project-local/composer PHP hints implemented; global IDE PHP level unavailable |
| PHP/Node runs, Composer, path mappings | API 1.4 project target integrated with host selection; raw argv and path mapping verified. Host-local version probes/global PHP level and JetBrains SDK metadata remain different |
| Automatic data source creation and credentials/introspection | API 1.4 owned native source, secure credential and connection request; file fallback when native grant is denied |
| Post-command refresh/open configuration | API 1.4 terminal-exit callbacks, including reclaim and unchanged configuration |
| Custom DDEV executable, terminal environment and WSL patching | Host-owned per-project/global executable chooser and native command environment; original local UNC patch not reproduced |
| Got-It tutorial and actionable recovery notifications | Native guided setup, one-time tour, environment checks and recovery/update actions implemented; the original overlay presentation is not reproduced |
| Error reporting | Opens this port's issue form on explicit request; no automatic Sentry reporting |
| Inherited YAML schema assistance (original Features wiki) | Complete bundled DDEV project/global schema completion, nested values, hover and validation implemented |

The original also reads only project-root `.ddev/config.yaml`; nested discovery
is not a confirmed parity gap. Services log/inspect tabs are additional port features.

The six original API requests for run targets, Database, terminal completion,
terminal environment, executable choice and window status are implemented in the
API 1.4 host and adopted here. Project-targeted background notifications
[#2157](https://github.com/l0ng-ai/openidea/issues/2157) are implemented upstream
in **API 1.5**, outside this port's 1.4 target. Its origin guards remain.
See [LIMITATIONS.md](../../LIMITATIONS.md#ddev-integration-arittiddev) and the
[source audit](SOURCE-AUDIT.md) for remaining platform and verification differences.

## Development and verification

```sh
cargo test
cargo clippy --all-targets -- -D warnings
openidea-plugin build
openidea-plugin check
openidea-plugin link
```

Use the **v0.1.40 or newer** plugin CLI. The SDK and workflow-host dependencies
are pinned to tag v0.1.40 (`37c2a6048eea89d4f6db645622612358b643862b`).
107 native tests (plus one opt-in live test) cover the original fixtures, configuration assistance, commands,
preferences, project isolation and API 1.4 integration regressions. Strict Clippy,
release Wasm build and package checks pass. The [Wasm probes](tests/wasm-smoke/README.md)
exercise editing providers with 50 ms deadlines and all 27 commands, native
Database callback/credential separation, keyed terminal completion, window
status, configuration removal and actual host target argv/path construction.
The [manifest-condition harness](tests/WHEN-SMOKE.md) checks 105 conditions with
the host evaluator. Native Services/Setup were visually checked in OpenIDEA
0.1.40 against a disposable stopped project, including the actual DDEV root icon.
A real native Configure command also updated the disposable project and opened
its YAML after terminal completion. The [optional live smoke runner](tests/LIVE-SMOKE.md)
now verifies real PHP/Node execution and mapped paths/argv, explicit Composer
root/hooks, MariaDB SQL and Xdebug DBGp connect-back/source URI. Native Database
connection/credential UI, full IDE debugging and remote/Windows behavior remain
unverified.
