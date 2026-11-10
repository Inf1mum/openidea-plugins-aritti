# JPA Assist

JPA and Spring Data support for Java and Kotlin in OpenIDEA. The plugin reads
the source text only: it ignores comments and strings, and finds classes by
simple name (next to the file first, then anywhere in the project).

## Live diagnostics (API 1.3, partial P09.3)

The versioned DiagnosticsProvider reads the supplied Java/Kotlin/XML document,
including unsaved edits. Open/save events no longer publish a second diagnostic
set or scan the project. The provider performs no file reads, source discovery,
build inspection, process execution or network operations.

Live rules reuse the existing lexical checks: entity eager fetch/final/Lombok
warnings; Kotlin data class, immutable property/collection warnings; conflicting
class roles and attribute signatures; known basic attribute-type incompatibilities;
local lifecycle callback signatures; literal `@Query` parameter references; and
local persistence/mapping XML values, required attributes and duplicate names.
Settings still gate their corresponding rules. Explicit foreign annotation/basic
type imports and Kotlin import aliases are conservatively quiet. Bare annotation
names retain the existing source convention; this is not compiler type resolution.

Positive JPA type facts may come from previously supplied/opened source documents
under the exact verified project root and qualified name. The cache never asserts
project completeness: unknown, expired, duplicate, library and inherited mappings
are quiet. Consumption also rechecks the current innermost project owner of each
matching cached qualified-name source using project metadata only. Opening a nested
project therefore withholds that source from outer-project positive type diagnostics
and direct-property repository completion until it belongs to the requesting owner
again. Repeated type checks in one diagnostic request share one ownership result (at most
32 distinct type-name ownership checks per request; remaining unknowns stay quiet);
no source discovery or file reads are added. File changes invalidate entries;
current text replaces a file's facts.
Limits: 512 KiB/current document, 2,048 annotation markers, eight roots, 256 files
and 4,096 attributes/root, 512 attributes/file, 120-second fact lifetime and 512
reported findings. Overflow drops facts rather than asserting absence.

Positive cached type identities now require a bounded named package/class and a
verified top-level class declaration at the parser's exact source position. Nested
or local declarations do not acquire package-plus-simple-name identities; no
containing-type/local-class name graph is guessed. Moving a type into a nested
scope invalidates its old fact. Dot-segment paths and malformed/oversized names
remain unknown; unrelated nested helpers do not hide a valid top-level identity.

Cached source-class hints require matching supplied source groups: the same
package-directory root, or the same flat directory when that layout is not present.
Main/test/other roots do not backfill each other's facts; no build/classpath module
identity is inferred. The cache retains only qualified name, source group and
attribute count, discarding unused imports/columns/type payloads. Existing limits
and owner/expiry checks remain. Cache-derived Enumerated results are Info labeled
as supplied-source matches with unresolved compiler/external bindings; exact
primitive/JDK scalar checks retain their separate warnings.

A narrow current-class `@NamedAttributeNode` name check now runs under
`jpaAssist.checkMappingNames`. It compares direct literal node names only against
this supplied Java entity's source-declared persistent fields. It requires exact
owned Entity/NamedEntityGraph/NamedAttributeNode annotations in one persistence
namespace, one explicit field Id, a balanced non-inherited/non-generic single
class, a matching named package/file under the owning root's `src/main/java`, and
complete plain field declarations. Scalar types must be primitives or explicit
qualified/single-import JDK types; implicit ambiguous types, wildcard imports,
unknown/module/submodule layouts, methods/getters/constructors, initializers,
access/converter/override mappings and unsupported field shapes remain quiet.
Nested subgraph nodes, dotted paths, escaped/dynamic/concatenated values and
foreign/mixed annotations are not resolved. No project discovery, file reads, graph-index or library-hierarchy queries are added. Findings label the current source model,
not effective XML/runtime mappings. Source is capped at128KiB/128fields/256annotations
by the existing strict snapshot; at most128 direct-node findings are added before
the existing combined512 result bound. Native typing/render acceptance is separate.

Lifecycle callback diagnostics under `jpaAssist.checkListenerSignatures` now require
an explicitly imported/qualified current Java Entity in the exact owning root's
named-package `src/main/java` path. A bounded single balanced class, no supertype,
generic or implemented interface, and one consistent persistence namespace are
required. Plain callback methods report static/final modifiers or non-void/argument
signatures at the method name; supported return tokens are void and primitives.
The live provider no longer infers listener role from the word `persistence`.
A later supported local method repeating a lifecycle event is also reported; one
method handling multiple distinct events is allowed. Unsupported or invalid callback
signatures do not establish duplicate-event facts.
Listener/mapped-superclass roles, foreign/unimported/mixed annotations, unknown
layouts and unsupported method syntax remain quiet. This is supplied-source
annotation validation, without effective XML mappings, classpath or discovery.

Live scalar type deny lists for Temporal/Enumerated/Version now require Java
primitive types or exact known JDK FQNs/single imports. Short JDK spellings need a
plain single non-inherited/non-generic Java class so local/inherited type names
cannot borrow those imports; bare implicit, foreign, nested or ambiguous owner
spellings remain unknown. Legacy lexical scalar helpers are fixtures only.
The separate positive cached source-model branch for Enumerated is retained with
full qualified source spelling, current project owner, ambiguity and expiry checks.
No new discovery or classpath lookup is performed. Positive cached role use now
requires that same plain current Java type context and unambiguous single-type
imports. Generic/inherited/nested/other type declarations suppress reference-owner
facts, including FQNs in those contexts. Visible current class-name shadows of a
qualified name's first segment remain unknown. Nonprimitive JDK comparisons use
the same context gate; Java primitive tokens remain unshadowable.

Relationship signature diagnostics now use the same exact scalar owner proof and
explicit known JDK collection FQNs/single imports. Foreign or shadowed types named
String/List/Map do not borrow basic/collection semantics. Unshadowable Java
primitive/array shapes remain supported. Structural duplicate-association and ID/
getter checks retain their existing behavior; no unknown target role is inferred.

The live Kotlin collection note now requires an exact kotlin.collections read-only
interface FQN or unique single import in a plain, closed, non-inherited single
class with body properties. Bare defaults, foreign names, type aliases, generic/
nested/inherited/primary-constructor contexts remain unknown. It is an information
note about read-only operations exposed to Kotlin source, not a claim of runtime
immutability or inability of a persistence provider to populate the collection.

For explicit literal `@Query(nativeQuery=true)`, the live named-parameter check
ignores standard SQL block/line comments and quoted text while retaining source
byte offsets. Unknown flags, source escapes, dynamic expressions, dollar/backtick/
bracket quoting, executable/hint/nested/unclosed comments and unclosed quotes stay
quiet. This is a literal source subset, without SQL dialect or database resolution;
legacy pure query fixtures remain separate. Any live query containing expression
or property-placeholder markers stays unknown, including JPQL templates; expression
operators/defaults cannot become literal method bindings. No expression evaluation
or environment/property lookup is performed.

The live missing-setter note is limited to a plain nonabstract current Java
Entity with only consumed canonical JPA annotations. Inherited/interface/generic/
nested/mapped-superclass or extra annotation/code-generator contexts stay unknown.
The note describes an absent source-declared setter and leaves generated/external
mappings unresolved. Structural mapping conflicts retain their independent checks;
no inherited setter or generator graph is queried. The same source-only note is
available for one class-level Access with an exact same-namespace fully qualified
AccessType.PROPERTY value (positional or named value); FIELD, dynamic/imported enum
aliases, member/multiple access overrides and incomplete contexts stay unknown.
A mapping must lead a supported public/protected nonstatic zero-argument getter;
method-body/nested-object mappings do not establish the outer property's role.
Void, private, generic, nonboolean is-getters, transient getters and unclosed class
contexts stay unknown for this optional source note.

Owned live attribute conflicts use annotations attached to declaration prefixes.
Mappings in initializers, method/anonymous bodies, nested type declarations and
annotation arguments do not belong to an outer field/getter. Direct ID/relation/
access conflicts remain supported, with a1024-member work bound for this path.
Legacy flat annotation fixtures remain separate.

Live entity eager/Lombok and scalar-type notes now share the declaration-site
proof. Helper/anonymous/initializer/nested-annotation mappings are not borrowed by
the current entity; Lombok class notes require class-level attachment. A closed
first selected class must own the parser's exact declaration position; other
managed-class layouts remain unknown, without walking a type hierarchy. Direct
notes retain source ranges, owned type checks and the1024-member work bound.

Owned getter property intent includes direct EmbeddedId as well as Id, with the
same source-only setter safeguards. Embedded key schema/type/relationship validity
is not inferred. Mixed-access results are Info describing observed field/getter
mapping placements and unresolved hierarchy/XML access, rather than claiming
which annotation the provider ignores; legacy pure warnings remain separate.

Explicit framework annotation owners must now match the supported canonical
names. Nested lookalike packages under javax/jakarta.persistence, Lombok and Spring
Data do not borrow framework behavior; Query is the JPA repository annotation
and Param is the common repository query annotation. This exact-name guard also
protects cache admission and strict current-source action snapshots. All consumed
framework annotation names now require canonical FQNs or unique canonical single
imports; missing/wildcard-only/aliased/ambiguous ownership stays unknown. The full
live mapping tag registry includes structural/access/ID and graph tags. Visible
local declarations/type-parameter/prefix shadows and mixed javax/jakarta annotation
namespaces remain quiet. Public positive fixtures use explicit canonical ownership;
legacy pure lexical helper fixtures remain separate.

The following broader checks are **no longer automatic**: missing IDs across an
inheritance hierarchy; repository ID/derived-property checks; named-query/entity-
graph-name and cross-owner mapping-name resolution; association target existence; external listener usage and
constructor resolution; Kotlin build-plugin/no-arg/all-open checks; and XML source-class
existence/persistence warnings. Their legacy algorithms remain regression fixtures,
not live diagnostic parity. Safely restoring them needs a bounded, project-owned
model with explicit completeness and mapping/build invalidation. Existing completion,
navigation, source quick fixes and atomic DTO/basic-attribute actions are retained;
build-derived quick fixes require already established build facts and are not warmed
implicitly by live requests. No new full-project review command invokes the unsafe
legacy global/simple-name cache.

Original comparison: `JpaObjectClassSignatureInspection`, `JpaAttributeTypeInspection`
and `JpaMissingIdInspection` use UAST/resolved persistence roles and model browsers;
our source subset cannot reproduce those semantics. The reviewed persistence tree
screenshot (`persistence-tool-window-0.png`, family/documentation evidence) shows
attributes and cross-entity relationships, not evidence of compiler/database parity.
Provider tests are separate from explicitly invoked legacy full-context fixtures.

## Legacy rule inventory (not a live support matrix)

This table records the historical source algorithms and quick-fix fixtures. Only
rules explicitly listed in the live section above run automatically now.

| Check | Severity | Quick fix |
|---|---|---|
| `@Entity` without `@Id` / `@EmbeddedId` / `@IdClass`, own or inherited (`entity-missing-id`; quiet when a superclass is a library class or not in the project) | error | |
| `@Temporal`, `@Enumerated`, `@Version` on a type JPA doesn't allow (`attribute-type`) | warning | |
| Lifecycle callback with the wrong parameters or return type (`entity-listener-signature`) | error | |
| `final` entity class (Java) | warning | Remove final |
| Kotlin `data class` entity (quiet when `equals`, `hashCode` and `toString` are all overridden) | warning | |
| Kotlin entity with a `val` attribute (`jpa-val-attribute`); `@Version val` still warns | warning | |
| Explicit source-owned Kotlin read-only collection on an association (`jpa-immutable-collection`) | info | Source API note; no runtime/provider failure claim |
| Entity that is an inner class, an interface or an enum, or is also `@Embeddable` or `@MappedSuperclass` (`jpa-entity-nested`, `jpa-entity-not-a-class`, `jpa-entity-multiple-roles`) | warning | |
| `@EntityListeners` class with no lifecycle callback (`jpa-listener-without-callbacks`) | warning | |
| Lombok `@Data` on an entity | warning | Replace with `@Getter` and `@Setter` |
| Lombok `@EqualsAndHashCode` on an entity | warning | |
| `FetchType.EAGER` on any association | warning | Fetch lazily |
| `@ManyToOne` / `@OneToOne` relying on the eager default | info | Fetch lazily (adds the `FetchType` import) |
| Direct attribute association/ID-kind conflicts or owned basic/collection relationship mismatch (`jpa-attribute-multiple-relations`, `jpa-relationship-type`, `jpa-multiple-id-kinds`) | warning | |
| Current-source field/getter mapping placement mismatch without `@Access` (`jpa-mixed-access`) | info | Effective hierarchy/XML access unresolved |
| Entity with property access (`@Id` on a getter, no mapping annotation on fields) whose annotated getter has no setter (`jpa-missing-setter`) | info | |
| Association whose target class is in the project but is an `@Embeddable`, a `@MappedSuperclass` or has no `@Entity` (`jpa-relationship-target`; quiet with `orm.xml`, for classes found twice or outside the project) | info | |
| `@EntityGraph("x")`, `@EntityGraph(value = "x")` or `getEntityGraph("x")` naming no `@NamedEntityGraph`; `@EntityGraph(attributePaths = …)` whose first segment is not a property of the repository entity (`jpa-unknown-entity-graph`, `jpa-unknown-entity-graph-path`; quiet with `orm.xml`, a non-literal graph name or a library superclass; setting `jpaAssist.checkEntityGraphs`) | warning | |
| Listener class with constructors but none public without arguments (`jpa-listener-constructor`; Spring-managed listeners are exempt) | info | |
| Kotlin entity with no no-arg constructor while the build names no `kotlin-jpa` plugin (`jpa-kotlin-plugin-missing`; read from `build.gradle(.kts)` and `pom.xml` up to the project root, quiet for version catalogs, convention plugins and `noArg`) | info | Add the plugin to the Gradle build (copies the Kotlin version) |
| Redundant `constructor()` when the build applies `kotlin-jpa`; redundant `open` when `allOpen` names `jakarta.persistence.Entity` | info | Remove it |
| Unknown attribute in `@AttributeOverride`, `@MapsId`, `@MapKey` or a top-level `@NamedAttributeNode` (`jpa-unknown-*`; quiet when the class is not in the project, its name exists twice or a superclass is a library class) | warning | |
| `:name` in a `@Query` string that is not a parameter of the method or its `@Param`; `@Query(name = …)` or `createNamedQuery(…)` naming no `@NamedQuery` (quiet with `orm.xml` or `jpa-named-queries.properties`) | warning | |
| `persistence.xml` / mapping file: invalid `transaction-type`, `shared-cache-mode`, `validation-mode`, `exclude-unlisted-classes`, `access`, missing or duplicate unit name, bad `<provider>` or `<class>` | warning | |
| `<class>` or mapping `class` that is not in the project's sources, listed twice, or without a JPA annotation (`jpa-xml-class-*`) | info | |
| Derived query naming a property the entity doesn't have | error | |
| Repository id type differing from the entity's `@Id` | error | |

The lexical Kotlin checks look at the first class in a file; explicit foreign collection imports are quiet in live diagnostics. Settings: `jpaAssist.checkKotlinEntities`, `jpaAssist.checkClassSignature`
and `jpaAssist.checkListenerUsage`. A listener is found by its file name and only when one file
matches; callbacks it inherits are not followed.

Inverse `@OneToOne(mappedBy = …)` sides aren't flagged, because `LAZY` has no
effect there without bytecode enhancement. Methods with `@Query` aren't parsed.

## Repositories

In a `persistence.xml`, typing inside `<class>` completes class names of the project's sources, and typing
inside `<property name="…">` completes the standard `jakarta.persistence.*` (or `javax.persistence.*`, when the
file uses that namespace) and common Hibernate property names; `value="…"` completes the values of
`hibernate.hbm2ddl.auto`, schema generation, shared cache and validation mode. Unverified in the IDE: how the host
replaces a name containing dots.

Repository completion uses bounded cached entity declarations with explicit Spring
Data base ownership and an exact package/import domain owner. It suggests known
direct source properties, derived-query starters, condition and ordering words.
Nested fields, inherited mappings, stored simple-name lookups, generated method
signatures and guessed imports are outside this subset. See the ownership audit
below for cache population, exclusions and remaining work.

## Navigation

A gutter arrow on an `@Entity` class opens the Spring Data repository that
declares it as domain type (the tooltip lists all of them; the arrow opens the
first). Repository files are found by the `*Repository.java` / `.kt` name. The
Only repositories naming the entity directly as the `…Repository<Entity, Id>` argument are found (a `UserRepo extends BaseRepo<User>` whose `BaseRepo` extends `JpaRepository` is not resolved), and repositories not named `*Repository` are missed. The repositories are read once and updated when a repository file is opened or saved. The reverse arrow, from a repository to its entity, comes from Spring Data.

## Generators

- **Generate JPA entity…** is a lazy FileAction in Java/Kotlin editors. It reuses
  the Entity wizard, asks for `jakarta.persistence` or `javax.persistence`, then
  previews the exact target and generated source. It fills the invoking empty
  buffer or creates a sibling with overwrite disabled. UUID strategy is offered
  only for Jakarta. **JPA: New Entity** now copies source only; it never launches
  an external writer or changes an empty editor.
- **Add JPA attribute with preview…** reuses the Attribute wizard and previews
  the complete resulting current entity. The host owns the undoable edit. The
  older direct-writing **JPA: New Attribute** command and context-menu entry are
  retired; invoke this editor action for the same type/association/options wizard.
  Discovery prepares no edits. Selection resolves the wizard, shows the complete
  result with `confirm_exact`, then returns only the invoking document's atomic
  edit after source/version/root checks. No optional `editor.write` grant is needed
  merely to resolve the host-managed FileAction.
- **Add inverse attribute to this entity…** runs from the intended target entity,
  rather than editing a second file from an owning-side CodeAction. It discovers
  owning `ManyToOne`, `OneToOne` or `ManyToMany` associations in the same package
  folder, verifies their persistence namespace and unambiguous local type import,
  and asks which association to mirror. The preview shows `mappedBy`, imports and
  the complete result. Only the invoking target document is edited; owner files
  stay read-only. Existing inverse names or mappings are rejected. The former
  unversioned cross-file inverse action is removed.

- On an entity's class line, ⌥⏎ offers **Generate equals() and hashCode()**
  (comparing the id, stable for Hibernate proxies) and **Generate
  toString()** (id and basic attributes only, never a lazy association).
- **JPA: Copy DDL of Current Entity / of All Entities**: `CREATE TABLE`,
  sequences, join tables and foreign keys, in PostgreSQL or MySQL (setting),
  with Spring Boot's naming (snake case), ready to paste into a Flyway migration.
- On an entity's class line, seven **Add @PrePersist callback** style actions (one per
  lifecycle annotation) add the method and its import. An action is skipped when the class already has
  a method of that name.
- **JPA: Copy DTO of Current Entity**: a Java record or Kotlin data class
  with the basic attributes and the ids of to-one associations.
- **JPA: Copy Entity from CREATE TABLE at Caret** (in any SQL file): the
  entity for that table, with `@Id`, `@Column`, `@ManyToOne` for foreign
  keys and, in Java, getters and setters. A setting picks Java or Kotlin.

The context-menu entries show only where they apply: Copy DDL and Copy DTO in
Java and Kotlin files, Copy Entity in SQL files.

Without the `clipboard` permission, a notification asks you to allow it in
Settings › Plugins.

## Persistence

A tool window listing the project's entities and their attributes (id,
associations, columns). Double-click an item to open its declaration. Refresh
rereads the project.

## Templates

New › JPA Entity / Spring Data Repository (Java and Kotlin); `$END$` leaves
the caret on the repository's entity type.


The P09.1 generation entry points now use FileActions for entity source creation,
attribute edits and inverse-side edits; legacy source/SQL/DTO copy helpers stay
copy-only. No process or project writer remains. Root must review exact task-scope
closure independently of broader original/native parity. Generation verifies project-root confinement, conventional
package/folder identity, exact invoking path/language/version/text and current
unsaved source before and after confirmation. Entity names, table literals and
paths are bounded; Java/Kotlin file and local type collisions are checked before
returning a non-overwriting create. Source snapshots are limited to 128 KiB;
folder discovery refuses 65 or more files per language and uses short deadlines.
Unknown/mixed annotation namespaces, aliased types, nested/multiple/inherited
entity models, conflicting imports and ambiguous local associations stay outside
this action subset. Every returned edit targets the invoking document, so the
host's resolve-time document version check protects a dirty target during the
wizard. Atomic application and Undo are provided by the host, not a file-writing
process. The existing entity and attribute renderers are reused.

The original audited `JpaAddInverseAttributeAction` (JPA plugin source artifact)
queries ORM relationship targets and uses a persistence manipulator; its bundled
before/after template mirrors an owning `OneToOne` with `mappedBy`. This
implementation reproduces that source shape from verified local parser facts,
with the target entity as the invocation document. Cross-package inverse edits,
ORM/XML-defined or library entities, type/classpath resolution and arbitrary
multi-file target version guarantees remain unimplemented. The SDK exposes no
per-target expected version in FileAction edits; read-only owner text is checked
again after preview, but an owner can still change in the final read/apply gap.
This is a remaining semantic freshness limit, not permission to overwrite an
owner buffer. Desktop previews, application, dirty-buffer lifecycle and undo
have not been exercised; native tests verify returned operations and guards.

## Limits

- Inheritance strategies aren't modelled: a superclass's attributes are
  added to the subclass's table.
- Classes sharing a simple name in different packages are told apart only by
  being next to the file that uses them.
- Clipboard generators remain available, but creating source files uses explicit
  FileActions; `process:tee` is no longer requested or used.
- Only classes are read as entities: one on an interface or enum is missing from Persistence
  and the generators.

```sh
cargo test                       # logic, against the SDK's mock host
openidea-plugin build            # → dist/plugin.wasm, validated like an install
openidea-plugin link             # load this folder in the IDE (Settings › Plugins)
```

## Create a reviewed scalar DTO (API 1.3)

On a supported entity, **Create scalar DTO with preview…** is an explicit FileAction.
It asks for a type name ending in `Dto` and a selection of offered fields, then confirms the
exact destination and complete new source with the invoking document version. The destination
stays beside the entity in its validated package and language. Confirmation uses `confirm_exact`:
a preview too large for the host confirmation is refused rather than clipped. Cancel returns
no file operations. Apply is one undoable, non-overwriting host Create; the action performs no
direct writes, shell/process calls or database access.

This is a source-local scalar subset using the existing entity parser and DTO generator:
Java records (Java 16+; source level/compiler not verified) and Kotlin data classes (nullable
transfer fields with null defaults, explicitly stated in the confirmation). It supports simple
primitive/platform spellings and explicitly imported/fully qualified `java.math` numbers,
`java.util.UUID`/`Date` and supported `java.time` values. Shadowed imports, unknown/custom types,
associations, embedded values, inherited/generic entity shapes and ambiguous duplicate fields
are omitted or refused. Relationship IDs are never guessed. Entity sources over 128 KiB,
more than 128 parsed attributes, or more than 32 offered scalar fields are outside this action.
Names use the existing ASCII identifier/keyword checks and a 128-byte limit; package and path
checks, both-language file collisions and declared-type collisions in the bounded target folder
are reused from the native-proven entity creator. Dirty destination buffers also block creation.
Current source text/version/project and collisions are checked again after prompts/confirmation;
last-moment output collisions still fail through the host's atomic non-overwrite operation.

There is no mapper, arbitrary output package/folder chooser, language-level proof, compiler
validation, nested DTO or full type-resolution claim. The separate scalar SQL draft below adds a bounded DDL preview; neither workflow adds live schema
diff or database truth: existing Copy DTO/Copy DDL commands retain their broader source-derived
heuristics and copy-only behavior. Review those drafts independently before using them.

Original evidence inspected: available pinned `20202` JPA generation actions require typed PSI,
module and persistence-role checks (for example `GenerateEntityListenerMethods`). No matching
original DTO wizard implementation or screenshot was present in the available dump. This scoped
DTO workflow reuses this port's established `generate::dto` contract and the supported host
FileActions contract; it does not claim full original generator parity. Deterministic tests cover
Java/Kotlin output, selection/Cancel, stale source/project, package/name/type/import collisions,
dirty destinations and discovery bounds. No new desktop generation acceptance is claimed.


## Reviewed scalar SQL table draft (partial P09.4)

**Create scalar SQL table draft with preview…** is a separate FileAction for a
simple Java field-access entity. Choose PostgreSQL or MySQL, an explicit unqualified
table name and a filename stem; the `.sql` draft stays beside the entity's validated
package. The confirmation shows source/version/package, dialect, exact destination
and complete SQL before one atomic non-overwriting Create. Cancellation writes
nothing. Existing or dirty targets, observed case-insensitive filename collisions, unknown
ownership and stale source versions refuse the action. Target checks run again
after confirmation; the host Create also refuses last-moment existing targets.

This draft requires one manual `@Id`, 1–32 supported scalar fields, a plain
non-generic Java class and no mapping annotations beyond unparameterized, owned
`@Entity`/`@Id`. Relationships, inheritance, constructors/methods, initializers,
converters, `@Column`/`@Table`, generated IDs, unknown/shadowed types and duplicate
snake-case column names are withheld. The existing source generator supplies the
scalar SQL types. Identifiers are quoted for the chosen dialect, including reserved
words. The chosen table name, snake-case columns and nullable non-ID columns are
explicit draft choices; physical naming strategies, schema/catalog, effective
nullability and dialect/server compatibility are unverified. No database lookup,
schema diff, deployment, mapper or automatic SQL execution is added. The broader
clipboard DDL generators remain separate and unchanged. Folder collision inventory
is capped at128 candidates; previews above the host confirmation bound are refused.
Project discovery honors ignores; unobserved ignored case variants remain outside
the extra filename inventory. The host still refuses exact existing destinations.
These source/MockHost results do not establish native preview/apply/undo acceptance.

## Attribute hover and local graph navigation (P09.2 partial)

Hover now describes a current Java/Kotlin source attribute: qualified declaring class,
written type, and explicit identifier/column/table annotations. Literal top-level
`@NamedEntityGraph` / `@NamedAttributeNode("name")` references hover with those same
source facts and Go to Definition navigates to the field/property declared in that
same class. Returned ranges select the exact nonempty token in UTF-16 coordinates.
The current supplied document wins over saved files; neither provider reads files,
searches the project, warms the legacy global model, invokes tools or fetches data.

This deliberately narrow ownership proof requires a verified project path, named
package, one balanced class, exact `jakarta.persistence` or `javax.persistence`
annotation imports (or fully qualified annotations), unique ASCII attribute names
and a literal unescaped single-segment graph reference. Wildcard imports, aliases,
explicit access overrides, subgraphs, dotted/dynamic/concatenated names, unrelated
annotations, malformed/ambiguous declarations and unknown targets yield no result.
Limits are 128 KiB/document, 256 annotations, 128 attributes, 128 bracket nesting and
512 characters per displayed source fact. Values are escaped for Markdown/HTML.
These facts do not assert effective persistence access, superclass/XML mappings,
compiler resolution, physical database names or live database state.

Original evidence: `JpaAttributeReference.resolve/resolveAttribute` traverses the
resolved persistence model; its `bindToElement` uses property-name manipulation.
`AttributeNodeNameReference.findTargetClass/resolve` determines graph/subgraph owners
through PSI/UAST/JAM and persistence roles. The reviewed
`persistence-tool-window-0.png` shows typed attributes, cross-entity associations and
Referenced by entries. Current-source hover and same-class navigation supply a
bounded subset, not that broader graph model.

No `DocumentIndexer`/field anchors are registered. Inherited attributes, getters,
external queries/graphs/XML and transformed property spellings are not a proven
complete same-spelling reference set, so scoped usages/automatic Rename remain
pending feasible ownership-model work. The subsequent repository-completion audit below replaces its historical
simple-name/discovery path with a strict cached direct-field subset. Live diagnostics, DTO/basic-attribute FileActions and
existing structure/action workflows are preserved. Native desktop acceptance is
separate from deterministic provider tests and the pinned package check.


## Repository completion ownership follow-up (P09.2 bounded subset)

The repository provider no longer calls `load_graph`, reads persisted `path:Name`
entries, searches files or consults the global simple-name class map. Its independent
cache contains only previously supplied/opened/saved current entity source facts,
verified under an exact project root and qualified domain name. Merely opening a
repository, having a domain file on disk, or reclaiming old storage does not warm
it. Known direct fields still complete; operators/ordering words continue through
the existing derived-name parser. Unknown/cold/expired/duplicate owners are quiet.

A single Java/Kotlin repository must explicitly import a known Spring Data base
(`JpaRepository`, `Repository`, `CrudRepository`, `ListCrudRepository` or
`PagingAndSortingRepository`), name a literal simple domain argument and resolve it
through its own package or one exact import. Custom/generic bases, wildcard-only
ownership, aliases, qualified/composite generic arguments and ambiguous owners
are not inferred. Current supplied entity text replaces cached facts, including
oversized/rejected edits; project switches cannot reuse another root's facts.
Entity facts reuse the strict attribute-hover source proof (explicit JPA annotation
owners, bounded single balanced class, unique ASCII field/property declarations)
and exclude inheritance/access overrides and unrelated annotations. This models
observed declarations, not a closed classpath or effective persistence metamodel.

Suggestions are restricted to direct fields and lexical derived-query continuations.
Nested association/container/type resolution is not guessed. Capitalization
collisions, underscore property spellings, partial Unicode identifier suffixes and
exact reserved ID-method suggestions are suppressed. Generated full declarations
and imports are withheld because their parameter/return/type ownership is not
established by this bounded model. Source replacement ranges are explicit UTF-16
spans; no method/import edits or Rename anchors are produced. Comments, strings,
annotation expressions, declared queries, field initializers and method bodies do
not receive derived-name completion.

Limits: 128 KiB/source document, 128 attributes/source fact, eight roots, 256 files
and 4,096 attributes/root, 120-second fact lifetime, 256-byte query word and 256
returned suggestions. Existing file-change events invalidate source facts; known
open/save or current-source observations repopulate them. Missing lifecycle events
can leave facts until expiry; providers never refresh them by reading files.
The original `PartTreeCompletionContributor` checks resolved repository/framework
ownership and PSI/UAST method positions before using `PartTreeParserContext` and
persistent/domain properties. Our lexical subset does not reproduce that compiler
and persistence model. Full scoped nested/inherited completion, generated
signatures and usages/Rename remain feasible owner-model work.

XML class-name completion retains its separate historical source-discovery path;
this follow-up changes repository completion only. Live diagnostics, source hover,
DTO/basic-attribute atomic actions and explicit generation retain their regression
coverage. Fixtures verify useful cached Java/Kotlin fields, cold/no-discovery and
storage isolation, exact imports, unrelated/duplicate owners, unsaved invalidation,
project switching, denied/expired facts, Unicode replacement, reserved IDs and
suppressed contexts. Native completion acceptance remains a separate check.


### XML descriptor ownership follow-up

Live XML inspections and XML completion now require a bounded namespace-owned
`persistence` or `entity-mappings` descriptor. Persistence and ORM namespace
families are distinct: the standard Java Sun, JCP and Jakarta URIs are admitted
only for their matching root. The existing namespace-less descriptor convention
remains supported. Prefixes are resolved in scope; reserved XML/XMLNS rebinding,
reserved URI aliases/defaults, unbound prefixes, malformed XML, foreign elements
or foreign shadowing withhold the descriptor. Legal explicit/implicit `xml:lang`
and alternate prefixes bound to the same descriptor URI remain usable.

Current-text inspections select only established descriptor paths for the
existing rules. For example, cache/validation mode checks belong to a direct
persistence unit, and ORM entity/access checks belong directly to entity-mappings.
Unknown external classes remain unknown, without missing-type warnings. This is
not full XSD validation, effective persistence-unit resolution or imported mappings.

Class completion requires exactly `persistence/persistence-unit/class`, including
prefixed forms; arbitrary XML `<class>` tags and bare persistence-unit fragments
are quiet. It retains the existing project-owned source-file inventory/cache;
this change does not add classpath/type resolution or redesign its lazy warming.
Property completion requires `persistence/persistence-unit/properties/property` and
unqualified name/value attributes. It derives `javax.persistence.*` versus
`jakarta.persistence.*` from the descriptor namespace instead of unrelated file
text. A narrowly unfinished double-quoted name/value property tag can be completed
only after its ancestor scope and a repaired descriptor are proved; namespace
changes/other attributes are not guessed. Current-text unfinished class content
under that same exact path remains usable.

Admission is capped at512 KiB,8,192 XML events,64 levels and a10 ms loop deadline;
over-budget/unknown input is quiet. A foreign subtree suppresses the whole source
model conservatively. DTDs, CDATA and text entity-reference nodes are currently
withheld; legal declarations requiring normalization unsupported by the raw
namespace resolver, XML1.1 prefix undeclarations and unsupported incomplete XML
can therefore be false negatives. These limits preserve uncertainty, not evidence
of absence. Java/Kotlin literal navigation, source caches, atomic generation,
DTO work and Rename ownership remain unchanged. Native acceptance of these new
XML guards/completion contexts is separate from package/regression verification.

Live listener constructor notes additionally support one literal same-file
`Owner.Audit.class` attachment to a plain top-level Java Entity in its matching
`src/main/java` package path. A public static direct nested listener with an
explicitly declared non-public no-arg constructor receives an informational source
note. Implicit constructors, external or dynamic attachments, inheritance,
generics, generated contexts and unknown annotation ownership stay quiet. This
does not resolve external/container construction or callback assignability.

For that same explicitly attached nested listener subset, plain lifecycle methods
are checked for static/final modifiers, primitive non-void return types and an
argument count other than one. Generic/annotated/varargs parameter forms and
unknown return types stay quiet. A single argument's assignability to the entity
is unresolved; this does not discover external listener roles.

Duplicate event checks on that source-attached listener require valid plain
instance-void callbacks with exactly the enclosing source Entity as their single
argument type. Distinct events on one method are legal. Unknown library/supertype
argument roles do not establish a duplicate-event model.

Live eager-fetch notes require a direct unique `fetch` member and a matching
canonical JPA FetchType enum literal (fully qualified or uniquely imported).
Dynamic/foreign/bare/ambiguous values and visible enum/package name shadows stay
unknown. Omitting fetch retains the source-labeled JPA to-one default note.
The automatic entity quickfix group now shares owned declaration admission.

Automatic entity quickfixes require bounded balanced current source, matching
language/path, a current project owner and one top-level explicit Entity with
canonical annotation ownership. Nested/foreign/unimported/local-class contexts
stay quiet. Enum edits use a fully qualified matching FetchType; Lombok Data
replacement uses fully qualified Getter/Setter so foreign imports cannot capture
it. These are ordinary current-request quickfix edits; other generators/build
actions retain their separate contracts and no new stale-application guarantee
or FileAction migration is claimed here.

Add lifecycle callback source actions now require that same current-source
admission plus a direct canonical class-level Entity on a plain nonabstract
class, no inheritance, generics or generation annotations. Generated callbacks
use the Entity annotation's exact javax/jakarta namespace and fully qualified
event names, without guessed imports. Existing event/method names suppress the
corresponding template. Ordinary CodeAction application semantics remain;
equality/toString and cached build actions are outside this callback subset.

Automatic equality/hashCode and toString source templates now use the same
existing plain Java scalar-field/manual-ID input subset as scalar SQL drafts,
plus direct current Entity ownership. The manual ID must be byte/short/int/long,
or a proved java.lang Byte/Short/Integer/Long/String spelling supported by that
source gate. Primitive IDs use direct comparison; reference IDs use a null guard
and equals. Explicit this.field references avoid local parameter name capture;
Object/Override names are qualified and CRLF is preserved. No cached inherited
model participates. Missing/generated/composite/foreign IDs, relationships,
methods, Kotlin and broader layouts stay outside this template subset. These are
source templates to review, without provider/proxy equality guarantees. Existing
DTO/SQL FileAction source algorithms and application contracts are unchanged.

Live class-role notes now require directly attached canonical annotations on a
supported complete declaration prefix. Entity/Embeddable/relative role metadata
nested inside annotation arguments, fields or methods cannot borrow a following
class/interface role. Direct role conflicts and invalid declaration kinds retain
exact source ranges; incomplete or unknown prefixes stay quiet.

Owned fetch notes and their source quickfixes additionally require the proved
plain current Java type context. Inherited, generic, implemented-interface,
extra-type and Kotlin layouts stay unknown, including omitted-fetch edits, so
unknown inherited names cannot capture an advertised enum/package expression.

Live Kotlin val notes now describe read-only source access as Info, with effective
JPA access/XML/generated/provider behavior explicitly unresolved. They require a
plain direct Entity body-property context and an exact supported Kotlin scalar
FQN or unique single import; inferred/bare/foreign types, constructors, inheritance,
generics and generation contexts stay quiet. Shared direct declaration filtering
also keeps initializer/annotation-value metadata from borrowing property roles.
Collection API notes reject visible Kotlin package-prefix shadows. Legacy pure
provider-impossibility warnings remain test fixtures, outside live acceptance.

Owned scalar and structural attribute checks now require one directly attached
class-level persistence role matching the selected source class. Entity markers
in annotation values, conflicting roles, local/incomplete/unknown declaration
contexts cannot authorize attribute mapping warnings. Direct Entity,
MappedSuperclass and Embeddable source positives remain supported.

Attribute hover/local graph snapshots and their repository-completion source
facts now share the direct top-level persistence-class gate. Function-local
Kotlin classes cannot borrow a package-level qualified name or remain cached as
a repository domain after current text moves them into a function. Supported
canonical top-level Java/Kotlin source hovers and field names remain usable;
Kotlin package/import prelude lines are handled without assuming semicolons.
No authoritative usages/automatic Rename or broader compiler graph is added.

Current-class NamedAttributeNode literal completion now offers the supported
source field names using the same bounded plain Java field-access owner proof
as local graph diagnostics. Complete unescaped string literals may contain an
empty or partial ASCII name; completion replaces only their content, retaining
quotes and exact UTF-16 ranges. Names come from the current document, with
source-only detail and external mappings explicitly unresolved. Inherited,
generic, subgraph, dotted/dynamic/concatenated, unknown scalar or package-prefix
shadow contexts stay quiet. No project discovery, external lookup, extra edits,
authoritative usages/index or automatic Rename is added. Diagnostic settings
remain independent of completion.

An explicit Rename source-local JPA attribute FileAction supports a strict Java
fields-only Entity document: one manual field-access ID, private scalar fields,
a selected non-ID field with a literal Column name, and exact current-class
NamedAttributeNode users. A closed annotation/prelude/field inventory refuses
methods, accessors, initializers, inheritance, unknown annotations/arguments,
subgraphs and unknown bindings. Review lists the declaration/local graph uses
and complete resulting source; only those name spans change, while physical
Column/entity/graph names and comments remain unchanged.

The action validates identifiers/collisions, rechecks supplied path/language/
version/text plus live buffer text and project ownership across prompts, and
consumes its rename token. Tokens are scope-stamped bookkeeping, not
cryptographic cross-instance uniqueness. No independent focused-editor version
is available inside the FileActions callback, and no stronger final apply-race
guarantee is claimed. External XML/queries/reflection/compiler bindings are not
updated; this is an explicit same-document refactor, not indexed host Rename.
The matching read-only Find Attribute Usages in This Document command uses the same closed inventory.

JPA: Find Attribute Usages in This Document is available from the command palette
and Java Entity editor context menu. Select a supported declaration or exact
local graph value; a picker lists its declaration and every supported graph use,
with current-document locations. Selecting a row navigates without edits; Cancel
does nothing. The command re-reads its captured editor document (including the
reported version), live text and project ownership before navigation, and refuses
drift/unknown contexts. This is a scoped source inventory, not a project-wide
Find Usages/index or compiler-binding claim. Rename and usages share the strict
private, non-ID, literal-Column, zero-method v0 grammar.

## Unreleased follow-up

Live attribute diagnostics now flag a second `@Version` field on a proved source-local field-access entity. The warning clears on current-text edits; inherited, property-access, ambiguous annotation and external mapping cases remain unknown.

## Native structure UI

The **Persistence** tool window uses the IDE's native entity/attribute tree, with table/key/
relationship icons, table and attribute type details, Expand All/Collapse All and
Refresh. **Locate Open File** expands the current owned entity using its exact
current editor snapshot (up to 512 KiB), including unsaved source. An Opened file
detail and Located status identify it; the API does not provide programmatic tree
selection or scrolling. This hint clears when the source/editor/project changes.
Member details expose declared ID and relationship roles beside source types.
Enter/double click and the Jump to Source context action navigate to the
current declaration. Entity context actions retain Copy DDL and Copy DTO with the
existing native prompts. Navigation checks current project ownership and resolves
current source coordinates; removed attributes and foreign-project rows refuse.

The reference is JetBrains' [Persistence tool window](https://www.jetbrains.com/help/idea/persistence-tool-window.html):
source-linked entity/field browsing and navigation are preserved. Persistence-unit,
converter and effective ORM grouping, database/runtime mappings and native screenshot
parity are not claimed. The packaged structure HTML bridge is no longer used.
