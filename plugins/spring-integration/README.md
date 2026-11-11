# Spring Structure Tools (aritti.spring-integration)

One **Spring Structure** tool window (right) with four sections, plus the editor support of three former plugins: Spring Integration (this one), Spring Batch and Spring Modulith. `aritti.spring-batch` and `aritti.spring-modulith` are no longer maintained: uninstall them, otherwise their windows and completions show twice.

- **Channels**: the Integration Channels diagram described below.
- **Batch**: Spring Batch jobs, steps, flows, splits and decisions as a diagram. XML job navigation, completion and transition/bean checks, `spring.batch.job.name` navigation, `jobParameters` key completion, late-binding scope checks, Spring Batch 5 migration hints (setting `springBatch.checkChunk`).
- **Modules**: Spring Modulith application modules with named interfaces, allowed and actual dependencies; checks for internal types and unlisted dependencies, `allowedDependencies` completion, quick fix to make a module open.
- **Diagram**: the module dependency diagram; the command "Spring Modulith: Copy Module Diagram (PlantUML)" copies it.

Source editor sections are gated by dependencies in the root build file (`spring-integration`, `spring-batch`, `spring-modulith`; multi-module builds and version catalogs count as unsure). Explicitly configured Batch runtime profiles instead use their validated artifact/classpath contract. The command ids and storage keys of the absorbed modules are prefixed `batch.` and `modulith.`; their old settings values are not migrated.

## Spring Integration

Spring Integration message channels in OpenIDEA, for Java, supported Kotlin annotations and XML configuration:

- **Channel index**: channels come from explicitly owned `@Bean` methods returning Spring `MessageChannel`, `PollableChannel` or `SubscribableChannel`, `MessageChannels.direct/queue/publishSubscribe/executor/flux/priority/rendezvous("x")`, the supported contiguous DSL's `.channel("x")`, and `<int:channel id>` / `<int:publish-subscribe-channel id>` in any integration namespace.
- **Endpoints**: `@ServiceActivator`, `@Transformer`, `@Filter`, `@Router`, `@Splitter`, `@Aggregator`, `@BridgeFrom`, `@BridgeTo`, `@InboundChannelAdapter`, `@Publisher`, `@MessagingGateway`, `@Gateway` channel attributes, `IntegrationFlow.from("x")`, and the XML `input-channel`, `output-channel`, `channel`, `request-channel`, `reply-channel`, `default-request-channel`, `default-reply-channel`, `discard-channel`, `error-channel` attributes. Each is read as consuming or producing (`input-channel`, `replyChannel` and the `channel` of an outbound adapter consume; `outputChannel`, `requestChannel`, `discardChannel` produce).
- **Go to Definition** on a channel name opens its declarations, or the endpoints consuming it when nothing declares it (Spring Integration creates input channels on demand). On an XML `ref` it opens the bean (XML `<bean id>` or a Java/Kotlin bean name).
- **Completion** of known channel names, and of bean names in `ref`.
- **Gutter arrows**: a consuming endpoint points to the producers sending to its channel, a producing one to the consumers.
- **Warnings**: an output-side channel (annotation or XML) that no endpoint consumes and no bean declares; a `ref` to a bean the project doesn't declare. Input channels, DSL channels, `${…}`/`#{…}` names and common infrastructure beans (`errorChannel`, `nullChannel`, `transactionManager`…) are never reported, and nothing is reported until the first full scan of the project.

Matching is text-based, by channel name as written in string literals and attributes; constants (`outputChannel = Channels.OUT`) aren't followed. Beans and channels from libraries, auto-configuration or other modules aren't seen. Kotlin sources count for bean names and annotations; the Kotlin DSL doesn't.

- **Channels** section of the Spring Structure window: message channels as nodes (declared ones accented) and each endpoint (service activator, transformer, filter, router, gateway, adapter…) as a small node, drawn input channel -> endpoint -> output channel from the project index; click a node to open its source. It refreshes on open and with its Refresh button, which rescans.

Not ported: the DSL flow chain edges in the diagram (`.from("a").channel("b")` shows both channels but no endpoint node), adapter-specific attribute checks (JMS, AMQP, HTTP…), the Kotlin DSL, SpEL.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Live diagnostic ownership and caches

The registered Spring Structure wrapper returns one versioned diagnostics result
for enabled Integration, Batch and Modulith checks. Gutter requests retain the
Integration navigation arrows and do not publish another Problems producer. A
current unsaved fix replaces only the matching findings in the combined result.
The old standalone Batch/Modulith plugins remain unmaintained and must be removed
to avoid their separate producers; their embedded parsers are reused here.

Typing requests do not discover files or read build files. Module/dependency
applicability is cached from the existing root-build heuristics. Optional `events`
warms those facts and bounded source passes on open/save/watched changes, with
build changes invalidating cached applicability. Navigation and views can also
warm their indexes. A missing event grant or cold/reclaimed instance can therefore
leave project-wide checks quiet until another supported interaction warms them.
Dependency detection is not an effective Maven/Gradle dependency model.

Local Batch checks use current text. Cross-file absence and Modulith boundary
checks require a complete source pass still within its 120-second trust interval;
updating one open document cannot refresh the rest of a stale project. Unavailable,
partial and expired evidence is quiet, not proof that every reference is valid.
Very large live documents (over 256 KiB) are skipped; cached Modulith live models
above 1,000 source files are conservatively omitted. Each module's event warming
has a 20 ms reading budget. File-change invalidation currently evicts all module
caches; other projects warm independently and never reuse an anonymous root.

Targeted tests cover combined findings, independent unsaved fixes, dependency
changes, cold/second-project isolation and no duplicate marker publication.
Native desktop acceptance for this combined provider remains separate.

## Explicit atomic XML channel creation (P20.4 subset)

At a root-level caret in an existing Spring Beans XML source, **Create XML message channel…**
asks for a literal channel ID and previews the complete exact resulting XML. Confirm returns one
API 1.3 atomic same-file insertion; Cancel returns no edit. It reuses an existing root-declared
prefix bound exactly to `http://www.springframework.org/schema/integration` and inserts a bare
`<prefix:channel id="…"/>` before the validated Beans root close. It adds no namespace/schema
mapping and changes no existing endpoint/reference. This is explicit source creation, not an
inferred missing-channel fix or a claim about runtime wiring. No files are created, overwritten
or deleted; no direct editor write, process, request or job execution is performed. Applying the
returned workspace edit uses the host's optional `editor.write` permission and native Undo.

The supported source is conventional, well-formed unprefixed `<beans>` XML with the default
Spring Beans namespace and an existing Integration prefix. Strict XML event/attribute checking
precedes the existing offset scanner. Nested Beans/profile scopes, local namespace/XML-attribute
rebindings, undeclared prefixes, DTDs, source entity references, malformed XML, ignored build/output
paths, non-root caret positions and self-closing/unconventional root closing tags are outside this
subset. Source is bounded to 12 KiB/2,048 XML events and normalized absolute source/root paths to
512 bytes. LF/CRLF and Unicode source content are preserved. IDs are at most 128 ASCII bytes,
starting with a letter/underscore and continuing with letters/digits/underscore/dot/hyphen;
`nullChannel` and `errorChannel` are reserved. No XML escaping or lossy name coercion is needed.

Resolution verifies source path/text/version and exact project root before and after prompts and
confirmation. Eight pending tokens are retained at most and tokens are single-use. A fresh,
command-owned Java/Kotlin/XML source inventory checks declared bean/channel names and aliases,
including separated XML aliases and decoded literal entity spellings in other files. Dynamic or
undecodable names, source over 512 KiB, discovery/read failures, the 1,024-path sentinel or an
incomplete three-second pass refuse creation instead of treating unknown facts as absence. The
name snapshot is rechecked after confirmation. Total scanned source is limited to 8 MiB, with
at most 8,192 distinct names / 1 MiB of name text; nested open projects retain their exact owner. Existing graph/cache/live diagnostics are not
mutated by this fresh inventory. `confirm_exact` refuses an oversized preview rather than allowing
host clipping. Changes after the final source/name check remain a host-application race; ignored
sources, imports outside SDK discovery, libraries and compiled runtime contexts remain outside
this source-level collision check.

Original evidence: inspected Spring Integration Patterns `263.7307.112` jar 20205. Its decompiled
`Channel$ChannelBeanTypeProvider` maps a channel without queue/priority/rendezvous/dispatcher
configuration to `DirectChannel`; `DefaultChannels` names `nullChannel` and `errorChannel`.
`SimpleMessageChannelTypeProvider` requires an ID, and `SpringIntegrationModelInspection` relies
on the typed Spring Beans/DOM model. No established create-channel generator was found in our
current implementation or the inspected jar's generator/action inventory, so this adds only the
explicit source-root-proved action rather than inventing typed relationships. Queue/custom channel
options, inferred endpoint linking, Batch job/step generation, Modulith module generation and
runtime job controls remain open. No matching original generator screenshot or new native desktop
acceptance is claimed. Mock tests cover registered wrapper dispatch, exact insertion/CRLF/Unicode,
unsaved source preservation, both cancellations, stale/forged tokens, root/version/content changes,
local and project alias collisions, namespace/malformed/bounds guards and incomplete inventories.

## P20.3 bounded XML channel navigation candidates

Current-text XML channel hover and Go to Declaration now resolve a unique literal
`channel` / `publish-subscribe-channel` ID directly below the same Spring Beans root,
using the exact core Integration namespace. Core bridge, service-activator, transformer,
filter, splitter, aggregator, router, gateway, inbound/outbound-channel-adapter and chain
channel attributes receive a local reference count and source declaration line. These
providers need no build discovery, project scan, network, saved-file read or cached model;
unsaved edits and UTF-16 token ranges apply immediately. Supported local reference tokens
with no unique local declaration return no target instead of guessing a project-wide match.
A same-file Spring bean/Integration endpoint ID, bean name/alias list or explicit alias that registers the
same spelling suppresses channel navigation and labels source ownership ambiguous.
Nested bean/channel declarations are conservative collision candidates; active profiles and
effective alias registration remain unresolved. Foreign-namespace beans, partial
name matches and an alias source name do not register a competing spelling.
The hover explicitly calls these source candidates: profiles/imports/runtime bean types
are not resolved. It does not claim absence of imported channels or complete usages.

Work is bounded to32KiB current text,4096 XML events,256 tokens and a cooperative10ms
parse budget. DTDs/entities, expression/escaped/whitespace names, nested profile blocks,
unknown namespaces and malformed input are excluded from this new provider. Names are
ASCII literals of at most128 characters; namespace prefixes are not rename tokens.
The pre-existing Java/DSL, XML bean-reference and project diagram paths retain their
older lexical model and limitations; this slice does not establish their semantic ownership.
Existing job-local Batch transition navigation is retained. No automatic host indexing,
Find Usages or Rename anchors are published: a file-local channel name cannot prove
a complete Spring context owner across imports, aliases, Java configuration and modules.
Job/module semantic indexing and safe whole-model Rename remain P20.3 gaps.

Reference audit: the original20205 `MessageChannelReferenceConverter` delegates to
`SpringBeanReferenceJamConverter` and module-owned `SpringIntegrationUtil.getMessageChannels`;
its `Channel` model extends `DomSpringBean`. That framework model/type ownership is
not reproduced by project-wide spelling equality here. The existing original
`SpringIntegrationModelInspection` visits CommonSpringModel DOM beans. These are
source/bytecode receipts, not matching new hover screenshots or native desktop acceptance.


Namespace admission review fix: XML source-candidate navigation and atomic XML
intentions use a scoped namespace-aware reader plus decoded reserved-URI checks.
Illegal `xml`/`xmlns` rebinding, aliases/defaults of reserved URIs (including escaped
aliases), empty prefixed bindings and unbound used prefixes are withheld. Legal
implicit/explicit `xml:lang` and ordinary namespace shadowing remain supported.
Legacy definition fallback is guarded too, so a rejected local model cannot
reintroduce a target from that same namespace-invalid invoking document. This
changes source admission only; runtime/type ownership and Rename authority are
unchanged, as are Java quick fixes and existing exact atomic previews.

The fallback guard examines current text only, without I/O, capped at 512 KiB,
8,192 events and a 10 ms loop deadline. Larger/over-budget documents are quiet.
This remains a conservative XML 1.0 framework subset: namespace declarations
requiring entity normalization unsupported by the reader, and XML 1.1 prefixed
namespace undeclarations, can be withheld even if valid in a broader XML model.
It does not validate an entire imported framework model or every cached target
file. Prior native receipts identify their earlier artifact hashes; post-fix
package/regression tests do not relabel them as native acceptance.


## P20.4 explicit local XML bridge

At the same supported Beans-root caret, **Link local XML channels with bridge…**
offers two unique, literal, self-closing bare `<prefix:channel id="…"/>`
declarations directly under that root. The input picker states that the bridge
adds a consumer; the output picker excludes that same input. Only this source's
bare DirectChannel forms are candidates. Queue/dispatcher/publish-subscribe and
other configured channels, duplicate IDs/aliases, expressions, imported/global
channels and more than 64 candidates are excluded. Existing project/model caches
are not used to invent wiring or channel types.

Resolution rechecks the existing dirty-aware source/version/project snapshot
between prompts, shows the exact resulting XML with `confirm_exact`, then returns
one atomic insertion without applying it. The existing Integration prefix and
LF/CRLF/Unicode source are preserved. Existing same-direction explicit bridges
are refused. Pending tokens remain bounded to eight and are single-use, including
cancellation/failure. No direct writes, process, network or runtime/job operations
occur. Source changes after final validation remain a host application race.

The source action adds an endpoint; it does not infer effective Spring-context
ownership, consumer ordering, imported aliases, whole-flow loops or polling.
Global duplicate configuration and runtime semantics remain outside this local
source proof. Java/Kotlin, Batch job/step and Modulith create/link workflows are
still P20.4 residuals. The graph presentation workflow is described below; this source action does not infer diagram semantics.

The original core `Bridge` model inherits input/output bean-pointer attributes
and identifies `BridgeHandler`, with optional pollers. The primary Spring
Integration Messaging Bridge contract permits a bridge with explicit input and
output channels; polling is needed for pollable inputs. This subset deliberately
uses only bare local DirectChannel source forms and generates no poller. The
retained Spring diagrams screenshots show navigation/diagram entry points, not
this new XML bridge wizard; no native bridge acceptance is claimed. Mocks verify
the registered wrapper, exact atomic result, cancellation, owner/version drift,
source ambiguity and duplicate-pair refusal.


## P20.1 Channels graph interaction follow-up

The existing packaged SVG Channels graph now has bounded **Zoom in/out/100%**
controls (50–200%), selected-node highlighting, and plain read-only domain/source
facts: channel or endpoint candidate, original node detail, source location and
cached incoming/outgoing link counts. Click/Enter keeps the existing source-open
behavior and records the selection; **Open selected source** repeats that action.
Zoom/selection belong to the concrete project root and reset on a project switch.
Unknown/missing owner renders a replacement empty message. No shared renderer,
packaged asset, provider, module parser or symbol graph was changed.

Source opens now require an exact node in the current project's graph, an owned
source path and a matching reference in a bounded dirty-aware reread (512 KiB)
before opening its recorded location. Forged paths and a cached reference whose
source changed are refused. This validates a lexical source candidate, not a
complete effective Spring context, semantic bean type or imported/runtime wiring.
No process or network request is added. All controls use the existing packaged
bridge and the shared diagram renderer's generated numeric SVG dimensions.

The original Integration diagram uses SpringElementWrapper/PSI-backed nodes with
tooltip/content categories; retained Spring diagram screenshots show zoom/1:1 and
diagram navigation. Our Channels subset adds source-backed selected facts and
bounded scaling while preserving its existing lexical model. The registered owner
now supplies the same presentation for Batch/Modulith, as described below. Original
semantic categories remain beyond these source-model facts. Mocks exercise the registered packaged wrapper, source drift,
forged/foreign paths, escaped facts, zoom bounds and project switch. Native graph
layout, keyboard/scroll behavior, light/dark themes and zoom remain unverified.


### Batch/Modulith source-open guards

Merged Batch diagram callbacks now accept only a unique current graph node from
this project's index, a path owned by the same project, and a matching Job/State
fact in a bounded current source reread. Unknown/forged paths and invalid numeric
coordinates are refused instead of falling back to line zero. Modulith graph and
module-tree callbacks likewise require an actually rendered model key and owned
source location. Their keys include a losslessly encoded absolute source-path
suffix, so identical logical module names in another project do not reuse a
captured row identity; the current parsed source unit must still match the selected
model's source facts. Dependency/interface/allowed-dependency tree navigation is
preserved for actual model rows. Source rereads are capped at 512 KiB.

These explicit read-only actions perform no process/network operation and do not
change providers, parser rules or automatic Rename. Facts not represented in the
parser projection (for example trailing comments) can change without invalidating
navigation; this is not exact byte identity or effective runtime context proof.
The graph/model cache and its discovery limits remain unchanged. Embedded Batch
and Modulith fixtures separately cover known navigation, forged keys, source drift,
foreign project and oversized reread. Their standalone plugin packages remain
unmaintained; the registered Spring Structure owner embeds these modules. New
module navigation acceptance remains source/mock evidence, not a desktop claim.


### All maintained diagram sections share the packaged workflow

Channels, Batch and the Modulith **Diagram** section now use one plugin-owned
presentation adapter over the existing shared packaged SVG renderer. Embedded
Batch/Modulith expose typed nodes, edges, source locations and status from their
existing models; the owner does not scrape HTML or reconstruct their parsers.
The Modules list remains its existing tree. Section ownership and parsing/
diagnostics/generation models remain unchanged.

Each graph supports 50–200% zoom, visible selected-node/source facts, displayed
incoming/outgoing link counts, clear selection and guarded source Open. State is
separate per graph/project root. Source facts require one unique location and the
same concrete project owner; foreign nested-project facts and ambiguous locations
are withheld. Missing/denied owner replaces the body with a safe message. Explicit
refresh retains existing bounded discovery/progress behavior; rendering adds no
process or network request, and emits no model-free refresh feedback loop.

Presentation is capped at 512 nodes, 2,048 displayed edges, 4,096 candidate source
records and 128 KiB of node-key text; each key is at most 16 KiB. Labels/details
are capped at 512/256 bytes (plus a truncation marker). Source-owner checks are
deduplicated by path. Caps or ownership exclusions mark the presentation partial;
displayed link counts never assert unseen relationships are absent. Scaling uses
the known renderer's numeric outer SVG dimensions below 1,000,000 pixels, retaining
its coordinates in a viewBox. Unsupported dimensions keep the original SVG.

Original Integration diagram contracts use PSI/SpringElementWrapper nodes and
semantic categories. Retained common Spring screenshots show diagram navigation,
zoom and 1:1; no original-specific Batch/Modulith gallery capture is available.
These source layouts fulfil the named packaged diagram workflow over our existing
parsers, while semantic context/type models, original categories, cross-model
usages/Rename and runtime job controls remain separate parity work. Tests include
actual registered packaged events for both embedded graphs, scoped source opens,
project switches, nested owners and presentation bounds. Native layout, scrolling,
zoom, keyboard and theme acceptance remain unverified.


## P20.4 Java/Kotlin DirectChannel configuration source

**Create DirectChannel configuration source…** offers a new sibling Java/Kotlin
file from one source-owned `@Configuration` context with an explicit Integration
import or qualified source reference. The declared package must match the source
folder. Configuration annotation identity is explicit/import-owned; aliased,
shadowed, missing, malformed and oversized source contexts stay quiet. This is a
file-level scaffold action, not extraction from a selected method/variable scope.

Inputs choose the new class and channel bean/method identifiers; the final native
review contains the exact target and full generated source. It creates one class
with explicit configuration bean name and one DirectChannel `@Bean` method using
qualified framework names. Kotlin class/method are open, requiring no all-open
compiler plugin. Generated line endings follow the text returned in the SDK Document.
The pinned native host normalizes that text to LF, so the raw-CRLF MockHost fixture
does not establish new-file CRLF inheritance from disk. Existing-source line-ending
metadata is separately managed by the host. Classpath availability,
component-scan/import registration and effective runtime bean/type ownership are
not resolved; the preview says so, and no project code runs during generation.

The host receives one atomic non-overwriting Create/Open operation after source
path/text/version/project checks around every prompt/review. Existing source is
never edited. Existing Java/Kotlin files/directories and known package type names
are refused, as are known bean/configuration IDs and namespace-root shadows.
The existing bounded source inventory is reused and compared after review:
1,024 owned paths, 512 KiB per file, 8 MiB total, three-second cooperative budget;
IDs and known declaration names each cap at 8,192 entries / 1 MiB of text.
Constant/concatenated or partly dynamic `@Bean`/stereotype names now refuse the
literal collision proof rather than silently falling back to a method/class name.
The inventory is lexical, includes conservative nested/local declaration names,
and does not resolve every quoted/aliased/package/classpath symbol or active
Spring context. Sources excluded by discovery/ignore limits remain outside it.

Current source/package/path bounds are 12 KiB / 512 bytes; identifiers are ASCII,
128 bytes maximum, without language keywords, namespace roots or reserved channel
IDs. Eight source-scaffold intents are kept in memory, separate from at most eight
XML intents; all are single-use on resolve/cancel/failure. Target appearance or
source changes after the final check remain host-application preconditions/races;
the host confines paths, rejects overwrite and protects dirty resource buffers.
No process, network request, direct project/editor write or SDK pin change occurs.

P20.4 still has broader source-workflow residuals: editing existing Java/Kotlin
configuration bodies, typed endpoint extraction/linking, Batch job/step and
Modulith module generation, classpath-dependent variants and runtime controls.
Nine new mocks cover public wrapper dispatch, Java/Kotlin output, cancellation,
source/version/root/dirty drift, IDs/types/files/directories, constant bean names
and invalid input. Native preview/Create/Undo acceptance remains unverified.


### P20.3 Batch transition scope correction

The registered owner now handles current XML `next`, transition `to` and stop
`restart` requests directly from their physical Batch job/flow scope. Completion,
source navigation and hover require the exact Batch namespace, supported attribute
shape and a unique literal direct state declaration in that same job/flow. Nested
flow members are kept in their own scope. Missing/duplicate/foreign candidates do
not fall back to another job, file or project. The embedded legacy target helper
also no longer performs that wrong-owner transition fallback; parent/bean lookup
retains its separate model.

This current-document path needs no module discovery, cached index, file read or
network call and reflects unsaved text immediately. Bounds are 32 KiB / 512 XML
elements / 256 attributes / 4,096 parser events, with a cooperative 10 ms budget.
Malformed/multiple-root/DTD/entity inputs, qualified target attributes, unsupported
expression names and unknown source shapes stay quiet. Hover/navigation identify
source candidates, not inherited/imported runtime states or flow reachability.
Cold/no-grant source fixtures demonstrate that known local targets need no project
filesystem authority. Other legacy cross-file bean/parent/config-name paths retain
their documented cache/discovery limitations.

No DocumentIndexer/automatic usages/Rename graph is registered. Exact lexical
ranges and physical scope do not establish complete effective Spring contexts,
aliases, external references or job/module ownership for Rename; P20.3 remains
partial model work. The primary Spring Batch flow contract describes steps within
an owning job and XML next/to transitions; original DOM/PSI analysis is broader.
Four public-wrapper fixtures plus an embedded fallback regression cover this
correction. New native transition/completion/hover acceptance remains unverified.

## Current-file channel occurrence picker

**Spring Integration: Current File Channel Occurrences** lists the supported literal
channel declaration and references in the current unsaved XML document. Place the
caret on a supported ID/reference. The picker gives source order, role, line and
UTF-16 column and opens only a listed location in that same unchanged document.
It requires one local channel declaration and refuses known competing bean, alias
or core endpoint registrations. Cancel, denied optional editor.read, stale source
or project ownership and invalid selections produce no navigation.

This is a bounded physical-source occurrence list, not host Find Usages/Rename or
an effective context query. It uses the same32KiB/4096-event/256-token/cooperative10ms
model as local channel hover, with no discovery, saved-file read, network or process
work. Imported/profile/runtime ownership and dynamic or non-core registrations
remain unresolved. Native picker/focus/scope-switch acceptance is unverified.

## P20.4 atomic create-and-import configuration variant

**Create and import DirectChannel configuration source…** creates the same sibling
Java/Kotlin configuration and adds one fully qualified Spring `Import` annotation
to the existing source-owned Configuration. One FileAction contains a nonoverwrite
Create/Open and one source Edit; review includes the complete generated file and
complete resulting source. Existing imports and configuration bodies are preserved.
Java uses a qualified class literal; Kotlin uses its `::class` form. CRLF and source
indentation are retained. The create-only variant remains available independently.

This variant is offered only without an existing Import annotation, wildcard
imports, generic/inherited configurations or detected namespace-root shadows. It does not merge existing Import
arrays, resolve composed annotations/classpaths, check application activation, or
infer effective runtime wiring. All prior ownership/version/collision/inventory
checks repeat around prompts and after review. Source intents stay bounded and
single-use. Kotlin source providers now also expose the supported annotation-based
Integration and Batch parameter features described below. Native combined
preview/apply/undo acceptance remains unverified; P20.4 stays partial.

## P20.4 local Batch step transition link

Place the caret on a step opening tag/ID, or inside an explicit `next` attribute,
to use **Link Batch step next to a local state…**. The picker offers unique other direct states in the same
physical source job/flow, followed by the exact complete resulting XML. The action
returns one source Edit inserting a missing `next` attribute or replacing only its
existing value; tasklet content and all other attributes remain unchanged. The host applies/undoes that edit. No job runs.

The source step, target and containing job/flow must be unambiguous source-owned
forms without parent/abstract inheritance; a source step with child next/end/fail/
stop transitions is excluded. This does not merge conditional rules, create steps,
resolve imported/inherited/runtime contexts, validate step types or analyze flow
reachability/cycles. Self, unchanged, duplicate and other-scope targets are excluded.

Root/path/text/version and candidate facts are rechecked around both prompts.
Four pending single-use intents are kept; denied project identity, cancellation,
stale source/owner and unknown/replayed tokens return no edit. The existing bounded
Batch source model applies (32KiB,512 elements,256 attributes,4096 events,10ms
cooperative parsing). If full resulting XML and context do not fit the host16KiB
confirmation limit, resolution refuses before asking for confirmation. Source-local
mock validation is separate from unverified native preview/apply/undo acceptance.

Absent-next insertion is offered only on the supported step opening tag, never
inside its tasklet/body or where a next attribute already exists. A synthetic
would-be attribute is checked with the same namespace/physical-scope model, then
only an insertion range in the original source is retained. Original and synthetic
parsing each use the existing bounded model; no synthetic ranges are published to
the host. Existing conditional/inherited/duplicate/other-scope guards still apply.

The Batch model also rejects XML declarations inside/after the root, duplicate
declarations and declarations preceded by whitespace/comments. A single leading
XML declaration is retained. This malformed-source guard applies to Batch local
navigation/completion/hover as well as link actions.

## Source refresh provenance and recovery

If file-saved/files-changed subscriptions are unavailable, the Structure view says
that automatic source updates are unavailable and asks for Refresh after changes.
Explicit Refresh starts a new source pass, clears old diagram selection/models and
rewarms module applicability/live caches for the current project. Render-driven
scan continuation remains separate from that user action, so a partial pass can
still finish. This is source data recovery, not a runtime refresh or job control.

State changes and watched source invalidation now send the established model-free
`ui-kit-refresh` notification as well as native view refresh, allowing an already
open packaged page to request its current scoped model. The message carries no
source data. Three new source tests cover denied-event restoration/recovery,
watched page invalidation and dependency-gate changes without events; native
refresh/Undo recovery on the new artifact remains separate from earlier76 action QA.

## Positive current-file Java channel hover

Java hover now shows bounded positive facts for a supported source-owned
Configuration containing explicit Spring `@Bean` methods returning Spring
`MessageChannel` and literal Spring `ServiceActivator` input/output references.
It reports the declaration line and supported current-file reference count on
the exact default method name or explicit literal bean name/reference range.
Qualified framework names and explicit single-type imports establish annotation/
return-type spelling; no project discovery, saved-file read or runtime service
is used. Current unsaved source and UTF-16 positions apply immediately.

The hover requires one supported current-file declaration without known competing
literal Bean/stereotype registrations. Dynamic/compound/array/escaped bean names,
custom/shadowed annotation/type imports, generic or inherited configurations,
unbalanced source and unsupported paths/languages stay quiet. It does not resolve
configuration imports, composed annotations, subclasses/actual channel instances,
implicit endpoint IDs, full Java syntax/types or effective runtime context. The
existing source action context bounds the document to12KiB; hover retains at most
128 source tokens with a cooperative10ms collection loop after parsing. No global
usages/Rename or new definition routing is added; P20.3 remains partial.

Endpoint annotation source facts require an explicit exact Spring Integration
annotation import or fully qualified spelling. Foreign/local/wildcard ambiguity,
concatenated or escaped channel strings, duplicate arguments, Java Unicode
preprocessing and Kotlin aliases/interpolation are withheld. This is a physical
source subset; composed annotations and complete type/classloader ownership
remain unresolved, and no indexed Rename graph is registered.

A Java/Kotlin Bean source is marked as a channel declaration only when its
Bean annotation and MessageChannel, PollableChannel or SubscribableChannel
return type have exact explicit Spring ownership. A type name ending in Channel
is insufficient; arrays, generic return shapes, local shadows and dynamic bean
aliases do not establish that declaration. Concrete channel implementations and
full external type inheritance remain unsupported. Unproved bean-name spellings are retained only for conservative collision and
uncertainty checks; they are not published navigation/completion facts.

DSL source facts require explicit exact Spring IntegrationFlow(s)/MessageChannels
static factory ownership, with same-spelled local receivers withheld. Only direct
contiguous proved from-to-channel chains establish string channel steps; unknown
intermediate receivers and lambda-bound channel factories are unresolved. Use an
explicit owned MessageChannels factory for that supported source subset. Dynamic,
escaped/interpolated arguments do not become literal channel facts.

XML channel/reference attributes must be unqualified model attributes. Foreign
prefixed lookalikes are withheld; an Integration namespace must equal the base
URI or use its slash module boundary. This boundary is not proof that a custom
module namespace handler is installed or that effective bean contexts resolve.

XML source facts require a complete single-root envelope with bounded depth and
valid namespace bindings. Incomplete/multiple roots, exterior text, misplaced
declarations, DTD/entities/escaped contexts and invalid comments are withheld.
The registered owner applies this source-admission gate before XML completion,
markers and live diagnostics, including embedded Batch. This conservative source
subset is not a general XML validator or proof of effective Spring configuration.

Embedded Batch transition/parent/bean links use only unqualified model attrs.
Typed Batch source states cannot borrow a job or wrapper ancestor from a foreign
namespace; direct Batch and beans-container source ownership remains supported.
Generic bean IDs and full effective XML context resolution remain separate limits.

The cached Batch diagram now uses the same complete-envelope admission as
current XML callbacks. Malformed project XML cannot establish source-owned
Batch graph nodes; a complete source recovers through Refresh. This remains
a cached physical-source model, not runtime flow reachability.

Published Java/Kotlin Bean source facts require explicit supported Spring
annotation ownership and ordinary scalar/array literal names. Foreign, dynamic,
generic/inherited or otherwise unsupported registrations remain non-navigable
collision candidates. Such uncertainty withholds negative Integration absence
warnings; remaining warnings describe supported indexed source facts, not runtime
absence. Creation still retains candidate collisions and refuses Java Unicode
preprocessing/Kotlin aliases or escaped/interpolated hidden names. Repeated alias
spellings at one declaration produce one source location. Embedded Batch Java bean provenance and full effective configuration ownership
remain unresolved.

XML Bean source facts require unqualified names in a supported top-level beans
context. Name aliases split on Spring delimiters and retain individual source
ranges; duplicate aliases in one element share one definition. Foreign attrs,
foreign elements and inner-bean scope remain non-navigable candidates. Unsupported
envelopes/attribute normalization carry uncertainty, and parsing is bounded to
512KiB per source. Creation separately retains raw conservative XML collision
candidates and refuses withheld XML rather than treating it as empty. Legacy
plain beans files still prevent duplicate-name creation.

Cached Integration and Batch facts recheck current innermost project ownership
before publication, using bounded canonical project metadata. Newly foreign
files are pruned from candidate caches without provider source reads. Metadata
limits fail closed. After project ownership changes back, use Refresh or a
watched source change to recover pruned facts. The shared index can still read
outer-prefix files during discovery; this is fact admission, not an SDK I/O or
symlink boundary, nor an atomic project-generation lease.

Source creation independently retains possible literal DSL channel names from
unproved fluent/lambda factory syntax. Those candidates are collision checks,
never published navigation/graph facts. Dynamic or unknown naming arguments
refuse creation; finite literal factory/lambda names still allow a different new
name. This is a bounded lexical inventory, not effective runtime registration.

Batch source job and step names require an exact imported or fully qualified
Spring builder constructor with an ordinary literal first argument. Factory
receiver spelling alone supplies no job fact. Unproved builders suppress
missing-job diagnostics; factory binding and runtime registrations remain
unresolved. Foreign/local types, aliases, inherited contexts, compound literals
and escaped strings are withheld.

Embedded Batch bean references now share Integration's exact annotation-origin
and literal-name admission. Foreign, compound, aliased and otherwise unproved
source names retain uncertainty metadata and cannot define tasklet/reader bean
references or completions. Unknown-bean warnings are withheld in those contexts.
SpringBootApplication facts likewise require the exact Spring annotation origin.
The plugin-local pure helper is shared; effective imported/runtime bean closure
remains unresolved.

Batch parameter-adder facts require a contiguous chain starting at an exact
imported or fully qualified Spring JobParametersBuilder constructor. Foreign
types, unrelated receivers, intermediate unknown calls and compound/escaped
keys do not publish parameter completion names. Source/XML late-binding
expression extraction remains a separate partial lexical model.

Source late-binding parameter reads require an exact Spring Value annotation
and one ordinary complete literal expression on a parsed member or parameter.
Foreign annotations, comments, ordinary strings and compound/dynamic values
cannot donate indexed completion keys. XML expressions and current caret
completion-context admission remain separately partial.

Indexed XML parameter reads require a valid complete envelope and a complete
literal expression in an unqualified owned Batch bean-reference attribute or
a standard top-level bean property/constructor-arg value. Comments, foreign
attributes/elements, encoded keys and malformed files cannot donate names.
Current caret-context admission and effective runtime parameters remain partial.

Parameter completion destinations use the same exact source/XML expression
admission as indexed facts. A memory-only sentinel validates the typed key
context, including empty or non-ASCII keys, without changing source. Comments,
foreign annotations/attributes and unsupported expressions receive no Batch
parameter suggestions; documents over512KiB are withheld.

A separate “Rename Batch step in this document…” refactor is available at a
unique literal direct step ID in one flat physical XML job. It updates that ID
and every proved direct step/decision next, child next@to and stop@restart
reference to it. The new ASCII ID cannot collide with document IDs, names,
aliases or existing target literals. Nested flows/splits/jobs, duplicate IDs,
inherited/imported states, dynamic/encoded/foreign or unknown target roles
refuse the action. This scope excludes bean references, other documents,
Java/Kotlin, runtime binding and global indexed Rename.

The action lists no edits until input and complete before/after review are
approved. One-use tokens capture the document, root and exact inventory;
version/text/owner checks run around prompts. It returns one grouped FileAction
edit and performs no direct write. Inventories are bounded to32KiB source,
512elements/256attributes/4096events/64edits; full review must fit the host's
16KiB confirmation limit. Final resolve-to-host-apply source leasing remains a
host boundary; native Apply/Notifications Undo acceptance must use this artifact.

“Spring Batch: Find Step Usages in This Document” opens a read-only picker for
the same bounded flat-job step model. Place one caret on the step declaration
or a proved direct next/to/restart target. The picker distinguishes declaration
and reference kinds and displays the full path, UTF-16 range, job/step and source
version. It reads only the focused editor through optional editor.read when the
explicit command runs; providers perform no new discovery or source reads.
Before navigation it rechecks the current editor's exact path/version/text,
project owner and model. Cancellation or stale/unsupported context opens nothing
and never edits source. Job/channel/module and cross-file usages, effective
contexts and global indexed Find Usages/Rename remain partial or absent.

## Unreleased follow-up

On a channel ID or bridge reference in a simple XML channel/bridge document, choose **Rename literal channel in this document…**. The action validates a unique new name and shows the complete before/after document before returning edits. Imports, profiles, nested endpoints and competing registrations are excluded.

### Source-local Batch and channel refactoring

The reviewed document rename action supports direct Batch step and decision IDs in a flat, single-job XML document. It is available from the declaration or a proved `next`, `to` or `restart` reference, and previews all exact changes before returning undoable edits. The existing Find Step or Decision Usages command uses the same physical-source inventory. Nested flows, imports, inheritance and runtime contexts remain outside this action.

Channel rename also covers literal channel-valued attributes on flat service activator, transformer, filter, splitter and aggregator endpoints. Handler `ref` and `method` values are preserved; dynamic expressions, unsupported attributes and a handler reference colliding with the selected channel cause the action to be withheld. These are explicitly document-scoped operations, not global indexed Rename.

Modulith boundary diagnostics and dependency graphs also include qualified project type names in explicit field declarations, `new`, `instanceof`, `extends`, `implements`, `throws`, annotations and class literals. Candidates must exactly match a declared project top-level type. Comments, strings, unknown types and ordinary receiver calls do not establish a dependency. This remains a source model: generic arguments, casts, nested types, arbitrary expression resolution and library types are not claimed.

The **Find Channel Usages in XML Import Tree** command follows literal relative imports and alias chains from the active XML root, then navigates exact physical occurrences. The **Rename channel in this XML import tree…** action returns one reviewed multi-file workspace edit, including alias targets while preserving alias names. Admission is limited to 16 files / 256 KiB, supported flat channels/endpoints/aliases and one project; profiles, wildcards, classpath imports, nested endpoints and unresolved or colliding aliases are rejected. The complete forward tree is reread around review; reverse imports and other contexts are explicitly excluded.

Java **Go to Declaration** and **Current File Channel Occurrences** use current-text, explicitly owned `@Bean`/`MessageChannel` declarations and literal `@ServiceActivator` channel names. They do not require a warmed project index and do not infer imported or runtime contexts.

Selecting an alias in the XML import-tree rename action renames that alias declaration and its direct users, preserving the underlying channel and other alias names. The review explicitly distinguishes alias rename from channel ID rename.

Modulith dependency completion includes `module::*`, and Go to Declaration on `allowedDependencies` navigates module declarations, named-interface packages/types and wildcard owners. On source refresh, declared module package directories receive native `O` (open) / `C` (closed) badges with explanatory tooltips. Badges use the fresh admitted Java/Kotlin source model, require an existing directory in the owning project, and are cleared when the source cache is reset. Full Kotlin/runtime type semantics and desktop badge acceptance remain separate.

On a proved cross-module internal-type import, **Expose imported type as a named interface…** offers a reviewed annotation edit to the unique public project type. Choose a new interface name; the action uses the fully qualified Spring annotation, preserves imports/package/visibility, refuses existing annotations or shadowed qualifiers, and rereads source/model facts around confirmation. It does not move files or overwrite a package descriptor. Calling modules may separately need to allow the new named interface.

Batch XML checks distinguish top-level declarations from inline subjobs, partition steps and anonymous split flows. Required IDs/flow parents and merge booleans follow the reviewed schema context. **Spring Batch XML compatibility** defaults to Auto (common checks only); explicitly selecting 5 or 6 enables version-specific migration diagnostics. Modern official `spring-batch.xsd` URLs are unversioned and do not prove a runtime version. Compatibility 6 reports removed `throttle-limit`/subjob `job-launcher`, the replacement `job-operator`, and the XML namespace deprecation; unknown explicit schema URLs suppress version assumptions. Additional operator/incrementer/extractor/handler/aggregator bean references participate in supported source navigation.

For an implicit closed module with no package descriptor, **Open imported type’s module…** previews a new fully qualified `@ApplicationModule(type = ...OPEN)` descriptor. The action states that this exposes the entire module, requires an owned existing package directory, and uses a non-overwriting atomic file create. Existing descriptors are preserved.

### Optional reviewed Batch CLI runtime

Configure `springBatch.runtimeProfile` as `runner-5.2.6` or `operator-6.0.6`, list explicit project-relative classpath directories/JARs (one per line), and select the exact compiled context plus its Java/Kotlin source. Runner also accepts an exact XML classpath resource and matching source. Profiles are disabled by default and require the optional approved `java` executable.

**Validate Configured CLI Profile** uses an embedded Java helper to inspect actual classpath artifacts without loading application code. It checks the pinned official core JAR, unique launcher/context, compiled binary name, runtime-selected multi-release entries and bounded source/classpath fingerprints. Manifest-added classpaths, escaping paths and shadowed launchers are refused. Helper sources are materialized and verified in the plugin's private data directory.

**Run Reviewed Job Operation** shows the actual argv, cwd, source and artifact identity, then validates again before starting an owned process. Runner supports start/next/stop/restart/abandon; Operator also supports recover. Up to three launch processes leave a fourth slot for control requests. **Poll CLI Output** reads all owned sessions; display output is bounded without stopping business execution. **Cancel CLI Process** ends one selected local process and does not send a Batch stop. A detected project switch ends old owned CLI processes before accepting another project's runtime command.

**Inspect Execution Repository State** separately reviews context startup and queries an exact execution ID through the configured `JobExplorer`/`JobRepository`. It returns job/instance/status/times and an observation time. This is an explicit point-in-time query, never an automatic editor/poll request. A CLI stop acknowledgement does not establish STOPPED; the execution query is separate.

Artifact fingerprints identify the selected compiled context and dependencies. They do not establish source build provenance or effective datasource identity; invoking the selected context includes its startup code and configured database connections. Real reproducible disposable qualification is documented in `tests/runtime-cli/README.md`; native OpenIDEA desktop acceptance remains separate.

Existing Kotlin Integration channel navigation/completion/markers and live source diagnostics are now registered through the merged providers. Kotlin Batch parameter-key completion uses the owned source builder/expression model. Java-only Batch migration and scope checks are withheld for Kotlin. Declared Kotlin roots, types, imports and @PackageInfo metadata now participate in the Modulith source model; full type/classpath/Kotlin DSL semantics remain separate.

Modulith source support includes Kotlin application roots, declared types/imports and literal `@PackageInfo` metadata for module/interface declarations. Metadata dependency completion, validation and source navigation work for supported Java package descriptors and representative metadata classes. Unsupported aliases/constants/interpolation or conflicting package metadata visibly mark the model incomplete and withhold negative boundary conclusions and badges. Ordinary Kotlin casts do not invalidate unrelated package metadata. Reviewed type-exposure edits support unique public Java and Kotlin target types. Creating an absent module package descriptor remains restricted to Java source roots.
