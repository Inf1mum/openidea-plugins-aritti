# Laravel compiled-component acceptance

This opt-in harness runs the actual Wasm component in the pinned v0.1.40 runtime.
A fixture host starts real local PHP child processes against a disposable Laravel
application. It implements project reads, storage, confirmation and view-message
callbacks; native terminal dispatch is recorded, not opened in a desktop window.

```sh
composer create-project laravel/laravel /tmp/laravel-artisan-acceptance-test --prefer-dist --no-interaction --no-scripts
openidea-plugin build plugins/laravel  # v0.1.40/API1.4 CLI
cargo run --manifest-path plugins/laravel/tests/wasm-smoke/Cargo.toml -- plugins/laravel/dist/plugin.wasm /tmp/laravel-artisan-acceptance-test
```

The app's directory name must start `laravel-artisan-acceptance-`. The harness
adds the authored probe command from `probe.php.inc` to `routes/console.php` if
absent, plus 180 authored catalog-only commands to exercise responses larger than
512KiB. It only runs the probe and metadata discovery. It tests ordered arguments
with spaces/Unicode, success/nonzero exit, bounded output draining, cancellation,
duplicate submission refusal, session history, denied process permission and stale
source identity. Failed-command notifications are checked for exactly one delivery
across repeated polls. All kept handles must be released. Do not point it at a real app.
The fixture application is left available for inspection; remove it after use.

The harness also writes authored model, enum, config and migration fixtures.
The compiled Config inventory must contain nested numeric keys, source expressions
and source-file cells. Filesystem
enumeration returns only these fixtures. Compiled completion checks exercise
inherited scopes, relation traversal, collection methods and enum casts with the
50ms callback budget, and assert that callbacks perform no host source reads.
The compiled Generators preview also returns the model, migration and factory
contents for a typed field, without creating files or starting PHP.
Editing callbacks are absent. This does not validate native desktop editor UI,
network services, Docker, real clipboard or native terminal output.

The disposable app also receives a small controller and two loop-generated URI
prefixes in `routes/web.php`. The compiled Routes refresh must run real
`artisan route:list --json`, expose all six generated routes and identify the
runtime snapshot without placeholder URIs.
