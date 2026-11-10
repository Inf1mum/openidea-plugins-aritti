# OpenIDEA plugins

**API 1.4 released (v0.1.40, 2026-11-09):** [Current blocker requalification](docs/parity-2026-11-08-refresh/api14-release-requalification/README.md). Canonical packages now target SDK v0.1.40 / API 1.4. See the [native migration record](docs/native-api-migration-implementation-2026-11-09.md) for implemented changes and remaining validation. Dated parity receipts remain historical.

Plugins for [OpenIDEA](https://github.com/l0ng-ai/openidea), each a sandboxed WebAssembly component (`wasm32-wasip2`) with a `plugin.toml` manifest. Some are original, some are ports of JetBrains Marketplace plugins. All ids use the `aritti.` prefix (`aritti.<directory name>`).

## Plugins

| Area | Plugins |
|---|---|
| Files and editing | `blade`, `dotenv`, `ignore`, `properties`, `twig`, `openapi`, `ideolog` |
| PHP frameworks | `laravel`, `symfony` |
| Spring | `spring-beans`, `spring-boot`, `spring-data`, `spring-security`, `spring-web`, `spring-integration`, `spring-osgi`, `spring-runtime`, `spring-shell`, `spring-ws`, `spring-initializr`, `jpa-assist` |
| Java frameworks | `quarkus` |
| Database migrations | `flyway`, `liquibase` |
| Tooling and workflow | `dependency-audit`, `ddev`, `devcontainer`, `kube-explorer`, `gitlab`, `tasks` |

Each plugin directory has its own `README.md` (features) and `LICENSE`.

- [PARITY.md](PARITY.md): how close each plugin comes to the IntelliJ plugin it stands in for.
- [LIMITATIONS.md](LIMITATIONS.md): what the OpenIDEA plugin API can't do yet, per port.

Active work: [post-migration feature and UI parity plan](docs/superpowers/plans/2026-11-08-post-services-parity.md), with a refreshed comparison for all 29 plugins.

## Unreleased API 1.4 migration

All packages now target SDK v0.1.40 / plugin API 1.4. JPA, Tasks, Properties,
Ideolog, Flyway/Liquibase lists and Spring Runtime observations use native views.
Twelve packages retain shared packaged UI for richer workflows;
[custom panels must use our UI kit and native conventions](docs/custom-panels-shared-kit-2026-11-10.md). See the
[native UI parity record](docs/native-ui-parity-2026-11-09.md) and the
[all-plugin UI adaptation review, including DDEV](docs/native-ui-adaptation-2026-11-10.md). See the [implementation and verification record](docs/parity-2026-11-08/implementation/README.md)
for completed work, checks and remaining parity tasks. Desktop acceptance is pending.

## Upstream follow-up

- [ ] Once a plugin using API 1.3 view grouping is published in this repository, reply to
  [the maintainer's request on OpenIDEA PR #2019](https://github.com/l0ng-ai/openidea/pull/2019#issuecomment-6711066426)
  with a direct link to that plugin and its manifest showing the grouped views.
- [ ] Once a plugin using API 1.3 `fs.plugin-dir` is published in this repository, reply to
  [the maintainer's request on OpenIDEA PR #2051](https://github.com/l0ng-ai/openidea/pull/2051#issuecomment-6714118039)
  with a direct link to that plugin using the private directory API.

## Layout

```
plugins/<name>/
  plugin.toml     manifest
  Cargo.toml      own [workspace]; no root Cargo workspace
  src/lib.rs
  LICENSE [NOTICE]
crates/           shared Rust code (spring-text)
docs/             audits and design notes
```

## Build

```sh
# once
cargo install --locked --git https://github.com/l0ng-ai/openidea --tag v0.1.40 ide-host --bin openidea-plugin

cd plugins/<name>
cargo test                 # logic against the SDK's MockHost
openidea-plugin build      # → dist/plugin.wasm, then validates the package
openidea-plugin link       # load the folder in the IDE; rebuild, then Settings › Plugins › Reload
```

Build everything:

```sh
for d in plugins/*/; do (cd "$d" && openidea-plugin build) || echo "FAILED: $d"; done
```

## Release

Versions are managed by release-please (see [CONTRIBUTING](.github/CONTRIBUTING.md)). A release is packed and signed with

```sh
openidea-plugin pack plugins/<name> --key <path outside repo> --publisher aritti
```

and published to the community registry [`l0ng-ai/openidea-plugins`](https://github.com/l0ng-ai/openidea-plugins) with one pull request per plugin version. The signing key never goes in this repo.

## Licence

MIT for original code (see [LICENSE](LICENSE)). Ports keep the original plugin's licence in their own `LICENSE`/`NOTICE`, which takes precedence for that directory.

Related plugin views share native tabbed tool windows: GitLab, OpenAPI, Symfony, Flyway, Liquibase, Ideolog and Kubernetes. Spring Runtime already uses the same native grouping. Services integrations remain in Services. See [native window consolidation](docs/native-view-groups-2026-11-10.md) for tab membership and validation.
