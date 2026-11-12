# Extension-row re-evaluation (2026-11-12)

Four read-only passes re-evaluated the 144 open E/A/I/F rows against the decompiled original (read-only reference) and our plugin. Per-part proposals: `ext-resolution-0..3.json`. Applied: 28 remaps (confidence medium or high) and 6 not-applicable decisions (non-F rows). Not applied: low-confidence remaps (F009, I005) and the F rows proposed not applicable (F001, F006, F008, F010, F012, F013, F017), which stay mapped to S005. Rows whose mapped targets are all verified inherit closure (`scripts/inherit-jpa-ledger.py`).

## Real capabilities of the original that our plugin lacks

| Row | Missing capability | Suggested owner |
|---|---|---|
| A005 | MISSING capability: create a new related entity with both relationship sides. Should be owned by G003 (structure creation) with G009 as secondary; today G003 has no claim and no implementation of this flow. | G003, G009 |
| A023 | MISSING: create related entity (relation to). Owner G003/G009. | G003, G009 |
| A024 | MISSING: create related entity (relation from). Owner G003/G009. | G003, G009 |
| E003 | MISSING: converter and custom-type gutter navigation from attributes. OpenIDEA supports line-markers (navigate icon). Owner G002 (related artifacts) or G025. | G002, G025 |
| E004 | MISSING: reverse converter-to-attributes gutter navigation. Owner G002 or G025. | G002, G025 |
| E014 | MISSING: cross-file usages and rename through mappedBy. The host supports [[contributes.documentIndexers]] (API 1.3) for definitions/references feeding Find Usages and Rename, which jpa-assist does not use. Owner G079 (related navigation), rename side G075. | G079 |
| E015 | MISSING: usages of entity members through MapStruct mapping literals; same indexer route as E014. Owner G079 / G075; G132 and G069 are not the capability. | G132, G069 |
| E016 | MISSING: Ctrl+click navigation from these literals. A definition provider or indexer could supply it. Owner G079. | G079 |
| E033 | Missing capability: a 'Dto & Projections' group under each entity in the Persistence tree. Owner G002 (claims G002-C2, G002-C3), cross-reference G079-C1. Needs DTO discovery per entity (dto_links.rs already has the linking rules). | G001, G002 |
| E034 | Part of the missing 'Dto & Projections' tree group (see E033); owner G002. | G001 |
| E035 | Part of the missing 'Dto & Projections' tree group (see E033); owner G002. | G001 |
| E041 | Missing capability: DTO discovery through MapStruct mapper methods. Owner G064 (reuse of existing DTOs); also feeds G079 and G076 sync. Ours never reads mapper interfaces for linking. | G064, G079 |
| E052 | Missing capability: MapStruct @Mapping-aware field correspondence in rename/sync (owner G076, also G075). G070 (mapping methods) is not the owner. Mapper generation writes @Mapping pairs (mapper_more_action.rs) but sync/rename never reads them. | G076, G070 |
| E061 | Missing capability: ModelMapper mapTo call completion. Owner G073 (G073-C2 postfix scaffolding) with G132 for the call form. | G073, G132 |
| E062 | Missing capability: create-method items on a repository instance. Owner G131 (repository methods found or created from a call). G130 (injection) is not what this does. | G131, G130 |
| E065 | Missing capability: create entity / create mapped superclass from an unresolved reference. Owner G133 (G013 for the creation itself). The method-creation branch belongs to G045, which also stays open. | G133 |
| E066 | Missing capability: context-driven DTO creation from a repository method return type. Owner G133 (G133-C3 covers only the postfix-triggered case, RS-575). | G133, G059 |
| E067 | Missing capability: DTO creation from a ModelMapper call. Owner G133, with G073. | G133, G073 |
| E068 | Missing capability: DTO creation from a MapStruct mapper context. Owner G133, with G069. | G133, G069 |
| E082 | Real capability absent: no projectView.context menu with creation commands, although the host supports that menu. Owner G003 (or G013). | G003, G013 |
| E083 | Same gap as E082: no project-tree creation submenu; each creator exists only as a code action or command. Owner G003. | G003 |
| E096 | Original is not G052 query completion; it injects into entity-view mapping literals. Not implemented. Owner G074 (entity views, verified) or G052. | G052 |
| E113 | Our completion lacks the plain columnNames attribute (addPrimaryKey, addUniqueConstraint). Owner G114 (verified for table/column completion; this is an undocumented tail). | G113 |
| E134 | MISSING capability: add attributes from database columns to an existing entity, owned by G096 (claims G096-C1/C2, not implemented). The DTO-attributes and auditing items belong to other rows. | G096 |
| E135 | G088 (column selection) is verified but implemented for new entities from SQL files, not for updating an existing entity; remove it as a target. Real owner is G096 (missing). | G096 |
| E136 | G087 (mapped-object inventory) is not what this palette item does; it is the entry to updating an existing entity from the DB (G096, missing). | G096 |
