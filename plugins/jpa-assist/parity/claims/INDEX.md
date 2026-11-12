# Documented-claims index (public documentation as reference evidence)

Generated 2026-11-11 from public JetBrains IDEA help pages for JPA Buddy. No parity claim. Claims are extracted by WebFetch (model-written extraction, so the `supports` fragments are paraphrase, not byte-exact quotes). Source tags: ED entity-designer, SD spring-data, DTO dto-generator, RE reverse-engineering, DV database-versioning, LQ liquibase, FW flyway, CA coding-assistance, PT persistence-tool-window, MAIN overview, JPAB jpa-buddy.com marketing page (all its /documentation/* subpages returned that same marketing page).

Verdict rule: `documented` = 3+ explicit claims, `thin` = 1-2, `none` = 0 explicit (implied-only counts as none). Only explicit claims count toward closure. Many `thin`/`documented` rows are documented only at the level of the feature's existence or its option list: the docs state no precise output (exact generated code, error text, defaults), so verifying a claim proves the option exists, not that our output matches the original.

## Conclusion
- Rows with at least one explicit claim: 134 of 141 (24 rows added 2026-11-12, see below)
- Rows with 3+ explicit claims: 55
- Rows with none: 7 (I008, S001, S002, S003, G086, G113, G129)
- Total explicit claims: 322 (of which headless-verifiable: 249)
- Closure caveat: a documented claim is usually an option/behaviour sentence; most rows also implement behaviour (limits, refusals, Java-only scopes, settings names) the docs never state, listed in each file's `undocumented`. Even a fully verified row stays partial for those parts.

| Row | Family | Capability | Verdict | Explicit | Headless | Sources |
|---|---|---|---|---|---|---|
| F005 | Inspection families | Entity construction | thin | 1 | 1 | ED |
| G001 | Entity Designer | Structure browsing | thin | 2 | 0 | PT,SD |
| G002 | Entity Designer | Related artifacts | documented | 3 | 0 | SD,ED,DTO |
| G004 | Entity Designer | Persistence units | thin | 2 | 0 | PT |
| G006 | Entity Designer | Mapping inspector | thin | 1 | 0 | ED |
| G007 | Entity Designer | Index wizard | thin | 1 | 0 | ED |
| G008 | Entity Designer | Named-query wizard | thin | 1 | 0 | ED |
| G009 | Entity Designer | Association advice | thin | 2 | 1 | ED,JPAB |
| G010 | Entity Designer | Validation editor | thin | 2 | 0 | ED,MAIN |
| G013 | Entity Designer | Entity generation | documented | 3 | 2 | ED,DTO |
| G014 | Entity Designer | Language selection | documented | 3 | 3 | ED,MAIN |
| G015 | Entity Designer | ID type choice | thin | 2 | 2 | ED |
| G016 | Entity Designer | Sequence parameters | thin | 2 | 2 | ED |
| G017 | Entity Designer | Embedded ID selection | thin | 1 | 1 | ED |
| G018 | Entity Designer | Add identifier | thin | 2 | 2 | ED |
| G019 | Entity Designer | Superclass extraction | documented | 4 | 4 | ED |
| G020 | Entity Designer | Lombok equality checks | thin | 2 | 2 | ED |
| G021 | Entity Designer | Lombok lazy exclusion | thin | 1 | 1 | ED |
| G022 | Entity Designer | Lombok constructor checks | thin | 1 | 1 | ED |
| G023 | Entity Designer | Lombok annotation editor | thin | 1 | 0 | ED |
| G024 | Entity Designer | Lombok generation defaults | thin | 1 | 1 | ED |
| G025 | Entity Designer | Converter creation | documented | 3 | 3 | ED |
| G026 | Entity Designer | Custom Hibernate types | documented | 3 | 3 | ED,MAIN |
| G027 | Entity Designer | Hibernate events | thin | 1 | 1 | ED |
| G028 | Entity Designer | Audited toggles | thin | 2 | 2 | ED |
| G029 | Entity Designer | Revision entity | documented | 5 | 5 | ED |
| G030 | Entity Designer | Inspection configuration | thin | 2 | 1 | ED |
| G031 | Entity Designer | Naming templates | thin | 2 | 2 | ED |
| G032 | Entity Designer | Physical naming | documented | 4 | 3 | ED,DV,PT |
| G033 | Entity Designer | Name constants | thin | 2 | 2 | ED |
| G034 | Entity Designer | Lifecycle templates | thin | 2 | 0 | ED,MAIN |
| G035 | Entity Designer | Equality utilities | thin | 2 | 1 | ED,MAIN |
| G036 | Spring Data | Repository parents | documented | 3 | 3 | SD |
| G037 | Spring Data | Batch repositories | thin | 2 | 2 | SD |
| G038 | Spring Data | Specification executor | thin | 1 | 1 | SD |
| G040 | Spring Data | Query builder | documented | 4 | 4 | SD |
| G041 | Spring Data | Result wrappers | thin | 2 | 2 | SD |
| G042 | Spring Data | Conditions and ordering | thin | 2 | 2 | SD |
| G043 | Spring Data | Parameter options | thin | 1 | 1 | SD |
| G044 | Spring Data | Attribute query intention | thin | 2 | 0 | SD |
| G046 | Spring Data | Keyword conventions | thin | 1 | 1 | SD |
| G047 | Spring Data | Existing query editing | thin | 1 | 0 | SD |
| G048 | Spring Data | Graph builder | thin | 1 | 1 | SD |
| G049 | Spring Data | Async conversion | thin | 1 | 1 | SD |
| G050 | Spring Data | Dynamic projection | thin | 1 | 1 | SD |
| G051 | Spring Data | JPQL extraction | thin | 1 | 1 | SD |
| G052 | Spring Data | Query completion | thin | 1 | 0 | SD |
| G053 | Spring Data | Projection fields | documented | 3 | 3 | SD |
| G056 | Spring Data | Auditing support | documented | 3 | 3 | SD |
| G057 | Spring Data | EntityView repositories | documented | 3 | 3 | SD,MAIN |
| G058 | Spring Data | Revision repositories | documented | 3 | 3 | SD,MAIN |
| G059 | DTOs and mappers | DTO forms | documented | 5 | 5 | DTO |
| G060 | DTOs and mappers | Fluent setters | thin | 2 | 2 | DTO |
| G061 | DTOs and mappers | Jackson handling | thin | 2 | 2 | DTO |
| G062 | DTOs and mappers | Lombok DTOs | documented | 3 | 3 | DTO |
| G063 | DTOs and mappers | Nested DTOs | thin | 2 | 2 | DTO |
| G064 | DTOs and mappers | DTO reuse | thin | 2 | 2 | DTO |
| G065 | DTOs and mappers | Flattening | thin | 1 | 1 | DTO |
| G066 | DTOs and mappers | SDK gating | documented | 3 | 3 | DTO,MAIN |
| G067 | DTOs and mappers | POJO conversion | documented | 3 | 3 | DTO |
| G068 | DTOs and mappers | Ordinary-class DTOs | thin | 2 | 2 | DTO |
| G069 | DTOs and mappers | MapStruct creation | documented | 3 | 3 | DTO,MAIN |
| G070 | DTOs and mappers | Mapping methods | thin | 1 | 0 | DTO |
| G071 | DTOs and mappers | Generic mappers | thin | 1 | 1 | DTO |
| G072 | DTOs and mappers | Mapper conventions | thin | 1 | 1 | DTO |
| G073 | DTOs and mappers | ModelMapper | documented | 4 | 3 | DTO |
| G074 | DTOs and mappers | Entity Views | documented | 6 | 6 | DTO,SD |
| G078 | DTOs and mappers | Validation transfer | documented | 4 | 4 | DTO |
| G081 | Database connections | Datasource discovery | thin | 1 | 0 | RE |
| G082 | Database connections | Datasource generation | documented | 4 | 4 | SD |
| G083 | Database connections | DDL/SQL options | thin | 2 | 2 | SD |
| G085 | Database connections | Database coverage | thin | 1 | 1 | SD |
| G087 | Reverse engineering | Mapped-object inventory | thin | 2 | 0 | RE |
| G088 | Reverse engineering | Field selection | thin | 1 | 1 | RE |
| G089 | Reverse engineering | Indexes/constraints | thin | 1 | 1 | RE |
| G090 | Reverse engineering | Schema annotation | thin | 1 | 1 | RE |
| G091 | Reverse engineering | Parent mappings | documented | 3 | 3 | RE |
| G092 | Reverse engineering | Enum creation | thin | 1 | 1 | RE |
| G093 | Reverse engineering | Unknown SQL types | documented | 3 | 2 | RE |
| G094 | Reverse engineering | Deferred mappings | thin | 2 | 2 | RE |
| G095 | Reverse engineering | View identifiers | documented | 3 | 3 | RE |
| G097 | Reverse engineering | Cardinality inference | documented | 5 | 4 | RE |
| G099 | Reverse engineering | Generation defaults | documented | 3 | 3 | RE |
| G100 | Reverse engineering | Database comments | thin | 1 | 1 | RE |
| G101 | Reverse engineering | Name cleanup | documented | 4 | 4 | RE |
| G102 | Reverse engineering | Naming algorithms | thin | 1 | 0 | RE |
| G103 | Reverse engineering | Vendor mappings | thin | 2 | 1 | RE |
| G104 | Database versioning | Model/DB diff | documented | 3 | 3 | DV,LQ |
| G105 | Database versioning | Model snapshots | thin | 2 | 2 | DV |
| G106 | Database versioning | Persistence scope | thin | 2 | 2 | DV |
| G107 | Database versioning | Initialization | documented | 3 | 3 | LQ,FW,DV |
| G108 | Database versioning | Danger classification | documented | 3 | 3 | DV |
| G109 | Database versioning | Rename merging | documented | 3 | 3 | DV |
| G110 | Database versioning | Null backfill | thin | 1 | 1 | DV |
| G111 | Database versioning | Change grouping | documented | 5 | 2 | LQ,FW,DV |
| G112 | Database versioning | Changelog creation | documented | 6 | 6 | DV,LQ,FW |
| G115 | Database versioning | Update/SQL preview | documented | 3 | 1 | DV,JPAB |
| G116 | Database versioning | Change templates | documented | 5 | 5 | LQ |
| G117 | Database versioning | Java migrations | documented | 3 | 3 | FW |
| G118 | Database versioning | SQL callbacks | documented | 4 | 3 | FW |
| G119 | Database versioning | Java callbacks | documented | 5 | 5 | FW |
| G120 | Database versioning | Naming macros | documented | 5 | 5 | LQ,FW |
| G122 | Database versioning | Single-entity DDL | thin | 2 | 1 | DV |
| G124 | Database versioning | Hibernate 6 mappings | thin | 1 | 1 | DV |
| G125 | Database versioning | Envers schema | documented | 3 | 3 | DV,MAIN |
| G126 | Database versioning | Custom SQL types | documented | 4 | 3 | DV |
| G127 | Database versioning | Naming strategies | documented | 3 | 2 | DV |
| G128 | Database versioning | Identifier policy | thin | 2 | 2 | DV |
| G130 | Coding assistance and minimalistic access | Repository injection | thin | 2 | 2 | CA |
| G132 | Coding assistance and minimalistic access | Mapper calls | documented | 3 | 2 | CA |
| G134 | Coding assistance and minimalistic access | Injection preferences | thin | 2 | 2 | CA |
| I008 | localInspection | com.intellij.jpb.inspection.NoArgsConstructorInspection | none | 0 | 0 | ED |
| S001 | Designer controls | Conditional precision/scale, orphan removal, collection ordering and write policies | none | 0 | 0 | - |
| S002 | Designer controls | Join/collection tables, overrides and ownership-aware controls | none | 0 | 0 | - |
| S003 | Designer controls | Inheritance strategy, discriminator and primary-key join controls | none | 0 | 0 | - |
| S004 | Query controls | Condition ordering, nullability, case, paging, sorting and related return-type creation | documented | 4 | 4 | SD |
| S006 | Interaction | Selection synchronization, pinning, draft retention and keyboard inspector | thin | 1 | 0 | PT |

## Rows with verdict none
- I008 com.intellij.jpb.inspection.NoArgsConstructorInspection
- S001 Conditional precision/scale, orphan removal, collection ordering and write policies
- S002 Join/collection tables, overrides and ownership-aware controls
- S003 Inheritance strategy, discriminator and primary-key join controls

## Added 2026-11-12 (24 rows that had no claims file)
G003, G005, G011, G012, G039, G045, G054, G055 (entity designer, Spring Data, DTO pages); G075, G076, G077, G079, G113, G114, G121, G123 (DTO generator, database versioning, overview and Ultimate pages; the dedicated Liquibase page returned 404, so G114 rests on the overview); G080, G084, G086, G096, G098, G129, G131, G133 (reverse engineering, coding assistance, database versioning). 59 claims, 54 explicit, 25 explicit and headless-verifiable. Only G054, G055, G075, G076, G077, G114, G121, G123, G133 and G084 have headless claims; the rest are desktop surfaces. G086, G113 and G129 have implied claims only. `ourFeature` values were assigned from file names and greps, not from reading the code.
