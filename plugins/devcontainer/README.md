# Dev Container CLI

Run the project's [Dev Container](https://containers.dev) with the [devcontainer CLI](https://github.com/devcontainers/cli). MIT; see `LICENSE`.

Requires the devcontainer CLI (`npm i -g @devcontainers/cli`) and Docker. The IDE keeps running on the host: OpenIDEA's remote development connects over SSH or WSL, not into a container.

## Commands

| Command | Runs |
|---|---|
| Dev Container: Choose Configuration… | Picks and remembers a configuration for the current project |
| Dev Container: Start Container | `devcontainer up` in a terminal |
| Dev Container: Rebuild Container | `devcontainer up --remove-existing-container` |
| Dev Container: Open Shell in Container | `devcontainer exec … bash -l` (or `sh -l`) |
| Dev Container: Show Configuration | `devcontainer read-configuration`, summarised in a notification (image or compose file, remote user, forwarded ports, feature count, `postCreateCommand`) |
| Dev Container: Stop Container | `docker stop` on the containers labelled with the project folder |
| Dev Container: Refresh Feature List | Downloads the feature index from containers.dev into the plugin's storage (see Completion) |
| Dev Container: Start Container (in Services)… / Rebuild Container (in Services)… | The same reviewed launch with a finite lifetime and cancellation (see below) |
| Dev Container: Show Services Output | Opens the Dev Container Operations view |

Programs are started with separate arguments, never through a shell line. Start and Rebuild show a concrete confirmation after configuration selection: project root, configuration path and snapshot size, executable and the exact argument vector as a JSON array; Rebuild explicitly shows `--remove-existing-container`. Cancel starts no terminal or process. Before launching, the plugin rechecks the invoking project identity, configuration file metadata and current project/editor text; changes, closure, deletion or lost read permission stop the launch. The preview supports configurations up to 256 KiB, is not an atomic lock on Docker or the filesystem, and does not cover referenced Dockerfiles, Compose files, features or lifecycle-script changes. The CLI reads saved disk files, so save edited configurations first.

## Configuration discovery

The plugin discovers `.devcontainer/devcontainer.json`, `.devcontainer.json` and `.devcontainer/<name>/devcontainer.json`, including hidden `.devcontainer` folders. When several exist, commands ask which to use; cancellation starts nothing. With optional `storage`, that choice survives plugin reloads and is scoped to the current project; **Choose Configuration…** (Tools or the Services toolbar) changes it. Deleted or invalid remembered files are discarded and discovered again. Without storage permission a choice is used for that command and may be requested again. Discovery follows project ignore rules and is capped at 256 configurations; ignored or additional files can be selected through the explicit setting.

Settings › Plugins › Dev Container CLI › **Configuration file** overrides discovery with a validated file inside the project. A missing override is an error, not a fallback to another configuration; clear the override to use the chooser.

Discovery and remembered/explicit choices exclude files whose current innermost open project differs from the invoking project; Start/Rebuild rechecks that ownership before reading the configuration and again after its review. These are current lexical project-owner checks, not filesystem-alias or universal symlink-identity guarantees.

## Creating configurations

In a project JSON/JSONC editor, invoke **Create Dev Container configuration…** from native intentions. Choose Base (Ubuntu), Node, Python, Java, PHP or Rust; enter the destination and display name; then confirm the exact absolute path and full JSON content. The host creates and opens one file as an undoable FileAction with `overwrite: false`; resolving the action itself writes nothing. Allowed destinations are `.devcontainer.json`, `.devcontainer/devcontainer.json` and `.devcontainer/<name>/devcontainer.json`, with a one-level name using letters, digits, `_` or `-` (up to 64 characters). Creation refuses existing files or buffers and file-shaped parents, validates project/source ownership and source version, and rechecks the target after confirmation; cancellation at any prompt returns no operation. The New menu also offers Dev Container Configuration starters, and Dev Container (Node, Python, Java, PHP, Rust, Docker Compose) templates based on the matching containers.dev reference image.

When the registry cache is readable and contains supported references, creation offers **Optional cached GHCR feature**: No additional feature or one reference, plus a separate review of explicit values when bounded cached Boolean/enum metadata exists. Defaults produce an empty options object; the final exact JSON review shows the chosen options and existing bundled feature options are preserved. A cold, unsupported or denied cache keeps the image-only flow; Refresh Feature List remains the explicit network command. Creation never fetches a feature descriptor, pulls an image/feature or runs a CLI. The picker rejects cached text above 900 KiB, examines at most 4,096 lines, offers at most 128 unique IDs, and only a conservative lowercase GHCR repository subset with an explicit numeric major tag; local/tarball/digest/unversioned references and arbitrary registries are outside the picker. Cached ID/name facts do not verify image compatibility, registry availability, feature options/defaults, installation code or runtime behavior.

**Existing project Dockerfile** selects a project file up to 8 KiB; the complete current text and exact generated JSON appear in the final review, with `build.dockerfile` relative to the configuration folder and `build.context` explicitly referencing the project root; no image field is retained. Included context files, ignore rules, build args/stages, base images and compatibility are not validated.

**Existing project Compose YAML or JSON** reads one owned source file up to 8 KiB and offers 1–64 explicitly declared services. Quoted service names, flow/block mappings and ordinary scalar/list values are accepted; YAML aliases, tags, duplicate decoded keys, complex mapping keys, multiple documents and non-finite values are refused. Select a declared service and enter a literal absolute container workspace path; the final review includes the complete current source and exact generated config. Interpolation, includes/extends/overrides, profiles, dependencies, environment files/secrets, mounts and runtime semantics are not resolved. Neither form edits the source or starts anything.

Creation is image-template generation only: no Compose files, Dockerfiles, remote templates, feature-option inference from names, or image-availability checks. Configuration creation and local inspections need only `project.read`.

## Dev Containers in Services

The Services tool window lists the project's containers, running and stopped (`docker ps -a` filtered by the `devcontainer.local_folder` label the CLI sets, so only containers started from this folder show; the label is taken from the CLI's behaviour and has not been checked against every CLI version). Select a row for Open Shell (`docker exec -it`) or Stop on a running container, or Start (`docker start`) on a stopped one; double-click opens a shell or starts a stopped container. The selected container has metadata and a managed follow-log tab. Selection is owned by the current project and cleared when it changes. Start/Stop/Shell re-read that project's container list and running state before acting; removed or unlisted IDs are refused. A non-container status row appears when the project is missing, Docker is unavailable/denied, or no project container exists; selecting it shows help rather than starting anything. Needs `process:docker`. A gutter marker on `image` that starts the container is not offered: a gutter marker can open a location but not run a command.

## Start and Rebuild in Services

**Start Container (in Services)…** and **Rebuild Container (in Services)…** use the same configuration choice and concrete review as the terminal commands, with the finite lifetime and cancellation limits shown before execution; the child receives the project explicitly as its CLI working directory, stdin is closed, and no shell or extra environment is introduced. **Show Services Output** opens the native Services **Dev Container Operations** view: one accepted operation in memory, scoped to its invoking project. Refresh Status performs at most four nonblocking reads of at most 16 KiB per stream; the latest 64 KiB of UTF-8 output is retained with explicit truncation; a reported exit drains output in bounded rounds before releasing the handle. Cancel CLI / Close Handle targets only that fresh operation identity and project; foreign, forged and previous-operation controls refuse. A failed new start keeps previous accepted output; temporary status errors keep the handle for retry. Explicit Refresh can close the owned handle only when a successful complete project inventory establishes its owner is gone; an unavailable inventory is labelled unknown, never treated as closure. This is an instance-owned finite CLI client, normally reclaimed with the plugin after about 300 seconds idle: no durable container/IDE backend, background polling or generic Stop/Rerun ownership. Killing the CLI cannot prove Docker build/container termination; CLI exit status is shown as a CLI result without inferring container readiness.

## Inspections

Run when `.devcontainer/devcontainer.json`, `.devcontainer/<folder>/devcontainer.json` or `.devcontainer.json` changes in the editor (comments and trailing commas are read as JSONC; files above 256 KB are skipped). Each group has a setting.

- `unknown-property` (Check properties): a top-level key that is not in the [specification](https://containers.dev/implementors/json_reference) but is within edit distance 2 of one, as a likely typo (`postCreateCommnad`). Other unknown keys, `x-*`, `$`-prefixed keys and everything under `customizations` stay quiet, since the property list may be incomplete.
- `features-malformed` (Check features): `features` that is not an object, a feature id that is empty or contains whitespace, or a feature value that is not an options object, a version string or a boolean.
- `folder-structure` (Check folder structure, information): a `devcontainer.json` that is not at `.devcontainer/`, `.devcontainer/<folder>/` or `.devcontainer.json`; looks at the file's path only.
- **Check known property types and enum values** (separate setting): validates known scalar enum choices, common root property shapes and port attribute types/enums. Valid JSONC comments and trailing commas remain supported; malformed, incomplete or unsupported composite values stay quiet. A conservative specification model, not a complete Dev Container JSON Schema implementation.

## Completion

In a `devcontainer.json`, completion offers the top-level properties not yet present and the values of `shutdownAction`, `userEnvProbe`, `waitFor` and the boolean properties. Schema-backed nested key completion covers `build`, `hostRequirements`, `otherPortsAttributes`, and each entry of `portsAttributes` (including protocol/on-auto-forward enums and boolean values). Existing properties are omitted and editing an existing property preserves its value; arbitrary extension/environment keys and arrays remain quiet. Keys of the `features` object are completed with feature ids (`ghcr.io/devcontainers/features/node:1`) from the public index at `containers.dev/static/devcontainer-index.json`. The index is downloaded only by Refresh Feature List and kept in storage, so completion is empty until you run it once and may lag behind the registry; a setting switches it off. Bounded cached feature Boolean/enum options also complete inside the selected feature object when the cache matches the current index; unsupported descriptors remain quiet. Needs the optional `storage` and `net:containers.dev` permissions. The provider is declared for the `json` and `jsonc` language ids.

Refresh Feature List also retains a bounded subset of the index's declared Boolean and string-enum options in a separate optional cache (name index must match). The metadata processor admits index text up to 3 MiB after retrieval, scans at most 512 collections / 4096 features, binds at most 128 creation references and stores at most 512 KiB; at most 64 option definitions per feature are examined and 8 supported options offered; enum lists have 1–16 unique strings up to 128 bytes each; values are bounded to 4096 bytes. A metadata-cache write failure reports a warning after names refresh.

## Run Targets (experimental)

Tools offers **Register Container as Run Target…** and **Prepare Project-root Tooling…**. Both inspect the running container on the local Docker endpoint, show what will be registered, and register only when you choose **Register**, after the project, configuration and container are checked again. Nothing is selected or launched for you. Registrations expire after 120 seconds. The registered container ID is remembered in plugin storage, and each Run or Test launch checks that exact container with `docker container inspect`: it is refused when the container was rebuilt, stopped or Docker is not reachable. Extra mounts below the project folder (such as a `node_modules` volume) are allowed. Absolute host paths and an arbitrary working directory are not supported.

## Permissions

| Permission | Used for |
|---|---|
| `project.read` (required) | Find and read `devcontainer.json` |
| `process:devcontainer` (optional) | Run the devcontainer CLI: up, rebuild, exec (shell in the container) |
| `process:docker` (optional) | List and stop containers labelled with the project folder (`docker ps`, `docker stop`) and open a shell in one (`docker exec`). Without it, Stop Container and the view are unavailable |
| `process:git` (optional) | Clone Repository (runs `git clone` in a terminal) |
| `storage`, `net:containers.dev` (optional) | Remember project configuration choices; keep the feature id list and refresh it from containers.dev |

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
