# Spring Web Services Tools

Spring Web Services support for Java, Kotlin and XML projects in OpenIDEA.
MIT; see `LICENSE`.

## Endpoint ↔ schema navigation

- On an `@Endpoint` class, a `@PayloadRoot(namespace = …, localPart = "x")` method (including each literal entry of `@PayloadRoots`) gets a gutter arrow to the top-level element `x` of a schema with that target namespace; Go to Definition on the `localPart` literal does the same. In the schema, the element gets an arrow back to the method that handles it. `@SoapAction`/`@Action` methods are ignored because they name no schema element.
- The namespace may be a string literal or a string constant of the same file: a uniquely owned same-file compile-time constant with a literal initializer (Java static final String field, including `Type.FIELD` for a unique top-level same-file type; Kotlin companion/object `const` field or explicit unique same-file object). Mutable, duplicated, computed, escaped or interpolated initializers, imports/long-qualified owners and ambiguous names stay unknown.
- Endpoint facts require a supported explicit import or the fully qualified Spring-WS annotation name for the endpoint class and every payload mapping, including each nested `@PayloadRoots` entry (Kotlin annotation-constructor entries without a written `@` included). Foreign or unimported same-simple-name annotations, wildcard-only or aliased imports, same-package-implicit contexts, shadows and aliases withhold ownership.
- Schemas are the `xs:schema` elements (any prefix) of the project's `.xsd` files and the schemas inside `.wsdl` files (a standalone schema root or a schema directly in WSDL 1.1 `definitions/types` with exact WSDL namespace ownership). Matching is text-based; schemas from libraries are not read. Unknown or dynamic endpoint namespaces stay quiet.
- Unknown/dynamic namespaces fall back to navigation/completion candidates over every schema, but unknown namespace facts cannot publish a missing-element assertion.

## Completion and hover

- Element names in `localPart = "…"`, from the schemas of the method's namespace.
- `classpath:` values at the nine supported scalar resource roles (see below) complete from the fresh owned resource-path family, filtering the prefix before the caret and replacing the complete attribute value.
- `marshaller`/`unmarshaller` attributes of direct-child `marshalling-endpoints` elements (Spring-WS namespace) complete and navigate to unique literal bean `id` values in the same document.
- `beans:ref bean="…"` children of `<sws:interceptors>` and its `<sws:payloadRoot>` / `<sws:soapAction>` selectors get the same same-document bean-id completion and definition navigation.
- `@Namespace`/`@Namespaces` URI values and `@XPathParam` absolute root local names navigate and complete against the warm owned schema inventory; direct annotations override container entries and method bindings override matching class prefixes (Spring's `NamespaceUtils` order). Adjacent same-package `package-info.java` bindings load through a watched, project-owned fresh cache.
- XPath namespace prefixes (ordinary steps, descendant selectors, attributes, predicates) navigate to physical package/class/method prefix declarations; string contents and XPath axes are skipped.
- `annotation-driven` marshaller/unmarshaller references include local OXM `jaxb2-marshaller` IDs; CXF JAX-WS bus references and `#implementor`/`#serviceBean` IDs navigate/complete in the same admitted XML context, replacing the ID portion while preserving `#`.
- OXM bound-class names and CXF implementation/service class attributes navigate and complete against a fresh watched project-source type inventory (implementation-only roles omit interfaces; serviceClass admits declared source interfaces). JAXB `context-path` values navigate to observed project package declarations and complete one colon-separated component at a time.

## Checks

- **Unknown element** (warning): `localPart` naming no top-level element of a known schema namespace.
- **Missing XML resources** (warning): explicit `classpath:` locations on `static-wsdl` and `dynamic-wsdl/xsd` elements, and `new ClassPathResource("…")` paths, not found under `src/main/resources`/`src/test/resources`.
- Current Spring-WS 2.0 vocabulary checks: required attributes, schema children, and canonical SOAP-binding boolean values, on the current document. Foreign owners and older/custom schema selections stay quiet.
- JAXB declarations check for required bound-class names and incompatible/missing binding sources.
- Warnings use the API 1.3 current-text diagnostics provider, independently of gutter requests, on cached project schemas only; diagnostic requests never discover schemas or access the network. Java/Kotlin fixes clear returned warnings immediately; XML requests refresh the current unsaved XSD/WSDL facts. Unknown or dynamic endpoint namespaces stay quiet; an element in another namespace cannot satisfy a known namespace. Cold/incomplete caches, unreadable schema events, documents over 256 KiB and schema files over 128 KiB suppress absence warnings conservatively; event refreshes recover an unreadable schema after a successful read or removal.

## XML resources

Go to Definition and completion for literal `classpath:` schema/WSDL/XSLT/policy resources, on these exact source-declared owner/property roles (attributes or one direct Beans `value` text child):

- `SimpleXsdSchema.xsd`, `CommonsXsdSchema` constructor, `SimpleWsdl11Definition.wsdl` (constructor or property);
- `PayloadValidatingInterceptor.schema` (`.xsd`);
- `PayloadTransformingInterceptor.requestXslt` / `.responseXslt` (`.xslt`); `XwsSecurityInterceptor.policyConfiguration` (`.xml`);
- `static-wsdl/@location` and `dynamic-wsdl/xsd/@location` on namespace-qualified elements (unqualified `location` attribute; prefix aliases, default namespaces and nested shadowing respected);
- Java `new ClassPathResource("…")` (exact canonical written class or explicit canonical import; one unchanged plain literal path).

Navigation needs a fresh, complete, watched resource-path inventory (warmed by activation and file lifecycle events — the optional `events` capability). Only unique conventional main/test resource candidates of the current project resolve; duplicates, foreign roots, cold/expired/capped inventories and ambiguous targets stay quiet. XML/XSLT resource content is never read by the path inventory. The inventory is a cached snapshot, not a synchronous filesystem guarantee. Relative servlet-context paths, external URLs, `classpath*:` patterns, entities, placeholders, traversal and globs produce no warning.

## WSDL publication

A `DefaultWsdl11Definition` bean shows `Published as /<bean name>.wsdl` in the gutter (under the `MessageDispatcherServlet` mapping).

## Generation

**Create explicit one-way DOM endpoint scaffold…** on the `name` value of a global element in a standalone `.xsd` prepares one new Java source. Choose an existing source-established Spring-WS endpoint package under `src/main/java`, enter class and method names, review the exact QName, destination and complete Java content, and confirm one atomic non-overwriting Create that opens the new class. Cancel at any prompt returns no edit.

The generated class uses fully qualified `@Endpoint`, `@PayloadRoot` and `@RequestPayload org.w3c.dom.Element` with a `public void` handler and a TODO — deliberately a one-way/no-response scaffold. Discovery requires strict well-formed XML, the exact XML Schema namespace binding, an unqualified literal `targetNamespace`, and a unique non-abstract, non-ref global element. Collision discovery is bounded (512 Java/Kotlin/XSD/WSDL paths, 128 KiB per source, 8 MiB cumulative, three-second pass) and refuses duplicate schema QNames, existing handlers, unresolved same-local-part handlers, package/type/target collisions and unestablished endpoint packages (explicit import or fully qualified annotation required). Targets must be existing project directories inside the host sandbox (paths up to 512 bytes; symlink escapes controlled by the host canonical sandbox). Source path/text/version/root, inventory, package choice and target vacancy are rechecked after confirmation; the final atomic Create is the authority for last-moment races. No process, schema compiler, network request or direct write is involved. Kotlin endpoints, WSDL code generation and model-aware generation are not provided.

## Commands

- **Spring WS: Find Observed QName Occurrences**: on a known `@PayloadRoot` local name or XSD/WSDL element name, lists bounded, namespace-matched observations in the current project and rechecks the selected source before opening it. Read-only; separate from Find Usages/Rename.
- **Spring WS: Refresh Source Models**: warms or continues the bounded XML type/package/schema inventories explicitly. Incomplete or capped results stay partial; editor completion does not start a scan.

## How it works

Schema facts require bounded, well-formed, namespace-valid XML with the exact XML Schema namespace on `schema` and global `element` declarations; reserved `xml`/`xmlns` rebindings, unbound prefixes, DTDs, malformed or oversized XML cannot establish facts. Omitted `targetNamespace` is the empty namespace. Duplicate same-schema declarations are withheld. QName completeness is tracked separately from resource-path inventory: includes, imports, redefine/override and unknown declarations suppress missing-element assertions while known literal declarations still support navigation.

XPath configuration (bean namespace-prefix pointers) is read where declared: an `XPathParamAnnotationMethodEndpointAdapter` bean's `namespaces` props support cached schema-namespace completion and definition.

Bounds: XML validation caps at 128 KiB, 4,095 events and 64 nesting levels; local names are 128-byte ASCII NCNames, namespace URIs at most 1 KiB, source files for scalar resource parsing at 32 KiB/512 elements; the resource-path inventory keeps eight roots, 4,096 paths, 2 MiB per extension family, 8 MiB total, 4,096-byte canonical paths and a two-minute discovery TTL; completion considers at most 64 ordered relative-name groups per request; previews use the shared 16 KiB guard.

Required capability: `project.read`. Optional: `events` (cross-file absence warnings, path-inventory lifecycle, package-info and project-source type warming — without it navigation and completion remain available) and `editor.read`. No network, process or direct filesystem writes.

## Not implemented

- PSI/type semantics, effective Spring bean contexts, runtime wiring, generated-WSDL validation and the full XML schema.
- Apache CXF beyond the explicit reference navigation above; WSDL generation; Spring OXM marshaller configuration checks; the complete `<sws:…>` configuration model and `xpath-endpoints` custom elements.
- Renamed/indexed QName anchors, Rename/usage graphs, library schemas, effective classpaths and module models.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
