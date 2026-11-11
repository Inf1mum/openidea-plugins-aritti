# Native generator compatibility acceptance

Previews use bounded, owned installed source reads and never execute a process.
The optional fixture tests execute generated PHP/JS only in their own temporary
output directories, without application bootstrapping, databases or a browser.
Package fixture installs disable Composer plugins/scripts and use public sources.

On 2026-11-10, both complete standard-catalog lint (typed and untyped) and actual
installed framework contract/behavior checks passed against these boundaries:

| Laravel | PHP |
| --- | --- |
| 10.0.0, 10.50.4 | 8.3 |
| 11.0.0, 11.57.1 | 8.3 |
| 12.0.0, 12.72.0 | 8.3 |
| 13.40.0, 13.42.0 | 8.4, 8.5 |

These checks cover all 36 standard catalog entries and exercised variants.
Laravel 10 class/interface/trait/enum and early missing Blade view stubs use
explicit authored skeletons with version bounds. Old mail/listener differences
are adapted from the actual installed stubs; unknown placeholders are rejected.

Livewire 2.12.8 / Dusk 7.13.0 (Laravel 10), Livewire 3.8.11 / Dusk 8.8.0
(Laravel 12), Livewire 4.4.8 / Dusk 8.8.0 (Laravel 13, PHP 8.5), and
stancl/tenancy 3.10.1 (Laravel 13) passed PHP lint and installed class contract
checks. Livewire 4 SFC/MFC outputs also passed the real Finder filename/location
contract including its default lightning prefix. Module output requires literal
package discovery roots; unsupported namespaced view adapters fail explicitly.
These checks do not launch a browser or migrate a tenant database.

Inertia generation respects metadata page path/prefix/suffix and
ignoreSettingsPaths. Existing standard page directories are offered only when
present; multiple roots/adapters require explicit choices. It requires an owned
locked PHP adapter identity and declared, installed frontend adapter. Native
minimal Vue, React and Svelte pages (TypeScript on/off) compiled successfully
against official installed adapters 3.11.0 and Vue 3.5.45 / Svelte 5.57.3 / Babel 7.29.13 compilers. Svelte
uses its standard `svelte:head`, rather than assuming an adapter Head export.
Custom package templates/base overrides require an explicit custom preset.

Run fixture tests with an isolated installed dependency root:

```sh
LARAVEL_NATIVE_GENERATOR_FIXTURE=/tmp/your-framework-fixture \
LARAVEL_NATIVE_PHP=/absolute/path/to/compatible/php CARGO_INCREMENTAL=0 \
cargo test --manifest-path plugins/laravel/Cargo.toml native_catalog::tests:: \
  -- --ignored
```

Select `official_installed_catalog_lints_every_kind_with_and_without_type_hints`
and `installed_framework_contracts_and_generator_behavior` for framework-only
fixtures. Package checks use `installed_livewire_dusk_and_tenant_package_plans`
with `LARAVEL_NATIVE_PACKAGE_FIXTURE`. Inertia compiler checks use
`installed_inertia_adapters_compile_native_pages` with
`LARAVEL_INERTIA_ADAPTER_FIXTURE` pointing at installed official frontend
adapters plus `svelte`, `@vue/compiler-sfc`, `@babel/core` and
`@babel/plugin-transform-react-jsx`. `LARAVEL_NATIVE_NODE` may select Node.
