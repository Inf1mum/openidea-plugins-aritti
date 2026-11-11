# Configuration inventory acceptance

The index now includes source-proven registered package defaults before publication.
Ordinary `ServiceProvider::mergeConfigFrom` preserves app string keys with a shallow
root merge, including entire nested-array replacement; integer keys are renumbered
in source order. Definitions and expression previews retain their original source
positions, including UTF-16 widths.

The separate Laravel 11+ bootstrap adapter checks the installed loader's merge
contract before adding its framework defaults. Only the loader's explicit
mergeable options receive its additional shallow merge level. PackageTools checks
owned, installed `Package`, `PackageServiceProvider`, `HasConfigs`, and
`ProcessConfigs` source before interpreting a straight-line typed `configurePackage`
chain with literal/default/multiple config names. Custom path/key/base overrides,
conditional registration, unknown selectors, ambiguous provider order, unreadable
app config, foreign bootstrap ownership, and exhausted budgets withhold affected
facts or authoritative missing-key diagnostics. No PHP runs during indexing.

Public behavior references:

- [Laravel package configuration](https://laravel.com/docs/13.x/packages#default-package-configuration)
- [Laravel configuration loader](https://github.com/laravel/framework/blob/13.x/src/Illuminate/Foundation/Bootstrap/LoadConfiguration.php)
- [PackageTools config registration](https://github.com/spatie/laravel-package-tools/blob/main/src/Concerns/PackageServiceProvider/ProcessConfigs.php)
- [PackageTools config options](https://github.com/spatie/laravel-package-tools/blob/main/src/Concerns/Package/HasConfigs.php)

Run pure/native MockHost tests:

```sh
cargo test --manifest-path plugins/laravel/Cargo.toml --lib metadata::config_parents::tests
```

Two ignored acceptance tests additionally read installed Laravel source fixtures
under `/tmp` (10.0.0, 11.0.0/11.57.1, 12.0.0/12.72.0, 13.40.0) and the installed
PackageTools source from the Filament 3 fixture. Run explicitly when present:

```sh
cargo test --manifest-path plugins/laravel/Cargo.toml --lib metadata::config_parents::tests -- --include-ignored
```

The independent runtime differential uses actual Laravel, outside the plugin:

```sh
LARAVEL_CONFIG_FIXTURE=/tmp/laravel-artisan-acceptance-20261110 \
  php -d zend.assertions=1 -d assert.exception=1 \
  plugins/laravel/tests/config-inventory/runtime.php
```

That differential passed with Laravel 13.40.0 / PHP 8.4.21 on 2026-11-11.
MockHost, installed-source, and PHP receipts do not establish native OpenIDEA
activation, completion, apply/undo, or desktop acceptance.
