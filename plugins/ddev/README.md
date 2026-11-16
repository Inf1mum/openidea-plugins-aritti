# DDEV Integration

DDEV project controls, service links and environment diagnostics for OpenIDEA,
built on native Services views and action groups.
MIT; see [LICENSE](LICENSE).

Requires **OpenIDEA v0.1.40 or newer (API 1.4)**. Runtime features require DDEV
on PATH and a working container runtime. Open the folder containing
`.ddev/config.yaml` as the IDE project, as DDEV expects.

## Services

Open **Services > DDEV**. The project node shows its status; children are the
services reported by `ddev describe --json-output`, plus Mailpit/Mailhog links.

- **Start / Restart / Stop**, **Open Shell** and **Open in Browser** reflect project state; unavailable actions are disabled. Actions are checked again when dispatched.
- **More Actions…** and the **DDEV status-bar item** open a context-aware project menu for Restart, Share, Delete, Power Off, container tools and setup.
- **Shell** uses the selected container (`ddev ssh --service …`); the project node and the status menu use `web`. Browser prefers HTTPS; double-click opens the node's URL. A service without a URL reports that it has none.
- **Overview** summarizes the project/service. **Inspect** shows DDEV JSON with credential fields redacted; **Configuration** shows the project YAML and the toolbar opens it in an editor; **Help** explains setup and recovery.
- The compact toolbar uses registered native IDE icons for all actions, with descriptive accessible labels. Project-free, unconfigured and unavailable states stay visible as read-only Services status rows; selecting them does not retry DDEV commands — **Refresh Status** is the explicit retry action. Recovery controls keep their state/permission checks.
- **Logs** uses the host's native streaming pane with `ddev logs <project-name> --follow --tail=200 --service …`; Timestamps adds `--time`. The project node uses `web`; synthetic Mailpit/Mailhog links have no separate shell/log. Standalone log tabs have project/service-specific identities and project names in their titles; reopening the same stream focuses its existing tab. The explicit project argument keeps a restarted stream targeted after a project switch.
- **Database** shows the published local connection details when DDEV reports them.

Configuration and Help remain available when Docker is stopped, DDEV is missing,
or `process:ddev` is denied. Runtime errors appear as an unavailable state rather
than hiding the project. **DDEV: Guided Setup** opens a native form with a setup
checklist, environment check, project preferences, installation guide and plugin
issue link. Preferences are saved only when you press Save Preferences; unrelated
settings are preserved, including valid inline TOML tables. **Reset Changes**
reloads the current project's saved values without writing or clearing other
projects' drafts. Save applies enabled database maintenance immediately when
runtime access is available, including with periodic watching disabled. The setup
form remains useful without a DDEV execution grant.

### Status watching

Every open configured project is initialized at startup. Status refreshes on a
one-minute schedule when enabled, on base/override/Compose configuration and
project preference changes, and when changing projects. File events are restricted
to the project root; nested/other-project configuration files do not trigger DDEV
reads, and removing the parent `.ddev` directory invalidates controls.
**Refresh** remains available manually. Snapshot and failure caching avoid
repeated DDEV processes on node selection. Selections stay separate per project
during the live session; the optional `storage` grant preserves them across idle
reclaim. Cached state is republished on editor focus and action lookup without
forcing another CLI read, and explicit runtime commands revalidate current CLI
state even when watching is disabled.

Long commands use keyed native terminals with `DDEV_NONINTERACTIVE=true`;
finishing or closing one refreshes status even with periodic watching disabled.
Failed exits show a warning; terminal output retention follows the host.
Repeating the same live operation focuses its existing terminal; different
projects/arguments use distinct keys. Configure opens the YAML only after a
successful terminal exit, including when its content is unchanged; file changes
alone do not imply command success. The pending Configure follow-up survives idle
instance reclaim via optional storage, expires after 30 minutes, and matches the
launched terminal operation. Different Configure arguments wait for the active
operation to finish; repeating the same active operation focuses its terminal.
Focusing the existing operation does not extend its deadline.

## Commands

All commands appear under **DDEV** in Find Action. Installation, release and issue
links, and revealing Services, also work without an open project. Common commands
appear in Tools and Services context menus. The status menu and command conditions
reflect project/CLI/database state. Delete and Power Off remain available during
startup and recoverable description failures. Stop and Restart can recover when
description is unavailable but DDEV is installed and the project is configured;
container tools require running state. Valid Compose service identifiers,
including leading punctuation, are supported with bound `--service=<name>` values.
Toolbar targets restore the current project's selection before dispatch.

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

Arguments are passed separately, without building a shell command string. The
tool-argument prompt accepts quoted arguments and cancellation. PHP/Node
explicitly request raw execution, preserving literal dollar signs, backticks and
empty arguments instead of asking DDEV to reconstruct a container shell command.
PHP/Node/Composer commands require a running project.

Configure Project chooses PHP from the plugin's explicit **PHP version** setting,
then `[languages.php].languageLevel` in `.openidea/settings.toml`, then an exact
`config.platform.php` version in `composer.json`. Composer patch versions are
reduced to major.minor; version constraints are not guessed. Otherwise DDEV
chooses. The plugin cannot read an effective global IDE language level.

## DDEV configuration assistance

Project and global DDEV YAML use the complete official **v1.25.5** schemas,
bundled for offline use. Completion covers nested properties (database, hooks,
daemons and exposed ports), enum/boolean values and engine-specific database
versions. Key/value hover and completion show schema documentation. Diagnostics
report YAML syntax, duplicate keys, unknown properties and invalid schema values.
Numeric version strings are inserted with quotes; existing colons are preserved.
Completion also works immediately after a colon, adding the required space.

Assistance is scoped to `.ddev/config.yaml`, `config.*.yaml`,
`global_config.yaml` and `global_config.*.yaml`. No DDEV process/network grant is
needed. Schemas are pinned, so future fields may trigger version-labelled
warnings. Files are checked individually (not merged with overrides);
single-document YAML up to 256 KiB is supported, with bounded depth/alias
expansion. Completion recovers the edited token and basic unclosed flow
collections; unrelated syntax errors can prevent suggestions. General YAML
highlighting/formatting remains the host's. Windows drive and UNC paths match
configuration filenames without ASCII case sensitivity; Unix paths remain
case-sensitive. See [schema provenance](schemas/README.md).

## Database setup

With **Maintain a DDEV database data source** enabled and `database.sources`
granted, startup and refresh maintain a plugin-owned native Database connection.
DDEV's password goes directly to the secure Database credential store and is
never included in connection settings or project files. MySQL, MariaDB and
PostgreSQL use the published port on `127.0.0.1` (the project server for remote
projects). The explicit command requests normal Database connection/introspection;
automatic setup also requests it at the first interactive refresh/focus of the
session. `project.write` is not required for native Database setup.

Without `database.sources`, the non-secret connection-file workflow remains
available with `project.write`: it preserves unrelated entries/comments, rejects
duplicate IDs and requires manual password entry/connection. After native setup
succeeds, a uniquely identified, explicitly marked legacy entry is removed when
project writes are permitted; unrelated or ambiguous entries remain intact.
Existing uniquely identified managed sources are reused after a project move,
including the marked legacy file entry; ambiguous managed identities are left
unchanged, and native setup repopulates the credential through the secure API for
the moved project (no password is migrated through project files).

Automatic native maintenance compares the desired connection and credential in
memory, checks that the owned source still exists, and skips identical upserts and
credential writes. The host disconnects on every upsert, so this keeps a
manual/live connection intact during normal status polling. Changed ports or
credentials clear the connection marker and allow a later interactive reconnect; a
removed source is recreated. A partial setup failure clears the cache so a
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
`/var/www/html`, configurable as **Project path inside the web container**. This
generic target invokes the container's Composer binary; the explicit Composer
command retains `ddev composer`. The generic target does not apply DDEV's
`composer_root`, pre/post-composer hooks or Composer-specific environment/sync
behavior. Unlike the guarded explicit tools, a built-in run through the static
target has no plugin pre-launch state check; DDEV may start a stopped project.
Environment forwarding into the container follows DDEV's own conventions. Choosing
a custom executable in Settings > Plugins > DDEV Integration > `process:ddev`
applies to commands, terminals, logs and targets; the host approves and pins it
globally/per project.

**Offer DDEV for project runs** controls registration. Invalid/changed mapping
preferences, disabled registration, removal of `.ddev/config.yaml` or a failed
runtime refresh withdraw the target; successful recovery registers it again.
Unchanged registrations are cached to avoid restarting target language servers on
every status read. The host can also run a configured language server in the
target when it is installed in the container; this remains a separate explicit host
selection. PHP connect-back Xdebug mapping and supported debugger placement use
the host's target machinery; Xdebug must be enabled/configured in DDEV.

## Settings and permissions

Settings > Plugins > DDEV Integration provides:

- Automatic status refresh (default on; every minute).
- Automatic update checks (default on; at most once per day, including failed attempts, with network and storage granted). Current versions stay quiet; newer versions notify with release-note, installation-guide and disable-check actions. The explicit command always reports its result.
- Automatic native data source maintenance (default on; `database.sources`).
- Offer a project execution target (default on) and its container path.
- Explicit PHP version for project configuration (empty by default).

A successful first project detection offers a one-time setup/Services tour when
storage is granted. Runtime failures offer Retry, Setup and Installation Guide
actions; failures stay quiet per project even when the error wording changes, and a
successful status read, including background recovery, allows the next failure to
notify again. Project-specific notification actions reject clicks from another
project's window; windowless notifications can still appear in every window, with
bodies identifying the originating project.

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

## Feature coverage

| Feature area | Support |
|---|---|
| Lifecycle commands, `ddev ssh`, HTTPS-first service links | Implemented through native terminals and Services |
| Status watching and action `isActive` checks | Automatic refresh, context keys and runtime guards; **60 seconds rather than 10**, due to the host's minimum schedule |
| Status bar and version settings | Native status/action picker and automatic/explicit checks |
| PHP version hints | Explicit/project-local/composer PHP hints; global IDE PHP level unavailable |
| PHP/Node runs, Composer, path mappings | API 1.4 project target integrated with host selection; raw argv and path mapping verified. Host-local version probes/global PHP level and IDE SDK metadata remain different |
| Automatic data source creation and credentials/introspection | API 1.4 owned native source, secure credential and connection request; file fallback when native grant is denied |
| Post-command refresh/open configuration | API 1.4 terminal-exit callbacks, including reclaim and unchanged configuration |
| Custom DDEV executable, terminal environment and WSL patching | Host-owned per-project/global executable chooser and native command environment; a local UNC patch is not reproduced |
| Tutorial and actionable recovery notifications | Native guided setup, one-time tour, environment checks and recovery/update actions; an overlay presentation is not reproduced |
| Error reporting | Opens this plugin's issue form on explicit request; no automatic Sentry reporting |
| YAML schema assistance | Complete bundled DDEV project/global schema completion, nested values, hover and validation |

This plugin reads only project-root `.ddev/config.yaml`; nested discovery is an
addition. Services log/inspect tabs are additional plugin features.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
openidea-plugin build
openidea-plugin check
openidea-plugin link
```
