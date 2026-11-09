# P13.2 component/reference ownership audit

Status: hover delivered for bounded current-document JSON; indexed usages and
automatic Rename remain pending feasible owner/reference-model work. No
DocumentIndexer export or manifest contribution is registered. Existing definition
and reference navigation remain unchanged.

Inspected original 14394 `SwObjectDefinitionReference`: it resolves a PSI definition
and implements rename by preserving the reference prefix through the last `/`
before replacing its terminal name. `SpecificationJsonSchemaFileProvider` selects
schema ownership through recognized specification type.
`SwDocumentationProvider` documents recognized URL fake elements using
`SwDocumentation` and source-fragment helpers, rather than treating every key as a
refactoring symbol. Cached original sources are under
`/private/tmp/port/14394/dec/intellij.swagger.core/`.

A plain JSON component name `Pet` and local reference `#/components/schemas/Pet`
have a possible exact same-spelling terminal token. That proves the spelling, but
not a closed rename set. Another file may contain `api.json#/components/schemas/Pet`,
a URI-encoded fragment may use `P%65t`, and YAML, JSON escapes or aliases may refer
to the same target. A document-only graph would knowingly omit those usages.
The host graph enables automatic Rename, so a subset of local anchors must not
be published as if those other references were safe to ignore.

Required ownership proof before publishing anchors:

- Canonical project/file/pointer ownership, including nested/sibling projects,
  relative target files and URI/JSON Pointer escaping rules.
- Bounded project reference discovery, failure/incomplete state and unsaved target
  changes; unknown external, dynamic and encoded ownership must suppress affected
  rename owners, not merely drop their references.
- Exact nonempty UTF-16 spans for supported same-spelling names; transformations
  need a separately scoped edit workflow, not unconditional graph replacement.
- Collision, duplicate-definition, cross-file and removal/reload fixtures; no
  structural/file tags, zero-width positions or synthetic definition anchors.

The pinned host `plugins/navigation.rs::valid_range` requires a strictly nonempty
range and in-bounds UTF-16 positions. A separate receipt extracts that function
verbatim and tests five range-shape cases. That checks the host's shape contract;
it does not prove semantic ownership or a complete reference graph. Hover tests
round-trip real component/reference spans, demonstrate transformed-reference
spelling differences and assert that indexing remains unregistered. No GUI
refactoring acceptance or complete automatic Rename parity is claimed.

## Current-document pointer documentation follow-up

Nested schema hover now follows only object-key schema positions (`properties`,
`items`, `additionalProperties`, `not`) beneath a local component/definition.
Each decoded JSON Pointer segment must re-encode identically and have a unique
actual source key at every ancestor. Duplicate version/owner keys, flattened
impostors, percent encoding, malformed tilde escapes, array indices and arbitrary
extension objects cannot establish this documentation owner. Exact source UTF16
reference ranges remain independent of rename replacement. Unsaved text/version
changes are read directly, with no project/cache/network access.

This does not close P13.2 indexed usages/Rename: the same target may still have
external-file, YAML or encoded users, whose complete ownership is not established
by current-document source. No new graph registration is introduced.

## Physical sibling navigation ownership

Definition/completion sibling reads now require source and resolved target to have
one identical innermost open-project owner. Only bounded canonical relative local
file paths are accepted; URI/search-path/percent/query aliases are not interpreted.
Both disk metadata and the current text returned by `Project::read_file` are
checked against the 1 MiB scan bound, including an unsaved editor that exceeds the
disk size. A `$id` key in source or target withholds physical sibling resolution
because JSON Schema base-URI ownership is not modeled. This is a conservative
source-navigation subset, not a complete URI resolver or rename-graph proof.

The former first-containing-project helper allowed a parent project to read a
nested project's reference target; a before-fix regression captures that result.
Same-owner unsaved target navigation and completion remain supported. No network
request, process, cross-project edit or index anchor is added. P13.2 remains partial.

Cross-file diagnostics now use the same bounded physical-sibling reader. An
unsupported URI, denied read, foreign owner, rebased source or oversized target
is Unknown, so it cannot manufacture a missing-file/pointer error. A host-proven
same-owner `NotFound` still yields the existing missing-file diagnostic. Source
checks are bounded before scanning. Discovery filters nested owners and cached
file suggestions recheck ownership, file existence and disk size on each request;
opening a nested project or removing a file cannot preserve that stale suggestion.
This does not change unrelated generator/preview fetch paths or prove those paths
share the same ownership boundary.

## JSON definition target source integrity

JSON Go to Definition now uses the same actual object-key ancestry proof as the
local-pointer documentation adapter. It validates the target's complete bounded
JSON, requires unique source keys at every decoded pointer ancestor, and checks
that JSON Pointer tilde escapes round-trip. Duplicate owners/definitions, dotted
flattened impostors, malformed targets, percent-encoded fragments, array indices
and `$id` base-URI declarations withhold this source location. Bounds are 512 KiB,
8,192 semantic/scanned nodes and 48 levels. Valid external JSON fragments need not
repeat an OpenAPI version. Escaped object names retain exact nonempty UTF16
source key ranges. Whole-file references retain file navigation. YAML source integrity is addressed
below; its complete ownership proof remains outstanding.

The former flattened Index returned a location through duplicate `components`
ancestors; the before-fix regression is retained. This does not change complete
usages/Rename status or the distinct diagnostic, marker, bundling and hover
interpretations of reference semantics.

## YAML definition target source integrity

YAML Go to Definition now validates one bounded YAML document with yaml-rust2
under its MIT license option before proving a pointer's actual mapping-key chain.
Aliases, anchors and tags are refused in parser events before loading; duplicate
mapping keys, `$id` rebasing, merge keys, multiple documents and malformed source
are quiet. Source limits are 512 KiB, 8,192 parser nodes and 48 nesting levels. Each
canonical tilde-decoded pointer segment must select a semantic string-key mapping
entry and a unique exact raw key span at the same scanner path. Dotted impostors,
transformed/complex/numeric keys, percent fragments, arrays and unsupported flow
child spans cannot establish this location. Simple plain or quoted block keys
retain exact nonempty UTF16 ranges; supported fragments need no version field.

The former flattened lookup navigated through duplicate YAML `components`
ancestors. Its before-fix regression is retained. JSON/YAML sources both navigate
same-owner current unsaved YAML fragments; duplicate unsaved targets withhold the
location. No graph is registered. YAML schema-reference hover is addressed below; diagnostics,
markers, complete URI resolution and complete usages/Rename remain distinct scope.

## Current-text YAML local schema-reference hover

The registered YAML hover now reuses the 179 strict mapping model/source proof.
It proves the selected literal `$ref` has an actual string-key mapping chain,
then resolves only a supported local schema pointer through another exact chain.
The root must have a supported string version: Swagger 2.0 or OpenAPI 3.0/3.1;
booleans require 3.1. Recognized nested schema keywords match the existing JSON
hover contract. Declared fields are rendered by the existing JSON markdown/bounds
helper. The returned range covers the exact source reference, including astral
UTF16 names. There is no project/filesystem/network/cache access in this path.

Unknown/external/encoded/base/array/alias/duplicate/dotted/transformed/flow-owned
forms remain quiet. Direct component-key/operation YAML hover is not included.
Diagnostics remain JSON-only in the live manifest, while hover now declares JSON
and YAML separately. P13.2 complete usages/Rename remains partial; no graph or
native acceptance is inferred from these source facts.

## Three-state unresolved-reference evidence

The open/save unresolved-ref checker no longer treats flattened Index membership
as ownership or nonmembership as absence. One bounded source proof is built for
the current text and each captured external text. Actual mapping traversal builds
an index of source `$ref` byte spans; each supported pointer traverses semantic
object/string-key owners and verifies every existing ancestor against a unique
exact scanner source key. Present, Missing and Unknown remain distinct.

Missing requires a valid supported pointer and an unambiguous actual parent that
cannot contain the requested member. Duplicate models, dotted-prefix impostors,
base declarations, unproved escaped/flow spans, arrays and encoded/malformed
pointers are Unknown. A genuine absent member in valid ordinary JSON/YAML remains
an error; escaped JSON object keys and canonical slash/tilde mappings are retained.
Unknown/invalid source refs are not read as target files. No graph is published.

Capture limits are 1,024 refs, 32 file reads, 4 MiB text and a conservative 16 MiB retained
model estimate. Proof limits remain 512 KiB/8,192 nodes/48 depth plus 4 MiB flattened
path bytes. Path-parameter integrity is addressed below. Other consumers retain their dated
scope: bundling, JSON structure diagnostics and runtime/native behavior
are not implicitly fixed or requalified. Frozen 183 native QA remains tied to its separate
private package, manifest, source and assets.

## Actual parameter-list ownership

Path-template checks now share the single bounded model used by unresolved refs.
The path item and operation must have actual source-key ancestry and the operation
span must match the scanned operation. Parameter arrays are read as actual ordered
semantic arrays; each supported entry's literal name/in or local ref field must
have an exact owned source span. This distinguishes a real list from synthetic
`parameters[0].name` keys and actual reusable parameters from dotted-prefix aliases.
It adds this particular parameter-array fact model, not general array-pointer
ownership for all other reference consumers.

Unknown/rebased/duplicate/alias/flow/transformed/incomplete/external/chained contexts
withhold missing/unused warnings. Each list is bounded to 1,024 items and each field
to 4 KiB. Valid shared/inline/canonical local reusable parameter facts still report
known template mismatches. Operation entries override a shared tuple by `(name,in)`,
as specified by the OpenAPI Operation Object; warnings select the effective own
entry's current source range. Marker ownership is addressed below. Bundling, structure and native/runtime
acceptance retain their separate scope. No graph is registered.

## Actual marker operation/ref/target membership

The marker provider now uses one strict source proof with a canonical semantic
reference-owner index. A source-backed operation owner must match its scanned
method span; a real ref must belong to that actual operation; a local target must
resolve through exact actual source keys. This replaces the retired marker-only
flattened operation_schema_refs function. Source-order choice is preserved even
when canonical metadata sorts differently. Exact target UTF16 ranges are nonempty.

Singleton YAML flow ref maps can establish marker-only metadata through their
strict semantic map, exact proved parent span and one equal literal ref string.
This does not make generic unresolved-reference/definition/hover flows supported.
Unproved flow contexts, duplicate/dotted/base/encoded/alias owners, external refs,
path-item composition and missing targets stay quiet. Marker output caps at 256;
canonical owner+target metadata caps at 4 MiB and contributes to the conservative
retained-model estimate. Duplicate-operationId ownership is addressed below. Bundling, structure and native
acceptance remain separate source scopes; no host index/Rename graph is exported.

## Actual duplicate-operationId facts

Duplicate ID reporting now consumes the shared strict operation owner and exact
string-field proof. It no longer counts values or assigns source locations from
first-key flat lookup. Two independently owned current source operations with
equal literal/JSON-decoded ID strings establish a duplicate; unknown contexts do
not. The exact source value ranges remain raw and UTF16 correct, not Rename
ranges. Fields use the 4 KiB string bound and at most 256 duplicate messages return.

The before-fix regression captures false duplication through duplicate path
owners; it also covers a dotted ID field impostor and numeric IDs. Genuine JSON
escaped/astral duplicates and ordinary YAML duplicates remain. Unknown composed,
base, transformed, control or excessive fields stay quiet. This is positive
current-document duplication evidence, not a proof of project-wide uniqueness.
Structure ownership is addressed below. Bundling and native/runtime acceptance remain separate.

## JSON structural source locations and pre-allocation metadata budgets

Structure validation now builds an actual semantic-tree location map from unique
raw/JSON-decoded source-key owners. Duplicate/unknown branches are blocked and
ambiguous flat node locations have no fallback. Valid array ordering can retain
a proved containing key span; primitive/empty-string tokens retain exact nonempty
spans. A supported unique version key establishes the root location. Independent
branches still validate, and `$id` does not suppress unrelated structural facts.
The earlier regression expecting a parent fallback for a dotted location collision
is intentionally updated to quiet behavior: that parent is not a general proof
of the exact structural owner.

A new opt-in JSON scanner reserves each key/path budget before allocating a
flattened path, including array item prefixes. Complete scan or budget failure is
explicitly None, never a partial absence model. Structure, JSON pointer/hover
proofs and guarded JSON definition/marker discovery adopt it; legacy keys()/partial
completion and generators are unchanged. The 4 MiB aggregate path budget now guards
allocation, including duplicate source branches discarded by serde_json's map.
Strict YAML accounting is addressed below; the prior post-scan caps are not
credited as pre-allocation protection. No graph, native
latency or quota proof is claimed.

## Pre-allocation YAML budgets and partial reader/completion guards

The strict YAML model now pairs with an OpenAPI-local bounded block scanner
adapted from our MIT spring-text/config implementation. Ordinary and partial
source key/value spans match that scanner. It reserves aggregate joined-path/key
budgets before allocation and enforces lexical stack depth; exhaustion returns
None, never a clipped absence model. No shared crate or generator behavior changes.

YAML pointer/hover/checker/marker proof callers adopt it. Provider preflight and
sibling base-metadata reads use bounded scans before legacy discovery. JSON partial
preflight distinguishes syntax incompleteness (keeps normal typing suggestions)
from exhausted budgets (Unknown). Open/save checking is guarded before discovery.
These guards use 8,192 keys/4MiB paths/48 YAML lexical depth and existing byte caps.
The controlled old scanner observation is 73,795 input bytes→12,553,612 path bytes;
the new caller refuses before 4 MiB metadata allocation. No pathological native OOM
was executed and no timing/quota acceptance is inferred. General YAML flow/URI/
array ownership and generator/other legacy command scans remain separate scope.

Reference provenance follow-up214: definition now selects literal source `$ref`
fields from the strict model, independently of the intentionally narrower
diagnostic reference index. Actual ordered-array source entries, JSON escaped
field names and bounded singleton YAML flow references retain navigation.
Description/block text and dotted lexical impostors stay quiet. Complete external
completion targets require strict pointer source ownership. No graph, Rename,
TryHTTP, generator or native acceptance expansion.

Completion follow-up215 applies the same proof to current-document reference
hints when a supported complete source model is available, suppressing literal
dotted target owners and description/block source impostors. Unsupported or
incomplete source fallback remains a typing heuristic, not a reference graph.
Keyword completion does not build this model. Existing partial typing tests stay.

Empty YAML reference values and unquoted # prefixes also retain the existing
partial typing fallback, since their semantic value is null/comment text.

Copy Reference follow-up218: actual failing caret-on-second-single-line-JSON-key
copied the first key before the fix. The command now passes full UTF16 Position
to a bounded strict source proof; actual mapping entries and ordered arrays
construct pointer ancestry, exact lexical key spans select the target, and
canonical ~0/~1 token escaping precedes UTF8 fragment percent-encoding. Empty,
unknown, duplicate, transformed/flow/alias/base and invalid-caret source is quiet.
Root openapi/swagger ownership and nonempty scalar version are required. Values
are not selected by sharing a line. Copied snippets are serialized; output4096
bytes maximum. MockHost tests prove current text and preserve existing clipboard
on refusal, with no network or process requests. Extraction/bundling/generators
are untouched; this does not add encoded/array navigation or graph authority.
