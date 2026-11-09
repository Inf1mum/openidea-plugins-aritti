# Dev Container CLI

Runs the project's [Dev Container](https://containers.dev) with the
[devcontainer CLI](https://github.com/devcontainers/cli):

| Command | Runs |
|---|---|
| Dev Container: Choose Configuration… | Picks and remembers a configuration for the current project |
| Dev Container: Start Container | `devcontainer up` in a terminal |
| Dev Container: Rebuild Container | `devcontainer up --remove-existing-container` |
| Dev Container: Open Shell in Container | `devcontainer exec … bash -l` (or `sh -l`) |
| Dev Container: Show Configuration | `devcontainer read-configuration`, summarised in a notification (image or compose file, remote user, forwarded ports, feature count, `postCreateCommand`) |
| Dev Container: Stop Container | `docker stop` on the containers labelled with the project folder |
| Dev Container: Refresh Feature List | Downloads the feature index from containers.dev into the plugin's storage (see Completion) |

The plugin discovers `.devcontainer/devcontainer.json`, `.devcontainer.json` and
`.devcontainer/<name>/devcontainer.json`, including hidden `.devcontainer` folders.
When several exist, commands ask which to use; cancellation starts nothing. With
optional `storage`, that choice survives plugin reloads and is scoped to the current
project. **Choose Configuration…** (Tools or the Services toolbar) changes it.
Deleted or invalid remembered files are discarded and discovered again. Without
storage permission a choice is used for that command and may be requested again.
Discovery follows project ignore rules and is capped at 256 configurations; ignored
or additional files can be selected through the explicit setting.

Settings › Plugins › Dev Container CLI › **Configuration file** overrides discovery
with a validated file inside the project. A missing override is an error, not a
fallback to another configuration; clear the override to use the chooser.
New › Dev Container Configuration adds a
starter `devcontainer.json`; New › Dev Container (Node, Python, Java, PHP, Rust, Docker Compose)
adds one based on the matching containers.dev reference image.

## Dev Containers in Services

The Services tool window lists the project's containers, running and stopped (`docker ps -a` filtered by the
`devcontainer.local_folder` label the CLI sets, so only containers started from this folder show). Select a
row for Open Shell (`docker exec -it`) or Stop on a running container, or Start (`docker start`) on a stopped one;
double-click opens a shell, or starts a stopped container. The selected container
has metadata and a managed follow-log tab. Selection is owned by the current
project and cleared when it changes. Start/Stop/Shell re-read that project's
container list and running state before acting; removed or unlisted IDs are refused.
Needs `process:docker`. The label
is taken from the CLI's behaviour and has not been checked against every CLI version. A gutter marker on
`image` that starts the container is not offered: a gutter marker can open a location but not run a command.

## Inspections

Run when `.devcontainer/devcontainer.json`, `.devcontainer/<folder>/devcontainer.json`
or `.devcontainer.json` changes in the editor (comments and trailing commas are
read as JSONC; files above 256 KB are skipped). Each group has a setting.

- `unknown-property` (Check properties): a top-level key that is not in the
  [specification](https://containers.dev/implementors/json_reference) but is within edit distance 2 of
  one, as a likely typo (`postCreateCommnad`). Other unknown keys, `x-*`, `$`-prefixed keys and
  everything under `customizations` stay quiet, since the property list may be incomplete.
- `features-malformed` (Check features): `features` that is not an object, a feature id that is empty or
  contains whitespace, or a feature value that is not an options object, a version string or a boolean.
- `folder-structure` (Check folder structure, information): a `devcontainer.json` that is not at
  `.devcontainer/`, `.devcontainer/<folder>/` or `.devcontainer.json`. It looks at the file's path only.

## Completion

In a `devcontainer.json`, completion offers the top-level properties not yet present and the values of
`shutdownAction`, `userEnvProbe`, `waitFor` and the boolean properties. Nested objects and arrays are not
completed. Keys of the `features` object are completed with feature ids (`ghcr.io/devcontainers/features/node:1`)
from the public index at `containers.dev/static/devcontainer-index.json`. The index is downloaded only by
Dev Container: Refresh Feature List and kept in storage, so completion is empty until you run it once and
may lag behind the registry. It needs the optional `storage` and `net:containers.dev` permissions. The index
layout was read from the live file; the download itself is not verified in a live IDE. A setting switches it off. The provider is declared for the `json` and `jsonc`
language ids; `jsonc` is not verified in a live IDE.

Requires the devcontainer CLI (`npm i -g @devcontainers/cli`) and Docker. The IDE
keeps running on the host: OpenIDEA's remote development connects over SSH or
WSL, not into a container.

## Permissions

| Permission | Used for |
|---|---|
| `process:devcontainer` (optional) | Run the devcontainer CLI: up, rebuild, exec (shell in the container) |
| `process:docker` (optional) | List and stop containers labelled with the project folder (`docker ps`, `docker stop`) and open a shell in one (`docker exec`). Without it, Stop Container and the view are unavailable |
| `storage`, `net:containers.dev` (optional) | Remember project configuration choices; keep the feature id list and refresh it from containers.dev |
| `project.read` (required) | Find and read `devcontainer.json` |
| `events` (optional) | Check `devcontainer.json` when opened or saved |

Programs are started with separate arguments, never through a shell line.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


Start and Rebuild now show a concrete confirmation after configuration selection:
project root, configuration path and snapshot size, executable and the exact
argument vector as a JSON array. Rebuild explicitly shows
`--remove-existing-container` and its removal behavior. Cancel starts no terminal
or process and preserves the project-scoped configuration choice. Programs still
receive separate arguments; the displayed array is not executed as a shell line.

Before launching, the plugin rechecks the invoking project identity, configuration
file metadata and current project/editor text. Changes, closure, deletion or lost
read permission stop the launch; run the action again to review its new target.
This preview supports configurations up to 256 KiB. The CLI reads saved disk files,
so save edited configurations first. The snapshot check is not an atomic lock on
Docker or the filesystem and does not cover referenced Dockerfiles, Compose files,
features or lifecycle-script changes. Shell, Clone and native Services lifecycle
operations keep their existing workflows. No container operation or desktop UI
was exercised for this change. This Start/Rebuild slice adds command review; configuration creation is described below.


### Create a configuration from bundled templates

In a project JSON/JSONC editor, invoke **Create Dev Container configuration…**
from native intentions. Choose Base (Ubuntu), Node, Python, Java, PHP or Rust;
enter the destination and display name; then confirm the exact absolute path and
full JSON content. The image and other defaults come directly from this package's
existing file templates. The host creates and opens one file as an undoable
FileAction with `overwrite: false`; resolving the action itself writes nothing.
The final confirmation is the preview: choosing a lazy action can apply it
immediately after its prompts finish.

Allowed destinations are `.devcontainer.json`, `.devcontainer/devcontainer.json`
and `.devcontainer/<name>/devcontainer.json`, with a one-level name using letters,
digits, `_` or `-` (up to 64 characters). Creation refuses existing files or
buffers and file-shaped parents, validates project/source ownership and source
version, and rechecks the target after confirmation. Cancellation at any prompt
returns no operation. Parent directories are created by the host workspace edit.
No Docker, devcontainer CLI, image pull or registry request runs during creation.
Existing project selections and Configuration file overrides remain unchanged;
after creation use Choose Configuration if several configurations exist.

This is image-template creation only. The existing Docker Compose New-menu
template remains available, but this intention does not generate Compose files,
Dockerfiles, feature options, remote templates or copies of existing configs
whose relative paths may change meaning. It does not check image availability or
supply a remote catalog. Source ownership and collisions are checked during
resolution; the host also enforces project confinement and non-overwriting create
on application. Desktop creation/undo has not been verified for this change.

Configuration creation and local inspections need only project.read. The devcontainer
process permission is optional; grant it for runtime commands. Creating a bundled
configuration does not require the CLI, Docker, network or process permissions.

Services keeps a non-container status row when the project is missing, Docker is unavailable/denied, or no project container exists. Selecting it shows help rather than starting a container; Refresh and the existing eligible configuration actions remain available.

### Current project ownership of configurations

Configuration discovery and remembered/explicit choices exclude files whose current
innermost open project differs from the invoking project. Start/Rebuild checks that
ownership before reading the configuration and again after its review; a previously
reviewed configuration reassigned to a nested project cannot launch. Configuration
creation checks destination ownership both before the preview and after confirmation,
even when the destination does not yet exist. Open the owning project to use its
configuration. These are current lexical project-owner checks, not filesystem-alias
or universal symlink-identity guarantees. The host still owns final nonoverwrite
checks/application; no source-to-apply transaction or container-runtime lock is added.

This canonical API1.3 update is separate from the frozen opt-in API1.4 candidate.
No staged SDK, overlay, lock or catalog was changed. MockHost tests and the pinned
package build/check validate this source slice; no actual CLI/container or native
project-switch journey was run.

### Finite managed Start/Rebuild CLI

**Start Container (Finite Managed CLI)…** and **Rebuild Container (Finite Managed
CLI)…** are separate explicit commands. They use the same configuration choice and
concrete project/configuration/argv review as the existing terminal commands, with
the finite lifetime and cancellation limits shown before execution. Source, metadata
and innermost project ownership are rechecked after review. The child receives that
project explicitly as its CLI working directory. Stdin is closed; no shell or extra
environment is introduced. Existing terminal commands and native container controls
keep their previous behavior.

**Show Finite CLI Output** opens the native Services **Dev Container CLI Operations**
view. It keeps one accepted operation in memory, scoped to its invoking project.
Refresh Status performs at most four nonblocking reads of at most16KiB per stream;
rendering and detail panes use captured state only. The latest64KiB of UTF-8 output
is retained with explicit truncation. A reported exit drains output in bounded
rounds before releasing the handle. The target/state pane records the reviewed
project, configuration and exact argv; output is separate from container logs.

Cancel CLI / Close Handle targets only that fresh operation identity and project.
Foreign, forged and previous-operation controls refuse without cancelling the
current handle. A failed new start keeps previous accepted output; temporary status
errors keep the handle for retry. Explicit Refresh can close the owned handle only
when a successful complete project inventory establishes its owner is gone. An
unavailable inventory is labelled unknown and never treated as project closure.

This is an instance-owned finite CLI client, normally reclaimed with the plugin
after about300seconds idle. It does not provide a durable container/IDE backend,
automatic background polling or generic Stop/Rerun ownership. Killing or dropping
the CLI cannot prove Docker build/container termination or rollback. CLI exit status
is shown as a CLI result, without inferring container readiness; use the existing
container inventory for its separately observed state. Captured project-controlled
output is local, bounded and nonpersistent; it is not a universally redacted log.
The CLI still reads saved disk files, including referenced build/lifecycle files,
which are not locked by the preview.

The shared MIT ui-kit confirmation helper is now a direct path dependency solely
for exact16KiB review bounds in managed/terminal launch and configuration creation.
No shared crate source, SDK revision, engine requirement or capability was changed.
The optional process:devcontainer grant already covers these commands. All105 tests
and pinned package build/check validate source behavior; native process/lifetime,
Services layout and real CLI/container acceptance remain unverified. The separate
API1.4 candidate remains dated until independently restaged.

### Optional cached feature selection during creation

When the existing registry cache is readable and contains supported references,
configuration creation offers **Optional cached GHCR feature**.
Choose No additional feature or one reference; cancelling this prompt cancels the
whole unresolved operation. Defaults produce an empty options object; when bounded
cached Boolean/enum metadata exists, a separate review can select explicit values.
The final exact JSON review shows the chosen options. Bundled feature options
remain unchanged. A cold,
unsupported or denied optional cache keeps the original image-only creation flow;
Refresh Feature List remains the explicit network command. Creation never fetches
a feature descriptor, pulls an image/feature or runs a CLI.

The picker rejects cached text above 900 KiB after the SDK retrieves the whole
value, examines at most 4,096 lines and offers at most 128 unique IDs. This is a
processing bound, not a pre-transfer storage bound. Only a conservative lowercase GHCR
repository subset with an explicit numeric major tag is offered; cached display
names are bounded and control characters removed. Local/tarball/digest/unversioned
references and arbitrary registries are outside this picker, not prohibited by the
Dev Container specification. Existing completion remains a separate cache consumer.

Cached ID/name facts do not verify image compatibility, current registry availability,
feature options/defaults, installation code or runtime behavior. Options are not
inferred from a name. The standard features object and OCI references are described
in the official [Features specification](https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features.md)
and [base schema](https://github.com/devcontainers/spec/blob/main/schemas/devContainer.base.schema.json).
This is a bounded source generator. Existing Dockerfile and Compose JSON forms
are described below; general YAML/remote templates and free-form option schemas
remain separate.

At the109 checkpoint, all109 package tests and the pinned API1.3 Wasm build/check passed. The public
before-fix fixture shows the old105 creation path omitting the selected cached feature.
New tests cover feature insertion, None/Cancel/invalid pick/denied storage, cache and
identifier bounds, and preservation of existing bundled feature options. No native
feature-picker or actual installation/container acceptance is claimed.

## Reviewed configuration from an existing Dockerfile

The creation intention also offers **Existing project Dockerfile**. Select a
project file up to8KiB; the complete current text and exact generated JSON appear
in the final review. `build.dockerfile` is relative to the configuration folder,
and `build.context` explicitly references the project root. No image field is
retained. The optional cached feature picker also supports the bounded cached
option review described below.

Resolution checks the source's current innermost project owner, regular-file
stat and complete text, then repeats those checks after final confirmation along
with destination vacancy and ownership. It returns one non-overwriting config
Create operation and never writes/edits the Dockerfile, builds or runs a tool.
Source/editor changes must be saved before Start. Included context files, ignore
rules, build args/stages, base images, syntax and compatibility are not validated.
This adds a bounded Dockerfile form; the Compose JSON sibling form is below.

## Reviewed configuration from existing Compose JSON

**Existing project Compose JSON** reads one owned strict JSON file up to8KiB and
offers1–64 explicitly declared services. Duplicate keys (including escaped key
aliases) anywhere in the JSON refuse; YAML and multi-file resolution are outside
this form. Select a declared service and enter a literal absolute container
workspace path. Mounts, running containers and workspace existence are unverified.
The existing bundled Compose template receives the destination-relative source
path, chosen service/name/workspace and optional reviewed cached feature.

The final review includes complete current Compose JSON and exact generated config.
Resolution repeats source owner/stat/text and destination owner/vacancy checks,
then returns only a non-overwriting config Create. It never edits Compose source
or starts services. Includes, interpolation, overrides, profiles, dependencies,
environment files/secrets, images/builds and runtime semantics are not resolved
or validated. Save source/editor changes before Start. Docker officially supports
[Compose JSON](https://docs.docker.com/compose/support-and-feedback/faq/#can-i-use-json-instead-of-yaml-for-my-compose-file);
[Dev Container guidance](https://containers.dev/guide/dockerfile#using-docker-compose)
defines the selected service and workspace fields. No native acceptance is added.


## Optional cached true/false and listed feature options

Refresh Feature List retains a bounded subset of the public index's declared
Boolean and string-enum options in a separate optional cache. Its exact stored
name-index text must match the current name cache. Existing old/cold/denied,
malformed, oversized or mismatched metadata keeps default-only behavior. Creation
never fetches metadata; the normal explicit refresh command still makes one index
request, without fetching any feature descriptor. A metadata-cache write failure
reports a warning after names refresh.

After selecting a feature, choose feature defaults or review cached options. Each
option offers Omit (use the feature's own default) plus explicit typed choices.
Cancel returns no creation operation. No cached default is automatically written.
The exact final config shows every selected Boolean/string value. Existing bundled
feature options are preserved. Cache data may be stale and does not verify current
registry descriptors, availability, image compatibility or runtime behavior.

The metadata processor admits index text up to3MiB after HTTP retrieval, scans
at most512 collections/4096 features, binds at most128 creation references, and
stores at most512KiB including the name-index binding. Reads retrieve the whole
storage value before size admission. At most64 option definitions per feature are
examined and8 supported options offered; enum lists have1–16 unique strings up
to128bytes each. Option IDs, descriptions and defaults are bounded/type checked.
String proposals/free-form values, unsupported types and invalid descriptors are
omitted. These are form scope limits, not constraints on the broader standard.

## Reviewed native Run Targets (API 1.4)

Tools offers a root-relative Run Target and an experimental mapped project-root tooling target. Preparation inspects a bounded approved local Unix Docker endpoint and exact running container/bind identity. Modal approval only prepares a snapshot: a separate fresh **Register once** button rechecks project/config/container facts before native registration. It never automatically selects a target or launches a command. Plans expire after120seconds; changed configuration and controlled container lifecycle actions withdraw matching registrations.

Mapped mode preserves stdin and declares the verified project bind for configured target language servers. It requires an installed server and manual host target selection. Absolute paths, arbitrary cwd, per-launch container reinspection and a general remote IDE backend are not promised. Existing finite managed CLI commands retain their approval and bounded-output behavior; registration does not make destructive Rebuild freely rerunnable. Native release/container/LSP acceptance remains separate from source tests and Wasm builds.
