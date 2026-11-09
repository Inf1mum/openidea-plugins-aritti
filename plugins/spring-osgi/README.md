# Spring OSGi

Spring Dynamic Modules (Spring OSGi) support for Spring XML files in OpenIDEA.
Port of JetBrains' Spring OSGi plugin (`com.intellij.spring.osgi` 253.28294.218,
Apache-2.0) from [intellij-obsolete-plugins/spring-osgi](https://github.com/JetBrains/intellij-obsolete-plugins/tree/07887c0596764e16977fd4570c2234bfc81dde49/spring-osgi),
by JetBrains s.r.o. and contributors. See `LICENSE` and `NOTICE`.

Works in any Spring XML file that declares `http://www.springframework.org/schema/osgi`
(or `osgi-compendium`) under any prefix or as the default namespace.

- **Inspections** (errors, with the original plugin's messages):
  - elements inconsistency: `interface` attribute with an `<interfaces>` element, `ref` with a nested `<bean>`, `comparator-ref` with a `<comparator>`;
  - service: the `ref` bean (or nested `<bean class>`) of an `<osgi:service>` must implement every exported interface;
  - listener: `bind-method`/`unbind-method` of an `<osgi:listener>` must be public, return `void` and take `(ServiceType, Map|Dictionary)` or `(ServiceReference)`; a listener without both methods must implement `OsgiServiceLifecycleListener`;
  - listener methods (`bind-method`, `unbind-method`, `registration-method`, `unregistration-method`) that don't exist in the listener's class.
- **Unresolved `ref`** (Info, `unresolved-osgi-ref`): a `ref` on `<osgi:service>`, `<osgi:listener>` or `<osgi:registration-listener>` naming no bean of the project. Info because beans from libraries or the container are invisible; placeholders are skipped.
- **Go to Definition and completion**: `ref`, `comparator-ref`, `depends-on` → beans (on `<osgi:service ref>` the beans implementing the exported interfaces sort first; XML ids/names/aliases of the project's Spring XML files, Java beans from stereotypes and `@Bean` methods); `interface` and `<osgi:interfaces><value>` → project Java classes (interfaces first); listener methods → the methods of the listener bean's class that fit the expected signature; while typing `<osgix:` the compendium elements `cm-properties`, `managed-properties` and `managed-service-factory` (hand-written list, whether the host triggers completion inside a tag name is unverified).

How it matches: classes by fully qualified name from the project's Java sources, supertypes through explicit imports and lexical source scopes; known listener parameter types by qualified name. A check only fires when the whole class hierarchy is in the project's sources. When the hierarchy reaches into a library (a superclass from a jar or an unresolved wildcard type), the check is skipped.

Not ported:
- type checks needing real type resolution: the registration method's first parameter against the service bean class, beans of an `<osgi:reference>` as service implementors; `ref` completion only sorts, it never hides beans;
- the "Interface is expected here" check (disabled upstream too);
- `ReferenceEffectiveTypeProvider` (no Spring model to feed bean types into), the compendium DOM model;
- Kotlin sources.

Generation uses the _Generate OSGi service, reference or collection bean…_ editor intention (service, reference, reference list or set). The `bundle` bean scope is accepted by the Spring Beans plugin's scope check and completion.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

Live checks use the API 1.3 diagnostics provider on the current editor text (up to
512 KiB), including unsaved changes. This request performs no project discovery,
file reads or network calls. Source-local checks work without project access;
cross-file checks use only cached source facts. Missing-name reports wait for a
completed source scan, and unknown external types remain unvalidated. Existing
completion/navigation requests can warm the source cache. The former empty gutter
provider has been removed; diagnostics are published once by the host.

This completes the existing local-check migration only. Indexed reference anchors,
additional generator parity and broader type/hover presentation remain separate parity work.


P21.3 adds a bounded project-source hierarchy and XML hover on OSGi interface, bean-ref and listener-method references. The report lists established cached source declarations and separately describes uncached/external candidates, ambiguous identities, source cycles and traversal limits. Duplicate fully qualified classes and duplicate bean declarations never select an arbitrary owner. Nested type lookup follows the current/enclosing lexical scopes; exact imports and imported outer types are handled, conflicting imports remain unresolved. A diamond is not a cycle. Ordinary classes have the known implicit `java.lang.Object` terminal; record/enum/annotation library ancestry remains unknown.

The graph is built once for a diagnostic request from at most 4,096 cached classes, traverses at most 128 ancestor identities and 512 edges per owner, and reports its incompleteness when bounded. Source files are limited to 512 KiB, 512 type declarations and 4,096 members; unbalanced/oversized source facts cannot prove absence. XML diagnostics inspect at most 2,048 elements and return at most 500 findings. Missing-name/hierarchy findings require a fresh complete source cache. XML hover is cache-only and does not discover files or resolve library types. Current XML edits update their own cached declarations; unsaved Java editor overlays, automatic source lifecycle warming and desktop hover rendering still need separate work. Existing completion/navigation can warm the cache.

Listener parameter checks accept established `java.util.Map`, `java.util.Dictionary`, `java.lang.Object` and `org.osgi.framework.ServiceReference` signatures and report known primitive/array/final-JDK mismatches or invalid arity. Unknown external parameter types remain quiet and are omitted from signature completion. Unknown inherited hierarchies also suppress invalid-signature findings because an external overload may apply. This is not full assignability, generic substitution, override/overload resolution or Spring effective bean-type analysis; the registration listener's service parameter remains unchecked. No Rename anchors or generator behavior were added by the hierarchy change.

The local original audit inspected `SpringOsgiServiceCommonInspection.checkIsImplemented/getSupportedTypes`, `BasicListenerMethodConverter.getPsiClass/checkType` and `ReferenceEffectiveTypeProvider.processEffectiveTypes`. The original uses PSI inheritance, classpath assignability and effective Spring bean types. This implementation covers only the established source subset above. Prior confirmation/FileAction desktop journeys remain dated evidence; the hierarchy/hover surface has MockHost/source tests and no new desktop acceptance.

Generation appears as **Generate OSGi service, reference or collection bean…** in native
editor intentions (API 1.3 FileActions). Resolving asks for the kind, Java interface
name and service ref, then shows the generated XML and target path in a confirmation.
The list/set choices create an explicit `interface` collection declaration with a generated `apiList`/`apiSet` name; they do not infer service implementations or runtime cardinality. Only acceptance returns a single edit for atomic host application and undo. The host's
lazy intention list has no concrete preview before these prompts; a resolved single-file
action can apply immediately. Resolution itself never writes the buffer. Cancelling any prompt changes nothing. There is no direct editor-write generation command.

This is a bounded source-local generator: insert before a top-level child or on a
blank line inside a single, closed Spring beans root, up to 512 KiB. It uses an OSGi namespace declared on that root (including a default OSGi namespace under a prefixed Spring beans root and single-quoted attributes),
refuses nested insertion/comments/CDATA, validates Java names, escapes XML refs,
and checks generated reference ids against current-buffer ids, names and aliases.
Reference/list/set generation refuses documents with imports or dynamic/entity-encoded declaration names because imported name
uniqueness cannot be established locally. It does not add namespaces/imports,
create Java classes, infer library types, validate service-ref existence or prove
uniqueness across independently loaded contexts. The action is tied to its source
path/version; invoke it again after document changes. Native fixture testing confirmed the action is available and applies through the host.
A follow-up adds explicit XML confirmation after observing immediate application; see
the desktop evidence record for verified paths and remaining limits.


Generation follow-up (P21.2): added the source-established OSGi `list`/`set`
reference collection forms and default OSGi namespace case. All generation first
requires complete well-formed XML, rejects duplicate attributes/DTD/entity nodes,
and caps XML event count/nesting. Action tickets retain up to eight bounded
source snapshots and revalidate exact path, text, language, version and owning
project before and after prompts; a different instance without the ticket refuses
resolution. Missing project access is quiet at action enumeration. All generation returns edits exclusively through FileActions.
Concrete edit previews use the shared 16 KiB exact-confirmation guard; oversized
previews refuse without consuming confirmation. Current-buffer IDs, aliases and
name lists are collision checked for all three named reference forms. No context
imports are traversed, namespaces/imports added, external types resolved, files
overwritten, or Rename graph emitted.

Comparison evidence: retained original
`patterns/OsgiSpringBeanGenerateProvider.java` derives the namespace prefix by URI;
`model/xml/List.java`, `Set.java`, `BaseReferenceCollection.java` and
`BaseOsgiReference.java` establish collection/reference bean forms. The retained
original tree does not contain the superclass generator templates, and the matrix
records original-specific screenshots unavailable, so this does not claim complete
original Generate-menu/template or visual parity. The prior native service/reference
journey remains evidence for that earlier path; the new collection/default-namespace
cases have focused source/MockHost tests and matching package checks only.


Reserved-namespace review fix: generation/reference validation now uses the
namespace-aware XML reader and decoded reserved-URI checks. Illegal `xml`/`xmlns`
rebindings, aliases/defaults of reserved URIs (including escaped URI aliases),
empty prefixed bindings and unbound used prefixes refuse the source model/action.
Legal implicit or explicitly correct `xml` bindings remain supported. This is a
source-validation correction; prior desktop acceptance receipts identify their
pre-fix artifacts. Focused regression/package verification covers the fix; no new
native fixture acceptance is inferred from those older receipts.

## Scoped XML source ownership follow-up

OSGi49 removes global prefix borrowing from diagnostics/navigation/hover: core OSGi elements and structural children now require their namespace in the element's own lexical scope. Rebound foreign prefixes, foreign interfaces/comparator/bean/value children and namespaced lookalike attributes cannot become OSGi source references. Compendium suggestions use the binding at the typed tag scope. Legacy unnamespaced Beans roots and the existing default OSGi/prefixed Beans form remain supported.

Closed-source diagnostics, definition and hover reuse the existing bounded namespace-aware validator; invalid reserved/unbound/QName bindings, malformed XML/comments, misplaced XML declarations and oversized current XML stay quiet and invalidate their own cached bean facts. Cached XML bean-class metadata now requires a Beans or supported legacy root before publishing class facts. Completion remains tolerant of a partial compendium tag, with scoped binding checks. Generation shares the stronger XML validation but keeps its prior atomic operations/prompts/pins.

A failing-before regression captures foreign rebound elements producing an OSGi inspection. Three new public regressions cover foreign/scoped declarations and children, navigation/hover/completion/compendium roles, malformed/oversized cache invalidation and correction recovery. Full49 tests and pinned API1.3 build/check are recorded separately. No global usages/Rename, effective DOM/classpath/import resolution, source lifecycle warming or new native acceptance is claimed. Unknown entity-encoded/dynamic namespace/model cases remain outside the lexical admission proof.

OSGi52 filters cached bean/class/method facts through the current innermost open-project owner. One bounded64-project root inventory is read per source-model request; no project files are discovered/read by diagnostics or hover. Observed root-inventory changes or denied/unknown ownership discard old model caches before reuse; only explicit existing navigation/completion paths warm source files again. A local facts view excludes known other projects while retaining established outer source hierarchies; invalid/unknown path ownership remains incomplete. Scope-only source paths are canonical and at most4096 bytes. Shared spring-text Index behavior and host APIs are unchanged.

Three new regressions capture the previously stale nested-project Go-to-Definition and service-type authority, and verify class/bean/method filtering, root close/reopen and grant-denial cold recovery, explicit navigation rewarming, unaffected known outer hierarchy completeness, root-count bounds and no process/network work. Full52/pinned API1.3 build/check are recorded. Automatic source lifecycle warming, unsaved Java overlays, effective cross-module/classpath sharing and native scope transitions remain separate gaps.

OSGi53 adds project-visible source-text checks to FileAction resolution before prompts and again after the concrete confirmation. The ticket/supplied Document comparison alone could not observe a changed actual editor buffer behind that unchanged packet; a failing-before regression now proves refusal and preserves the changed text. This supplements existing path/version/language/project ticket guards and leaves actual atomic apply/undo with the host. It does not create a final filesystem/editor transaction or guarantee pre-transfer bounds for the SDK whole-text read.

Three existing MockHost fixtures were aligned with real resolver context: the opened editor path now matches the action Document path, and the preview-overflow fixture provides an actual source buffer. No production regression credit is claimed for those fixture errors. Full53/pinned API1.3 build/check are recorded separately; native apply/Undo receipts remain dated earlier artifacts.

OSGi57 separates proven source name declarations from class hierarchy completeness. Java bean candidates require exact owned Spring stereotype/Bean annotations and unescaped literal name metadata (up to32 aliases/256 bytes each/4096 facts per file). Foreign, unimported, shadowed, dynamic/concatenated, conflicting or nested annotation owners cannot manufacture bean-class checks. ControllerAdvice package selectors are not bean names. Unsupported name metadata quiets missing-source-name advisories while ordinary Java class facts remain available. The existing Component fixture now imports its real annotation; the missing import was a fixture correction.

XML inner-bean IDs are not global declarations; computed/entity-encoded names are withheld. Conditional-profile and XML factory-method/bean declarations retain literal navigation names without asserting effective bean type. Java conditional declarations similarly do not establish a definite bean class. Imported contexts keep name inventories incomplete. Missing-name Info text now explicitly describes cached source absence and unresolved imported/computed/runtime beans.

Four new regressions capture the former foreign-annotation service-type error and cover independent annotation ownership, literal aliases, dynamic/nested forms, preserved class facts, inner/dynamic/factory/profile XML roles and recovery. Full57 tests/pinned API1.3 build/check are recorded. This is a conservative source model, not a component-scan/meta-annotation/name-generator/runtime container implementation, and no host Rename graph or new native acceptance is inferred.

Exported interface types must also have a unique established project-source identity before service mismatch errors are emitted; unresolved external types remain quiet even when the implementor's own hierarchy is complete. Nested binary `$` names normalize only for source type relationships, never as Rename authority. Inline service/listener factory declarations do not assert the factory class as the produced bean type. Bean completion inserts XML-escaped names and entity-encoded refs do not become false missing-name advisories. Two old hierarchy fixtures now include the Api source they claimed to establish; that fixture repair receives no independent production bug credit.

OSGi58 declares project.read/editor.read/editor.write as optional, matching the existing source-only DiagnosticsProvider fallback. Cheap XML rules can run on supplied text without demanding project discovery or editor mutation grants; unavailable ownership keeps cached facts quiet. Source navigation/completion still need project access, focused generation needs editor.read, and direct command edits require editor.write. Denied operations return their existing error without fallback writes or processes. The new read/write-denial command regression and existing no-project live-diagnostic tests cover degradation; native grant/activation UX is unverified. No SDK or new capability was added.

OSGi59 removes the earlier context-menu command and its direct editor mutation path. The same four generation choices, prompts, exact preview and guards remain in lazy FileActions. Editor read/write capabilities are no longer requested; the host supplies the Document and owns atomic application/undo. OSGi58 capability notes above describe the historical command, superseded here.

OSGi60 registration-listener completion now excludes a first service parameter when cached source facts establish that the exported bean cannot be assigned to it; Object and known supertypes remain valid. Primitive/array parameters are excluded. Unknown external or incomplete source hierarchy remains unresolved and does not produce an incompatibility error. This mirrors the original registration converter's service-bean assignability check only within the bounded source model; it does not implement effective JVM assignability, generic substitution or new diagnostics.

OSGi61 subscribes to Java/XML file changes plus open/save events. Relevant notifications invalidate cached source facts and generation tickets without reading/discovering files; navigation/completion warms again. Event/path batches are bounded at 256, overflow invalidates conservatively. Subscription denial retains source-only behavior. This is guest lifecycle handling; native event delivery, unsaved Java buffer synchronization and diagnostic republishing are not newly verified.

OSGi62 requires both explicit bind/unbind names to be nonempty and resolved in cached source before bypassing the listener lifecycle-interface requirement. Merely supplying empty or missing names no longer hides that source-established diagnostic. Unknown listener hierarchy remains quiet.

OSGi63 declares optional events capability so the lifecycle subscriptions introduced in61 can actually be granted. MockHost capability fixture verifies FilesChanged/open/save admission with events and quiet source-only fallback when denied. The earlier62 frozen native generator package is preserved; no lifecycle native acceptance is added.
