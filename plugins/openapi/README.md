# OpenAPI

One shared UI-kit window contains the OpenAPI, Changes, Request / Response tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

Editing support for OpenAPI 3 and Swagger 2 files in YAML and JSON, with a
Swagger UI preview. A file is an OpenAPI document when it has a top-level `openapi` or
`swagger` key with a value. Every other YAML or JSON file gets no completion, no
actions and no problems, and any problems left from an earlier edit are cleared
when it is opened or saved.

This is an independent implementation, not affiliated with JetBrains or
SmartBear.

## `$ref` completion and definition

Inside a `$ref` value, completion offers the reusable components of the document
(`#/components/schemas/Pet`, or `#/definitions/Pet` in Swagger 2); in a file
without an `openapi`/`swagger` key, its top-level keys. A `$ref` still being
typed completes, with or without a closing quote, and only the value is
replaced. Slashes and tildes are escaped (`a/b` becomes `a~1b`). Go to
Definition also resolves pointers into `paths`
(`#/paths/~1pets/get`). Canonical `~0`/`~1` object-key pointers are supported; percent-encoded and array target pointers remain quiet.

For a relative file (`../common.yaml#/Pet`) the pointers of that file are
offered, resolved against the spec's directory. `https://` references are left
alone. When the value has no `#` yet, the YAML, YML and JSON files of the
project are offered as relative paths (`./schemas/pet.yaml`, `../common/pet.json`).
That list is read when a YAML or JSON file is opened or saved (at most 500 files
per extension, hidden and ignored paths excluded by the host), so a file created
since then shows up after the next open or save.

Go to Definition on a `$ref` opens the target, in the same file or the
referenced one.

## Problems

Checked when a file is opened or saved.

| Code | Severity | Message |
|---|---|---|
| `unresolved-ref` | error | ``Cannot find `<ref>` `` for a pointer that doesn't exist, ``Cannot find file `<file>` `` for a missing file |
| `duplicate-operation-id` | error | ``operationId `<id>` is used by more than one operation`` |
| `path-parameter` | error | ``Path parameter `<name>` is not declared for <METHOD> <path>`` |
| `path-parameter` | warning | ``Path parameter `<name>` is not in the path <path>`` |

A YAML or JSON file that has a root `paths` object of `/...` items with HTTP methods
but no `openapi` or `swagger` key (a spec fragment) gets an info hint,
`missing-openapi-root`, on `paths`. The setting `checkSpecCandidates` switches it
off, and files over 1 MiB are skipped.

A `$ref` into a file that exists but can't be read (a directory, too large, not
UTF-8) is not flagged. JSON files are checked like YAML ones, except a JSON file whose root
is an array. Path parameters are skipped when the parameter list can't be
read reliably: flow style, a `$ref` that isn't local, or a local `$ref` that
can't be resolved or lacks `name` or `in`.

## Schema modeline

When the file has no `# yaml-language-server: $schema=...` line, the code action
"Add the OpenAPI schema for the YAML language server" inserts it at the top, for
the official schema of the file's version (2.0, 3.0, 3.1 or 3.2). The built-in
YAML language server then validates the file.

## Endpoints view

The OpenAPI Endpoints view (right side) lists `METHOD /path` for every OpenAPI
YAML or JSON file (up to 1 MiB) of the current window's project, with the operationId and file name as
detail. Activating an entry opens the operation. Refresh re-scans the project.

It is the shared route list: a flat list: the generic native endpoint icon, the path, and grey text with the operationId after it. *Filter* narrows the list to one tag (the status line says which), *Clear* restores it, and *Flat/Tree* groups the rows by tag. Row actions: *Copy as curl* and *Create HTTP Request File* (writes `http/<verb>-<path>.http`, needs `project.write`; without it the request is copied). Limits: the grey text follows the path rather than sitting in a column, there is no request panel beside the list, and no gutter icon on the handler; these wait on upstream OpenIDEA requests. The HTTP file is written beside the spec.

## Preview

Swagger UI 5.34.1 beside the text. It opens automatically for `openapi.*`,
`swagger.*`, `*.openapi.*` and `*.swagger.*` with a `.yaml`, `.yml` or `.json`
extension. For any other YAML or JSON file, use Open With and pick API Preview.
The preview follows edits as you type. A Reload button in the preview asks for the
referenced files again (not verified in a live IDE).

- Relative `$ref`s to other files: implemented, not yet verified in a live IDE
  (an `openidea-plugin link` check is pending). The plugin reads the files the
  open document references and hands them to the page, which serves them to
  Swagger UI. Only one level is loaded (files referenced from those files are
  not), at most 50 files and 512 KiB in total; larger ones are skipped. The
  files are re-sent only when they changed.
- A Redoc button switches the preview to Redoc 2.5.4 (and back). Redoc loads on first use and has its search
  turned off (it needs a worker). Not verified in a live IDE.
- There is no Try it out: the frame has no network.
- The dark theme is a colour inversion of the light one.

## Write Requests for the HTTP Client

The command "OpenAPI: Write Requests for the HTTP Client" writes one request per
operation of the focused file to `<name>.http` beside it and opens it (it asks before
replacing an existing file). Writing needs the optional `project.write` permission; without it the
requests are copied to the clipboard. The base URL comes from
`servers[0].url` (OpenAPI 3) or `schemes`, `host` and `basePath` (Swagger 2), and
defaults to `http://localhost`. Path parameters become `{{name}}` variables.
Without both permissions the plugin shows a warning instead.

## Live templates

In an OpenAPI or Swagger file, completion offers four snippets: `oas3` (an OpenAPI 3.1 root with
info and a GET path), `opnp` (a path item), `endp` (an operation with parameters and a response)
and `swag` (a Swagger 2.0 root). They are plain completion items, offered anywhere in the file, and
the tab stops are not verified in a live IDE.

## Keyword and enum completion

In an OpenAPI 3 file (YAML in block style, or JSON), completion on a blank or partly typed key offers the keys of the
object at that indentation (`info`, `paths`, a path item's methods, an operation's `summary`, `parameters`,
`responses` with common status codes, schema keywords, and so on), taken from a fixed tree of the OpenAPI
Specification. After `key: ` it offers the enum values of `in`, `style`, `type`, `format`, `scheme`, the
booleans, and so on (`null` as a type only for 3.1). It is quiet in Swagger 2, flow style, inside a
multi-line scalar, and under keys it doesn't know (such as `x-` extensions, `example`, `callbacks`).
Keys already present in the object are offered again. The skeletons stay available next to the keys.
In JSON the keys come from the objects around the caret (arrays are transparent); typing inside quotes
completes the name only, outside quotes it inserts `"name": `, and enum values are inserted quoted.

## Extract, bundle and copy

- Code action "Extract inline schema to components/schemas", with the caret on the line of a `schema:`,
  `items:`, `additionalProperties:` or a property name whose value is an inline block mapping (or a JSON
  object). The block moves to `components.schemas` (created if missing, with the comma handled in JSON) under
  a generated name (`ListPetsResponse`, `Owner`; a number is appended when taken) and is replaced by a
  `$ref`. Rename the new schema afterwards.
- "OpenAPI: Write Self-Contained Spec" writes `<name>.bundled.<ext>` beside the file (clipboard without the
  optional `project.write` permission) with every `$ref` to another file replaced by a pointer
  into `components` and the referenced objects copied there (including what they reference, recursively;
  a recursive reference is copied once). A YAML spec bundles YAML files (target a block mapping), a JSON spec
  bundles JSON files; mixing them is refused. An object that is not under `/components/<section>/` lands in `schemas`.
- "OpenAPI: Copy Reference to the Key at the Caret" copies `$ref: '#/components/schemas/Pet'` (or the JSON form)
  for the key on the caret's line.
- "OpenAPI: Compare With Last Revision" compares the open file with its version in `HEAD` in the
  OpenAPI Changes view. It needs the optional `git` permission and a committed file.

## Markers, icons and detection

Every operation whose body has a local `$ref` that resolves gets a gutter marker that opens the first such
target. `openapi.*` and `swagger.*` files (and `*.openapi.*`, `*.swagger.*`) get the OpenAPI icon. The setting
`specGlobs` (comma-separated, for example `api/*.yaml, *.contract.json`; a pattern without `/` matches the file
name, with `/` the end of the path) makes completion, go to definition, problems and the endpoints view work
in more files, including fragments without an `openapi` key (in a fragment, unresolved local `#/` pointers
are not reported, since they may point into the root file, and the "missing root key" hint is off). The
preview is not affected: its file patterns are fixed in the manifest, so use Open With and pick API Preview
for other names. The icon and
marker behaviour is not verified in a live IDE.

## Templates

New › Swagger 2.0 (YAML) and Swagger 2.0 (JSON) create a minimal Swagger 2.0 document too. New › OpenAPI 3.1 (YAML), OpenAPI 3.1 (JSON) and AsyncAPI 3.0 (YAML) create a minimal valid document:
`info` with title (`$NAME$`) and version, and for OpenAPI a `GET /health` with a 200 response; for AsyncAPI
one channel, one operation and one message. AsyncAPI files are not OpenAPI documents, so the plugin's checks and
preview leave them alone.

## Licences

The plugin code is MIT. It bundles Swagger UI (Apache-2.0) and Redoc (MIT), whose licence and
notice files are in `web/vendor/`; see `NOTICE.md`.

## Limits

- JSON is read by a small scanner: string escapes other than `\"`, `\/` and
  `\\` in keys are not decoded, and a document that stops being valid JSON is
  checked only up to that point.
- Flow-style mappings are only seen for `$ref`.
- Problems appear on open and save, not while typing.
- Problems caused by another file's content refresh only when the spec itself
  is opened or saved.
- A component name containing dots can make a shorter wrong pointer resolve
  (`#/components/schemas/com` when `com.acme.Error` exists).
- No indexed Rename/Find Usages or structure view; bounded source hover is described below.
- The endpoints view re-scans the project on each render.
- The schema modeline is YAML only; JSON specs get no schema validation. Keyword completion is OpenAPI 3 only.
- The endpoints view reads a JSON file in full before the 1 MiB cap applies.
- The preview isn't opened automatically for other file names, and loads only
  the files the open document references directly.

```sh
cargo test                       # logic, against the SDK's mock host
openidea-plugin build            # → dist/plugin.wasm, validated like an install
openidea-plugin link             # load this folder in the IDE (Settings › Plugins)
```

## Live JSON structure diagnostics (P13.3 subset)

API 1.3 diagnostics now check the supplied unsaved JSON document without project
access, discovery, network requests or external reference reads. This provider is
separate from the existing open/save semantic checks above (references, operation
ids and path parameters); its `openapi-structure-*` codes do not duplicate those
rules. Those legacy semantic checks retain their existing cross-file behavior.
The JSON language server keeps generic JSON syntax/schema ownership: malformed
JSON, unrelated documents, ambiguous root version fields and unsupported versions
produce no new structure diagnostics. No JSON language-server association is
changed.

The bounded subset recognizes `swagger: "2.0"`, `openapi: "3.0.<patch>"` and
`openapi: "3.1.<patch>"`. It validates:

- Root info and title/version strings; required paths in 2.0/3.0; at least one
  of paths/components/webhooks in 3.1; selected root object/array shapes.
- Path/operation object shapes, operationId strings, responses object presence
  in 2.0/3.0 and nonempty responses when supplied, response descriptions, content
  media object shapes, and request-body content/required fields.
- Parameter array/object shape, name/in, version-specific locations, required
  path parameters, boolean required flags, and schema/content conflicts.
- Reusable responses/parameters (Swagger 2 and OpenAPI 3), request bodies and
  headers (OpenAPI 3), using the same checks as inline objects. Swagger vendor
  extensions and reference siblings are not treated as concrete definitions.
- Root/path/operation server objects, required URL and variable defaults, string
  enum choices; OpenAPI 3.1 additionally enforces nonempty choices/default membership.
  The corresponding 3.0 SHOULD advice is not reported as a schema error.
- Component/definition and inline content/parameter schema object shapes,
  supported type forms, array items in 2.0/3.0, properties and selected composition
  shapes. Boolean schemas and type unions/null are accepted in 3.1; boolean
  additionalProperties is accepted in older versions too. Selected schema keyword
  shapes include required property-name arrays, enum arrays, numeric/size bounds
  and scalar annotations; exclusive bounds follow their version-specific types.

Reference targets and schema-reference siblings are not interpreted. Custom 3.1
JSON Schema dialects are not assumed; schema checks are skipped under unknown
root jsonSchemaDialect or local $schema. This is not full JSON Schema validation:
cross-keyword constraint semantics, format validation, unknown fields, remaining reusable component kinds,
security schemes, callbacks, full reference/dialect resolution and other schema
keywords remain outside this subset. OpenAPI 3.2 is not validated by these new
rules. A clean result means only that this subset found no issue.

Requests are limited to 512 KiB, 8192 JSON nodes, depth 48, and 256 diagnostics.
Documents exceeding a bound are skipped. Locations use existing source-key spans
and UTF-16 conversion; missing fields and ambiguous flattened paths use the
nearest unambiguous parent key (or version key), rather than inventing a range.
Generic syntax failures are left to the JSON language server.

Contracts were compared with the original 14394
`SpecificationJsonSchemaFileProvider` (version recognition and embedded schema
selection) and bundled `swagger_2_0.json`, `openapi_3_0_0.json`, and
`openapi_3_1_0.json`. Authoritative version references:
[Swagger 2.0](https://spec.openapis.org/oas/v2.0.html),
[OpenAPI 3.0.4](https://spec.openapis.org/oas/v3.0.4.html), and
[OpenAPI 3.1.1](https://spec.openapis.org/oas/v3.1.1.html).
The original delegates full validation to its JSON Schema service; that service
and its complete embedded schemas are not reproduced here.

## Source hover and component ownership (P13.2 partial)

Hover on JSON component declarations and operation method/operationId/summary
fields shows facts declared in the current unsaved document: title, summary,
description, scalar type/format, operation id, deprecated flag and boolean schema
value where present. A local JSON fragment reference to a supported component
shows that declaration's facts. Nested local schema pointers through object-key
`properties`, `items`, `additionalProperties` and `not` also expose proven target
facts; Swagger definitions and OpenAPI 3.1 boolean schemas follow their version.
Every pointer ancestor must have one actual source owner; duplicate roots, duplicate
ancestor keys, malformed/percent-encoded pointers and flattened-key impostors stay quiet.
Array-index paths and arbitrary extension-object targets are not inferred as schemas.
These documentation targets do not create replaceable symbols. Supported roots are Swagger 2 and OpenAPI 3.0/3.1;
Swagger definitions/parameters/responses/securityDefinitions and OpenAPI component
categories are recognized. Descriptions are displayed as escaped text, not
trusted Markdown/HTML. No project/filesystem/network reads or target fetching
occur. External and unresolved references, unknown versions, malformed documents,
YAML and ambiguous flattened source paths remain quiet for this hover provider.
Existing navigation still handles its previously supported forms.

Hover is limited to 512 KiB of source, 8192 nodes/source keys, depth 48 and 8 KiB
of output, with individual fact strings bounded. Only source declarations are
reported; reference chains, external schemas, inherited/merged schema semantics,
JSON Schema evaluation and runtime request behavior are not inferred.

Indexed usages and automatic Rename remain unregistered. Exact local token
spelling alone does not prove complete cross-file/encoded-reference ownership.
[The component anchor audit](ANCHOR_AUDIT.md) records original rename/documentation
contracts, the host's nonempty UTF-16 range requirement and the remaining feasible
project owner/reference-model work. This is partial P13.2 delivery, not a platform
blocker or complete refactoring parity claim.

The original API 1.3 redirect preflight remains recorded in
[the execution audit](TRY_IT_OUT_AUDIT.md). Released API 1.3 hosts retain read-only
request preparation/export; guarded execution on supported API 1.4 hosts is
implemented below. The historical preflight does not describe the current command
or capability declarations.

### Reviewed local requests on API 1.4 hosts

`OpenAPI: Try Local GET/HEAD Request…` prepares a read-only **OpenAPI Local Request / Response** preview. Preparation never sends. The native **Send once** button checks the fresh project/editor, exact source text/version and single-use 120-second ticket before sending, without a subsequent modal. Switching source/project or editing the specification invalidates the preview; responses remain scoped to the original source. The current package requires API 1.4; runtime context checks still refuse execution before any prompt or network operation on an unsupported or unknown host. Execution is enabled only from native activation Context API 1.4 or later in major version 1, whose host checks redirect grants before connection. The SDK and minimum engine are 1.4.

This bounded subset supports JSON OpenAPI 3.0/3.1 (128 KiB maximum), one literal `127.0.0.1` server with explicit port, GET/HEAD and declared scalar path/query values. It requires the optional `net:127.0.0.1` grant and refuses additional network grants. Authentication/security requirements, header/cookie parameters, request bodies, arbitrary hosts, referenced/variable servers and Swagger 2/YAML execution are unsupported. No browser network, CLI or secret-store values are used.

The exact preview states that API 1.4 may follow up to ten redirects within granted `127.0.0.1`, including other ports/paths; other hosts and HTTPS-to-HTTP downgrades are denied. Host proxy/TLS settings still apply. Requests time out after five seconds. The host transfers/buffers up to 8 MiB; this plugin subsequently accepts at most 256 KiB and displays at most 64 KiB as read-only plain text, omitting response headers. The acceptance bound is not a network transfer bound. Deterministic tests cover cancellation, fresh Send ownership, stale/replayed tickets and bounded responses; native UI acceptance remains separate.

After a valid local **Send** fails, the request/response view retains only a bounded
constant status: a typed host permission denial, unsupported request/response, or
generic incomplete/failed request. Raw host errors, proxy data, headers and a prior
successful body are not shown in this status. The outcome belongs to the exact
project/editor/source text and version; a fresh render hides it after drift. A new
preparation or activation clears it. Unknown/replayed controls do not overwrite an
existing valid preview or outcome, and no retry, prompt or request is automatic.


### Captured comparison ownership and packaged refresh

Compare commands now invalidate an already mounted **OpenAPI Changes** page with
an empty `ui-kit-refresh` notification; each frame renders its own current model.
The snapshot belongs to its open project, innermost document owner and exact
active revision text/version. Missing/denied/foreign roots, nested ownership
changes and an edited/different source replace rows with a safe empty state.
Editor-focus events request a re-render; observed source save/deletion/change
retires the capture. Rendering reads only current editor/ownership metadata,
never scans files, runs Git or fetches HTTP. The captured base remains historical
until recomputation or an observed file event; this does not prove unsaved edits
in other, inactive buffers are already reflected. Navigation to a local base
also rechecks exact base text before opening its captured location.

Inputs are capped at1MiB each and the visible cache at1,000 changes with an explicit
partial label. Activating a row requires exact cached membership/current owner
and source; arbitrary location keys are refused. Historical HEAD removal rows do
not fabricate a working-file location. This is comparison transport/ownership
hardening, not new semantic diff/visual parity or native acceptance. Try Local
GET/HEAD behavior and its source file are unchanged.

### Physical sibling references

Go to Definition and `$ref` completion read relative local sibling files only
within the same innermost open project. Target files and their current editor text
must fit the 1 MiB scan bound. Absolute/URI/percent/query aliases, unavailable or
foreign-project targets, and documents with `$id` rebasing are withheld. Unsaved
target text wins when supported by the host. This source-navigation subset does
not establish complete usages or automatic Rename ownership. HTTP execution gates
are unchanged; navigation does not fetch remote references.

Cross-file reference diagnostics share this ownership boundary and stay quiet
when a target is unknown or unsupported; known same-project missing files still
report an error. Discovered file suggestions are filtered by project owner and
revalidated against current project membership and file metadata.

JSON pointer definitions additionally require a unique actual object-key chain
in valid target JSON (512 KiB, 8,192 nodes, 48 levels). Duplicate/dotted impostors,
encoded fragments, malformed tilde escapes, array-owned pointers and `$id`
rebasing are withheld. Literal tilde/slash object names remain navigable through
canonical JSON Pointer escaping with exact UTF16 source key ranges. Whole-file locations keep their existing behavior; YAML pointer checks are described below.

YAML pointer definitions now validate one bounded document and prove each unique
actual string-key owner against its exact source span. Simple plain/quoted block
keys support canonical slash/tilde escaping. Duplicates, aliases/anchors/tags,
merge keys, `$id` rebasing, malformed/multiple documents, transformed/complex keys,
percent fragments, arrays and unsupported flow child spans stay quiet. Limits are
512 KiB, 8,192 parser nodes and 48 levels. Same-project unsaved YAML targets work
from JSON or YAML references; this does not add indexed Rename. Current-text local YAML schema-reference hover is described below.

### Current-text YAML schema reference hover

Hover over a literal local `$ref` to a recognized Swagger 2.0 definition or
OpenAPI 3.0/3.1 component schema to see bounded declared type/title/description and
related source fields. Nested `properties`, `items`, `additionalProperties` and
`not` schema pointers use the same ownership proof. Boolean schemas require 3.1.
Version fields must be supported strings. Both the source reference key chain and
target key chain must have unique exact raw source locations in the strict YAML
model. Escaped names retain canonical tilde/slash pointers and exact UTF16
reference ranges. Facts come from current unsaved text, without project access,
filesystem reads, reference fetching or a Rename graph.

The same 512 KiB/8,192 parser-node/48-depth source limits apply. Markdown and field
lengths use the existing JSON hover bounds. External references, array-owned
sources/targets, aliases/tags/anchors/merges, base-URI declarations, duplicate or
flattened owners, transformed source scalars, unsupported flow spans and encoded
pointers stay quiet. This is local schema-reference documentation; direct YAML
component-key/operation hover and cross-file cached documentation are separate.

### Known missing references versus unknown ownership

The existing open/save reference checker now distinguishes Present, Missing and
Unknown physical source pointers. It builds each bounded local or referenced
JSON/YAML model once and proves the actual source `$ref` mapping chain as well as
every existing target owner. Ordinary valid missing targets keep their exact
reference diagnostic. Ambiguous duplicate/flattened owners, malformed or rebased
models, unsupported encoded/array/transformed/flow-owned source forms and invalid
pointer syntax stay quiet instead of manufacturing absence. JSON escaped string
keys and canonical tilde/slash names remain supported. This is source presence,
not runtime resolution or a complete usages/Rename set.

Each check processes at most 1,024 scanned refs, 32 unique file reads and 4 MiB of
captured target text, with a conservative 16 MiB retained-model accounting limit.
Each proof has 512 KiB/8,192-node/48-depth bounds and 4 MiB of flattened-path storage.
Skipped, over-budget or unreadable targets are Unknown. A proved same-owner
missing file retains the earlier missing-file diagnostic. Path-parameter ownership is addressed below. Marker ownership is addressed below. Bundling, structure diagnostics and live
provider scope remain distinct;
this addition does not requalify those consumers or provide a native latency claim.

Path-parameter checks now share that strict source proof instead of reading a
flattened parameter index. A real path-item/operation owner, an actual parameter
array and exact owned name/in/reference fields must establish each fact. Canonical
local reusable parameter references are supported; external/encoded/chained refs,
path-item refs, incomplete or duplicated list entries, base/duplicate/dotted
impostors and unproved transformed/flow fields keep the context Unknown. Lists
are limited to 1,024 items and fields to 4 KiB. Ordinary known missing/unused parameter
messages retain their source ranges. Operation-level `(name, in)` entries override
matching path-item entries, so an overridden declaration does not get a second
warning. No cross-file reads, runtime execution or Rename graph are added here.

Operation gutter navigation now requires an actual operation owner and a real
current-text local reference with an exact source-backed target key. It chooses
the first supported reference in source order and retains nonempty UTF16 ranges.
A narrow singleton YAML flow reference is supported when its strict semantic map
and exact parent/value span prove the literal ref; general flow child ownership
remains separate. Duplicate/dotted/base/alias/encoded or unproved source contexts,
external refs and unresolved targets produce no marker. At most 256 markers and
4 MiB of reference-owner metadata are retained within the strict source bounds.
No file/project/network calls or automatic Rename anchors are added.

Duplicate-operationId diagnostics now count only actual operation owners and
owned string fields from that same strict source proof. Duplicate/dotted owners,
invalid numeric fields, uncomposed path-item refs and unproved transformed/base
contexts cannot create duplicate facts. Genuine declared duplicates retain exact
UTF16 value ranges, including escaped JSON and astral text. IDs are bounded to
4KiB and returned duplicate diagnostics to 256. Current source changes remove old
facts on the next check; no cross-file reads or global uniqueness proof are added.

JSON structure diagnostics now use actual tree-owned source locations. Duplicate
version or owner keys and ambiguous dotted locations cannot fall back to an
unproved parent span. Independent owned branches keep their errors; real dotted
schema names, JSON key escapes, primitive array values and empty-string tokens
retain nonempty UTF16 ranges. Base-URI metadata does not disable structural type
facts, since this path does not resolve references. The earlier ambiguous-parent
fallback is intentionally replaced by quiet behavior for that ambiguous location.

Strict JSON structure/pointer/hover proof callers now reserve key and aggregate
path budgets before scanner allocation and reject incomplete/exhausted scans as
Unknown. This prevents long prefixes, including discarded duplicate JSON branches,
from allocating a huge flattened model before its 4 MiB check. Existing source/node/
depth bounds remain; JSON definition/marker discovery runs behind that proof guard.
The legacy scanner APIs remain unchanged; provider completion adds the budget-only
preflight described below, preserving normal partial typing. Generation is unchanged; this
is not a claim of global parser or native latency/quota hardening.

Strict YAML proof paths now use an OpenAPI-local bounded adapter of our MIT
block-style scanner. It preserves ordinary/partial lexical spans while reserving
key/path budgets before joining strings; key, path or lexical-depth exhaustion
returns no partial model. Definition/hover/checker/marker proofs adopt it, and
provider entry/sibling-read guards refuse amplified metadata before legacy scans.
The shared scanner and generator/runtime workflows are unchanged.

Completion still accepts ordinary incomplete JSON/YAML. Its partial JSON preflight
separates syntactic incompleteness from budget exhaustion; exhausted models and
sibling metadata stay Unknown. The guards use 8,192 keys, 4 MiB aggregate flattened
paths and 48 YAML lexical levels within their existing source-byte limits. A
controlled 73,795-byte YAML fixture produced 12,553,612 path bytes in the old scanner;
the new guard rejects before crossing 4 MiB. This is a measured host-test allocation
shape, not an observed native OOM or a native latency/quota guarantee.

Definition selects actual literal `$ref` fields from a complete bounded source
model, including supported array source entries and singleton YAML flow maps.
Description text and dotted-key impostors do not establish reference fields.
External pointer completion filters complete sibling content through actual
source-backed target ownership; incomplete current-file completion stays available.

For a complete source model supported by the ownership proof, current-document
reference completion also filters source fields and target pointers. A source
that cannot establish this complete model retains the existing partial typing
hints; these hints do not establish navigation, diagnostics or Rename ownership.

Empty YAML reference values and unquoted # prefixes also retain the existing
partial typing fallback, since their semantic value is null/comment text.

Copy Reference selects the key token at the full caret position in a supported
complete current OpenAPI/Swagger source. It uses actual mapping names and array
indices, escapes JSON Pointer tokens, then percent-encodes the UTF-8 URI fragment.
Literal dots/brackets are names rather than inferred nesting; literal percent
signs are encoded rather than reinterpreted as URI escapes. The copied JSON or
YAML snippet is serialized safely. Values, blank space, ambiguous owners,
unsupported source forms and pointers over 4096 bytes leave the clipboard alone.
Source proof keeps its 512 KiB/8192-node/48-depth/4 MiB path bounds. This static
command can construct array/encoded references; navigation retains its narrower
supported target subset. No complete usages or Rename graph is established.

## Unreleased follow-up

Use **OpenAPI: Find Component Occurrences in This Document** on a component key or local `$ref`. The picker includes the declaration and exact local references, including references into that component. External incoming references and global Rename are outside this command.
