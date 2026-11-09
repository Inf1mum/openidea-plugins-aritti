# Spring Data Tools

Spring Data repository support for Java and Kotlin projects in OpenIDEA:

API 1.4 native SQL injections cover source-owned Java JPA `@Query` methods with
`nativeQuery = true` and one direct, unescaped string literal. Ranges preserve the
physical source coordinates. JPQL, SpEL, concatenations, text blocks, transformed
escapes and Kotlin strings stay outside this injection subset. A configured SQL
language server is required for embedded LSP features; no database schema/type
model or embedded Rename/formatting is supplied.

- **Derived query checks**: repository methods without `@Query`/`@NativeQuery`/`@Procedure` are parsed the way Spring Data parses them (`find|read|get|query|search|stream|count|exists|delete|remove` … `By`, criteria split on `And`/`Or`, operator keywords, `IgnoreCase`/`AllIgnoreCase`, `OrderBy … Asc|Desc`). Each property path is resolved on the domain class (longest camel-case prefix first, `_` as explicit separator, nested types, collections unwrapped): an unknown property is a warning at that part of the name. The criteria must take as many arguments as the method declares (`Between` 2, `IsNull`/`True`/`Empty`… 0, others 1; `Pageable`, `Sort`, `Limit`, `ScrollPosition`, `Class` don't count).
- **Completion** of derived query names: after `findBy`, `countBy`, `findAllByNameAnd`… the entity's properties (and nested ones one level deep), then operator keywords, `And`/`Or`/`OrderBy`, `Asc`/`Desc`.
- **Navigation**: a gutter arrow on the repository opens its domain class (and `<Repository>Impl` when there is one); Go to Definition on a property part of a method name opens the field.
- **`@Query` checks**: a warning on a `:name` parameter that no method parameter binds (by `@Param("name")` or by the parameter name), and on `@Modifying` without `@Query`. In JPQL/HQL queries (not `nativeQuery = true`, not concatenated strings): a warning on an entity name after `from`/`join`/`update` that looks like a typo of a project entity or class (other unknown names stay quiet: they may come from a library or `orm.xml`; `from` inside functions such as `extract(year from d)` is not a declaration), and on `alias.property` paths (`u.address.city`) naming no property of a project entity; library types stay quiet.
- **`@Query` completion**: entity names after `from`/`join`/`update`, and the properties of an identification variable's entity (or of a nested type) after `u.`.
- **Type compatibility checks**: the existing parameter/return-kind parser remains tested, but live `query-parameter-type` and `query-return-type` findings are deferred until qualified library/source types can be established. `springData.checkQueryTypes` controls the new derived-query hover and criterion hints.
- **MongoDB**: in a `@Query` of a repository file that imports `org.springframework.data.mongodb`, a warning (`mongo-unknown-field`) on a top-level key whose first path segment is no property of the domain class (nor `id`, `_id`, a `$` operator or a `@Field`/`@Column`/`@JsonProperty` name). Nested documents are not followed; a domain class with a library superclass is not checked.
- **JDBC checks**: namespace-guarded local `@Sequence` and `@MappedCollection` checks remain live under `springData.checkJdbcMapping`. Cross-root `jdbc-shared-entity` findings are deferred until qualified aggregate ownership is modeled; the previous parser remains tested.
- **Derived query tooltip**: a gutter marker on each derived query method that reads the name in words (`find User where first name and last name, order by age desc`).
- **Show Repository Inherited Members** (command and editor context menu): lists the members a repository inherits from Spring Data's own interfaces (`CrudRepository`, `ListCrudRepository`, `PagingAndSortingRepository`, `JpaRepository`, `MongoRepository`, `QueryByExampleExecutor`, `JpaSpecificationExecutor`, reactive ones); custom parent interfaces are not expanded. Available in Java and Kotlin files.
- **`repository` snippet**: typing `repo` at the top level of a Java or Kotlin file offers a repository interface skeleton.
- **Sort the query fields** (code action in a MongoDB `@Query` string): orders the top-level fields of the filter document alphabetically (nested documents are kept as written; the `sort` attribute is never reordered because its order matters).
- **Generate repository**: on a supported top-level entity, the code action **Generate CrudRepository for …** previews a new Java/Kotlin interface in the same package folder. The host creates and opens it as one atomic, undoable FileAction with overwrite disabled.
- **File template**: Spring Data Repository (Java and Kotlin) remains available in the New File menu.
- **Kotlin**: repositories and entities in `.kt` files are read like Java ones.

A repository is an interface extending a `…Repository`/`…RepositoryBase` type with type arguments (the first is the domain type) or annotated `@RepositoryDefinition(domainClass = …)`. Properties are the non-static, non-transient fields (not `@Transient`) and record components of the domain class and its superclasses. Matching is text-based, by simple type name among the project's Java and Kotlin sources: when the domain class, a superclass or a traversed type isn't a project class (a library type), nothing is reported.


P18.1 live providers use the supplied Java/Kotlin document version and a bounded source cache warmed during activation and optional file events. Diagnostics, hover, inlays and gutter markers perform no project scan or file read and no manual diagnostic publication. Unsaved current-source snapshots replace their cached entry. Absence checks and query hints require fresh, complete, unambiguous project facts; cold, malformed, oversized, conflicting or stale facts stay quiet. Inputs are limited to 128 KiB, cached source files to 256 type/2,048 member declarations, model construction to 5,000 class names, diagnostics to 500 and inlays to 200.

The live repository subset requires an explicitly imported known Spring Data base (`Repository`, `CrudRepository`, `ListCrudRepository`, `PagingAndSortingRepository`, `JpaRepository`, `MongoRepository`, `ReactiveCrudRepository`), with no local/conflicting import shadow. Query/NativeQuery/Modifying/Param annotations require an exact supported fully qualified spelling or one unambiguous explicit framework import for the simple spelling. Foreign qualified spellings cannot borrow an unrelated Spring import; conflicting supported imports and a local qualified-name root stay quiet. Domain classes must have a unique qualified source owner visible from the repository package or an exact import. Association/supertype links use their declaring source package/imports; unrelated same-simple-name classes do not compete. Unknown/library, alias, generic-hierarchy and ambiguous owners stay quiet. Dynamic/concatenated query strings remain quiet. Custom repository bases and wildcard-import-only ownership remain outside the completion/navigation/live subset. A nongeneric interface using an explicitly imported or fully qualified `org.springframework.data.repository.RepositoryDefinition` is supported when its sole annotation supplies exactly one simple Java `.class` or Kotlin `::class` literal for each of `domainClass` and `idClass`, with no competing base interface. Domain ownership still comes from the same unique qualified project-source cache, never the annotation spelling alone. Aliases, constants, qualified domain literals, duplicate/unknown attributes and ambiguous or cold domains stay quiet. `idClass` is a declared literal shape here, not an inferred persistence ID/type claim.

Derived-method hover describes a query only after every property part resolves to cached source. Parameter declaration inlays show the criterion associated by the known derived-argument order; two-argument/unknown operators do not get speculative labels. Verified special Spring Data parameters are excluded. These are source declaration hints, not call-site SQL bindings, query plans, inferred database types or runtime results. Repository generation and external language-server ownership remain unchanged. Completion and Go to Definition now share this bounded cached ownership model.

The local original audit inspected `JpaRepositoryExplicitMethodQueryProvider.getAnnotationsFqns/getLanguage`, `SpringDataDomainModel`, the Spring Data Page/return-type inspection descriptions, and the logical repository method presentation contracts. Their PSI/ORM/store model is broader than this cache-backed subset. The original 71 parser/generation regressions are retained independently of the stricter live provider ownership contract, with additional live-provider tests. Full query language injection, qualified type/hierarchy analysis, classpath metadata, custom store/query-provider semantics and desktop scheduling/rendering/close-discard acceptance remain gaps. No proprietary implementation was copied.

Repository generation uses current source facts only: explicitly imported or
fully qualified `jakarta.persistence.Entity` / `javax.persistence.Entity`, MongoDB
`Document`, or relational `Table`, with exactly one locally declared ID from the
matching persistence namespace. Fields, Java record components, no-argument
getters and Kotlin primary-constructor properties are supported. Known scalar
identifier types are boxed for Java and mapped to Kotlin built-ins; UUID and
BigInteger are supported when their namespace is explicit. There is no assumed
`Long` fallback. Nested/generic/inherited entities, composite/embedded IDs,
unknown custom identifier types, wildcard/aliased annotation identity and
ambiguous facts do not offer this action.

The package must be a named Java/Kotlin package matching the source folder.
Generation stays in that folder and accepts a new ASCII interface name, bounded
to 128 bytes. Source snapshots are bounded to 128 KiB; at most four pending
actions are retained. Resolution verifies the exact document path, language,
version and text, project identity, and the current dirty-aware source text
before and after the concrete confirmation. Existing Java or Kotlin targets and
other top-level types in the target folder are rejected. Collision discovery is
bounded to 64 files per language, 128 KiB per file and two seconds per pass;
incomplete or unreadable checks refuse creation. The host additionally confines
paths, checks the document version during resolution, and rejects a target that
appears before atomic application. Existing source buffers are never edited.
No external writer, process, network request or direct project write is used.

P18.2 is partial. The original audited
`SpringDataJpaRepositoryGenerator#generateRepository` /
`selectBaseRepositoryClassAndGenerateRepository` selects an ORM entity, a base
repository class and a target directory before invoking
`SpringRepositoryComponentGenerator`; it uses ORM/PSI services and reference
shortening. This implementation supplies a fixed `CrudRepository<Entity, ID>`
creation workflow from established local facts. It does not resolve library
classpath availability, cross-source-root package/type collisions, arbitrary
base repositories, ORM model entities or repository-method generation. Static
file templates remain available for manual cases. Native tests verify returned
operations and guards against the v0.1.39 host contract; desktop preview,
application, dirty-buffer lifecycle and undo have not been exercised.

Not ported: language injection and highlighting in `@Query`, a preview of the resolved query, other store specifics (geo `Near`/`Within` arguments aren't counted, nested MongoDB documents aren't followed), navigation to repository fragments other than `<Repository>Impl`.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


### Derived-name completion and field ownership audit (P18.3 partial)

Completion and definition no longer discover files or construct a project-global simple-name model. They read the current document and previously populated project cache. A cold cache, unknown library property, conflicting qualified owner, incomplete hierarchy or unsupported alias yields no invented fields. Explicit lifecycle discovery remains separate. Suggestions are capped at 512 and source input at 128 KiB. Standard container traversal requires a proven explicit standard-container import; custom or unproved generic containers do not manufacture element properties.

Each proposed direct/nested spelling must resolve back to its intended property chain. A direct `addressZipCode` wins over `address.zipCode`; the nested alternative becomes `Address_ZipCode`. The first existing longest camel-case head commits even when its tail fails. These rules follow the [Spring Data Commons 3.5 property-expression contract](https://docs.spring.io/spring-data/commons/reference/3.5/repositories/query-methods-details.html), also checked against the current Commons reference. Leading/doubled underscores, acronym edge cases, expanded Unicode capitalization and ambiguous source members are supported only when the spelling round-trips; full framework grammar/store equivalence is not claimed.

Declared/native/dynamic queries, comments, strings and default method bodies do not receive derived-name suggestions. JPQL field completion requires an explicitly owned JPA `Query`, literal unescaped source and known alias/type links. Navigation preserves actual declaration ranges, including UTF-16 offsets; getter ranges remain navigation targets rather than replaceable field tokens.

The original `SpringDataCommonsCompletionContributor` uses resolved PSI/UAST and PartTree parser context; our lexical cache does not establish equivalent semantic ownership. No field anchors or indexing export are registered: getter spellings, transformed derived fragments, external repositories/queries and runtime mappings do not form a proven complete same-spelling rename set. Scoped usages/Rename therefore remain pending feasible owner-model work. Regression fixtures cover cold/no-discovery, unrelated versus conflicting owners, domain-owned nested imports, direct/nested collisions, longest-head failure, suppressed contexts and Unicode navigation. Existing atomic repository generation and live provider tests remain included.


### RepositoryDefinition source follow-up

Spring Data Commons 3.5 declares `RepositoryDefinition` equivalent to extending
`Repository`, with class-valued domain and identifier attributes. The original
`SpringRepositoryDefinition` resolves these through the exact annotation FQN and
JAM/PSI class attributes. The bounded local subset above restores useful existing
provider behavior for proved source-owned annotation repositories without
classpath resolution, indexing/Rename, provider discovery or additional IO.
Current document versions and UTF-16 ranges use the existing live-provider path.
The generation FileAction workflow and its fixed `CrudRepository` target remain
unchanged. Native annotation-repository scheduling/rendering is unverified.

### Query annotation ownership follow-up

Data92 fixes a reproduced current-text diagnostic ownership issue: an explicit foreign `@other.Query` could previously borrow a separate Spring Query import, and multiple supported Query imports were treated as one identity. Each Query/NativeQuery/Modifying/Param spelling now passes an independent exact FQN or unique simple-import check; foreign or ambiguous method annotations suppress the method rather than invent query facts. A qualified `org` root shadow is rejected. Two regressions cover public diagnostic suppression, all four annotation families, unsaved correction and UTF-16 query ranges. Existing source/cache and no-Rename boundaries remain.

The reviewed original explicit JPA query provider selects annotation FQNs and inspects the qualified NativeQuery identity; the public Spring annotation contracts likewise identify concrete annotation types. Our bounded lexical ownership check does not resolve arbitrary aliases, inheritance, runtime store selection or full JPQL/SQL semantics. No original implementation/messages copied. Full92 tests and pinned API1.3 build/check are recorded in the accompanying receipt; no new desktop acceptance.

Data94 extends exact annotation admission to the source-local JDBC Sequence/MappedCollection field and record/constructor-component checks. Unknown annotation identities are removed from the checked view while their property declarations remain available to the cached source model. A captured before-fix foreign Sequence regression and two new field/component tests cover foreign qualifiers, ambiguous imports, local shadowing, Java records, Kotlin constructor properties and unsaved recovery. Initial test fixtures used unsupported same-line Kotlin imports/missing Java terminators; they were corrected before final validation, with no parser bug credit. This does not add original UAST/inherited collection-type semantics or effective JDBC module support.

### Current project ownership of cached source facts

Data96 rechecks every cached model source against the current innermost open-project root before publishing derived/query completion, definition, markers, hover, inlays or model diagnostics. One bounded root inventory (64 projects maximum) replaces stale lexical parent ownership without source discovery or file reads in editor requests. Opening a nested project therefore cannot leave its previously cached classes available to the outer repository. Unknown/invalid/oversized root inventories withhold model facts; excluded sources make model completeness false, so absence checks and completeness-dependent hints stay quiet. Known unaffected outer properties still supply positive completion/navigation. Canonical absolute source/root paths and project boundaries are required.

A public regression captures the pre-fix stale hover after nested-project opening; two added tests cover hover/inlay/completion/diagnostic suppression, unaffected outer navigation, malformed/foreign paths, root-count bounds and no process/network work. Full96 tests and pinned API1.3 build/check are recorded separately. This does not add effective module/classpath sharing across projects, global usages/Rename or native project-switch acceptance.

### Explicit named-query source hints

Data99 adds hover over an explicitly owned literal JPA Query annotation and parameter inlays for unique literal `@Param` aliases which also occur as named placeholders in that query. Repeated placeholder use yields one hint per bound declaration. Hover counts the current query names and matching explicit aliases; compiler parameter-name metadata, query validity, rewriters and effective runtime binding are not inferred. Only abstract source methods, known current project domain models, the existing checkQueryTypes switch, and unescaped literal query values up to4096 bytes are admitted. Native/dynamic/ambiguous/foreign forms stay quiet. Separate tooltips identify explicit alias facts versus derived-criterion order. Hover ranges are half-open.

Three added tests cover UTF16 positions/range filtering/current text/FQN, duplicate/dynamic/foreign/native suppression, and cache/settings/project/size gates. Full99 and pinned API1.3 build/check are recorded. Public Spring Data3.5 Param/Query contracts identify named alias binding and declared query/native semantics; the original explicit JPA provider uses exact annotation FQNs. This is a source presentation subset, not a database plan or runtime resolution. P18.1/3 and native acceptance remain separately tracked.

Data101 adds source-owned repository identifier hover over a known cached domain match. It shows the declared domain and bounded escaped source path/line, and distinguishes complete-within-cache from incomplete/stale positive inventories. It never infers the persistence identifier type, custom base semantics or effective runtime activation. Unlike query/criterion hints, this positive metadata display may survive an incomplete inventory when the particular current-owner domain fact is known; stale/foreign owners, cold/unknown domains and disabled hints remain quiet. Two regressions cover Unicode ranges, no source re-read, owner/cold/unknown gates, partial provenance, Markdown-safe paths and half-open bounds. Full101 tests/pinned API1.3 build/check and native limits are recorded separately.
