# Quarkus Tools

Ported from Red Hat's Quarkus Tools for IntelliJ (EPL-2.0), `com.redhat.devtools.intellij.quarkus` 2.6.1. Completion and checks for Quarkus configuration in OpenIDEA, plus Dev Mode run configurations:

- Key completion in `application*.properties`, `application*.yaml|yml` and `META-INF/microprofile-config.properties`: full dotted keys in `.properties`, the next segment under the current parent in YAML. Each item shows the property's type, and its documentation gives the description, the default and any deprecation.
- Value completion: enum values, `true`/`false` and log levels.
- Profiles: `%dev.`, `%test.` and `%prod.` are offered at the start of a key. Profiled keys (`%dev.quarkus.http.port`, and YAML under `"%dev":`) complete and are checked as their base key.
- Project keys: `@ConfigProperty(name, defaultValue)` on fields and parameters, and `@ConfigMapping(prefix)` interfaces (`@WithName`, `@WithParentName`, `@WithDefault`, `namingStrategy`, `Optional<T>`, nested interfaces of the same file).
- Warnings: deprecated keys, and unknown keys inside a known group of two or more segments (`quarkus.http.portt`). Quoted map keys (`quarkus.log.category."io.acme".level`) and keys of extensions that aren't bundled are never flagged. Unknown-key warnings appear only after the whole project index has been read.
- Quarkus: Open code.quarkus.io (Find Action) opens the project generator in the browser.
- Dev Mode run configurations: `mvn quarkus:dev`, `gradle quarkusDev` and `quarkus dev`.

The plugin acts only in Quarkus modules, found by an `io.quarkus` mention in the nearest `pom.xml`, `build.gradle` or `build.gradle.kts` up to the project root. The answer is cached per directory for at most 30 seconds. Cached definition/diagnostic/hover requests go quiet after expiry until lifecycle discovery establishes a fresh proof. Delivered build changes invalidate ownership before any warming budget is spent. Conventional Gradle `gradle/libs.versions.toml` plugin aliases (`alias(libs.plugins.quarkus)`) are detected from exact `io.quarkus` plugin IDs. Hyphen/underscore alias accessors are normalized; malformed TOML, ambiguous normalized aliases and custom catalogue declarations remain unknown. Catalogue and settings changes invalidate module proof before editor caches can reuse it.

Go to Definition now links exact literal `@ConfigProperty(name = "…")` injection names and written configuration leaf keys through a read-only cached provider. It requires unique known targets in the same verified project and conventional `src/main/java` / `src/main/resources` module. Profile-stripped keys, named instances and derived `@ConfigMapping` member associations no longer publish symbol anchors or gutter associations: those ranges were unsafe for the host's automatic Rename. Full details and limits are in the P15.4 section below. Typed Qute expression/member resolution is available through the optional pinned Qute/JDT broker described below; generic HTML ownership remains unchanged.

Where keys come from:

- Quarkus 3.40.1's own configuration metadata for about 170 of the most-used `io.quarkus` extensions (86 of them contribute their own keys: 2,797 properties), bundled in `data/quarkus-properties.tsv` (Apache-2.0, see `NOTICE`). Regenerate it with `python3 -I scripts/gen-metadata.py 3.40.1 data/quarkus-properties.tsv [extra-artifact-id…]`; the extension list is `EXTENSIONS` in the script, ids without metadata are reported and skipped.
- The MicroProfile Config keys of the original plugin's static metadata.
- `@ConfigProperty` and `@ConfigMapping` declarations in the project's Java sources.

The plugin reads source and declared metadata only. Dependency-JAR config models now have the explicitly selected Maven subset below; other extensions/layouts remain unknown.

Dev Mode run configurations:

- They need `mvn`, `gradle` or `quarkus` on `PATH`, and each is offered only once the matching `process:` capability is granted.
- A detector can only match marker files by name, so they show in every Maven or Gradle project once granted, Quarkus or not.
- `./mvnw` and `./gradlew` can't be run (a `process:` program must be on `PATH` or absolute).

Checks (each group has a setting): duplicate keys, lines without a separator and empty values (`quarkus.checkSyntax`); values that are not a number, enum constant, log level, duration or memory size, never booleans (`quarkus.checkValues`); `${...}` expressions that look like a typo of a key defined in the same `application.properties|yaml|yml` (never in profile files, never for names that could come from other files, the environment or `-D`; `quarkus.checkExpressions`). An empty value is only a hint, for a property whose metadata documents a default (Quarkus treats it as unset); other empty values are not reported.

Named instances: a key like `quarkus.datasource."users".jdbc.url` is completed (type, default, documentation, values) and checked as `quarkus.datasource.jdbc.url`, for the roots the metadata lists as named (datasource, hibernate-orm, flyway, liquibase, mongodb, redis, oidc, oidc-client, mailer and a few more). Typing the name after the quote and a dot completes the rest of the key; the name itself is never suggested. Map entries such as `quarkus.log.category."io.acme".level` are still not checked.

Java sources (text only, no type information): `@ConfigProperty(name = "` and `@ConfigMapping(prefix = "` complete keys and prefixes from the bundled metadata and the project. `quarkus.checkAnnotations` (on) reports an `@RestClient` injection whose interface, found in the project with `@Path`, lacks `@RegisterRestClient`, and `@Scheduled` with a `cron` of other than 6 or 7 fields (quiet when `quarkus.scheduler.cron-type` is set anywhere), a malformed `every`, neither of `cron` and `every` (a nonempty `cron` takes precedence over `every`); values with `{...}` or `${...}` are skipped. `quarkus.checkUnassigned` (off) flags a `@ConfigProperty(name = "x")` with no `defaultValue`, not `Optional`, whose key no `application*` file, `microprofile-config.properties` or `.env` sets (profiles ignored; quiet for `quarkus.*` keys, bundled keys with a default and until the whole project is read; system properties, `-D`, other config sources and `@ConfigMapping` members are invisible to it). `quarkus.checkHealthMetrics` (on) flags a class annotated `@Liveness`, `@Readiness` or `@Wellness` that does not implement `HealthCheck`, a top-level `HealthCheck` class with none of them, a `@Gauge` in a bean with a scope other than `@ApplicationScoped` and a `@Gauge` method returning a string, boolean or void (OpenAPI annotations are not checked). Quick fixes: Add `@RegisterRestClient` (with its import) to the interface that an `@RestClient` injection names, implement HealthCheck, add `@Liveness` or `@Readiness`, use `@ApplicationScoped`, Add defaultValue (by the field's type) and Add the key to the nearest existing `application.properties`.

Required keys (off by default, `quarkus.checkRequired`): an Info note on a key whose sibling is set while a key the metadata lists as required (no default, not `Optional`) is missing from every `application*` file and `.env`, for example `quarkus.tls.key-store.jks.path` next to `...jks.password`. Only 13 such keys exist in the bundled metadata, since most mandatory settings of Quarkus extensions are `Optional` in the model; values from system properties, other config sources or `-D` are invisible to the check, which is why it is opt-in.

Current-text diagnostics, metadata hover and type inlays use API 1.3. The
existing config checks and Java annotation checks return diagnostics for the
current unsaved document; gutter requests no longer publish a second copy.
Activation and optional file events warm module, source/config and `.env` facts
outside those editor callbacks. Build changes invalidate module proof; failed
reads invalidate absence facts. Diagnostics, hover and inlays do not discover
files, read `.env` or contact a server. Completion and existing quick-fix paths
retain their earlier bounded lookup behavior.

Cold/unknown module proof stays quiet. Unknown-property, unassigned/required-key
and REST-client absence checks require complete, usable caches and optional
`events` permission; local value/syntax/annotation checks may use the established
bundled/current-text facts without that permission. Inputs over 128 KiB and
config documents over 512 keys suppress these diagnostics; results are capped
at 500 diagnostics and 200 inlays. Oversized cached source/config files mark
absence facts incomplete. REST-client diagnostic identities now use namespace
and imports; duplicate qualified interface names and nested interfaces stay
unknown. Project roots also scope cached build proof. Existing metadata and
annotation parsing remain textual rather than compiler/type resolution.

Hover over a known config key or literal Java `@ConfigProperty` name shows its
declared type, description, declared default and deprecation/replacement from
bundled or cached source metadata. Config inlays show the declared type only;
actual configured/runtime values are never displayed by this feature. Profiles
and known named-instance roots normalize through the existing metadata rules.
Unknown, dynamic and conflicting cached metadata declarations produce no hover
or inlay. Java, properties and YAML hover and properties/YAML inlay contributions
are registered separately. Current Java/config snapshots update only their
cached file, so unsaved fixes do not wait for the index TTL.

P15.1 remains partial. The original audited
`MicroProfileConfigHoverParticipant` / `PropertiesHoverParticipant` and
`MicroProfileConfigASTInlayHint` use Java PSI/context participants, including
converter/default-value hints. This slice supplies metadata hover and config
type hints, and migrates the existing safe textual inspections; it does not
resolve converters, runtime config sources, full dependency metadata or effective
injected values. Cache lifecycle after editor discard/close, scheduler hover,
Java converter/default inlays and desktop debounce/hover/inlay rendering remain
unverified or unimplemented. Legacy multi-file quick-fix ownership/version
behavior has not been expanded by this migration.

Not ported: the remaining Java-side MicroProfile/Quarkus language server features
(code lens, broader Java definition/diagnostics and JAX-RS symbols), native acceptance of the optional typed Qute broker,
the in-IDE project wizard (use code.quarkus.io), debugger auto-attach to Dev Mode
and the run dashboard, and keys from extensions outside the bundled set.


### Qute empty-template creation (P15.3 partial)

A Java field with an explicitly imported or fully qualified
`io.quarkus.qute.Template` type and `io.quarkus.qute.Location` annotation offers
**Create empty Qute template** when the annotation contains one unescaped,
explicit relative `.html` or `.txt` string. Static fields, ambiguous/shadowed
names, dynamic expressions and extensionless lookup names remain quiet. Java
listing uses a module proof cached by lifecycle events (maximum age 120 seconds);
it does not discover build files inside the editor request. Alternatively, open
a current Maven `pom.xml` with a direct literal, unscoped `io.quarkus:quarkus-qute`
or `io.quarkus:quarkus-rest-qute` dependency to enter a relative template path.
Only conventional module `src/main/resources/templates` targets are supported.
Profiles, dependency management, property-based coordinates, Gradle and custom
template roots do not establish this proof.

Resolution requires the original document text/version, a concrete owning
project/module, current project-visible source and fresh matching Maven proof. It previews the
exact target and exact content (**empty, zero bytes**), then rechecks source,
project, build text, target collision and ordinary directory parents. Cancel
returns no edit. The result is one host-managed non-overwriting Create operation
with editor opening and Undo; the plugin does not write files, start processes
or fetch starter content. Unsaved target buffers and existing files are refused.
Project reads include open unsaved buffers and do not prove disk-saved state.
These reads are freshness checks, not atomic source-version preconditions: the
SDK leaves a final read/host-prepare gap; target non-overwrite is enforced by the
host operation. No source or inverse file is modified.

The audited original `QuteGenerateTemplateAction` (Quarkus plugin 2.6.1) asks the
Qute language server for `qute.command.generate.template.content`, while
`AnnotationLocationSupport` resolves Java assignment/type context and
`TemplateNameStrategy` supports implicit naming strategies. This action does
not reproduce those semantic services: it explicitly creates an empty file.
Generated model content, constructor assignments, `@CheckedTemplate`, injection,
extensionless/naming-strategy lookup, Qute expression completion/validation,
remote starter catalog/codestarts and language-server ownership remain separate gaps.
Native desktop preview/apply/Undo acceptance for this action remains unverified.


Source code: this plugin's source repository, directory `plugins/quarkus`.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


## Selected dependency config models (partial P15.2)

**Quarkus: Read Config Model from Selected Maven Dependency JAR…** lists supported
literal direct dependencies in the current project's root `pom.xml`, then reviews
the exact project/POM, GAV, declared scope and standard Maven-cache archive before
reading bytes. It requires optional `fs.read:~/.m2/repository`; a stat preflight
checks grant/type/size, and the real host enforces canonical/symlink grant boundaries.
The current project-visible POM (including unsaved text) must remain unchanged after
confirmation and parsing. No Maven/Gradle/Java, deployment-artifact resolution,
cache/home scan, network, code execution or extraction is involved.

Supported metadata is the modern layout already used by this repository's bundled
metadata generator: `META-INF/quarkus-config-doc/quarkus-config-model.json`, with
`configRoots[].items`, explicit `ConfigProperty` records and `path.property`.
Known `ConfigSection` records can contain bounded nested items. Only literal plain
property names are imported, with their declared types, scalar defaults,
deprecation replacements and static `enumAcceptedValues.values.*.configValue` hints.
Optional `quarkus-config-javadoc.json` descriptions are joined by exact
`sourceType.sourceElementName`, treated as bounded escaped text. No source class
is loaded or resolved. Unknown item kinds, map/quoted/named/dynamic/array paths,
additional path aliases, dynamic converters and required-key inference are skipped.
Conflicting canonical names and unsupported structures fail without installing a
partial replacement. An archive with no supported properties supplies no facts.

`quarkus-extension.properties` provides deployment coordinates, not configuration
keys. `quarkus-config-roots.list` and older `quarkus-javadoc.properties` need class
resolution to determine keys and types. They are deliberately **not** converted
into a configuration catalog. Runtime JARs without the modern config model remain
unsupported; the command never follows an inferred `-deployment` artifact. Nested
or fat-JAR layouts and unknown model revisions are not guessed.

The selected config model feeds the existing completion, hover, diagnostic and type
inlay lookups from memory, after project-source metadata and before bundled fallback.
Hover on a selected-model key labels declared GAV/scope, archive/entry paths and a
SHA-256 source fingerprint. Every known-key hover now labels the winning catalog:
project-source current/cached declarations, selected dependency-JAR snapshot, or
bundled fallback. The bundled label includes the packaged TSV header's Quarkus and
MicroProfile source dataset versions; these are not inferred project dependency versions.
Source declarations retain precedence over selected models and packaged fallback.
The lookup performs no provider filesystem IO and does not show configured/runtime values. Existing module proof is preserved:
a selected archive does not itself prove that an arbitrary source directory is a
Quarkus module. It adds no class/enum resolution, injection graph or Rename anchors.

One snapshot/project is retained for ten minutes, up to eight exact project roots;
it is session-only and must be imported again after idle reclaim/reload. Matching
POM open/save/change events invalidate it when events are granted. Providers never
reread the archive/POM, so unobserved changes remain historical-snapshot limitations.
Cancelled, denied, missing, corrupt or unsupported replacements retain the previous
still-valid snapshot within the same POM source scope; an explicit command that
observes changed POM bytes invalidates the old digest-bound snapshot even if the
new import fails. Expired or invalidated facts are not reused. Source declaration
and cached metadata are provenance, **not resolved/runtime classpath membership**.

Maven scope is deliberately narrow: root direct literal groupId/artifactId/version,
JAR/default type, compile/runtime/provided/test scope, no classifier/true optional.
BOM/dependency management, parent/property/profile resolution, Gradle, systemPath,
alternate caches, timestamped snapshots and transitives remain gaps. POMs must be
complete supported XML (256 KiB, 4,096 elements, nesting 32, 128 dependencies);
DTD/entity/comment/CDATA and foreign namespaces are rejected.

The private archive reader copies Spring Boot Tools' bounded parser with an adapted
entry allowlist: classic stored/deflate ZIP only, 8 MiB archive, 2,048 entries,
256-byte UTF-8 names, 1 MiB central directory, 4 KiB extra/comments and archive
comment, 64 MiB total declared expansion, 512 KiB/metadata entry and 768 KiB combined,
100:1 expansion ratio. Encryption, ZIP64, unsupported methods, duplicate/path-trick
entries, overlapping regions and inconsistent headers/descriptors are refused.
Metadata CRC/length/UTF-8 are checked; SHA-256 fingerprints bytes rather than
verifying publisher signatures. Model parsing is limited to 128 roots, 4,096 items,
nesting 16, 256-byte names/types, 1,024-byte defaults/enum values and 128 enum values.
Static fixtures include an independently generated ZIP; workflow tests use an
injected `/m2` MockHost root because its pinned implementation does not expand `~/`.
Production always uses the declared Maven-cache grant; native desktop grant and
canonical-path acceptance remains separate.

Original/source audit: decompiled `BuildToolDelegate.getDeploymentJarId` reads
`META-INF/quarkus-extension.properties` to locate a deployment artifact;
`QuarkusConfigRootProvider` resolves PSI fields/types and joins their Javadoc
properties. Those contracts do not make extension coordinates/class lists a key
catalog. `scripts/gen-metadata.py` and `NOTICE` record the modern Quarkus 3.40.1
config-model/Javadoc layout used for bundled data. This subset follows that static
layout and preserves the existing Qute intentions/current-text documentation flow;
it does not reproduce the original PSI/classpath engine.


### Project-source navigation and anchor audit (P15.4 partial)

The native definition provider preserves a bounded useful configuration subset:
from a plain config leaf key to its one exact literal Java `ConfigProperty` name,
and back. Annotation ownership requires the explicit MicroProfile import, with no
conflicting import/local type; dynamic, escaped, concatenated, generic-shadowed and
unknown names remain quiet. Parent YAML nodes, profiles, named instances and
placeholders are excluded. Both current caches observe unsaved supplied text before
lookup; providers do no discovery, file reads, build execution, network or regex
expression/type inference.

Destinations must come from a fresh complete project cache with granted event
tracking, within the same conventional module source/resource layout. Duplicate
known targets yield no result. Source facts have a 120-second lifetime; module ownership has a separate 30-second lifetime. Source/provider
input is capped at 128 KiB, 64 Java types, 128 bracket nesting, 512 declarations per
file, 512 cached files and 4,096 candidate records per lookup; oversized/incomplete
or denied context yields no navigation. The returned UTF-16 range selects the
actual source literal/key. This is source navigation, not a runtime effective-config
or classpath claim. Nonstandard/generated/test layouts and broader ConfigMapping
navigation still require an owner/path model.

The previous indexer mixed `cfg#`/`java#` associations, profile stripping, normalized
keys and kebab-transformed method names across owners. The host uses these anchors
for automatic Rename too, so its definitions/references/markers are now empty,
clearing the unsafe path on reindex. The read-only provider above preserves proven
navigation without registering replaceable symbols. Scoped usages/Rename and
ConfigMapping/profile gutter associations are not claimed.

Original contracts audited: `MicroProfileConfigDefinitionParticipant` supplies the
qualified ConfigProperty annotation/name contract to the PSI definition participant.
`QuteJavaDefinitionAction` passes project URI, resolved source type, field, method
and parameter to `QuteSupportForTemplate.getJavaDefinition`, which resolves PSI
classes, inherited fields/methods and record components. `QutePsiReference.resolve`
itself is not evidence of equivalent member/type resolution. Our existing Java
cache contains configuration/REST facts rather than a complete classpath/source
owner model; no Qute type/member destinations are invented from template text.
Qute project-source type/member navigation remains pending feasible model/LS work.
Prior JAR metadata, source diagnostics and atomic Qute creation are preserved.

Fixtures cover bidirectional exact ranges after emoji, cold/no-discovery behavior,
unsaved source replacement, duplicate/module boundaries, project switch/removal and
denial, annotation impostors, malformed/oversized/escaped/dynamic names, and
withheld transformed/profile/named anchors. Native Go to Definition acceptance is
separate from deterministic tests and the pinned package check.


Module-proof follow-up: a native deletion test found that a bare cached positive
module boolean could survive source-index refreshes indefinitely when a build-file
event was missed. Positive and negative module facts now carry a non-renewing
30-second observation time. Cache-only editor requests never refresh that proof by
reading the filesystem. A missed event can therefore leave an at-most-30-second
stale window; then navigation/live metadata become quiet until lifecycle discovery
proves the module again. Subscribed events are invalidation hints, not a guarantee
of immediate delivery.

Build invalidation is processed before the bounded event-warming loop, even if its
deadline is already exhausted. Explicit build deletion also records a bounded
in-memory tombstone, preventing retained editor/source text from reasserting that
build until a creation/modification/save observation. At most 1,024 tombstones and
64 directory module proofs are retained; tombstone overflow suppresses module
proofs until plugin restart. The same delivered-deletion guard protects the existing
Qute Maven proof. Fixtures cover expiry with fresh source indexes, cache-only
navigation/diagnostics/hover, expired-batch deletion, retained readable deleted
source and explicit recreation. Native retest is required against this new build.


### Offline Maven/Qute starter (P15.3 bounded subset)

Allow optional **editor.read** for the source-owned starter commands.
Open a Java or XML file in the parent project, then run **Quarkus: Prepare Offline
Maven/Qute Starter…**. Enter groupId, portable lowercase artifact/folder name,
project version and an **explicit Quarkus 3 platform version**. The plugin does not
fetch or resolve that version. The preview shows every destination and exact UTF-8
content for four files: `pom.xml`, `src/main/resources/application.properties`,
`src/main/resources/templates/hello.txt`, and a scope/provenance README.

Preparation does not write. In the same unchanged source file, invoke **Create
Reviewed Quarkus/Qute Starter…** from intentions within five minutes; its separate
resolution again previews all content and returns four atomic non-overwriting
Create operations, opening the new POM. Existing destinations/files, open unsaved
targets, changed source/version/owner and expired or replayed plans are refused.
It only creates a vacant direct child of the existing project, with no merge,
source edit, wrapper, build, process or HTTP request. Project reads include unsaved
buffers; source checks leave the SDK's final read/host-prepare gap. The host's
atomic create/non-overwrite contract supplies target conflict handling.

After applying, **Quarkus: Open Reviewed Starter…** checks all four expected files
and asks to open the folder through the ordinary project trust/open flow in a new
window. This acknowledges a request, not successful project import/trust. Missing,
changed or partially applied files refuse opening. Current project-visible content
can include unsaved buffers and does not certify disk bytes or extra files.

This is a small offline Qute scaffold: a platform BOM, Quarkus Maven plugin, direct
`quarkus-qute` dependency and literal text template. It has no Java sources, REST
endpoint or generated test suite; future Java sources need an explicitly chosen
compiler plugin/release. Version availability, JDK compatibility, dependency
resolution/build/runtime behavior, remote registry streams/extension selection,
Gradle and downloaded codestarts are unverified. **Open code.quarkus.io** remains
available for the official generator. The original wizard's registry/download
workflow is broader; neither it nor Qute language-server semantics are emulated.
No language registration, language-server configuration or type resolution is
changed. Native starter preview/apply/open acceptance has not been run.

Qute Maven dependency proof now also checks coherent XML namespaces: reserved
xml/xmlns rebinding, unbound prefixes and malformed declarations cannot enable a
creation action. Existing conservative unprefixed Maven namespace/no-namespace
support is retained; prefixed element layouts remain intentionally unsupported.

### Explicit Qute template source links (P15.4 subset)

Go to Definition on the exact unescaped `@Location("pages/home.html")` value of a
non-static, source-proved Quarkus `Template` field opens that exact known template
file at its start. The existing structured Java import/type/annotation checks and
direct Maven Qute-module proof apply. A same-spelled template in another module or
nested project is not a substitute. This is a literal path link, not Qute expression,
member/type resolution or runtime template lookup, and adds no indexed Rename graph.

Activation/file events warm at most 256 template paths (project query cap), retaining
at most 16 module inventories; provider requests do not search/stat/read files or call
network/build tools. Inventories expire after 30 seconds; cold, failed, capped,
unknown-owner or denied-watch states stay quiet until lifecycle warming. Existing
module/source bounds also apply. Only explicit portable ASCII `.html`/`.txt` paths
below conventional `src/main/resources/templates` are linked. No suffix guessing,
fragments, custom roots, implicit field naming or CheckedTemplate lookup is inferred.

Template/build events invalidate inventory before discovery deadline checks. Delivered
removal tombstones dominate retained readable old source until creation/modification
is observed. At most 1024 bounded tombstones are retained; overflow disables these
links until plugin restart. Deleting/recreating ancestor directories without matching
file observations can conservatively keep links quiet. Undelivered changes remain
subject to the 30-second stale window; target existence is a cached observation, not an
atomic filesystem/LS truth. Native navigation acceptance has not been run.

Selected JAR dependency proof also validates coherent XML namespaces before offering
an import: reserved xml/xmlns bindings, unbound element/attribute prefixes, foreign
or mixed element namespaces are refused. Legal fully prefixed Maven documents and
legal `xml:lang` remain supported, as do the existing legacy no-namespace POMs.
Valid XML using mixed namespace/no-namespace layouts or unsupported entity forms
can remain conservatively quiet. This establishes a literal source owner, not a
resolved Maven model or runtime classpath.

Module gating now uses bounded literal build declarations instead of raw
`io.quarkus` substring matches. Maven requires an admitted direct literal
`io.quarkus` dependency; Gradle supports top-level literal Quarkus plugin IDs or
production dependency coordinates outside ordinary comments/strings. Merely
mentioning the group in a POM description/comment or unrelated string is quiet.
Unsupported Groovy slashy/dollar-slashy/triple-single-quoted forms remain unknown
(the conservative slash guard also withholds scripts using division). Source
facts do not establish an effective classpath, inherited profile, dependency
version, runtime or arbitrary Gradle model. Existing Qute dependency predicate
and provider no-discovery/freshness gates remain.

Project-source catalog/config/client consumers and exact config definitions
recheck the current deepest host project owner for cached paths. A nested opened
project's declarations cannot become outer-project completion/hover/inlay facts
or definition targets, including when ownership changes after warming. Retained
foreign/unproved source facts suppress absence-based claims rather than imply
complete outer-project discovery. Shared indexes are unchanged; this is source
namespace ownership, not module dependency/classpath resolution.

Java ConfigProperty/ConfigMapping metadata and ConfigProperty editor injections
require a literal framework-qualified annotation spelling or one matching
explicit import (or known same-package spelling), with no same-file simple-name
shadow. Foreign imports and unresolved/wildcard-only simple names stay unknown.
Key/prefix completion also proves a real annotation token and a flat name/prefix
string argument, preserving partial strings without reading comments or string
contents as annotations. Known fully qualified annotations supply metadata;
this source identity check does not resolve external types/classpaths or add
semantic Rename authority.

Source annotation scalar metadata accepts exact ordinary unescaped string
literals. Compound names/prefixes/defaults, escapes and text blocks remain
unknown; a first string fragment is never substituted for a computed value.
Unsupported declarations/method scalars are omitted while supported facts in the
same file remain available. Such source models are retained as incomplete, so
they cannot authorize absence-based unknown-key claims. Constant-expression and
compiler string semantics are not emulated.

ConfigMapping WithName/WithDefault/WithParentName modifiers require their literal
SmallRye namespace identity. Explicit foreign modifiers are ignored; unresolved
lookalikes withhold the affected member and keep the source model incomplete.
Naming strategies require a known literal ConfigMapping enum owner, including
explicit static constant imports; an arbitrary `.VERBATIM`/`.SNAKE_CASE` suffix
never selects a strategy. Ordinary known modifiers/default naming remain; no
external annotation/type graph is resolved.

Same-file ConfigMapping group expansion follows only a literal unqualified type
in an unambiguous lexical owner recorded by the source AST. Two nested groups
with the same simple name keep their own fields; a qualified external spelling
is never replaced by the first local type. Unproved or competing bindings are
withheld and mark the source model incomplete. No inheritance/import/classpath
or Qute type graph is synthesized.

REST-client registration proposals preserve the qualified injected source type.
Duplicate cached declarations withhold the proposal, and the uniquely selected
owned target is re-read/reparsed before edits are returned; changed package or
registration state cannot reuse the cached identity. The proposal uses an FQN
annotation at the actual declaration, preserving compact package/import headers
without import collisions. This is a fresh source proposal, not a guarantee of
later host apply/native Undo or effective CDI registration.

Add Key proposals use an owned `application.properties` in the same literal
`src/main` module as the Java document. They re-read bounded target keys before
proposing an append, and withhold unsupported writer spellings or existing keys.
Sibling-module runtime configuration sharing is not inferred. Later host apply
and concurrent target changes remain outside this proposal-time check.

Scheduler diagnostics require a literal `io.quarkus.scheduler.Scheduled`
annotation or an unambiguous imported/same-package owner. Explicit foreign,
unresolved, wildcard-only or locally shadowed simple names stay quiet; literal
fully qualified framework annotations retain source diagnostics. This does not
resolve inherited annotations or an effective scheduler/runtime model.

REST-client injection checks likewise require the MicroProfile `RestClient`
annotation's literal owner on fields/parameters. Foreign or unproved annotation
names cannot create a registration diagnostic or cross-file registration fix;
fully qualified framework annotations remain supported. Effective CDI injection
and inherited/external registration remain outside the source model.

The REST source catalog records each top-level interface's own literal JAX-RS
`Path` or MicroProfile `RegisterRestClient` annotation. A file-wide annotation
mention cannot tag unrelated interfaces. Foreign annotations supply no framework
facts; unresolved registration keeps absence claims unknown. Both `javax` and
`jakarta` literal JAX-RS owners and fully qualified annotations are recognized.

Gauge checks require the literal MicroProfile metrics annotation owner. Scope
rewrite candidates require supported `javax`/`jakarta.enterprise.context`
annotation owners (ApplicationScoped, RequestScoped, SessionScoped, Dependent).
Foreign or unresolved annotations and unsupported scopes are not rewrite targets;
fully qualified Gauge annotations retain source diagnostics. Effective bean scope
and runtime metric registration remain unproved.

Health checks use literal supported MicroProfile qualifier and HealthCheck type
owners. Unknown implemented interfaces or superclass inheritance cannot prove
HealthCheck absent; unproved qualifier spellings withhold absence diagnostics.
Messages describe direct source declarations rather than effective runtime
registration. Implement-interface proposals use a fully qualified HealthCheck
without changing existing imports. Unsupported Wellness/AsyncHealthCheck owners
and external inheritance/meta-annotations remain unknown.

Metadata unwraps `Optional<T>` only when its literal type owner is
`java.util.Optional` (FQN or unambiguous import/same-package proof). Foreign,
unresolved or locally shadowed Optional types cannot expose nested mapping
fields as framework facts. Their source spelling remains a custom type; no
converter or classpath model is invented.

Health-qualifier and Gauge-scope annotation proposals use fully qualified source
spellings and preserve existing imports, so an imported foreign simple name
cannot capture the generated annotation. These remain source proposals; later
host apply and classpath availability are unverified.

### Additional Qute editor support

Within fresh known conventional Qute template modules, static include tags
navigate to uniquely owned cached template files and offer cached include paths.
Section snippets complete in HTML/plaintext templates without replacing the host
language. Source-fact Info notes identify static includes absent from a complete
known inventory while acknowledging custom runtime locators. Java implicit
injected Template fields and supported CheckedTemplate method declarations link
to exact templates; ambiguous variants, overloads, dynamic paths and custom
naming strategies stay unknown. This does not add a complete Qute Java type model,
full language-server semantics or template Rename.

## Optional typed Qute language server

The [Qute/JDT adapter setup](adapters/README.md) supplies a broker for official
Qute Language Server and Eclipse JDT LS with the official Qute/MicroProfile JDT
extensions. Hash-pinned profiles, isolated per-session Java workspaces and exact
native project settings are included. The native client starts one custom broker
session for Java and `qute-html`; the broker handles Qute custom type/model callbacks
through its own JDT server and propagates unsaved Java changes, closes and watched
file changes before accepting refreshed type results.

Only the exact `.qute.html` suffix is contributed automatically, with HTML mode
and grammar. Ordinary templates can use explicit project file-type overrides;
generic HTML and Java are not reassigned by plugin activation. Setup requires the
optional approved toolchain and deliberate project configuration. The adapter
ships no IntelliJ code or bundled proprietary artifact. Reproducible opt-in live
tests exercise actual Maven Quarkus fixtures, inferred CheckedTemplate parameters,
Java member completion and stale-field withdrawal/restoration. Native desktop
visual/keybinding acceptance remains separate.

Java source lenses/document symbols now expose exact literal JAX-RS mappings and
scheduler triggers. Their action opens revalidated read-only declaration evidence;
it does not invent running URLs. Java inlays show declared ConfigProperty defaults
and ConfigMapping defaults/converters, with canonical annotation ownership; they
are source facts rather than evaluated runtime values.
