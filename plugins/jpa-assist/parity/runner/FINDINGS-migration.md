# Findings: migration, Flyway, Liquibase, datasource claims (RS-300 .. RS-331)

Written by the coordinator from the scenario agent's report (the agent could not write this file). Documentation-derived evidence only: nothing here compares against the original plugin. Each scenario was run 3 times, one runner process at a time under `/tmp/jpa-runner.lock`; all 32 passed `--validate`, none was flaky, and receipts were byte-identical across the 3 runs of every scenario. Re-verify on an idle machine before using as ledger evidence (see README, "Reliability").

Runner issue observed: when several scenarios share one runner process, later ones sometimes failed with "action not offered; offered []" (earlier batch run, before the load instructions). The same scenarios were stable with one process each. Treat as a possible state leak between scenarios in one process until investigated.

## Results (pass/pass/pass or fail/fail/fail)

| Scenario | Claims | Result |
|---|---|---|
| RS-300 | G109-C1 | pass |
| RS-301 | G104-C1, G104-C3 | pass |
| RS-302 | G105-C1 | pass |
| RS-303 | G105-C2 | pass |
| RS-304 | G104-C2, G106-C1 | fail |
| RS-305 | G107-C2 | fail |
| RS-306 | G107-C1 | fail |
| RS-307 | G112-C4, G112-C6, G115-C3 | fail |
| RS-308 | G116-C3 | fail |
| RS-309 | G116-C4 | pass |
| RS-310 | G120-C4 | fail |
| RS-311 | G120-C3, G120-C4 (major/minor/patch part) | pass |
| RS-312 | G120-C5 (prefix U, separator `___`) | pass |
| RS-313 | G120-C5 (defaults) | fail, dependsOnFix (fixed on branch arnaud-ritti/jpa-fixes) |
| RS-314 | G117-C2, G117-C3 | pass |
| RS-315 | G117-C1 | fail |
| RS-316 | G118-C1, G118-C3 | pass |
| RS-317 | G119-C2, G119-C5 | fail |
| RS-318 | G119-C4 | fail |
| RS-319 | G119-C1 | fail |
| RS-320 | G082-C4, G085-C1 | fail |
| RS-321 | G082-C2, G082-C3, G083-C1, G083-C2 | fail |
| RS-322 | G032-C1, G032-C3, G127-C2 | pass |
| RS-323 | G032-C2, G127-C1 | pass |
| RS-324 | G125-C1 | pass |
| RS-325 | G125-C3 | fail |
| RS-326 | G125-C2 | fail |
| RS-327 | G066-C1 | fail |
| RS-328 | G066-C2 (negative) | pass |
| RS-329 | G066-C2 (positive) | pass |
| RS-330 | G066-C3 | pass |
| RS-331 | S004-C2 | pass |

## Findings: observed versus documented

- **RS-304 (G104-C2, G106-C1):** docs say a persistence unit must be selected. With a two-unit persistence.xml the baseline action never asks; its first prompt is the "Incomplete schema information" confirm and the scope is the whole project.
- **RS-305, RS-306 (G107-C1, G107-C2):** docs describe an init DDL / init changelog from the model. Both actions refuse with "No schema baseline exists; run 'Create schema baseline…' first." There is no init mode.
- **RS-307 (G112-C4, G112-C6, G115-C3):** docs say changes are appended when the file exists. The action refuses: "already exists or has an open buffer; nothing is overwritten."
- **RS-308 (G116-C3):** addColumn with onFail=HALT/onError=CONTINUE is honoured; a dropColumn row with onFail=WARN/onError=HALT produces no `<preConditions>` (preconditions only on createTable and addColumn).
- **RS-310 (G120-C4):** for `1.2.3-SNAPSHOT+build4` the action is refused: the expanded name contains a character outside ASCII letters, digits, '.', '_' and '-'.
- **RS-313 (G120-C5, dependsOnFix):** default naming produced `V3__defaults.sql`, colliding with the Java migration `V3__BackfillInvoiceStatus`. Fixed on branch arnaud-ritti/jpa-fixes (0fa0be56); re-run after merging.
- **RS-315 (G117-C1):** docs say the migration has a `PreparedStatement` stub with an empty SQL string; the generated `migrate` only obtains the connection.
- **RS-317 (G119-C2, G119-C5):** `supports()` and `handle()` are generated; `canHandleInTransaction()` and `getCallbackName()` are not generated (inherited from `BaseCallback`).
- **RS-318 (G119-C4):** docs say the default package is `db.callback`; the callback is created next to the invoking file.
- **RS-319 (G119-C1):** docs list the fields Name, Callback name, Callback event, Can handle in transaction, Source root, Package; the action asks an event multi-select and "Callback class name", then a confirm.
- **RS-320 (G082-C4, G085-C1):** docs list 8 databases; we offer 4 (PostgreSQL, MySQL, MariaDB, H2). HSQLDB, Db2, SQL Server, Oracle missing.
- **RS-321 (G082-C2, G082-C3, G083-C1, G083-C2):** no DDL-auto choice and no Show SQL / Format SQL toggle.
- **RS-325 (G125-C3):** adding a column to audited `Client` migrates `client` only; the `client_aud` counterpart is "SKIPPED (unsupported): nullability of column score is unknown".
- **RS-326 (G125-C2):** `spring.jpa.properties.org.hibernate.envers.audit_table_suffix=_history` is ignored; the baseline still names `client_aud` and `invoice_aud`.
- **RS-327 (G066-C1):** with `java.version` 11, "Create scalar DTO with preview…" (record form) is still offered.

## Per row (explicit headless claims)

| Row | Covered | Failing | Not offered / not expressible |
|---|---|---|---|
| G082 | C2, C3, C4 | C2, C3, C4 | C1 needs an existing IDE data source; not expressible |
| G083 | C1, C2 | both | none |
| G085 | C1 | C1 | none |
| G104 | C1, C2, C3 | C2 | DB as source or target is not offered |
| G105 | C1, C2 | none | New > Data Model Snapshot menu not expressible |
| G106 | C1 | C1 | C2 (limit scope to selected entities) not offered |
| G107 | C1, C2 | both | C3 (optional scope limit) not offered |
| G108 | none new (C2 in RS-006) | none | C1: levels named Safe/NeedsBackfill/DataLoss, no colours; C3: customisable levels not offered |
| G109 | C1 | none | C2, C3 (merge to a rename) live in the packaged web view; no headless surface |
| G110 | RS-006 only | none | none |
| G111 | none | none | C2, C5 (primary/secondary/ignored changelogs) not offered |
| G112 | C4, C6 | C4, C6 | C5 (save as clipboard/scratch/console) and file-type choice not offered |
| G115 | C3 | C3 | other claims not headless |
| G116 | C3, C4 | C3 | C5 (dbms properties) not offered |
| G117 | C1, C2, C3 | C1 | source root and package choice not offered |
| G118 | C1, C3 | none | C4 is IDE recognition; not expressible |
| G119 | C1, C2, C4, C5 | all four | C3 (in-transaction toggle) not offered |
| G120 | C3, C4, C5 | C4 (`+` refused), C5 defaults (dependsOnFix) | prefix R not offered |
| G066 | C1, C2, C3 | C1 | Maven multi-module not exercised |
| G032 | C1, C2, C3 | none | C4 not headless |
| G127 | C1, C2 | none | custom strategy section not covered |
| G125 | C1, C2, C3 | C2, C3 | `@RevisionEntity` table not covered (fixture has none) |
| S004 | C2 | none | named-vs-positional control and C3 not offered; C1, C4 in RS-104 |
| G126, G128, G122, G081 | none | none | no matching option in our actions |
| G031, G033 | none | none | left to the entity family |

## Row-level conclusion

No row has every explicit headless claim covered and passing. Rows passing everything we could express: G105 (C1, C2) and G118 (C1, C3; C4 is IDE-side). G032 and G127 pass their expressible claims but have a claim that is not headless or not covered. Every other row has a failing claim, a not-offered claim, or both.

## Open after the fix round (2026-11-12)

- **RS-306 (G107-C1):** still fails 3 of 3. The Liquibase init changelog skips every table with a decimal column because `decimal(p,s)` syntax is not shown in the Liquibase docs read (column change-type reference and data-type handling page: only `VARCHAR(255)` is sized, decimals appear as bare `DECIMAL`/`NUMBER` names). Left failing rather than emit unverified syntax. Foreign keys without a constraintName are skipped for the same reason.
