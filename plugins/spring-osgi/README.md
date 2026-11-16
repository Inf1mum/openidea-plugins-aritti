# Spring OSGi

Spring Dynamic Modules (Spring OSGi) support for Spring XML files in OpenIDEA.
MIT; see `LICENSE`.

Works in any Spring XML file that declares `http://www.springframework.org/schema/osgi`
(or `osgi-compendium`) under any prefix or as the default namespace.

## Checks

- **Elements inconsistency** (error): `interface` attribute with an `<interfaces>` element, `ref` with a nested `<bean>`, `comparator-ref` with a `<comparator>`.
- **Service** (error): the `ref` bean (or nested `<bean class>`) of an `<osgi:service>` must implement every exported interface. Exported interface types need an established project-source identity; unresolved external types stay quiet.
- **Listener** (error): `bind-method`/`unbind-method` must be public, return `void` and take `(ServiceType, Map|Dictionary)` or `(ServiceReference)`; a listener without both named methods must implement `OsgiServiceLifecycleListener`; the named listener methods (`bind-method`, `unbind-method`, `registration-method`, `unregistration-method`) must exist in the listener's class. Registration-listener completion excludes a first service parameter when the cached source facts establish that the exported bean cannot be assigned to it.
- **Unresolved `ref`** (Info, `unresolved-osgi-ref`): a `ref` on `<osgi:service>`, `<osgi:listener>` or `<osgi:registration-listener>` naming no bean of the project. Info because beans from libraries or the container are invisible; placeholders are skipped.

## Navigation, completion and hover

- `ref`, `comparator-ref` and `depends-on` navigate and complete to beans: XML ids/names/aliases of the project's Spring XML files, Java beans from stereotypes and `@Bean` methods. On `<osgi:service ref>` the beans implementing the exported interfaces sort first (completion never hides beans). Completion inserts XML-escaped names.
- `interface` and `<osgi:interfaces><value>` complete project Java classes, interfaces first.
- Listener methods complete to the methods of the listener bean's class that fit the expected signature.
- While typing `<osgix:` the compendium elements `cm-properties`, `managed-properties` and `managed-service-factory` are offered (scoped binding checks apply).
- Hover on OSGi interface, bean-ref and listener-method references lists established cached source declarations and separately describes uncached/external candidates, ambiguous identities, cycles and traversal limits. XML hover is cache-only.

## Generation

**Generate OSGi service, reference or collection bean…** is a native editor intention (service, reference, reference list or set; the default OSGi namespace and prefixed Beans roots are supported). It asks for the kind, Java interface name and service ref, shows the generated XML and target path in an exact confirmation, and applies as one atomic host edit (native Undo works). Cancelling any prompt changes nothing.

The generator is source-local: it inserts before a top-level child or on a blank line inside a single, closed Spring beans root (up to 512 KiB), uses an OSGi namespace declared on that root, refuses nested insertion/comments/CDATA and documents with imports or dynamic/entity-encoded declaration names, validates Java names, escapes XML refs, and checks generated reference ids against current-buffer ids, names and aliases. It does not add namespaces/imports, create Java classes, infer library types, validate service-ref existence or prove uniqueness across independently loaded contexts. The list/set choices create an explicit `interface` collection declaration with a generated `apiList`/`apiSet` name.

## How it works

Classes are matched by fully qualified name from the project's Java sources; supertypes go through explicit imports and lexical source scopes, and a check only fires when the whole class hierarchy is in the project's sources (a jar superclass or unresolved wildcard type skips the check). Duplicate fully qualified classes and duplicate bean declarations never select an arbitrary owner. Facts are scoped to the current innermost open project and invalidated on relevant file-change/open/save events (bounded batches; overflow invalidates conservatively); navigation and completion warm the source cache again.

OSGi elements and their structural children require their namespace in the element's own lexical scope; rebound foreign prefixes, foreign lookalike elements/attributes and invalid reserved/unbound QName bindings cannot become OSGi source references. Cached XML bean-class metadata requires a Beans or supported legacy root.

Bounds: the hierarchy graph reads at most 4,096 cached classes, traverses at most 128 ancestor identities and 512 edges per owner, and reports its own incompleteness when bounded; source files are capped at 512 KiB, 512 type declarations and 4,096 members; XML diagnostics inspect at most 2,048 elements and return at most 500 findings; exact confirmations use the shared 16 KiB preview guard.

Live checks use the API 1.3 diagnostics provider on the current editor text (up to 512 KiB), including unsaved changes, with no project discovery, file reads or network calls; source-local checks work without project access, and the optional `project.read` and `events` capabilities are only needed for cross-file facts and live invalidation (denied capabilities degrade to source-only behavior without fallback writes).

The `bundle` bean scope is accepted by the Spring Beans plugin's scope check and completion.

## Not implemented

- Type checks needing real type resolution: the registration method's first parameter against the service bean class, beans of an `<osgi:reference>` as service implementors, effective JVM assignability, generic substitution, override/overload resolution.
- The "Interface is expected here" check; the compendium DOM model; a Spring model feeding effective bean types (`ReferenceEffectiveTypeProvider`).
- Kotlin sources; indexed Rename anchors; automatic source lifecycle warming and unsaved Java editor overlays.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
