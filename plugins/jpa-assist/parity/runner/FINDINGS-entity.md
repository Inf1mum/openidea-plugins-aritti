# Findings: entity, Hibernate, Envers, naming and SQL-import claims (RS-200 .. RS-241)

Written by the coordinator from the scenario agent's report (the agent could not write this file). Documentation-derived evidence only: nothing here compares against the original plugin. Each of the 39 scenarios was run 3 times, one runner process at a time under `/tmp/jpa-runner.lock`: 20 pass, 19 fail, 0 flaky, receipts byte-identical across the 3 runs. An earlier run without the lock (load about 120) gave spurious "action not offered" failures that were discarded. The `cancelEach` and `stale` checks were dropped from these scenarios to keep runs short; only `preview` and `undo` remain where they applied. Re-verify on an idle machine before using as ledger evidence (see README, "Reliability").

## Scenario results (3 runs each)

| Scenario | Claims | Result |
|---|---|---|
| RS-200 converter auto-apply | G025-C2, C3 | pass |
| RS-201 Hibernate user type | G026-C1 | pass |
| RS-202 Hibernate event listener | G027-C1 | pass |
| RS-203 toggle `@Audited` on class | G028-C1 | pass |
| RS-204 Envers needs dependency | G028-C2 | pass |
| RS-205 revision entity example output | G029-C5 | FAIL |
| RS-206 toggle `@Audited` on field | G028-C1 | pass |
| RS-210 SQL many-to-one and inverse | G097-C3 | FAIL |
| RS-211 SQL one-to-one | G097-C1 | FAIL |
| RS-212 SQL PK-is-FK `@MapsId` | G097-C2 | FAIL |
| RS-213 SQL junction table | G097-C4 | FAIL |
| RS-214 SQL view entity | G095-C1..C3 | FAIL (C1 check passes) |
| RS-215 SQL enum class | G092-C1 | pass |
| RS-216 unmapped type TODO | G094-C1 | FAIL |
| RS-217 unmapped type must be chosen | G093-C1 | pass (option-level) |
| RS-218 SQL comments | G100-C1 | FAIL |
| RS-219 indexes in `@Table` | G089-C1 | pass |
| RS-220 schema in `@Table` | G090-C1 | pass |
| RS-221 id generation SEQUENCE | G099-C1 | pass |
| RS-222 id generation UUID | G099-C1 | FAIL |
| RS-223 fetch setting lazy | G099-C2 | pass |
| RS-224 fetch setting default | G099-C2 | pass |
| RS-225 validation annotations | G099-C3 | FAIL |
| RS-226 prefix list | G101-C1 | FAIL |
| RS-227 reserved-word suffix | G101-C2 | FAIL |
| RS-228 language picker, Java project | G014-C1 | FAIL (fixed on branch arnaud-ritti/jpa-fixes for detected-absent Kotlin; recheck after merge) |
| RS-229 language picker, Java module | G014-C3 | FAIL (same) |
| RS-230 default naming via `copy-ddl` | G032-C1 | pass |
| RS-231 Standard naming via `copy-ddl` | G032-C2, G127-C1 | FAIL |
| RS-232 migration, default naming | G032-C3, G127-C2 | pass |
| RS-233 migration, Standard naming | G032-C3, G127-C2 | pass |
| RS-234 migration audit tables | G125-C1, C3 | FAIL |
| RS-235 audit table suffix | G125-C2 | FAIL |
| RS-236 custom Java type column | G126-C1 | FAIL |
| RS-237 id type and strategy prompts | G015-C1, G018-C2 | pass |
| RS-238 sequence id example | G015-C2, G016-C1, G016-C2 | pass |
| RS-239 embedded id lists embeddables | G017-C1 | FAIL, inconclusive |
| RS-240 name constants | G033-C1, C2 | pass |
| RS-241 naming template for new entity | G031-C1 | pass |

## Findings: observed versus documented

- **G029-C5 (RS-205):** the revision entity has no `@Table(name = "revinfo")` (it would map to `audit_revision`); listener and `@RevisionNumber`/`@RevisionTimestamp` parts pass.
- **G097-C3 (RS-210):** `Book` gets `@ManyToOne` + `@JoinColumn`, but `Author` has no `@OneToMany(mappedBy)` and no `LinkedHashSet`.
- **G097-C1 (RS-211):** a unique FK yields `@ManyToOne`, no `@OneToOne`; the other side gets no `@OneToOne(mappedBy)`.
- **G097-C2 (RS-212):** a PK that is also an FK, answered "Normal entity", keeps `@Id` but has no `@MapsId`, no `optional = false`; the plugin offers a JOINED parent instead.
- **G097-C4 (RS-213):** a junction table is refused ("composite primary key is not generated"); no `@ManyToMany`/`@JoinTable`.
- **G095 (RS-214):** C1 passes (identifier-column prompt lists the view's columns). C2 half fails: `@Immutable` present but the view has setters where docs say getters only. C3 fails: no `protected` no-arg constructor.
- **G094-C1 (RS-216):** a skipped `geometry` column is only listed as "deferred" in the preview; no commented-out field with a TODO marker.
- **G100-C1 (RS-218):** comments are offered as Javadoc only; no `@Comment` alternative (the Javadoc output passes).
- **G099-C1 UUID (RS-222):** `sqlImportIdGeneration=uuid` yields no `GenerationType.UUID`; the setting offers identity, sequence, auto, none. SEQUENCE and numeric-keys-only pass (RS-221).
- **G099-C3 (RS-225):** `title varchar(120) not null` gets no `@NotNull`/`@Size(max = 120)`; no setting exists.
- **G101-C1 (RS-226):** prefix `tbl_,old_` is refused; only one table prefix is supported, no suffixes or comma lists.
- **G101-C2 (RS-227):** a column named `class` makes the whole table refused instead of producing `classField`.
- **G014-C1, C3 (RS-228, RS-229):** language picker shown in a Java-only project and in the Java-only module. The fix on arnaud-ritti/jpa-fixes removes it when Kotlin is detected absent; most Maven fixtures keep Kotlin Unknown (a `<parent>` counts as indirect), so recheck.
- **G032-C2, G127-C1 (RS-231):** with `PhysicalNamingStrategyStandardImpl` configured, `copy-ddl` still prints `pet_type`, `birth_date`; the migration generator honours the strategy (RS-233 passes).
- **G125-C1, C3 (RS-234):** after `@Audited` on `Client`, the migration creates `revinfo` but not `client_aud` (statement skipped: "column revtype: PostgreSQL has no 1-byte integer type").
- **G125-C2 (RS-235):** `audit_table_suffix=_history` is not honoured; no `client_history`.
- **G126-C1 (RS-236):** a `Money`-typed attribute is never offered a target SQL type ("Every change is withheld or unsupported").

## Not covered

| Claim | Why |
|---|---|
| G013-C2, C3 | not offered: no from-existing-class action; C1 is a desktop surface |
| G014-C2 | covered by RS-009 |
| G019-C1, C3 | not offered: methods cannot be selected to move; referenced attributes are not auto-included (C2, C4 in RS-003) |
| G018-C1 | desktop surface (palette, toolbar, Alt+Enter) |
| G025-C1 | desktop surface (inspector, structure Add button) |
| G026-C2, C3 | not offered: no parameter option, no inspection with a quick fix |
| G029-C2, C3, C4 | not offered: no parent or table name choice, no existing-listener pick, no source root/package choice |
| G031-C2 | not expressible: derived names come from templates, not from the physical strategy |
| G091-C1..C3 | not offered: no `@MappedSuperclass` parent drop-down; only JOINED parents between selected tables |
| G093-C3 | not offered: no Hibernate Types suggestions |
| G094-C2 | not offered: no "Uncomment as is" / "Remove column mapping" actions |
| G101-C3 | covered by RS-008 |
| G101-C4 | not offered: no option to replace ORM references with basic attributes |
| G103-C2 | not offered: no override targets in the SQL import |
| G126-C3, C4 | not offered: no target SQL type prompt, no custom SQL type setting |
| G017-C1 | inconclusive: RS-239 fails 3 of 3 with "No eligible Java composite key" even for a plain public `PairKey` added by setup; cause not isolated |
| G002 | no headless-verifiable explicit claim |
| G105, G106, G128, G124, G009, F005 | left to other families |

## Row conclusions

No row in this family reaches "all explicit claims covered and passing" with full coverage. A row listed as satisfied is still partial for undocumented behaviour and for desktop surfaces.

| Row | Passing | Failing | Conclusion |
|---|---|---|---|
| G013 | none | none (C2, C3 not offered) | not satisfied |
| G014 | C2 | C1, C3 | not satisfied (recheck after the fix) |
| G015 | C1, C2 | none | satisfied |
| G016 | C1, C2 | none | satisfied |
| G017 | none | inconclusive | not satisfied |
| G018 | C2 | none (C1 desktop) | headless claim passes |
| G019 | C2, C4 | none (C1, C3 not offered) | not satisfied |
| G025 | C2, C3 | none (C1 surface) | headless claims pass |
| G026 | C1 | none (C2, C3 not offered) | not satisfied |
| G027 | C1 | none | satisfied |
| G028 | C1, C2 | none | satisfied |
| G029 | C1 | C5 | not satisfied |
| G031 | C1 | none (C2 not expressible) | not satisfied |
| G032 | C1, C3 | C2 | not satisfied |
| G033 | C1, C2 | none | satisfied |
| G089 | C1 | none | satisfied |
| G090 | C1 | none | satisfied |
| G091 | none | none (not offered) | not satisfied |
| G092 | C1 | none | satisfied |
| G093 | C1 (option-level) | none (C3 not offered) | not satisfied |
| G094 | none | C1 | not satisfied |
| G095 | C1 | C2, C3 | not satisfied |
| G097 | none | C1..C4 | not satisfied |
| G099 | C1 (SEQUENCE), C2 | C1 (UUID), C3 | not satisfied |
| G100 | none | C1 | not satisfied |
| G101 | C3 | C1, C2 (C4 not offered) | not satisfied |
| G103 | none | none (C2 not offered) | not satisfied |
| G125 | none | C1, C2, C3 | not satisfied |
| G126 | none | C1 (C3, C4 not offered) | not satisfied |
| G127 | C2 | C1 | not satisfied |

## Notes

- RS-232 to RS-235 pin the file name with `flywayVersionPattern` and a date answer (`V20240131.0003__…`); the `0003` follows the numbering before the Java-migration fix, so rerun them once arnaud-ritti/jpa-fixes is merged.
- RS-238 accepts qualified or simple annotation names because the plugin writes `@jakarta.persistence.Id`.
- RS-203/206 split G028-C1 into class and field; RS-221/222 split G099-C1 into SEQUENCE and UUID. NONE and IDENTITY are not checked separately.
