# Inspection mapping (F and I ledger rows)

Not a parity claim. Maps each `Inspection families` (F) and `localInspection` (I) ledger row to a rule in this plugin, or `missing`.
Paths are under `plugins/jpa-assist/src/` unless noted. Status words follow README: **live** = runs automatically in the diagnostics provider
(`live.rs` dispatch, `src/live.rs:132-210`); **explicit/legacy** = legacy algorithm kept as fixture or reachable only through an explicit
action/quick-fix feed, not live; **missing** = no rule. "Intent" for missing rows comes only from the ledger text and the public
JPA Buddy docs (https://www.jpa-buddy.com/documentation/ , entity-designer inspections screenshot named in
`docs/jpa-buddy-gap-analysis-2026-11-10.md`); the docs name inspections but do not list the catalogue, so everything else is `intent unverified`.
Wave 2 (below) read the decompiled original (`audit/jpa-buddy-263.7307.112`, read-only) to learn intent; nothing was copied. Rows added by this increment are marked NEW (`inspections_more.rs`, live, each with a `jpaAssist.check*` toggle).

## F rows

| Id | Capability | Our rule | Status | Missing: inferred intent |
|---|---|---|---|---|
| F001 | Column mapping on a relationship | none | missing | None of the 12 inspections the original registers checks `@Column` on an association (read from source), so there is no original behaviour to follow; semantics in providers unverified. Not implemented. |
| F002 | Unsupported persistent field type | `attribute-type` (`checks.rs:590`) and NEW `jpa-unsupported-type` (`live_project.rs`, Warning) | live (partial) | Original `UnsupportedTypeWithoutConverterInspection` (I003): a non-static, non-transient field whose type is not basic, enum, entity, generic, `Serializable`, with no relationship/`@Embedded`/`@EmbeddedId`/`@Lob`/`@Transient`/`@Type`/converter. Ours flags only a project class proven plain (no supertypes, no persistence annotation, no mapping XML, no project `AttributeConverter` mentioning it); JDK and library types stay silent. |
| F003 | Serializable composite key | NEW `jpa-composite-id-not-serializable` (Error) | live (partial) | Original `CompositeIdClassSerializableInspection` (I011): `@EmbeddedId` member whose class does not resolve as Serializable; skipped on Hibernate 6 projects; error highlight. Ours: `javax.persistence` sources only (Hibernate 6 uses `jakarta`), field `@EmbeddedId`, key class with no supertypes at all. `@IdClass` is not checked (the original does not either). |
| F004 | Converter type compatibility | `jpa-converter-type-mismatch` (`local_hierarchy.rs`, Info, live; code now in `KNOWN_CODES`) | live (partial) | Original `ConverterSpecifiedIncorrectlyInspection` (I009): `@Convert(converter = X.class)` whose `AttributeConverter` entity type is not assignable from the field type; fix removes `@Convert`. Ours compares exact JDK types only, no assignability, no fix. The original's default level is warning; our rule keeps Info (existing test). |
| F005 | Entity construction | NEW `jpa-no-arg-constructor` (`inspections_more.rs:152`); Kotlin side: `jpa-kotlin-plugin-missing` (`checks.rs:985`), `jpa-lombok-constructors` (`lombok_checks.rs:385`) | live (Java explicit ctors only; Lombok/Kotlin delegated) | Entities with no explicit constructor, or Lombok-generated ctors, are not judged. |
| F006 | Entity-valued field without relationship mapping | NEW `jpa-entity-field-without-relationship` (Warning) | live (own rule) | The original's `UnsupportedTypeWithoutConverterInspection` explicitly skips entity-typed fields, so no original counterpart in the 12 inspections; implemented from the ledger text with positive target-role facts (target resolved as an `@Entity` in the same source root). |
| F007 | Absent identifier | `jpa-source-identifier-absent` (`local_repository.rs:40`, info, live); `entity-missing-id` (`checks.rs:427`, inherited-aware, legacy not live) | live (info, source-local) + legacy | XML/provider/inherited ids unresolved. |
| F008 | Conflicting identity declarations | `jpa-duplicate-embedded-id` (`local_identifiers.rs:72`, live), `jpa-multiple-id-kinds` (`checks.rs:786`, live), `jpa-embeddable-identifier` (`checks.rs:698`, live) | live (partial) | General composite-key modelling absent. |
| F009 | Equality owner/class checks | `jpa-lombok-equals`, `jpa-lombok-equals-field`, `jpa-lombok-data`, `jpa-data-class` (equality-on-entity inspections I006/I007/I012) | live (partial) | No original inspection validates a hand-written `equals`; the original's equality inspections are the Lombok and Kotlin data-class ones. Hand-written `equals` validation: no counterpart, not implemented. |
| F010 | Converter role annotation | `jpa-converter-not-attribute-converter` (`local_hierarchy.rs`, live) | live (partial) | The original's I009 only checks the type match (see F004); a class named in `@Convert` that cannot be an `AttributeConverter` is our own rule. Missing `@Converter` role annotation: no original counterpart. |
| F011 | Multiple version fields | `jpa-duplicate-version` (`local_versions.rs:86`, live, source-local); `attribute-type` for version types | live (partial) | Inherited/effective mappings unknown. |
| F012 | Paged results without paging input | `plugins/spring-data/src/checks.rs:94` `page-without-pageable` | other plugin, live there | Not in jpa-assist. |
| F013 | Inverse one-to-one lazy caveat | none; README documents that inverse `@OneToOne(mappedBy)` is deliberately not flagged | missing | No original inspection among the 12 registered mentions `mappedBy` (the only `mappedBy` uses in the decompiled plugin are entity-designer UI updaters and the create-inverse intention). No original counterpart; not implemented. |
| F014 | Table mapping under single-table inheritance | NEW `jpa-incompatible-table` (Warning) | live | Original `IncompatibleTableAnnotationInspection` (I010): entity whose hierarchy root is a different entity with SINGLE_TABLE or unspecified strategy and which carries `@Table`; fix removes `@Table`. Ours: closed literal `extends` chain (`local_inheritance::root_strategy`), no mapping XML, no fix. |
| F015 | Kotlin generated equality/string methods | `jpa-data-class` (`checks.rs:249`), `jpa-val-attribute` (`checks.rs:375`), `jpa-immutable-collection` (`checks.rs:394`) | live (partial) | Full generated-method semantics not modelled. |
| F016 | Lombok string loading/recursion | `jpa-lombok-to-string` (`lombok_checks.rs:302`, live) | live (partial) | Exclusion fixes for existing `@ToString` incomplete per ledger. |
| F017 | Write query marker | `plugins/spring-data/src/checks.rs:100` `query_needs_modifying` | other plugin | Not in jpa-assist. |
| F018 | Lombok constructor generation | `jpa-lombok-constructors` (`lombok_checks.rs:385`, live) | live | Ledger text says "no live rule found"; the rule exists in the current tree but its ledger row has not been re-verified. |
| F019 | Lombok entity equality | `jpa-lombok-data` (`checks.rs:262`), `jpa-lombok-equals` (`checks.rs:271`), `jpa-lombok-equals-field` (`lombok_checks.rs:302`), all live | live (partial) | Complete replacement workflow not equivalent. |

## I rows (descriptor `localInspection` entries; mapped F rows in the ledger)

| Id | Capability (class name) | Our rule | Status | Missing: inferred intent |
|---|---|---|---|---|
| I001 | EntityIdMissingInspection | see F007 | live (info) / legacy | |
| I002 | DuplicateVersionAnnotationInspection | `jpa-duplicate-version` | live | |
| I003 | UnsupportedTypeWithoutConverterInspection | NEW `jpa-unsupported-type` | live (partial) | See F002. |
| I004 | LombokToStringLazyInspection | `jpa-lombok-to-string` | live (partial) | |
| I005 | LombokToStringLazyIncludeInspection | `jpa-lombok-to-string` (include side unverified) | live (partial) | |
| I006 | LombokEqualsAndHashCodeInspection | `jpa-lombok-equals` / `jpa-lombok-equals-field` | live | |
| I007 | LombokDataInspection | `jpa-lombok-data` | live | |
| I008 | NoArgsConstructorInspection | NEW `jpa-no-arg-constructor` (Java), `jpa-lombok-constructors`, `jpa-kotlin-plugin-missing` | live (partial) | Entities with no explicit constructors not judged. |
| I009 | ConverterSpecifiedIncorrectlyInspection | `jpa-converter-type-mismatch` | live (partial) | See F004. |
| I010 | IncompatibleTableAnnotationInspection | NEW `jpa-incompatible-table` | live | See F014. |
| I011 | CompositeIdClassSerializableInspection | NEW `jpa-composite-id-not-serializable` | live (partial) | See F003. |
| I012 | DataClassEqualsAndHashCodeInspection (Kotlin) | `jpa-data-class` (`checks.rs:249`) | live | |

G030 (Inspection configuration): boolean `jpaAssist.check*` toggles only; no severity/scope configuration. Unchanged.

## Rules added in this increment (src/inspections_more.rs)

Gate for all: `attribute_info::snapshot` (one closed class, every annotation exactly `jakarta|javax.persistence.*`, no `@Access`, bounded
size, project-owned path). Any foreign annotation (Lombok, `@Override`, Spring) or wildcard import makes the class quiet. Kotlin
constructors with generic types also stay quiet because the existing `trusted_annotations` heuristic treats `<` in the header as a possible type-parameter shadow.

| Code | Severity | Toggle | Languages | Source |
|---|---|---|---|---|
| `jpa-no-arg-constructor` | error | `checkNoArgConstructor` | Java | Jakarta Persistence 3.2 s2.1: "must have a public or protected constructor with no parameters". Only when explicit constructors exist and none qualifies. |
| `jpa-final-member` (fix: Remove final) | warning | `checkFinalMembers` | Java | s2.1: "Every method and persistent instance variable of the entity class must be non-final." Hibernate User Guide 3.4: final state accessors stop proxy generation. Static/transient/`@Transient` quiet. |
| `jpa-generated-value-without-id` | warning | `checkGeneratedValueId` | Java, Kotlin | `GeneratedValue` Javadoc: "may be applied to a primary key property or field". |
| `jpa-generated-value-type` | warning | `checkGeneratedValueType` | Java, Kotlin | `GenerationType` Javadoc: IDENTITY/SEQUENCE/TABLE for `Long`/`Integer`/`long`/`int`; UUID for `UUID`/`String`; s2.4 non-portable otherwise. `AUTO`, unknown types and non-`GenerationType.X` spellings quiet. |
| `jpa-column-length-type` | warning | `checkColumnLength` | Java, Kotlin | `Column.length` Javadoc: "Applies only to columns whose type is parameterized by length". Flagged on numeric/BigDecimal/boolean only. |
| `jpa-column-precision-type` | warning | `checkColumnPrecision` | Java, Kotlin | `Column.precision`/`scale`: "Applies only to columns of exact numeric type". Flagged on String/boolean/char only (temporal excluded: Hibernate 6 uses precision for timestamps). Literal 0 quiet. |
| `jpa-unidirectional-one-to-many` | info | `checkUnidirectionalOneToMany` | Java, Kotlin | Hibernate 6.6 User Guide `@OneToMany`: unidirectional uses a link table; bidirectional "is much more efficient". Quiet with `mappedBy`, `@JoinColumn(s)` or `@JoinTable`. |

Considered and skipped: `@Id` on floating-point (the 3.2 spec text fetched no longer carries that warning; unverified), `@Enumerated`/`@Temporal`
misuse (exists: `attribute-type`), missing `@Id` (exists: F007), final entity class (exists: `jpa-final-entity`), `mappedBy` target
existence (needs project-wide completeness; README keeps it non-live), `@Lob` with `@Enumerated` (no spec statement found).

## Wave 2: legacy rules wired live, new rules, unmapped rows (`src/live_project.rs`)

Intent confirmed from the decompiled original (read-only): the 12 `localInspection` registrations (the descriptor lists each twice, 24 lines; all `enabledByDefault`,
no explicit level, default problem highlight `GENERIC_ERROR_OR_WARNING` = warning; `EntityIdMissing`, `DuplicateVersion`, `NoArgsConstructor` and
`CompositeIdClassSerializable` override it with `GENERIC_ERROR`, i.e. error). Existing `ORIGINAL_INSPECTION_CODES` forces Warning on `jpa-source-identifier-absent`,
`jpa-duplicate-version` and `jpa-no-arg-constructor`; the originals of those three highlight as errors, so that earlier decision is worth revisiting.
Added to `ORIGINAL_INSPECTION_CODES`: `jpa-unsupported-type`, `jpa-incompatible-table` (their originals use the default level). Not added:
`jpa-composite-id-not-serializable` (original error), `jpa-converter-type-mismatch` (already live as Info; an existing test pins that).

| Code | Was | Toggle | Rule |
|---|---|---|---|
| `jpa-kotlin-plugin-missing`, `jpa-redundant-constructor`, `jpa-redundant-open` | legacy | `checkKotlinEntities` | legacy `inspect_kotlin_build` over the literal Gradle/Maven build files from the file's directory up to the project root; unfamiliar builds (`alias(`, `subprojects`, `noArg`, ...) unknown |
| `jpa-listener-without-callbacks` | legacy | `checkListenerUsage` | external listener resolved by imports/package, same source root, no mapping XML at all |
| `jpa-relationship-target` | legacy | `checkAttributeSignatures` | association target resolved as above; not an entity, or an embeddable/mapped superclass; quiet with `targetEntity` |
| `jpa-unknown-attribute-override`, `jpa-unknown-maps-id`, `jpa-unknown-map-key` | legacy | `checkMappingNames` | owner attributes from the closed `extends` chain of strict persistence-only sources; quiet when XML may declare the owner |
| `jpa-unknown-entity-graph`, `jpa-unknown-entity-graph-path` | legacy | `checkEntityGraphs` | Spring Data `@EntityGraph` (exact import) on a repository with an exact Spring Data base and an in-project entity; names need a complete scan of at most 128 Java and 128 Kotlin files with every graph name a literal; `getEntityGraph(...)` calls are not judged (receiver type unproven) |
| `jpa-unknown-named-query` | legacy | `checkQueryReferences` | `@Query(name = "...")` in such a repository, same completeness rule; `createNamedQuery(...)` calls are not judged |
| `jpa-xml-class-unknown`, `jpa-xml-class-not-persistent` | legacy | `checkPersistenceXml` | unknown only when the class's package has sources in the project and no file of that name exists; not-persistent only when found once and no mapping file exists |
| `jpa-repository-id` | legacy | none | NOT wired: its live form is `jpa-source-repository-id` (same comparison with source ownership proof), wiring both would report twice. Remains legacy. |
| `jpa-unsupported-type`, `jpa-entity-field-without-relationship`, `jpa-incompatible-table`, `jpa-composite-id-not-serializable` | new | `checkUnsupportedFieldType`, `checkEntityFieldRelationship`, `checkIncompatibleTable`, `checkCompositeIdSerializable` | see F002, F006, F014, F003 |

Unmapped ledger rows (`unmapped.md`), read from the original:

| Row | Original behaviour | Plugin counterpart |
|---|---|---|
| A003, A037 | `SafeDeleteMethodAction`: runs the IntelliJ platform `SafeDeleteHandler` on a method selected in the JPA Buddy tool window's repository inspector | Not applicable: the platform Safe Delete refactoring (usage search, conflict dialog) has no OpenIDEA plugin API. |
| A038 | `SafeDeleteClassAction`: same handler for a class | Not applicable, same reason. (Attribute safe delete, G075/G076, is the part with a counterpart.) |
| E018 | `JpabLiteralExpressionAnnotator`: warning annotator for string literals whose JPA Buddy hard reference does not resolve. The only hard references are `mappedBy` (JPA association annotations), MapStruct `@Mapping` source/target, and Hibernate `@Type(type=...)` | None live: no rule checks any of the three. See `unmapped-resolution.json` (E018). The unknown-name rules for `@MapKey`, `@AttributeOverride`, `@MapsId` and entity graphs are our own and have no original reference. |
| E038 | `JpabEventDataProvider`: tells the platform's action-event machinery that the tool window toolbar place is a global action place and that the ORM framework is JPA | Not applicable: IntelliJ-internal context plumbing with no user-visible behaviour. |

Not implemented (no original inspection, intent unverifiable): F001, F013, hand-written `equals` validation (F009). F012 and F017 stay in `plugins/spring-data`.
Residual: the new and wired rules read other files from disk (saved state); an unsaved edit in another file is not seen until it is saved.
