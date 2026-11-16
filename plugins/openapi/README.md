# OpenAPI

Editing support for OpenAPI 3 and Swagger 2 files in YAML and JSON in OpenIDEA, with a Swagger UI preview.
MIT; see `LICENSE`.

One shared UI-kit window hosts the OpenAPI, Changes and Request / Response tabs; commands select their corresponding tab.

A file is an OpenAPI document when it has a top-level `openapi` or `swagger` key with a value. Every other YAML or JSON file gets no completion, no actions and no problems, and problems left from an earlier edit are cleared when it is opened or saved. The setting `specGlobs` (comma-separated, for example `api/*.yaml, *.contract.json`; a pattern without `/` matches the file name, with `/` the end of the path) makes completion, go to definition, problems and the endpoints view work in more files, including fragments without an `openapi` key (in a fragment, unresolved local `#/` pointers are not reported, since they may point into the root file, and the missing-root hint is off). The preview patterns are fixed in the manifest, so use Open With › API Preview for other names.

## `$ref` completion and navigation

Inside a `$ref` value, completion offers the reusable components of the document (`#/components/schemas/Pet`, or `#/definitions/Pet` in Swagger 2); in a file without an `openapi`/`swagger` key, its top-level keys. A `$ref` still being typed completes, with or without a closing quote, and only the value is replaced. Slashes and tildes are escaped (`a/b` becomes `a~1b`). Go to Definition also resolves pointers into `paths` (`#/paths/~1pets/get`); canonical `~0`/`~1` object-key pointers are supported, percent-encoded and array target pointers stay quiet.

For a relative file (`../common.yaml#/Pet`) the pointers of that file are offered, resolved against the spec's directory; targets must be physical siblings within the same innermost open project (unsaved target text wins when supported), within the 1 MiB scan bound. Absolute/URI/percent/query aliases, unavailable or foreign-project targets and documents with `$id` rebasing are withheld. `https://` references are left alone. When the value has no `#` yet, the YAML, YML and JSON files of the project are offered as relative paths, read when a YAML or JSON file is opened or saved (at most 500 files per extension, hidden and ignored paths excluded by the host), so a new file shows up after the next open or save. Go to Definition on a `$ref` opens the target, in the same file or the referenced one.

JSON pointer definitions require a unique actual object-key chain in valid target JSON (512 KiB, 8,192 nodes, 48 levels); duplicate/dotted impostors, encoded fragments, malformed tilde escapes, array-owned pointers and `$id` rebasing are withheld. YAML pointer definitions validate one bounded document (same limits) and prove each unique string-key owner against its exact source span: plain/quoted block keys with canonical slash/tilde escaping work; duplicates, aliases/anchors/tags, merge keys, malformed/multiple documents, transformed/complex keys, percent fragments and arrays stay quiet. Literal tilde/slash object names remain navigable. Definition selects actual literal `$ref` fields (including supported array entries and singleton YAML flow maps); description text and dotted-key impostors do not establish references. Completion still accepts ordinary incomplete JSON/YAML with partial typing hints; models that exhaust the scan budgets stay unknown.

## Problems

Checked when a file is opened or saved (live diagnostics below are separate).

| Code | Severity | Message |
|---|---|---|
| `unresolved-ref` | error | ``Cannot find `<ref>` `` for a pointer that doesn't exist, ``Cannot find file `<file>` `` for a missing file |
| `duplicate-operation-id` | error | ``operationId `<id>` is used by more than one operation`` |
| `path-parameter` | error | ``Path parameter `<name>` is not declared for <METHOD> <path>`` |
| `path-parameter` | warning | ``Path parameter `<name>` is not in the path <path>`` |

A YAML or JSON file that has a root `paths` object of `/...` items with HTTP methods but no `openapi` or `swagger` key (a spec fragment) gets an info hint, `missing-openapi-root`, on `paths`; the setting `checkSpecCandidates` switches it off, and files over 1 MiB are skipped. A `$ref` into a file that exists but can't be read (a directory, too large, not UTF-8) is not flagged. JSON files are checked like YAML ones, except a JSON file whose root is an array. Path parameters are skipped when the parameter list can't be read reliably: flow style, a non-local `$ref`, or a local `$ref` that can't be resolved or lacks `name` or `in`.

The reference checker distinguishes Present, Missing and Unknown physical source pointers: ordinary valid missing targets keep their exact diagnostic and proved same-owner missing files keep the missing-file error, while ambiguous duplicate/flattened owners, malformed or rebased models, unsupported encoded/array/transformed/flow forms and invalid pointer syntax stay quiet. Each check processes at most 1,024 scanned refs, 32 unique file reads, 4 MiB of captured target text, a 16 MiB retained-model accounting limit, 512 KiB / 8,192-node / 48-depth proofs and 4 MiB of flattened-path storage; skipped, over-budget or unreadable targets are Unknown. Path-parameter checks share this strict source proof (real parameter arrays, owned name/in/reference fields, canonical local reusable parameter references, operation-level `(name, in)` overriding path-item entries, lists limited to 1,024 items and fields to 4 KiB). Duplicate-operationId diagnostics count only actual operation owners and owned string fields, bounded to 4 KiB IDs and 256 diagnostics, with exact UTF-16 value ranges.

Every operation whose body has a local `$ref` that resolves gets a gutter marker (packaged icon) that opens the first such target; markers require an actual operation owner and an exact source-backed target, with a narrow singleton YAML flow reference supported. At most 256 markers are retained.

## Live JSON structure diagnostics

API 1.3 diagnostics check the unsaved JSON document without project access, discovery, network or external reference reads; the `openapi-structure-*` codes don't duplicate the semantic open/save checks. Malformed JSON, unrelated documents, ambiguous root version fields and unsupported versions produce no structure diagnostics (generic JSON syntax stays with the JSON language server).

Recognizes `swagger: "2.0"`, `openapi: "3.0.<patch>"` and `openapi: "3.1.<patch>"` and validates: root info/title/version; required paths in 2.0/3.0 and at least one of paths/components/webhooks in 3.1; path/operation object shapes, operationId strings, response presence and descriptions, content media objects and request-body content/required fields; parameters (shape, `name`/`in`, version-specific locations, required path parameters, boolean flags, schema/content conflicts); reusable responses/parameters (2 and 3), request bodies and headers (3) with the same checks as inline objects; server objects (required URL and variable defaults, string enums; 3.1 additionally nonempty choices/default membership, with 3.0 SHOULD advice not reported); and schema object shapes, supported type forms, array items in 2.0/3.0, properties, selected composition and schema keywords (required name arrays, enum arrays, numeric/size bounds, scalar annotations, version-specific exclusive bounds; boolean schemas and type unions/null in 3.1). Reference targets and schema-reference siblings are not interpreted; custom 3.1 JSON Schema dialects and unknown `jsonSchemaDialect`/local `$schema` skip the schema checks. OpenAPI 3.2 is not validated by these rules, and a clean result means only that this subset found no issue.

Requests are limited to 512 KiB, 8,192 JSON nodes, depth 48 and 256 diagnostics; documents over a bound are skipped. Locations use actual tree-owned source-key spans with UTF-16 conversion; missing fields and ambiguous flattened paths use the nearest unambiguous parent key, and ambiguous dotted locations stay quiet rather than falling back to an unproved span.

## Hover

Hover on JSON component declarations and operation method/operationId/summary fields shows facts declared in the current unsaved document: title, summary, description, scalar type/format, operation id, deprecated flag and boolean schema value. Local JSON fragment references and nested schema pointers through `properties`, `items`, `additionalProperties` and `not` expose the proven target's facts; Swagger definitions and 3.1 boolean schemas follow their version. Duplicate roots, duplicate ancestor keys, malformed/percent-encoded pointers, flattened-key impostors, array-index paths, extension-object targets, external references, YAML and unknown versions stay quiet. Only source declarations are reported — reference chains, inherited/merged semantics and JSON Schema evaluation are not inferred.

Hover over a literal local `$ref` to a recognized Swagger 2.0 definition or OpenAPI 3.0/3.1 component schema in YAML works the same, with the same ownership proof for both source and target key chains, canonical tilde/slash pointers and exact UTF-16 ranges. Escaped names and 3.1 boolean schemas are supported; aliases/tags/anchors/merges, base-URI declarations, duplicate or flattened owners, transformed scalars, unsupported flow spans and encoded pointers stay quiet.

Hover is limited to 512 KiB of source, 8,192 nodes/source keys, depth 48, 8 KiB of output and bounded fact strings. Descriptions are displayed as escaped text, not trusted Markdown/HTML.

## Schema modeline

When the file has no `# yaml-language-server: $schema=...` line, the code action "Add the OpenAPI schema for the YAML language server" inserts it at the top, for the official schema of the file's version (2.0, 3.0, 3.1 or 3.2); the built-in YAML language server then validates the file. YAML only — JSON specs get no schema validation.

## Keyword, enum and snippet completion

In an OpenAPI 3 file (YAML in block style, or JSON), completion on a blank or partly typed key offers the keys of the object at that indentation (`info`, `paths`, a path item's methods, an operation's `summary`, `parameters`, `responses` with common status codes, schema keywords, and so on), taken from a fixed tree of the OpenAPI Specification. After `key: ` it offers the enum values of `in`, `style`, `type`, `format`, `scheme`, the booleans, and so on (`null` as a type only for 3.1). It is quiet in Swagger 2, flow style, inside a multi-line scalar, and under keys it doesn't know (such as `x-` extensions, `example`, `callbacks`). Keys already present are offered again. In JSON the keys come from the objects around the caret (arrays are transparent); typing inside quotes completes the name only, outside quotes it inserts `"name": `, and enum values are inserted quoted.

In an OpenAPI or Swagger file, completion also offers four snippets: `oas3` (an OpenAPI 3.1 root with info and a GET path), `opnp` (a path item), `endp` (an operation with parameters and a response) and `swag` (a Swagger 2.0 root), offered anywhere in the file.

## Endpoints view

The OpenAPI Endpoints view (right side) lists `METHOD /path` for every OpenAPI YAML or JSON file (up to 1 MiB) of the current window's project, with the operationId and file name as detail; activating an entry opens the operation. *Filter* narrows the list to one tag (the status line says which), *Clear* restores it, and *Flat/Tree* groups the rows by tag. Refresh re-scans the project (the view re-scans on each render). Row actions: *Copy as curl* and *Create HTTP Request File* (writes `http/<verb>-<path>.http` beside the spec, needs the optional `project.write`; without it the request is copied). The grey text follows the path rather than sitting in a column; there is no request panel beside the list and no gutter icon on the handler.

## Preview

Swagger UI 5.34.1 beside the text. It opens automatically for `openapi.*`, `swagger.*`, `*.openapi.*` and `*.swagger.*` with a `.yaml`, `.yml` or `.json` extension; for any other YAML or JSON file, use Open With and pick API Preview. The preview follows edits as you type, and a Reload button asks for the referenced files again.

- Relative `$ref`s to other files: the plugin reads the files the open document references and hands them to the page, which serves them to Swagger UI. Only one level is loaded, at most 50 files and 512 KiB in total; larger ones are skipped, and files are re-sent only when they changed.
- A Redoc button switches the preview to Redoc 2.5.4 (and back); Redoc loads on first use and has its search turned off.
- There is no Try it out: the frame has no network.
- The dark theme is a colour inversion of the light one.

## Requests for the HTTP client

The command "OpenAPI: Write Requests for the HTTP Client" writes one request per operation of the focused file to `<name>.http` beside it and opens it (it asks before replacing an existing file); it needs the optional `project.write` permission, without it the requests are copied to the clipboard. The base URL comes from `servers[0].url` (OpenAPI 3) or `schemes`, `host` and `basePath` (Swagger 2), and defaults to `http://localhost`. Path parameters become `{{name}}` variables. "OpenAPI: Compare With Last Revision" compares the open file with its `HEAD` version in the OpenAPI Changes view (optional `git` permission and a committed file).

**OpenAPI: Try Local GET/HEAD Request…** prepares a read-only Request / Response preview. Preparation never sends; the native **Send once** button checks the fresh project/editor, exact source text/version and a single-use 120-second ticket before sending. Switching source or editing the specification invalidates the preview. Supports JSON OpenAPI 3.0/3.1 (128 KiB max), one literal `127.0.0.1` server with explicit port (or a single literal `https://host` server with the optional `net:ask` grant, where the host asks before the first request to each host), GET/HEAD and declared scalar path/query values; requires the optional `net:127.0.0.1` grant and refuses additional network grants. Authentication, header/cookie parameters, request bodies, referenced/variable servers, Swagger 2 and YAML execution are unsupported. API 1.4 may follow up to ten redirects within granted hosts; other hosts and HTTPS-to-HTTP downgrades are denied; requests time out after five seconds; at most 256 KiB is accepted and 64 KiB displayed as read-only plain text, omitting headers. After a failed Send the view retains only a bounded constant status (permission denial, unsupported request/response or generic failure) scoped to the exact source/version; drift or a new preparation clears it, and nothing is sent automatically.

## Extract, bundle, copy and compare

- Code action "Extract inline schema to components/schemas", with the caret on the line of a `schema:`, `items:`, `additionalProperties:` or a property name whose value is an inline block mapping (or a JSON object). The block moves to `components.schemas` (created if missing, with the comma handled in JSON) under a generated name (`ListPetsResponse`, `Owner`; a number is appended when taken) and is replaced by a `$ref`.
- "OpenAPI: Write Self-Contained Spec" writes `<name>.bundled.<ext>` beside the file (clipboard without the optional `project.write` permission) with every `$ref` to another file replaced by a pointer into `components` and the referenced objects copied there (recursively; a recursive reference is copied once). A YAML spec bundles YAML files, a JSON spec bundles JSON files; mixing them is refused. An object that is not under `/components/<section>/` lands in `schemas`.
- "OpenAPI: Copy Reference to the Key at the Caret" copies `$ref: '#/components/schemas/Pet'` (or the JSON form) for the key at the caret's full position, using actual mapping names and array indices, JSON Pointer escaping and percent-encoded UTF-8 URI fragments; literal dots/brackets are names, literal percent signs are encoded. Values, blank space, ambiguous owners, unsupported forms and pointers over 4096 bytes leave the clipboard alone.
- "OpenAPI: Find Component Occurrences in This Document": a picker with the declaration and exact local references to a component key or local `$ref`, including references into that component. External incoming references are outside it.
- "OpenAPI: Find Component Usages in Project": follows exact physical component identity through local and relative-file `$ref` values, including admitted array references and descendants; the picker offers the declaration and source-backed incoming references with file/line navigation. Read failures, unsupported models and discovery limits are explicitly marked partial; remote/encoded identities are not treated as local paths.
- On a physical component key or `$ref`, **Rename component in visible project sources…** offers an explicit reviewed workspace edit: it checks every discovered JSON/YAML source, declaration collisions, exact reference spans, discovery limits and unchanged source before returning edits; unsupported/dynamic/discriminator ownership, security-scheme names, transformed/encoded names and unknown source models refuse the whole edit; new names use plain letters, digits, underscore and dash. Ignored files, external projects and runtime-generated references are outside it.
- "OpenAPI: Compare With Another Spec…" lists added, removed and changed operations, schemas and sections against a spec you pick, in the OpenAPI Changes view. "OpenAPI: Export Comparison as Markdown" exports the owned comparison summary to `<spec>.comparison.md` beside the compared specification (clipboard without `project.write`), with an overwrite review; editing either compared source invalidates the export until comparison is run again. The comparison snapshot belongs to its open project, innermost document owner and exact revision text/version; the visible cache is capped at 1,000 changes with an explicit partial label.
- "OpenAPI: Breaking-Change Report Against Another Spec…" picks an older spec in the project and writes a reviewed Markdown report beside the current file that classifies findings as breaking, non-breaking or info with old and new locations (local `$refs` only, bounded size and depth). It covers a documented subset of openapi-diff/oasdiff rules for paths, operations, parameters, request bodies, responses, enums, schema properties, constraints, nullability, oneOf/anyOf members, readOnly/writeOnly, deprecated and security requirements, with direction-aware severity; `allOf` is flattened, `additionalProperties` is not analysed, and the rule subset is not complete.

## Draft Spec from Source Mappings

"OpenAPI: Draft Spec from Source Mappings…" scans project Java/Kotlin sources (bounded files and bytes) for Spring MVC, JAX-RS/Quarkus and Micronaut controllers and, after a review prompt, writes a new `openapi-draft.yaml` (never overwritten) with literal paths, path/query parameters, request bodies, `@ResponseStatus` codes, unique operationIds and component schemas for plain records/POJOs. Constants, ant patterns, regex path variables and unreadable types are skipped or left as TODO placeholders and listed at the top of the file.

## Code generation

"OpenAPI: Generate Client, Server or Documentation…" integrates an independently installed, approved `swagger-codegen` stable 3.x executable (optional `project.write` and `process:swagger-codegen`); its installed `langs` catalog supplies the generator picker, and it never installs tools. Input supports Swagger 2.0/OpenAPI 3.0; 3.1/3.2 generation is not admitted. Generation runs in a new owned `.openidea/codegen-stage-*` directory, never in the final destination; the review shows source, generator, staging and final paths. After a successful matching terminal exit and expected CLI metadata, **Apply staged generated API directory…** returns a reviewed native workspace directory Rename, refusing an existing final destination; one Undo reverses the move. Stop a running generator in its keyed native terminal; **Forget Completed Generation Stage** clears tracking while leaving files intact; reloading the plugin loses in-memory tracking. `--skip-overwrite true` protects ordinary staged files. The approved external process has its own authority to resolve referenced inputs — review them first.

"OpenAPI: Generate With Options or Templates…" retrieves the selected generator's `config-help -l` option names/descriptions; choose known options and values, a project-owned custom template directory (or bundled defaults) and the default/Mustache/Handlebars template engine, reviewed before launch. Complex additional-property strings containing comma/equals are refused; a bounded project-relative generator JSON config (known scalar options only, no secrets, shown in the review) can be imported.

## Quick fixes

At the root `openapi`/`swagger` key, quick fixes add missing info/paths sections and (OpenAPI 3 only) components, preserving existing source (ambiguous duplicate owners don't offer additions). **Add path** (on `paths`) and **Add query parameter** (on a path item or operation, merging only into block-style or JSON lists) insert skeletons; add operation/response quick fixes also work in strict JSON specs (exact token scanner, single reviewed edit; duplicate keys, comments and non-strict JSON are refused). YAML quick fixes **Add get/post/put/delete operation** on a path key and **Add 200/400/404/default response** on an operation `responses` key insert a minimal skeleton at the exact end of an existing block mapping (flow style, anchors, multi-document, JSON and `$ref` path items are refused).

## Templates

New › OpenAPI 3.1 (YAML), OpenAPI 3.1 (JSON), Swagger 2.0 (YAML), Swagger 2.0 (JSON) and AsyncAPI 3.0 (YAML) create a minimal valid document: `info` with title (`$NAME$`) and version, and for OpenAPI a `GET /health` with a 200 response; for AsyncAPI one channel, one operation and one message. AsyncAPI files are not OpenAPI documents, so the checks and preview leave them alone.

## How it works

`openapi.*` and `swagger.*` files (and `*.openapi.*`, `*.swagger.*`) get the OpenAPI icon. The optional capabilities are `project.write`, `clipboard`, `process:git`, `storage`, `net:127.0.0.1`, `net:ask` and `process:swagger-codegen`; required are `project.read`, `editor.read` and `events`. Commands that write fall back to the clipboard without `project.write`.

Strict JSON/YAML proof paths reserve key, flattened-path (4 MiB aggregate) and lexical-depth (48 levels) budgets before building models, within the existing source-byte limits, so long or duplicate-prefixed documents cannot allocate oversized models; completion's partial preflight separates syntactic incompleteness from budget exhaustion and preserves ordinary partial typing. Sibling and cross-file checks share the project-ownership boundary above. Indexed usages and automatic Rename are not registered: exact local token spelling alone does not prove complete cross-file/encoded-reference ownership.

## Limits

- JSON is read by a small scanner: string escapes other than `\"`, `\/` and `\\` in keys are not decoded, and a document that stops being valid JSON is checked only up to that point.
- Flow-style mappings are only seen for `$ref`.
- Problems appear on open and save, not while typing; problems caused by another file's content refresh only when the spec itself is opened or saved.
- A component name containing dots can make a shorter wrong pointer resolve (`#/components/schemas/com` when `com.acme.Error` exists).
- No indexed Rename/Find Usages or structure view.
- The endpoints view re-scans the project on each render and reads a JSON file in full before the 1 MiB cap applies.
- The schema modeline is YAML only; keyword completion is OpenAPI 3 only; JSON structure diagnostics skip 3.2.
- The preview isn't opened automatically for other file names and loads only the files the open document references directly.

## Development

```sh
cargo test                       # logic, against the SDK's mock host
openidea-plugin build            # → dist/plugin.wasm, validated like an install
openidea-plugin link             # load this folder in the IDE (Settings › Plugins)
```
