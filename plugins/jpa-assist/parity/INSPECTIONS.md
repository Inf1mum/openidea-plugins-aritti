# Inspection mapping (F and I ledger rows)

Not a parity claim. Maps each `Inspection families` (F) and `localInspection` (I) ledger row to a rule in this plugin, or `missing`.
Paths are under `plugins/jpa-assist/src/` unless noted. Status words follow README: **live** = runs automatically in the diagnostics provider
(`live.rs` dispatch, `src/live.rs:132-210`); **explicit/legacy** = legacy algorithm kept as fixture or reachable only through an explicit
action/quick-fix feed, not live; **missing** = no rule. "Intent" for missing rows comes only from the ledger text and the public
JPA Buddy docs (https://www.jpa-buddy.com/documentation/ , entity-designer inspections screenshot named in
`docs/jpa-buddy-gap-analysis-2026-11-10.md`); the docs name inspections but do not list the catalogue, so everything else is `intent unverified`.
Nothing was decompiled. Rows added by this increment are marked NEW (`inspections_more.rs`, live, each with a `jpaAssist.check*` toggle).

## F rows

| Id | Capability | Our rule | Status | Missing: inferred intent |
|---|---|---|---|---|
| F001 | Column mapping on a relationship | none | missing | Ledger: `@Column` versus association incompatibility. Spec says `@JoinColumn` maps association columns; whether providers ignore or reject `@Column` there is unverified, so not implemented. |
| F002 | Unsupported persistent field type | `attribute-type` (`checks.rs:590`, Temporal/Enumerated/Version) | live (partial) | General persistent-type/converter classification: intent unverified beyond the ledger text. |
| F003 | Serializable composite key | none | missing | Ledger: composite-key contract (`@IdClass` class serializable). Needs the key class from another document; cached facts do not hold it. Intent beyond that unverified. |
| F004 | Converter type compatibility | none | missing | Ledger: effective `AttributeConverter<X,Y>` generic target matching the attribute type. Needs resolved generics; intent unverified beyond the ledger. |
| F005 | Entity construction | NEW `jpa-no-arg-constructor` (`inspections_more.rs:152`); Kotlin side: `jpa-kotlin-plugin-missing` (`checks.rs:985`), `jpa-lombok-constructors` (`lombok_checks.rs:385`) | live (Java explicit ctors only; Lombok/Kotlin delegated) | Entities with no explicit constructor, or Lombok-generated ctors, are not judged. |
| F006 | Entity-valued field without relationship mapping | none | missing | Ledger: field whose type is an entity but carries no association annotation; needs positive target-role facts. Not implemented. |
| F007 | Absent identifier | `jpa-source-identifier-absent` (`local_repository.rs:40`, info, live); `entity-missing-id` (`checks.rs:427`, inherited-aware, legacy not live) | live (info, source-local) + legacy | XML/provider/inherited ids unresolved. |
| F008 | Conflicting identity declarations | `jpa-duplicate-embedded-id` (`local_identifiers.rs:72`, live), `jpa-multiple-id-kinds` (`checks.rs:786`, live), `jpa-embeddable-identifier` (`checks.rs:698`, live) | live (partial) | General composite-key modelling absent. |
| F009 | Equality owner/class checks | none (generation only) | missing | Ledger: validate a user-written `equals`; intent unverified. |
| F010 | Converter role annotation | none | missing | Ledger: general converter-declaration diagnostic (`@Converter` role); intent unverified. |
| F011 | Multiple version fields | `jpa-duplicate-version` (`local_versions.rs:86`, live, source-local); `attribute-type` for version types | live (partial) | Inherited/effective mappings unknown. |
| F012 | Paged results without paging input | `plugins/spring-data/src/checks.rs:94` `page-without-pageable` | other plugin, live there | Not in jpa-assist. |
| F013 | Inverse one-to-one lazy caveat | none; README documents that inverse `@OneToOne(mappedBy)` is deliberately not flagged | missing | Ledger: general LAZY advice does not model owning-side/provider limits. Intent unverified. |
| F014 | Table mapping under single-table inheritance | none | missing | Ledger: inheritance strategy/discriminator model absent; `@Table` on a SINGLE_TABLE subclass is the likely target but unverified. |
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
| I003 | UnsupportedTypeWithoutConverterInspection | none (see F002 `attribute-type`) | missing | Name suggests a persistent type with no converter; intent unverified. |
| I004 | LombokToStringLazyInspection | `jpa-lombok-to-string` | live (partial) | |
| I005 | LombokToStringLazyIncludeInspection | `jpa-lombok-to-string` (include side unverified) | live (partial) | |
| I006 | LombokEqualsAndHashCodeInspection | `jpa-lombok-equals` / `jpa-lombok-equals-field` | live | |
| I007 | LombokDataInspection | `jpa-lombok-data` | live | |
| I008 | NoArgsConstructorInspection | NEW `jpa-no-arg-constructor` (Java), `jpa-lombok-constructors`, `jpa-kotlin-plugin-missing` | live (partial) | Entities with no explicit constructors not judged. |
| I009 | ConverterSpecifiedIncorrectlyInspection | none | missing | Intent unverified (converter mis-specification). |
| I010 | IncompatibleTableAnnotationInspection | none | missing | Name suggests `@Table` incompatible with the inheritance setup (F014); unverified. |
| I011 | CompositeIdClassSerializableInspection | none | missing | See F003. |
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
