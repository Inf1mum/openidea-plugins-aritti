# OpenIDEA plugin API facts and feasibility classes (API 1.3)

Updated 2026-11-08 against upstream main `0bfd4fa3ccbd759b0482e4a7b7e49350774f6e1d`
(v0.1.39). This replaces the API 1.1 assumptions used in the original audit.
All 230 originally B rows have been reviewed in the
[decision ledger](reclass-api-1.3.md): **10 F / 65 F* / 155 B**. The five source
tables now contain **109 F / 273 F* / 155 B**, totaling 537 classified rows.
Existing F/F* rows were not re-audited; these are feasibility classes, not
implementation completion counts.

Classes: **F** feasible through an available API path; **F*** feasible with an
explicit scope, correctness risk or implementation cost; **B** at least one
required integration still lacks an API. A grouped row can remain B while
individual subtasks become feasible. A plugin-owned UI can meet an own-view
requirement without a native contribution point, but does not supply native
database, console or refactoring hooks.

## Track U status

| Track | API 1.3 status |
|---|---|
| U1 references / rename / hover | HoverProvider exists. The document index graph supplies Find Usages and previewed undoable Rename on exact name anchors. No generic framework Search Everywhere symbol contributor, automatic move-file hook or semantic name transformation is implied. |
| U2 file creation | FileActions supports atomic create/rename/delete plus text edits. Project.write_file creates/replaces project files and parents under project.write; it refuses dirty-buffer conflicts. Scoped fs writes and a private plugin directory also exist. |
| U3 discovery / events / persistence | Hidden paths are discoverable except .git internals. find_files still excludes ignored/excluded files. files-changed and subscribe_paths support incremental updates. Storage is 64 MiB; fs.plugin_dir is persistent and private. Scoped fs.read_dir/read_bytes can support library metadata discovery, not automatic classpath semantics. |
| U4 type/member resolution | Still unavailable. Project.symbols is bounded search over the current in-memory index, not LSP forwarding, assignability, member lookup or a resolved classpath. The maintainer defers semantic queries to async API 2.0. |
| U5 prompts / opening UI | pick, pick_many, input and confirm work in window commands/events and lazy action resolution. input_multiline and pick_directory have their documented command/event restrictions. reveal_view opens an owned view; open_project opens a directory. No general file picker, native settings-page jump or arbitrary external-file editor opening is implied. |
| U6 filters / trigger characters / built-in syntax | Event path filters exist. SemanticTokens colours built-in languages. Generic provider path filters, custom completion triggers and typed-character/pairing hooks remain unavailable. |
| U7 live diagnostics | DiagnosticsProvider checks versioned unsaved documents after a debounce; stale results are discarded. Manual publish_diagnostics remains available. |

## APIs relevant to reclassification

- Providers retain a 50 ms soft editing budget and share their instance with
  commands/events. Compute slow metadata in background/command paths and use
  bounded caches. Indexers build cross-file associations outside the edit path.
- Indexed name anchors use UTF-16 ranges covering the exact replaceable name.
  Markers are excluded from rename. Dynamic, escaped or context-dependent
  references need safe scopes or exclusions, not blanket textual replacement.
- InlayHintsProvider exists; it does not expose arbitrary clickable inlay
  commands, code vision, editor decorations or folding/selection setters.
- Package SVG line-marker icons work. Indexed markers with multiple definitions
  open a chooser. Direct marker command clicks remain deferred.
- host.start returns a managed Child with poll/write/wait/kill. Process grants
  remain explicit and pinned; this does not expose arbitrary existing terminal
  output or register native console filters. Streaming log tabs are available;
  the code-opened tab path does not yet support remote projects.
- Packaged web views can provide split layouts, tables, filters, command forms,
  custom diagrams and plugin-backed content tabs. Do not assume interactive
  Body::Html scripts work: desktop srcdoc CSP remains a source-level concern.
- Settings support pages, groups, arrays, tables and actions; form values can
  be read-only. These do not expose all IDE settings or dynamically register
  arbitrary language associations.
- EndpointsProvider publishes routes to Search Everywhere, .http completion
  and Open in HTTP Client. It is not a generic query API for every other
  framework's routes and does not itself add an Endpoints tool window.
- Project.exists/stat and read_file NotFound replace older missing-file guesses.
  host.project_of locates a document's project. Scoped byte reads cap at 16 MiB;
  reading JAR bytes still requires locating and parsing the archive.
- Commit drafts/messages and before-commit checks are available. They do not
  imply native changelist decorators or an after-successful-commit event.
- Plugin translations/locale and file-template exact filenames/prompted variables
  are available. net hosts and process programs remain manifest declarations;
  clipboard reads, native database/debugger/project models, language injection,
  general native outline/structure contributions and arbitrary decorations
  remain absent from the reviewed API.

## Evidence

Pinned [plugin documentation](https://github.com/l0ng-ai/openidea/blob/0bfd4fa3ccbd759b0482e4a7b7e49350774f6e1d/plugins/README.md),
[WIT](https://github.com/l0ng-ai/openidea/tree/0bfd4fa3ccbd759b0482e4a7b7e49350774f6e1d/wit),
[index graph contract](https://github.com/l0ng-ai/openidea/blob/0bfd4fa3ccbd759b0482e4a7b7e49350774f6e1d/docs/plugins/indexing.md).
Maintainer rationale: [semantic queries #1944](https://github.com/l0ng-ai/openidea/issues/1944),
[injection #1956](https://github.com/l0ng-ai/openidea/issues/1956),
[partial marker acceptance #1951](https://github.com/l0ng-ai/openidea/issues/1951).
MCP [#1752](https://github.com/l0ng-ai/openidea/pull/1752) and the Endpoints window
[#883](https://github.com/l0ng-ai/openidea/pull/883) were open at review time and
are not counted as shipped APIs.
