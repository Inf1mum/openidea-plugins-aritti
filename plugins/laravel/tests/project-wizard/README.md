# New-project installer adapter

`Laravel: New Laravel Project…` creates a fresh application through Composer or
an installed Laravel installer. It does not run Composer/installer creation in
the editor callback; the reviewed command launches in a keyed native terminal.
The terminal retains installer output and incomplete files are never deleted.

The Laravel installer adapter reads its installed version and `new --help`.
Only documented option names with the expected argument kind are offered.
The guided choices cover starter/custom Composer packages, database, tests,
Boost, supported authentication/team/component modes, frontend package manager,
local Git/branch and development releases. Other choices remain interactive in
the terminal. The complete version and help output are rechecked after the
argv review. Unsupported/conflicting options are refused; remote repository
creation, `--force` and arbitrary flags are not exposed.

The destination is rechecked before launch. Composer may use an empty directory;
Laravel installer creation requires an absent directory. The latter adapter
refuses Windows drive destinations because the pinned integration cannot prove
the installed tool's absolute-drive handling; Composer remains available.
Filesystem inspection needs a matching read grant. Installers can execute their
normal dependency scripts, as the concrete launch review explains.

`Laravel: Recover Project Installation…` lists retained attempts when private
plugin storage is available. It can open a partial directory through the normal
project trust flow, start a wholly new reviewed wizard in another destination,
or forget a record after the user confirms its original process has stopped.
Recovery never replays the old command into a partial directory. An unresolved
record blocks destination reuse even after plugin reload. Matching terminal
exit events can finish a persisted attempt after reload. Without storage, the
wizard clearly reports that recovery tracking is session-only.

Recovery is an installation ledger, not a process supervisor: a retained running
record may describe an interrupted session or a process still running in its
original terminal. The adapter cannot prove that a lost terminal has stopped.
It never silently changes such a record into permission to reuse a directory.
Records do not contain terminal output; that output stays in the original
terminal. Desktop terminal retention, project opening and remote-machine path
behavior still require acceptance in the actual host.

## Validation

Unit tests cover exact argv, option arity and conflicts, cancellation, filesystem
denial, help/version drift, destination reuse, partial-directory preservation,
recovery opening without execution, forget without deletion and matched exit
events across reload.

An opt-in read-only fixture test invokes only `laravel --version` and
`laravel new --help --no-ansi`, then validates a generated argument plan:

```sh
CARGO_INCREMENTAL=0 cargo test --manifest-path plugins/laravel/Cargo.toml \
  project_wizard::tests::installed_laravel_help_roundtrip_without_installing \
  -- --ignored
```

It passed locally against Laravel Installer **5.28.1**. Composer **2.10.1** version
discovery was also checked.

A separate real-install acceptance on 2026-11-10 used the official Composer
`laravel/installer:5.28.1` distribution, PHP 8.5.5 and Node 24.18.0 in disposable
`/tmp` directories. The reviewed `new --svelte --database=sqlite --phpunit
--no-boost --no-interaction` command completed with Laravel 13.42.0, migrated
SQLite and a production Vite manifest. The official skeleton Composer/Node
scripts were reviewed before execution. A whitelisted environment used isolated
Composer/npm configuration; the Herd-modified system installer was not run.

An earlier genuine attempt failed during the official starter install hook
because npm was absent from its restricted PATH. The partial directory was
retained. Repeating the real installer command refused `Application already
exists!` with exit 1; an aggregate before/after hash verifies all retained files
were unchanged. The second successful install used a different destination.
See `installer-5.28.1-acceptance.json` for the bounded receipt. These process
checks supplement the native recovery ledger tests; they do not claim desktop
host terminal-retention or project-trust UI acceptance.
