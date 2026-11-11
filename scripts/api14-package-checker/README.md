# Released API 1.4 parity checks

This small independent workspace pins the host engine and manifest parser to
OpenIDEA v0.1.40 (`37c2a6048eea89d4f6db645622612358b643862b`). It compiles the
actual Wasm component and checks host linking and typed base exports. It also
verifies every declared optional provider/indexer/view interface is actually
exported, rather than treating successful base linking as proof of dispatch. It does
not instantiate plugin workflows or replace the full installer's contribution
and asset validation.

```sh
cargo build --locked --manifest-path scripts/api14-package-checker/Cargo.toml
scripts/api14-package-checker/target/debug/openidea-parity-package-checker \
  plugins/dotenv plugins/dotenv/target/wasm32-wasip2/release/openidea_plugin_dotenv.wasm
```

For the full non-Laravel check, also obtain the matching official CLI in an
isolated location. This does not replace a user's installed CLI:

```sh
cargo install --locked --git https://github.com/l0ng-ai/openidea --tag v0.1.40 \
  --root /tmp/openidea-parity-tools ide-host --bin openidea-plugin
python3 scripts/check-non-laravel-parity.py \
  --cli /tmp/openidea-parity-tools/bin/openidea-plugin \
  --output /tmp/openidea-parity-results --jobs 2
```

The runner tests/builds all 29 non-Laravel packages (or repeatable `--plugin`
selections), records source stability and artifact hashes, validates each exact
fresh component, and runs the full CLI against a temporary package containing
that artifact. It continues after a failure and exits nonzero for any failure,
source drift or blocked validation. Repeatable `--reuse <results-directory>` reuses
only previous passes with identical package/shared source, tool hashes and exact
Wasm digest; changed inputs are tested again. Source, canonical `dist`, installed plugins
and release versions are not modified. This is build/package evidence, not a
claim of desktop or original-feature parity.

Synchronize shared packaged UI without touching Laravel:

```sh
python3 scripts/sync-plugin-ui.py --exclude laravel
python3 scripts/sync-plugin-ui.py --exclude laravel --check
UI_QA_EXCLUDE=laravel node scripts/check-custom-ui-parity.mjs
```

The browser check requires Playwright; `PLAYWRIGHT_MODULE` can point to an
existing module. `UI_QA_DIR` chooses its screenshot/receipt destination.

Freeze current public original-plugin metadata and compatibility branches:

```sh
python3 scripts/refresh-parity-originals.py --additional-id 18813 \
  --output /tmp/openidea-parity-originals.json
```

This uses the existing original-ID inventory in ignored repository audit docs;
`--inventory` can select a preserved copy. No proprietary binaries are downloaded.

CI also validates newly built non-Laravel packages with the pinned official Linux
API1.4 CLI, so contribution-schema mistakes cannot pass on Cargo checks alone.

[Real-Wasm runtime smoke](../runtime-smoke/README.md) separately checks isolated
view dispatch, permission denial and a native Properties apply/Undo transaction.
It is not a substitute for desktop or authenticated service acceptance.
