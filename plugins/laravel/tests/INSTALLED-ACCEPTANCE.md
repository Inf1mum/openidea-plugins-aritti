# Installed-fixture acceptance (the 44 `#[ignore]` tests)

These tests load real installed Composer/npm packages and run real PHP. They need disposable fixtures
and PHP 8.5 (`LARAVEL_NATIVE_PHP`, plus `php` on `PATH` for the Blade/Monolog/MCP scripts). Run:

```sh
cargo test --manifest-path plugins/laravel/Cargo.toml --lib -- --ignored --test-threads=1
```

Received 2026-11-11: **44 passed, 0 failed** (PHP 8.5.5, Laravel 13.40.0, Livewire 4.4.8, Filament 3-5).

| Variable | Fixture |
| --- | --- |
| `LARAVEL_NATIVE_PHP` | PHP 8.5 binary |
| `LARAVEL_NATIVE_PACKAGE_FIXTURE`, `OPENIDEA_LARAVEL_PACKAGE_FIXTURE`, `LARAVEL_NATIVE_GENERATOR_FIXTURE` | installed Laravel + Livewire package app (`/tmp/openidea-package-parity-fixture`) |
| `OPENIDEA_RELATION_FIXTURE`, `OPENIDEA_PACKAGE_RELATIONS_FIXTURE` | app with Lift, adjacency-list, CTE, PowerJoins (`/tmp/openidea-package-relations-lift`) |
| `OPENIDEA_OCTOBER_FIXTURE` | `composer require october/rain` (allow `composer/installers`) |
| `OPENIDEA_LARAVEL_PEST_FIXTURE` | installed Pest app (`/tmp/openidea-package-parity-pest`) |
| `LARAVEL_INERTIA_ADAPTER_FIXTURE` | `npm install @inertiajs/vue3 @inertiajs/react @inertiajs/svelte vue @vue/compiler-sfc svelte react @babel/core @babel/plugin-transform-react-jsx` |
| `LARAVEL_ROUTE_FIXTURE`, `LARAVEL_DTO_FIXTURE` | copies of a stock Laravel app (writes allowed) |
| `LARAVEL_PROMOTION_FIXTURE`, `LARAVEL_EXTRACTION_FIXTURE` | copies named `/tmp/laravel-artisan-acceptance-*` (guarded basename) |
| `LARAVEL_ROUTE_LIST_FIXTURE` | `php artisan route:list --json` of the stock app |
| `LARAVEL_ARTISAN_METADATA_FIXTURE` | `php artisan list --format=json` of the stock app |
| `LARAVEL_CONFIG_FIXTURE_DIR` | the stock app's `config/` directory |
| `LARAVEL_MONOLOG_AUTOLOAD` | the stock app's `vendor/autoload.php` |
| `LARAVEL_APPLICATION_MCP_FIXTURE` | copy named `laravel-mcp-milestone6-*` with `laravel/mcp`, prepared by `tests/fixtures/prepare-application-mcp.php`, then `php artisan package:discover` |

The stock app is `composer create-project laravel/laravel` (see `tests/wasm-smoke/README.md`).
Fixtures are never the user's applications and are not committed.
