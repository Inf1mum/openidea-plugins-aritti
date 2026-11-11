# Spring Web Services Tools

Spring Web Services support for Java and Kotlin projects in OpenIDEA:

- **Endpoint ↔ schema**: on an `@Endpoint` class, a `@PayloadRoot(namespace = …, localPart = "x")` method (including each literal entry of `@PayloadRoots`) gets a gutter arrow to the top-level element `x` of a schema with that target namespace; Go to Definition on the `localPart` literal does the same. In the schema, the element gets an arrow back to the method that handles it.
- **Completion** of element names in `localPart = "…"`, from the schemas of the method's namespace.
- **Unknown element**: a warning on `localPart` when a schema of that namespace exists but has no such top-level element.
- **Classpath resources**: Go to Definition on a `new ClassPathResource("…")` path opens `src/main/resources/<path>` (or `src/test/resources`), and a `.xsd`/`.wsdl` path not found there is a warning.
- **Spring WS XML resources**: Go to Definition and cached missing-resource warnings for explicit `classpath:` locations on namespace-qualified `static-wsdl` and `dynamic-wsdl/xsd` elements. Prefix aliases, default namespaces and nested namespace shadowing are respected; the `location` attribute must be unqualified.
- **WSDL publication**: a `DefaultWsdl11Definition` bean shows `Published as /<bean name>.wsdl` in the gutter (under the `MessageDispatcherServlet` mapping).

Schemas are the `xs:schema` elements (any prefix) of the project's `.xsd` files and those inside `.wsdl` files. For navigation and completion, the namespace may be a string literal or a string constant of the same file (`NAMESPACE_URI`); otherwise elements of every schema match. Matching is text-based. Schemas from libraries are not read, and `@SoapAction`/`@Action` methods are ignored because they name no schema element.

Warnings use the API 1.3 current-text diagnostics provider, independently of gutter requests. They use only cached project schemas: activation and file events warm the cache, while diagnostic requests never discover schemas or access the network. Java/Kotlin fixes clear returned warnings immediately; XML requests refresh the current unsaved XSD/WSDL facts. Unknown or dynamic endpoint namespaces stay quiet, and an element in another namespace cannot satisfy a known namespace. The optional `events` permission enables cross-file absence warnings; without it navigation and completion remain available. Cold/incomplete caches, unreadable schema events, documents over 256 KiB, and schema files over 128 KiB suppress absence warnings conservatively. Event refreshes recover an unreadable schema after a successful read or removal.

Parity status: P26.1 is partial. This migrates the existing local endpoint/resource checks; indexed QName Rename/reference parity and the remaining upstream inspections are still unimplemented. Native tests cover current-text fixes, unsaved schemas, namespace isolation, cache and permission fallbacks, and event recovery. Desktop debounce, editor-close/discard refresh, and screenshots have not been verified.

`@PayloadRoots` accepts Java single/array annotation values and Kotlin annotation-constructor varargs/arrays; qualified annotation names are supported. Each entry retains its own namespace and literal `localPart` range, sharing the handler target. Namespace defaults to the empty namespace when omitted. Dynamic container values and malformed entries are skipped; dynamic local parts are not guessed. This follows the framework’s [`PayloadRoots.value(): PayloadRoot[]` contract](https://docs.spring.io/spring-ws/docs/3.1.7/api/org/springframework/ws/server/endpoint/annotation/PayloadRoots.html). The audited original `SpringWebServicePayloadRoot` model likewise pairs namespace and local part for schema references; this text-based addition does not provide its PSI/type semantics.

P26.2 remains partial: source-local container extraction and the XML classpath-resource slice are implemented and tested. The XML slice follows the framework’s [`web-services-2.0.xsd`](https://github.com/spring-projects/spring-ws/blob/main/spring-ws-core/src/main/resources/org/springframework/ws/config/web-services-2.0.xsd), specifically `static-wsdl/@location` and `dynamic-wsdl/xsd/@location`. `sws_resources` and `xml_namespace` in `src/lib.rs` enforce the exact `http://www.springframework.org/schema/web-services` namespace and literal paths. Relative servlet-context paths, external URLs, `classpath*:` patterns, entities, placeholders and traversal are unresolved and produce no warning. Navigation uses the existing bounded schema index; diagnostics remain cache-only. No Rename anchors are added.

The original XML audit (`com.intellij.spring.ws.model.ws.MarshallingEndpoints#getMarshaller/getUnmarshaller`, `XpathEndpoints#getNamespaces`, and `Namespace#getPrefix/getUri`) exposes bean pointers and XPath namespace configuration. Effective type/bean-context and XPath semantics remain unimplemented, along with `annotation-driven` marshaller references and interceptor semantics. This resource-only slice does not validate generated WSDLs, endpoint runtime wiring or the full XML schema.

Not ported: Spring OXM marshaller configuration checks, Apache CXF, WSDL generation, full `<sws:…>` XML namespace configuration.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Explicit one-way DOM endpoint scaffold (P26.3 subset)

On the `name` value of a global element in a standalone `.xsd`, **Create explicit one-way DOM
endpoint scaffold…** prepares one new Java source. Choose an existing source-established
Spring-WS endpoint package under `src/main/java`, enter a class and method name, then review the
exact QName, source/version, package evidence, destination and complete Java content. Confirm
returns one atomic FileActions Create with `overwrite=false` and opens the new class. Cancel at
any prompt returns no edit. The invoking schema and existing endpoint source remain unchanged;
there is no process, schema compiler/JAXB generator, network request or direct file/editor write.

The complete class uses fully-qualified `@Endpoint`, `@PayloadRoot(namespace,localPart)` and
`@RequestPayload org.w3c.dom.Element`, with a `public void` handler and an explicit business-logic
TODO. This is deliberately a **one-way/no-response scaffold**, not a generated implementation of
a WSDL request/response operation. The framework's [documented endpoint method contract](https://docs.spring.io/spring-ws/site/reference/html/server.html)
supports W3C DOM payloads by default and a void return for no response. XSD body/type validation,
JAXB/WS type bindings, response QNames, component scanning/adapter registration, effective
classpath/library versions and runtime SOAP behavior are not inferred. No validation or business
logic is generated from schema structure. New Kotlin endpoints, WSDL code generation, transport/
application-context generation and the original model-aware generator suite remain outside it.

Discovery validates the current source only: strict well-formed XML and attribute checks, exact
XML Schema namespace binding, an unqualified literal `targetNamespace`, and a unique nonabstract,
non-ref global element directly under the root schema. Local elements, embedded WSDL/app-info
schemas, entity/DTD declarations, unknown/dynamic namespaces, malformed/duplicate names and foreign
namespace shadowing are quiet. Predefined/numeric XML entities in QName attributes are decoded;
no external entity is resolved. Local names are bounded ASCII NCNames (128 bytes); namespace
strings are at most 1 KiB with no controls/whitespace/quotes/backslashes/template braces.

Resolve-time collision discovery is fresh and project-owned, bounded to 512 Java/Kotlin/XSD/WSDL
paths, 128 KiB per source, 8 MiB cumulative source and a three-second pass. Missing/unreadable,
oversized or malformed schema facts refuse generation rather than guessing QName uniqueness.
Duplicate schema QNames, existing literal handlers for the selected QName, unresolved handlers
that could use the same local part, and declared package/type or `.java`/`.kt` target collisions
are refused. Other literal local parts do not require namespace resolution. Escaped Java/Kotlin
source identifiers (`\u`/backticks) are outside this conservative collision proof. Existing endpoint
package evidence requires an explicit Spring-WS Endpoint import or fully-qualified annotation;
foreign/ambiguous short annotation identities do not establish a destination. No namespace-to-Java
package conversion is guessed, and a local `org` type cannot shadow the qualified generated names.

Paths are normalized absolute slash paths up to 512 bytes. Every target parent must be reported
as an existing project directory; the host canonical sandbox controls root escapes, including
symlinks (in-root link targets may be reported as directories). Source path/text/version/root,
the full observed source inventory and package choice, and target vacancy are checked around
prompts and again after confirmation. Pending actions are capped to 8 single-use tokens. Exact
confirmation refuses host clipping; LF/CRLF and Unicode source surroundings are preserved.
The final atomic Create remains the authority for a last-moment existing-file/dirty-buffer race;
SDK inventory ignores, external/generated/library schemas/types and effective module classpaths
remain outside the bounded source proof. No automatic Rename graph or generated resource link is
added.

Original comparison: inspected 18506 `CreateSpringWebServiceApplicationContextFix` uses a stable
directory pointer and an IDE write command; the jar's generator inventory contains transport
framework providers (Email/Embedded HTTP/JMS), not an established endpoint/JAXB source generator
in this implementation. `SpringWebServicePayloadRoot` pairs namespace/local-part schema identity.
This adaptation keeps source-directory/QName proof and a concrete write preview while leaving the
stronger PSI/module/type/configuration generation contracts unclaimed. No matching original
scaffold screenshot or new desktop acceptance is claimed. Mock tests cover complete emitted
source/handler parsing, Unicode/CRLF/unsaved schema preservation, cancellation/replay, root/version/
content changes, XML/QName/namespace guards, duplicate/unknown mappings, targets/type/package
collisions and inventory limits. The existing live diagnostics, PayloadRoots, markers and indexes
are unchanged.

## Source-local XML marshaller references (P26.2 partial)

In a well-formed Spring beans XML document, the exact Spring-WS namespace
`http://www.springframework.org/schema/web-services` now supports completion and
Go to Definition on unqualified `marshaller` and `unmarshaller` attributes of
direct-child `marshalling-endpoints`. Suggestions are unique literal `id` values
from direct-child beans in that same document; completion filters the typed prefix
and replaces the whole attribute value. Navigation selects the exact UTF-16 ID
value in the current document, including unsaved text. This is a source link, not
a claim that the bean has a compatible marshaller type or wins at runtime.

The implementation reads only the supplied document: no project discovery,
filesystem/network calls, bean instantiation or provider cache warming. It accepts
at most 128 KiB, 4,095 XML events, 64 nesting levels, 256 distinct IDs and 256 references. Duplicate
IDs are withheld. Imports, aliases, bean-name aliases, root profiles and nested
beans contexts suppress the model. Foreign/shadowed/unbound namespaces, qualified
attributes, dynamic/entity-encoded IDs and malformed XML remain quiet. There are
no new absent-bean diagnostics or automatic Rename/index exports. Existing
PayloadRoots, schema/resource diagnostics/navigation and atomic scaffolding remain.

Original comparison: the audited 18506 descriptor registers
`SpringWebServicesCustomNamespaces`, whose bytecode registers
`marshalling-endpoints`; `model.ws.MarshallingEndpoints#getMarshaller` and
`getUnmarshaller` return `GenericAttributeValue<SpringBeanPointer<?>>`. This
addition implements their explicit local reference interaction, while PSI/type
conversion, imported/module bean contexts, annotation-driven wiring, OXM/CXF and
XPath namespace semantics remain outside this subset. The original-specific
screenshot inventory has no matching image, so no visual parity or desktop
acceptance is claimed. Tests cover exact Unicode ranges, default/prefixed and
shadowed namespaces, unsaved edits, duplicate IDs, prefix replacement, denial of
all host capabilities, malformed/oversized XML and context ambiguity.


Reserved-namespace review fix: generation/reference validation now uses the
namespace-aware XML reader and decoded reserved-URI checks. Illegal `xml`/`xmlns`
rebindings, aliases/defaults of reserved URIs (including escaped URI aliases),
empty prefixed bindings and unbound used prefixes refuse the source model/action.
Legal implicit or explicitly correct `xml` bindings remain supported. This is a
source-validation correction; prior desktop acceptance receipts identify their
pre-fix artifacts. Focused regression/package verification covers the fix; no new
native fixture acceptance is inferred from those older receipts.


## Literal XML bean classpath resources (P26.2 partial)

Go to Definition on a literal `value="classpath:/…"` now supports these exact
source-declared Spring beans/property pairs:

- `org.springframework.xml.xsd.SimpleXsdSchema` / `xsd` / `.xsd`;
- `org.springframework.ws.wsdl.wsdl11.SimpleWsdl11Definition` / `wsdl` / `.wsdl`.

Only a direct Spring beans-root child with that fully qualified `class`, and one
unqualified direct `property name/value` child, is supported. Both default and
prefixed beans namespaces work; current unsaved XML is used. Imports, aliases,
nested/profile contexts, parent/factory/type indirection, dynamic or entity-encoded
paths, duplicate matching properties, ref/nested-value alternatives and foreign/
reserved/unbound namespaces stay quiet. The new branch reads no files, discovers
no inventory and runs no process/network request during navigation.

Resolution requires a **fresh complete watched resource-path family inventory**,
warmed by plugin activation, file lifecycle events or other existing warm paths.
Grant `events` for this branch's lifecycle ownership. Cold, expired, failed/capped
or unwatched inventory returns no target, without an absence warning. An exact
resource path must have one candidate under `src/main/resources` or
`src/test/resources` in the invoking document's project; duplicate module/main/test
paths or an independently owned nested project withhold navigation. Observed removals retire resource paths; content readability/schema meaning is
not certified by this metadata inventory. QName read failures remain separately
tracked. This remains a cached snapshot,
not a synchronous filesystem existence guarantee; unobserved changes and the
final event/lookup gap remain outside its authority. Reload/open/save a supported
schema/source to warm an expired inventory; render does not refresh it.

Resource families over4,096 paths are withheld; conventional resource-directory
path components must match exactly. Source parsing is capped at32 KiB/512 elements and uses the existing bounded
namespace-aware XML validation. Resource spellings are ASCII relative paths up
to512 bytes, after an exact `classpath:` with at most one leading slash; no
traversal, glob, percent-encoding, URL or inferred library resource. Navigation is
to a file position, not a symbol/usage/Rename anchor. No Spring effective bean,
classpath/module graph, runtime schema/WSDL generation, XML mutation or missing-
resource diagnosis is added. Existing sws-location, Java resource, marshaller,
PayloadRoots, diagnostics and atomic scaffold behavior is preserved.

Original comparison: retained `SpringWebServicesResourceTypeProvider` constructor
includes `SimpleXsdSchema.xsd` and `SimpleWsdl11Definition.wsdl` among its resource
filters. Additional literal filter subsets are documented below; broader list,
constructor and effective type/context semantics remain separate. The original-specific screenshot inventory has no matching
image, so this source-backed navigation change has no visual/full parity claim.
Focused public-provider/lifecycle/cold/ambiguity/Unicode tests and matching API1.3
package checks cover the slice; native positive/negative navigation is still open.

## Source QName namespace and incompleteness safety (P26.1 follow-up)

Schema facts now require bounded, well-formed namespace-valid XML and the exact
XML Schema namespace on `schema` and direct global `element` declarations. The
supported containers are a standalone XML Schema root or a schema directly in
WSDL1.1 `definitions/types`, with exact WSDL namespace ownership. Foreign or
qualified `targetNamespace`/`name` attributes, malformed/reserved/unbound XML,
entity/dynamic/unsupported name spellings and unsupported wrappers cannot establish
a QName. Omitted targetNamespace remains the empty namespace; UTF-16 source
ranges still select the literal global name. Duplicate same-schema declarations
are withheld. Facts are bounded by the existing128KiB/4,095-event XML validator,
64levels,128-byte ASCII local names and1,024-byte literal namespace URIs.

QName completeness is tracked separately from resource-path inventory. Includes,
imports, redefine/override, unknown/invalid declarations or contexts suppress
missing-element assertions, while known literal positive declarations still
support navigation. Malformed/foreign current unsaved XML retires its old facts
and makes negative QName claims quiet; a corrected current document restores
them. A physically known schema/WSDL path remains a resource candidate even when
its schema meaning is unknown. This does not fetch dependencies, validate XSD
types, infer chameleon include namespaces, provide an effective module/classpath
model or publish host QName usages/Rename anchors. Existing endpoint annotation
matching, runtime/context limits and native acceptance remain separate.

## Additional source-declared XSD/WSDL resource filters (P26.2 subset)

The exact `PayloadValidatingInterceptor` class now supports its literal `schema`
property as an XSD resource link. Literal scalar constructor inputs additionally
support `SimpleXsdSchema` and `CommonsXsdSchema` for XSD, and
`SimpleWsdl11Definition` for WSDL. These pairs/classes are present in the retained
original SpringWebServicesResourceTypeProvider constructor.

Constructor support requires exactly one direct Spring `constructor-arg`, with
one plain `value` and an absent index or literal index0. Named/typed/ref/nested
values, multiple arguments, factory/parent indirection, foreign namespaces and
wrong extensions are quiet. Exact source FQNs, the prior classpath/path/context
guards,32KiB/512-element source bounds, and existing fresh complete watched
resource inventory remain required. The provider adds no discovery, reads,
process, network, missing-resource assertion, effective constructor/type binding
or host Rename graph. Current XML and UTF-16 literal ranges drive the links.

XSLT/security resource filters are added below. Constructor arrays/lists,
imported/effective bean contexts and native acceptance remain separate. Prior
Desktop50 stays tied to45; source/provider tests and a new package build do not
transfer that native result to the new artifact.

## Owned endpoint namespace constants (P26.1 follow-up)

A namespace is now known only from an exact supported string literal or one
uniquely owned same-file compile-time constant with a literal initializer. Java
unqualified names resolve within the endpoint class; explicit `Type.FIELD` can
resolve a unique top-level same-file type, with import/name ambiguity withheld.
The field must be an explicit static final String. Kotlin supports an owned
companion/object `const` field and an explicit unique same-file object. Mutable
fields, unrelated same-named fields, imported/long-qualified owners, duplicates,
computed/escaped/interpolated initializers and unsupported contexts stay unknown.
Namespace text is bounded at1,024bytes; whitespace/control/escape/interpolation
spellings are not guessed. Omitted/empty namespace remains the empty namespace.

LocalPart additionally requires one exact unescaped128-byte ASCII NCName subset;
computed/escaped/dynamic/invalid local spellings cannot establish a source QName.
The existing unknown-namespace navigation/completion candidate fallback remains,
but unknown namespace facts cannot publish a missing-element assertion. This
reads only current parsed source and cached schemas; no host I/O/export/grant was
added. Constant chains, inherited/interface constants, full Java/Kotlin semantics,
annotation-owner/type resolution and safe host Rename remain separate. Native
acceptance for this artifact is unverified.

## Well-formed namespace ownership for sws locations

The `static-wsdl/@location` and `dynamic-wsdl/xsd/@location` source branch now
passes through the same strict bounded namespace/XML validator as the other XML
workflows before creating resource facts. Invalid reserved-prefix bindings,
malformed documents, DTDs or source over128KiB/4,095XML events remain quiet and
cannot produce a location link or missing-resource assertion. Legal implicit or
explicit `xml` bindings remain supported. Current unsaved context corrections
restore the existing literal path behavior; no new resource discovery/read or
network/process call was added. This is XML source admission, not full sws schema
validation or effective Spring context resolution. Native acceptance remains
unverified for this artifact.

## Explicit endpoint annotation ownership (P26.1 follow-up)

QName facts now require a supported explicit import or the written Spring-WS
fully qualified annotation name for the endpoint class and every payload mapping.
The PayloadRoots container and each nested PayloadRoot entry are checked
independently, including Kotlin annotation-constructor entries without a written
`@`. Foreign or unimported same-simple-name annotations no longer create endpoint
facts, navigation/completion candidates or missing-element assertions.

Current source type/generic-name shadows, Kotlin typealiases/import aliases,
conflicting imports and a shadowed package root withhold ownership. Fully
qualified names remain supported when only the short name is shadowed. Explicit
normal Java/Kotlin imports work; wildcard-only, aliased, same-package-implicit,
inherited/meta-annotation and unresolved external type contexts remain unknown.
The owner table is built once per parsed callback, without discovery, file reads,
network/process calls, new grants or a host index/Rename export. This proves
written source ownership within the supported subset, not classpath identity.

Older positive tests used unimported annotation names as fixture shorthand. Those
fixtures now spell the annotation FQNs explicitly, preserving line layouts and
existing behavior assertions; this correction is not a new parser feature. New
independent tests reproduce and guard foreign/unimported/mixed owners, explicit
imports/FQNs, shadowing/aliases and nested container ownership. Native attribution,
full type semantics and safe host usages/Rename remain separate.

## Generation source-proof consistency (P26.3 follow-up)

The scaffold collision inventory now accepts existing mapping text only when its
localPart/namespace is an exact unescaped, uninterpolated literal. Java octal
escapes or Kotlin templates cannot be mistaken for a different QName and thereby
permit a duplicate scaffold. Unsupported mappings block the bounded source
availability proof; their runtime value is not guessed. Existing package evidence
uses the same explicit annotation-owner table as endpoint facts, including static
import conflicts and source shadows, rather than a weaker duplicate matcher.

These guards change no scaffold body, operation kind, target selection, preview
or grant. The only exported generator is still the lazy standalone-XSD one-way
DOM FileAction with exact review and one non-overwriting Create. No eager writer,
command mutation, plugin template, WSDL/JAXB/context or transport generator is
added. Native creation/Undo evidence remains dated to Desktop24's artifact; new
source/build tests do not upgrade that native receipt.

## Exact XSLT/security resource properties and separate path authority

Cached resource navigation now additionally supports exact source FQNs and
properties `PayloadTransformingInterceptor.requestXslt`, `.responseXslt` (literal
lowercase `.xslt`) and `XwsSecurityInterceptor.policyConfiguration` (`.xml`).
These are the three remaining registrations in the inspected original resource
provider constructor. The existing strict Spring beans-root/property/context,
namespace, classpath-path and scalar-value guards apply; no constructor behavior,
subclass/alias type, `.xsl` alias or resource completion is inferred for them.

Physical/workspace path presence now has an independent per-root/per-extension
inventory for XSD/WSDL/XSLT/XML. Activation and filtered file lifecycle events warm
it; definition requests only consume fresh complete cached paths and recheck
project ownership/unique conventional resource targets. XML/XSLT resource content
is never read, parsed, fetched or executed by this path inventory. Existing XML
source text supplied by the editor is still parsed to establish the property
owner. File/editor path observations do not certify saved/readable/valid content
or synchronous existence; unobserved changes and the event/lookup gap remain.

Limits are eight retained roots,4,096 paths and2MiB path bytes per extension
family,8MiB retained path bytes total,4,096-byte canonical absolute paths and a
two-minute discovery TTL. Capped/cold/failed/expired families withhold navigation;
one XML-family failure cannot poison XSD paths or schema QName authority. Observed
create/modify/remove updates do not renew the discovery TTL. Positive lifecycle
admission requires scoped File metadata; directory/missing paths retire and
unknown metadata withholds that family. More than256 relevant
paths in one event batch clears resource trust until later lifecycle warming;
unrelated paths trigger no resource scan. Warm budgets are cooperative and cannot
preempt a synchronous host glob query.

The schema/QName cache keeps its original XSD/WSDL globs and separate read/content
completeness. Policy XML/XSLT resource-path lifecycle updates do not populate
that QName cache or suppress known-schema missing-element diagnostics. Conversely, oversized/unknown schema
content no longer suppresses an independently established resource path. Main/test
ambiguity, independently owned nested roots, foreign namespaces, duplicate
properties and unsupported source/path forms remain quiet. No missing-resource
assertion, new host graph/grant, security/XSLT execution or native acceptance claim
is added.

## Cached resource path completion for the nine scalar roles

All nine established scalar filter roles now share the same strict owner/namespace
model for navigation and completion. At an explicit literal `classpath:` value,
completion uses only the fresh owned resource-path family, filters the prefix
before the UTF-16 caret and replaces the complete attribute value. Existing
leading-slash style is preserved. Suggestions are plain File items with no
additional edits; no file or resource body is read and no discovery runs in the
request. Invalid/dynamic/entity/traversal/glob/URL values, even beyond the caret,
remain quiet. Empty or partial schemes are not guessed into classpath intent.

Only unique conventional main/test resource candidates from the same current
project are offered. Duplicate module/main/test names, nested foreign roots, cold/
expired/capped/unwatched inventories and missing identity withhold suggestions.
At most64 ordered relative-name groups are considered per request; each candidate
URI retains the role's established extension/ASCII/path bounds. This is cached
source path assistance, not effective classpath, validity or type resolution.
Current unsaved values, Unicode before the literal and LF/CRLF ranges are retained.
Native completion/apply/attribution remains unverified for this artifact.

Source `ClassPathResource` navigation requires the exact canonical written class
or an explicit canonical import, without local/import/typealias ownership
collisions. Only one unchanged plain literal path is supported; overloaded,
escaped, dynamic, traversal and no-extension forms remain quiet. The established
XSD/WSDL/XSLT/XML families use the same fresh lifecycle-owned physical inventory
as XML resource navigation: exactly one conventional main/test target in the
current project is required, and requests do not discover files or read bodies.
Other resource families and effective classpath/Class-relative resolution remain
unsupported. Endpoint QName definitions, markers and candidates require a known
exact matching schema namespace; unknown/unmatched namespaces are quiet.

For an explicit `XPathParamAnnotationMethodEndpointAdapter` bean, a single
`namespaces` property containing plain `props` / `prop key="prefix"` URI text
supports cached schema namespace completion and definition. Every XML element
must belong to the Spring Beans namespace; only unique, nonreserved literal
prefixes and plain bounded URI text are supported. Completion replaces the
whole URI text with up to64 known unique schema namespaces, preserving UTF16
ranges in current unsaved text. No request discovery, schema fetch or body read
runs. Imports/profile/nested contexts, indirect/effective types, merged/duplicate
prefixes, ambiguous schema namespaces, incomplete caches and unknown values
remain quiet. This does not evaluate XPath or implement the complete
`xpath-endpoints`/`namespace` custom element model. Native acceptance for this
new property path remains unverified.

Established scalar resource properties also accept one direct Beans `value`
text child, using the same exact class/property roles and cached path authority
as the `value` attribute. Definition/completion spans cover only its unchanged
literal text, including correct UTF16 offsets after Unicode/CRLF source.
Value/ref alternatives, duplicate properties/children, type overrides, foreign
owners, nesting, comments, CDATA/entities and whitespace/control expansion
remain unsupported. This property-only addition does not extend constructor
child forms, XPath binding, OXM/CXF typed DOM, libraries or effective classpaths.
It performs no request-time resource discovery/content read and has no new
native acceptance.

## Unreleased follow-up

Use **Spring WS: Find Observed QName Occurrences** on a known `@PayloadRoot` local name or XSD/WSDL element name. It lists bounded, namespace-matched observations in the current project and rechecks the selected source before opening it. This is read-only navigation, separate from global Find Usages/Rename.

### Interceptor bean reference assistance

Within a closed Spring Beans XML document, `beans:ref bean="…"` children of
`<sws:interceptors>` and its `<sws:payloadRoot>` / `<sws:soapAction>` selectors
receive same-document bean-id completion and definition navigation. Namespace
ownership, duplicate-id suppression and context import/profile guards match the
existing marshaller reference behavior. This identifies written local bean ids;
it does not infer interceptor types or the effective runtime application context.

### XPath namespace and root-element assistance

Explicit Java/Kotlin `@Namespace` and `@Namespaces` URI values navigate and
complete against the warm, owned schema inventory. Direct annotations override
container entries; method bindings override matching class prefixes while
preserving other class bindings, following Spring's `NamespaceUtils` order.
Adjacent same-package `package-info.java` bindings are loaded through a watched,
project-owned fresh cache. Providers stay quiet when that authority is cold,
denied, mismatched or removed; explicit class/method bindings retain their own
source authority. The occurrence command reads sibling package authority afresh
and rechecks selected source and target identities after its picker.

A plain `@XPathParam("/p:Request/…")` absolute root links/completes its local name
against global schema elements in the proved namespace. Descendant searches,
predicates, dynamic expressions and unknown prefixes remain quiet. The bounded
read-only QName occurrence command includes these roots and explicit XML
`<sws:interceptors><sws:payloadRoot namespaceUri="…" localPart="…">` selectors.
It does not evaluate XPath or claim complete cross-project Rename authority.

### XML contracts and source type assistance

The current Spring-WS 2.0 vocabulary receives current-document required-attribute
and schema-child checks, plus canonical SOAP-binding boolean completion/checks.
Foreign owners and explicitly older/custom schema selections stay quiet. These
checks work independently of a warmed filesystem inventory.

`annotation-driven` marshaller/unmarshaller references include local OXM
`jaxb2-marshaller` IDs. CXF JAX-WS bus references and explicit `#implementor` /
`#serviceBean` IDs navigate/complete within the same admitted XML context; edits
replace the ID portion while preserving `#`.

OXM bound-class names and CXF implementation/service class attributes navigate
and complete against a fresh watched project-source type inventory. OXM and
implementation-only roles omit interfaces; serviceClass admits declared source
interfaces. Duplicate names, foreign project facts, missing watch authority and
cold caches withhold targets. This is source navigation, not effective runtime
assignability or complete library classpath resolution. JAXB declarations also
check for required bound-class names and incompatible/missing binding sources.

XPath namespace-prefix assistance covers ordinary steps, descendant selectors,
attributes and predicates. It navigates to physical package/class/method prefix
declarations and skips string contents and XPath axes. Child local names are not
presented as global schema element references.

Use **Spring WS: Refresh Source Models** to warm or continue the bounded XML
type/package/schema inventories explicitly. Incomplete or capped results remain
partial; editor completion does not start a new source scan.

JAXB `context-path` values also navigate to observed project package declarations
and complete one colon-separated package component at a time. The replacement
keeps neighboring components and separators intact. Source packages do not prove
that JAXB `ObjectFactory`/`jaxb.index` resources are present on the runtime classpath.
