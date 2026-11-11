# JPA Assist: closing the 115 partial rows as `verified-parity`

Scope: investigation and feasibility only. Nothing here is a parity claim; no ledger status changed; the checker contract
(`scripts/check-jpa-parity.py` lines 43-58) is untouched. Reference stays JPA Buddy `com.haulmont.jpab` 263.7307.112
(archive sha256 `8402d2c1...a176` in `requirements.json`). Evidence paths below that start with `/tmp/jpa-closure/` are local
run output of this investigation, not committed.

## Short answer

`verified-parity` needs, per row, `acceptance` cases with `referenceEvidence` = what the ORIGINAL plugin produced. Only a human
with a licensed IDE and the original installed can produce that, so **no amount of headless work closes any row**. What we
can do without a desktop is make everything on OUR side of each comparison (implementationEvidence, GC-1..GC-4/6/7 checks,
diagnostic rows, prompt sequences, exact previews) machine-produced, repeatable and hash-pinned, and shrink the human
sessions to: (L2) 21 rows with desktop-only surfaces on our side plus one smoke per action family, and (L3) the original-plugin
capture for all 115 rows (24 DF scenarios + 96 AC scenarios already written). 69 of the 115 rows additionally carry a
recorded build gap that would fail comparison, so they need code before L3 is worth spending.

## (A) Evidence ladder

| Level | What | Who/where | Produces | Status |
|---|---|---|---|---|
| L0 | `cargo test` against SDK `MockHost` | agent, anywhere | unit evidence; not accepted by the contract alone | have |
| L1 | Released OpenIDEA host (v0.1.42, API 1.5), real Wasm, disposable fixture copy, scripted prompts, native edit engine | agent, headless | implementationEvidence JSON + hashes | proven in Part 2 (31/31 checks) |
| L2 | Desktop IDE session (`ACCEPTANCE.md` scenarios, receipts) | human | rendering/UX receipts for our side | kit exists, 2 example receipts only |
| L3 | Original JPA Buddy differential (`differential/`, `compare-jpa-output.py`) | human | `referenceEvidence` per row | kit exists, nothing captured |

Row closure = L3 pass (reference vs ours, same inputs) + implementationEvidence (L1 or L2) + local evidence files + code
paths. L1 and L2 never substitute for L3.

### Part 1: what the released host can prove (host source v0.1.42, `/tmp/jpa-closure/openidea`, commit e702e4d)

(a) Scripted prompts: YES, no upstream change needed. Plugin `host::pick/pick_many/input/input_multiline/confirm` become
`Prompt::{QuickPick,Input,Confirm}` (`crates/ide-plugin/src/types.rs:88-103`; `wit_host.rs:365` pick, `:562` input, `:573` confirm).
`StoreData::ask` (`wit_host.rs:309-326`) routes them to the invoking client when a FileAction is being resolved
(`action_prompt_client = invocation.client`, `runtime.rs:1495,1506`) so `Invocation.client` must be `Some`. The host registers the
waiter then emits `PluginEvent::PromptRequested{prompt_id,prompt}` (`ide-host/src/plugins/api.rs:1850-1885`; variant
`plugins/types.rs:320`); the embedder answers with `PluginHost::answer_prompt(id, Option<Vec<String>>)` (`plugins/mod.rs:1768`).
The event sink runs on the calling thread, so answering inside the sink callback is race-free (the Properties and Laravel
smokes already do this). Answer = `Some([key])` for pick, `Some([text])` for input, any `Some` for confirm, `None` = Cancel
(`wit_host.rs:573-576`: confirm returns `answer.is_some()`). The full prompt (title, items, preview message up to 16 KiB) is
visible to the harness, so the exact preview is captured. Limit: no real prompt UX, ordering of widgets, keyboard flow.

(b) Native edit engine + grouped Undo: YES. `Documents::prepare_workspace_edit` / `apply_workspace_edit` / `undo_workspace_edit`
(`ide-workspace/src/documents/workspace_edit.rs:338,388,825`) emit `IdeEvent::WorkspaceEditApplied{id}`, which the sink
turns into the Undo handle. Observed: the engine writes the edit to disk itself (no Save step), Undo restores the buffer, a
following `save` restores disk to the pristine hash. File creations (FileEdit::Create) work the same way (Laravel smoke).

(c) Provider requests: YES for diagnostics, completion, hover, definition, line markers, code actions, resolve
(`ide-host/src/plugins/providers.rs:399,425,441,461,471,491,578`; semantic tokens/inlay hints/injections also exist). The harness
passes `ProviderDocument` snapshots of documents opened through `host.project(..).documents.open`, and sends
`WorkbenchEvent::FileOpened` through `post_events` + `flush_events` (`plugins/mod.rs:1891,1978`) to warm the plugin's cross-file
cache like the IDE. Commands run headlessly via `PluginHost::run_command` (`plugins/mod.rs:1365`); `CopyToClipboard` text is
delivered as `PluginEvent::CopyToClipboard` (`plugins/types.rs:311`, `api.rs:1831`), so Copy DDL/DTO text is capturable.

(d) Native views and the Persistence tree model: YES. `PluginHost::render_view` (`plugins/mod.rs:1392`) returns the view model
(toolbar + `tree` body with key/parent/label/icon/detail/expandable); `view_event` (`:1405`) takes Button/Select/Expand/
Activate/Message (`ide-plugin/src/types.rs:762-778`). Observed: `structure` returns 86,827 bytes of tree JSON with 35 entity
nodes. NOT rendered: packaged web views (the Laravel README records the host returns a fallback message headlessly).

(e) What L1 can NEVER prove: desktop rendering and layout, native tree appearance/icons as drawn, packaged web-view/Designer
interaction (DOM, focus, drag, bridge timing in a real webview), theming/dark mode, keyboard navigation and shortcuts, real
clipboard integration, real prompt widgets and focus, lightbulb/menu placement and ordering, settings panel rendering,
perceived latency, `process:java` consoles against a real JDK/database. And never: the original plugin's behavior.

### Host pin mismatch (must be decided)

`scripts/runtime-smoke/Cargo.toml` pins rev `37c2a60` = v0.1.40 = plugin API **1.4** (`runtime.rs:57` at that rev). `jpa-assist`
declares `engines.openidea = "^1.5"`. Built as-is, `jpa_assist` compiles but install fails:
`engines.openidea "^1.5" does not match this IDE's plugin API 1.4.0`. v0.1.42 (`e702e4da7b0333e81bc818e26be72db1d157ec5d`,
API 1.5, `runtime.rs:58`) works: `laravel` and `jpa_assist` both compile unchanged against it; I did not compile `main.rs`.
Changing the rev is not an additive Cargo edit, so I left it. I ran the proof from a copy with the rev swapped (see Part 2).

## Part 2: feasibility proof (done)

New binary `scripts/runtime-smoke/src/bin/jpa_assist.rs` (+ additive `[[bin]] jpa_assist` in `Cargo.toml`). It installs
`plugins/jpa-assist/dist/plugin.wasm` (built with the v0.1.42 CLI, sha256 `5d9d4c90...9c58`) into a private host and works on a
temp copy of `boot-jpa-java-jakarta`.
Result on the v0.1.42 host: **31 PASS, 0 FAIL**, exit 0 (log `/tmp/jpa-closure/run-v0.1.42.log`, output dir `/tmp/jpa-closure/out`).

| Check group | Outcome |
|---|---|
| Live diagnostics | AttributeTypeDefects, RelationDefects, EntityCallbackDefects equal GOLDEN rule/file/line; all 61 `base` GOLDEN rows equal live rows, 0 missing, 0 extra |
| Providers | line markers on an entity (1), hover answered, derived-query completion (`findByName`, `findByProducts` on Category), 7 code actions on Customer |
| Views | `structure` tree model (35 entity nodes, none outside source `@Entity` classes; an `@Entity` interface is correctly absent), `designer` and `source-diagram` initial models |
| Command | `copy-ddl` on Category runs, 1 prompt scripted, clipboard text captured (`CREATE TABLE category ...`) |
| FileAction "Add validation constraint with preview..." on Category | 4 prompts scripted (pick attribute, pick constraint, input message, confirm); preview contains the full expected file; resolved edits applied by the native engine equal an independent oracle byte for byte (sha `f7407f77...3df0`); grouped Undo restores the original (sha `faa587bf...4f8c`); tree hash back to pristine |
| Cancel at each of 4 prompts | tree unchanged, no WorkspaceEditApplied, resolve returns `Cancelled` |
| Stale buffer edit after prompt 1 and at the final confirm | refused (`OutOfSync: the document changed meanwhile`, plugin `Error::Stale`, `runtime.rs:910`), only the harness's own edit in the buffer |

Findings worth knowing: (1) the plugin, not the host, raises Stale (it re-reads via `current()` before building edits, so the
check after prompt 1 fires after the 3rd prompt); (2) the native engine saves to disk by itself; (3) completion is quiet for
entities with inherited state (Customer extends Auditable) by design, so probes must use plain entities; (4) the run needs
about 3.6 GB of debug target for the host build (disk was nearly full during this run).

Not done in Part 2 (all feasible with the same mechanism): the repository FileAction, CRLF variant (GC-6), denied `editor.write`
(GC-5: install with a Consent lacking it), no-overwrite (GC-7), Kotlin/javax/Envers/multimodule fixtures, definition,
Designer bridge ops via `ViewEvent::Message`. Nothing blocks scripting; no upstream issue is required for prompts. Optional
upstream asks, low priority: (i) a documented headless test client in `ide-host` (a `PromptScript` helper) so each smoke does
not re-implement the sink; (ii) a headless rendering of packaged web views (would move the 17 web-surface rows from L2
toward L1; large).

Reproduce:
```sh
export PATH=$HOME/.cargo/bin:$PATH
cd plugins/jpa-assist && /tmp/openidea-cli-0142/bin/openidea-plugin build && cd ../..
# in a copy of scripts/runtime-smoke with the 5 rev fields set to e702e4da7b0333e81bc818e26be72db1d157ec5d and Cargo.lock removed:
cargo build --bin jpa_assist
target/debug/jpa_assist "$PWD" /tmp/jpa-closure/out plugins/jpa-assist/parity/fixtures/boot-jpa-java-jakarta
```
With the repo's pinned rev the binary builds and exits 1 at install with the API-1.4 message above.

## (B) All 115 partial rows

Legend. Modules: implementation files (basenames, truncated). L1: FA = FileAction end to end (prompts, preview, apply, Undo,
Cancel, stale, CRLF, denied grant, no-overwrite); DX = diagnostics vs GOLDEN; TV = tree model; CP = completion; CMD = command +
clipboard text; VW = web-view bridge messages only (UI not covered); LB = library behaviour reached only through another row's
surface. AC = `ACCEPTANCE.md` scenario numbers (AC-nnn, first 4); DF = differential scenario numbers (DF-nnn, `-` = none
exists). L2 = what only a desktop can show: W web view/Designer, S settings panel, T native tree appearance, K keyboard;
`-` = optional family smoke only. Bucket: `build` = the finding records a gap that would fail comparison (listed in Gap);
`ready` = no build gap recorded, original named in a DF scenario; `unknown` = no build gap recorded but no DF scenario names
the original behaviour (or `originalActionDocumented: false`). Every row needs L3. Buckets and tags are derived by script from
module names and `finding` text; they are a planning heuristic, and a row in `ready` still has baseline `Partial` and may
fail L3.

| ID | Capability | Modules | L1 | AC | DF | L2 | Bucket | Gap (from finding) |
|---|---|---|---|---|---|---|---|---|
| G001 | Structure browsing | structure_related,lib | TV | 001,002,003,004+1 | 024 | T | build | orm.xml/classpath unit membership, DTO nodes, duplicate simple names, native rendering |
| G002 | Related artifacts | structure_related | TV | 001,002,004,006+1 | 024 | T | build | DTO/projection nodes absent: no identification helper exists |
| G004 | Persistence units | structure_related | TV | 001,005,089 | 024 | T | ready | - |
| G006 | Mapping inspector | designer,file_actions,wizard,relationship... | FA+VW | 010,011,012,013+4 | 024 | W | ready | - |
| G007 | Index wizard | index_query_action | FA | 031 | 012 | - | build | entities without @Table, cross-table name collisions |
| G008 | Named-query wizard | index_query_action | FA | 032 | 012 | - | build | javax 2.1 behaviour unverified |
| G009 | Association advice | inspections_more,checks | DX | 081,085,091 | 002 | - | ready | - |
| G010 | Validation editor | validation_action | FA | 033 | 011 | - | build | other constraints, editing existing constraints, XML |
| G013 | Entity generation | generation,creation,identifier_action,wiz... | FA+VW | 015,020,021,022+1 | 001,023 | W | build | Module/language selection, superclass/composite ID options |
| G014 | Language selection | project_facts | LB | 015,020,021,022+2 | 001,023 | - | ready | - |
| G015 | ID type choice | generation,creation,identifier_action,wiz... | FA+VW | 015,020,022,023+1 | 001 | W | build | Full original type/provider scope |
| G016 | Sequence parameters | generation,creation,identifier_action,wiz... | FA+VW | 015,023,025 | 001 | W | build | Provider/database certification |
| G017 | Embedded ID selection | embedded_id_action,file_actions | FA | 026 | - | - | build | Key creation, Kotlin/record/inherited keys, broader source/classpath scope, equality semantics  |
| G018 | Add identifier | generation,creation,identifier_action,wiz... | FA+VW | 023,025 | - | W | build | Inherited, composite and effective mapping contexts remain open |
| G019 | Superclass extraction | extract_superclass,file_actions | FA | 027 | 004 | - | build | identifier/relationship moves, Kotlin, reference updates in other files |
| G020 | Lombok equality checks | lombok_checks | DX | 083 | 010 | - | build | JPA Buddy's own page names no rules, so rule-parity is unverified |
| G021 | Lombok lazy exclusion | lombok_checks | DX | 083 | 010 | - | build | JPA Buddy's own page names no rules, so rule-parity is unverified |
| G022 | Lombok constructor checks | lombok_checks | DX | 083 | 010 | - | build | JPA Buddy's own page names no rules, so rule-parity is unverified |
| G023 | Lombok annotation editor | lombok_checks,file_actions | FA+DX | 034 | 010 | - | build | JPA Buddy's own page names no rules, so rule-parity is unverified |
| G024 | Lombok generation defaults | wizard,settings | VW | 035 | - | WS | unknown | - |
| G025 | Converter creation | converter_action | FA | 036,096 | 012 | - | build | Kotlin, primitives/dates/enums, existing-converter detection, persistence.xml registration |
| G026 | Custom Hibernate types | hibernate_action | FA | 037 | 012 | - | ready | - |
| G027 | Hibernate events | hibernate_action | FA | 038 | 012 | - | ready | - |
| G028 | Audited toggles | envers_action | FA | 039 | 013 | - | unknown | - |
| G029 | Revision entity | envers_action | FA | 040 | 013 | - | unknown | - |
| G030 | Inspection configuration | live,settings | DX+VW | 082 | 022 | WS | unknown | - |
| G031 | Naming templates | naming_action,ddl_action,settings | FA+CMD+VW | 078 | 019 | WS | build | generate::ddl and identifier_action still hard-code snake_case; no runtime strategy detection ( |
| G032 | Physical naming | naming_strategy,schema_snapshot | LB | 078 | 019 | - | unknown | - |
| G033 | Name constants | naming_action | FA | 042 | 010 | - | build | separate constants class, embedded/collection columns |
| G034 | Lifecycle templates | local_callbacks | DX | 044,086 | 020 | - | unknown | - |
| G035 | Equality utilities | equality_action | FA | 043 | 010 | - | unknown | - |
| G036 | Repository parents | repository_action,file_actions | FA | 045 | 008 | - | build | No Kotlin compile check |
| G037 | Batch repositories | repository_action | FA | 046 | 008 | - | ready | - |
| G038 | Specification executor | repository_action | FA | 045 | 008 | - | ready | - |
| G040 | Query builder | query_method_action | FA | 048 | 009 | - | build | relationship paths, Distinct, Sort/Limit params, unresolved-call creation, editing existing que |
| G041 | Result wrappers | query_method_action | FA | 048 | 009 | - | build | relationship paths, Distinct, Sort/Limit params, unresolved-call creation, editing existing que |
| G042 | Conditions and ordering | query_method_action | FA | 048 | 009 | - | build | relationship paths, Distinct, Sort/Limit params, unresolved-call creation, editing existing que |
| G043 | Parameter options | query_method_action | FA | 048 | 009 | - | build | relationship paths, Distinct, Sort/Limit params, unresolved-call creation, editing existing que |
| G044 | Attribute query intention | query_method_action | FA | 049 | 009 | - | build | relationship paths, Distinct, Sort/Limit params, unresolved-call creation, editing existing que |
| G046 | Keyword conventions | query_method_action,settings | FA+VW | 049 | 009 | WS | build | pick_many order preservation on a real host unverified |
| G047 | Existing query editing | query_edit_action | FA | 050 | 009 | - | build | pick_many order preservation on a real host unverified |
| G048 | Graph builder | entity_graph_action | FA | 051 | 012 | - | ready | - |
| G049 | Async conversion | query_convert_action | FA | 052 | 009 | - | ready | - |
| G050 | Dynamic projection | query_convert_action | FA | 052 | 006 | - | ready | - |
| G051 | JPQL extraction | query_extract_action | FA | 053 | 009 | - | build | exists `case when count(e) > 0` form and Like-with-null unverified |
| G052 | Query completion | jpql_language,repository_completion | CP | 054,088 | - | - | build | completion inside the injected jpql document (needs source context the SDK does not appear to c |
| G053 | Projection fields | projection_action | FA | 055 | 006 | - | build | open/@Value and nested projections, fluent-setter classes, synchronisation on rename |
| G056 | Auditing support | auditing_action | FA | 041 | 014 | - | build | OffsetDateTime runtime conversion unverified |
| G057 | EntityView repositories | revision_repo_action | FA | 047 | 013,008 | - | ready | - |
| G058 | Revision repositories | revision_repo_action | FA | 047 | 013,008 | - | ready | - |
| G059 | DTO forms | projection_action | FA | 055 | 005 | - | ready | - |
| G060 | Fluent setters | dto_action | FA | 056 | 005 | - | build | Kotlin classes, Value+Jackson, entity-to-DTO sync, catalog/transitive dependency detection |
| G061 | Jackson handling | dto_action | FA | 056 | 005 | - | build | Kotlin classes, Value+Jackson, entity-to-DTO sync, catalog/transitive dependency detection |
| G062 | Lombok DTOs | dto_action | FA | 056 | 005 | - | build | Kotlin classes, Value+Jackson, entity-to-DTO sync, catalog/transitive dependency detection |
| G063 | Nested DTOs | nested_dto_action | FA | 057 | 005,006 | - | ready | - |
| G064 | DTO reuse | dto_convert_action | FA | 058 | 005 | - | build | Lombok @Value / Kotlin data class conversion sources not implemented |
| G065 | Flattening | nested_dto_action | FA | 057 | 005,006 | - | ready | - |
| G066 | SDK gating | project_facts | LB | 058,084 | 023 | - | ready | - |
| G067 | POJO conversion | dto_convert_action | FA | 058 | 005 | - | build | Lombok @Value / Kotlin data class conversion sources not implemented |
| G068 | Ordinary-class DTOs | dto_action | FA | 056 | 005 | - | build | Kotlin classes, Value+Jackson, entity-to-DTO sync, catalog/transitive dependency detection |
| G069 | MapStruct creation | mapper_action,file_actions | FA | 059 | 007 | - | ready | - |
| G070 | Mapping methods | mapper_more_action,mapper_action | FA | 060 | 007 | - | ready | - |
| G071 | Generic mappers | mapper_more_action,mapper_action | FA | 060 | 007 | - | ready | - |
| G072 | Mapper conventions | mapper_more_action,mapper_action | FA | 059,060 | 007 | - | ready | - |
| G073 | ModelMapper | modelmapper_action | FA | 062 | 007 | - | ready | - |
| G074 | Entity Views | modelmapper_action | FA | 063 | 007 | - | ready | - |
| G078 | Validation transfer | dto_action | FA | 056 | 005,011 | - | build | Kotlin classes, Value+Jackson, entity-to-DTO sync, catalog/transitive dependency detection |
| G081 | Datasource discovery | datasource_facts | LB | 065 | 018 | - | build | Oracle thin SID form, Micronaut, H2 driver/default user unverified |
| G082 | Datasource generation | datasource_action | FA | 065 | 018 | - | build | Oracle thin SID form, Micronaut, H2 driver/default user unverified |
| G083 | DDL/SQL options | datasource_facts | LB | 065 | 018 | - | unknown | - |
| G085 | Database coverage | datasource_facts | LB | 065 | 018 | - | unknown | - |
| G087 | Mapped-object inventory | ddl_parse,ddl_entities_action | FA | 066,071 | 017 | - | build | enums (G092), cardinality inference beyond ManyToOne, schema selection, existing-entity sync, v |
| G088 | Field selection | ddl_parse,ddl_entities_action | FA | 066 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G089 | Indexes/constraints | ddl_parse,ddl_entities_action | FA | 067 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G090 | Schema annotation | ddl_parse,ddl_entities_action | FA | 067 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G091 | Parent mappings | ddl_entities_action | FA | 067 | 017 | - | build | @MapsId not generated (unverified) |
| G092 | Enum creation | ddl_parse,ddl_entities_action | FA | 068 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G093 | Unknown SQL types | ddl_parse,ddl_entities_action | FA | 068 | 017 | - | build | enums (G092), cardinality inference beyond ManyToOne, schema selection, existing-entity sync, v |
| G094 | Deferred mappings | ddl_parse,ddl_entities_action | FA | 068 | 017 | - | build | Whether a String binds to types like jsonb/geometry is unverified |
| G095 | View identifiers | ddl_parse,ddl_entities_action | FA | 069 | 017 | - | build | whether Hibernate schema generation tries to create a table named after a view is unverified |
| G097 | Cardinality inference | ddl_parse,ddl_entities_action | FA | 069 | 017 | - | build | enums (G092), cardinality inference beyond ManyToOne, schema selection, existing-entity sync, v |
| G099 | Generation defaults | settings,ddl_entities_action | FA+VW | 070 | 017 | WS | ready | - |
| G100 | Database comments | ddl_parse,ddl_entities_action | FA | 069 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G101 | Name cleanup | ddl_parse,ddl_entities_action | FA | 066 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G102 | Naming algorithms | ddl_parse,ddl_entities_action | FA | 066 | 017 | - | build | Hibernate behavior with PostgreSQL named enums and per-database delimiter mapping unverified |
| G103 | Vendor mappings | ddl_parse,ddl_entities_action | FA | 069 | 017 | - | build | enums (G092), cardinality inference beyond ManyToOne, schema selection, existing-entity sync, v |
| G104 | Model/DB diff | schema,schema_snapshot | LB | 019,073 | 015,016 | - | build | No UI, baseline file I/O, SQL emission, dialect mapping, grouping or backfill editor yet |
| G105 | Model snapshots | schema,schema_snapshot | LB | 019,072 | 015,016 | - | build | No UI, baseline file I/O, SQL emission, dialect mapping, grouping or backfill editor yet |
| G106 | Persistence scope | schema_snapshot | LB | 076 | 013 | - | unknown | - |
| G107 | Initialization | schema_snapshot | LB | 072 | 015 | - | ready | - |
| G108 | Danger classification | schema | LB | 019,073 | 015,016 | - | build | No UI, baseline file I/O, SQL emission, dialect mapping, grouping or backfill editor yet |
| G109 | Rename merging | schema | LB | 019,073 | 015 | - | ready | - |
| G110 | Null backfill | sql,liquibase,migration_action,designer | FA+VW | 019,073,074 | 015,016 | W | build | Liquibase identifier quoting in <where> and per-database value escaping unverified |
| G111 | Change grouping | designer,migration | VW | 019 | 015,016 | W | ready | - |
| G112 | Changelog creation | liquibase,sql,liquibase_action,migration_... | FA | 073,074 | 015,016 | - | build | rename decision UI, defaults/identity emission, MySQL AlterNullability, Liquibase output, group |
| G115 | Update/SQL preview | migration_action | FA | 019,073,074 | 015,016 | - | ready | - |
| G117 | Java migrations | flyway_misc_action | FA | 075 | 015 | - | build | documentation.flyway.dev was unreachable from the agent, so facts are verified by javap/reflect |
| G118 | SQL callbacks | flyway_misc_action | FA | 075 | 015 | - | build | documentation.flyway.dev was unreachable from the agent, so facts are verified by javap/reflect |
| G119 | Java callbacks | flyway_misc_action | FA | 075 | 015 | - | build | documentation.flyway.dev was unreachable from the agent, so facts are verified by javap/reflect |
| G122 | Single-entity DDL | migration_action | FA | 079 | 015 | - | ready | - |
| G124 | Hibernate 6 mappings | hibernate_action | FA | 037 | 020 | - | unknown | - |
| G125 | Envers schema | schema_snapshot | LB | 077 | 013 | - | unknown | - |
| G126 | Custom SQL types | hibernate_action | FA | 037 | 020 | - | unknown | - |
| G127 | Naming strategies | naming_strategy,schema_snapshot | LB | 072,078 | 019 | - | unknown | - |
| G128 | Identifier policy | ddl_action,settings | FA+CMD+VW | 078 | 015 | WS | ready | - |
| G130 | Repository injection | injection_action | FA | 061,064 | 008 | - | build | method-call scaffolding (G131), mapper injection (G132), unresolved-object creation (G133) |
| G132 | Mapper calls | mapper_more_action,mapper_action | FA | 060,061 | 007 | - | ready | - |
| G134 | Injection preferences | settings | VW | 064 | 008 | WS | ready | - |
| I008 | com.intellij.jpb.inspection.NoA... | inspections_more | DX | 080 | 021 | - | build | Not implemented: F001, F003/I011, F004, F006, F009, F010/I009, F013, F014/I010, I003 |
| S001 | Conditional precision/scale, or... | wizard,lib,designer,attribute,designer | VW | 012,013,017,028 | 002,003 | W | build | Precision/scale, write policies, existing-attribute editing |
| S002 | Join/collection tables, overrid... | wizard,lib | VW | 013,017,029 | 002,003 | W | build | Collection tables, attribute/association overrides, join-table constraints, referencedColumnNam |
| S003 | Inheritance strategy, discrimin... | generation,wizard,creator,designer | FA+VW | 016,096 | 004,001 | W | build | editing existing hierarchies, superclass resolution/strategy cross-check against the root, supe |
| S004 | Condition ordering, nullability... | query_edit_action,query_method_action | FA | 049,050 | 009 | - | build | pick_many order preservation on a real host unverified |
| S006 | Selection synchronization, pinn... |  | TV | 010,011,014,018 | - | K | build | continuous caret events and original keyboard-popup parity remain open |
| F005 | Entity construction | inspections_more | DX | 080,085 | 021 | - | build | Not implemented: F001, F003/I011, F004, F006, F009, F010/I009, F013, F014/I010, I003 |

### Counts (script-derived, 115 rows)

| Question | Rows |
|---|---|
| Recorded build gap (would fail L3 comparison; build first) | 69 |
| No recorded build gap, original action named in a DF scenario ("closable with no further build", still needs L1+L2+L3) | 32 |
| No recorded build gap, original behaviour unknown (no DF scenario or `originalActionDocumented: false`) | 14 |
| Of the 69 build rows, also unknown original | 16 |
| L1 has an FA surface (GC-1..4,6,7 automatable) | 83 |
| L1 has DX / CP / TV / CMD surface | 9 / 1 / 4 / 2 |
| VW (web-view bridge only) / LB (library only) | 17 / 14 |
| L2 mandatory (T 3 + W 17 + S 7 + K 1; W and S overlap on 7 rows) | 21 |
| L2 optional family smoke only | 94 |
| L3 required (every row, by contract) | 115 |

"closable with no further build" is therefore 46 rows, and only 32
of them have an original behaviour documented well enough to script the comparison without discovery. 5 rows
(G017, G018, G024, G052, S006) have no DF scenario at all; S006 also has no `implementation` array, which the contract
requires for `verified-parity`. G121, G129 and S005 are not among the 115 (status unverified/blocked): they need code or a
blocker decision before they can enter this ladder.

## (C) Ledger mechanism that does not weaken the contract

Keep `verified-parity` exactly as the contract defines it. Add an optional per-row object, visible progress without any
status change:

```json
"evidenceLevel": {
  "L0": {"tests": ["cargo test jpa_assist::validation_action"], "commit": "<sha>"},
  "L1": {"harness": "scripts/runtime-smoke/src/bin/jpa_assist.rs", "wasmSha256": "...", "receipt": "plugins/jpa-assist/parity/receipts/L1/<run>.json", "checks": ["scripted-prompts","preview","undo","cancel","stale"]},
  "L2": {"receipts": ["plugins/jpa-assist/parity/receipts/AC-033.json"]},
  "L3": {"receipts": ["plugins/jpa-assist/parity/receipts/DF-011.json"], "referenceArchiveSha256": "8402d2c1..."}
}
```

Changes to make in `scripts/check-jpa-parity.py` (not done here):
1. Accept `evidenceLevel` as optional on any row; validate keys L0..L3, that every path exists under the repo and that receipts
   parse against `receipts/receipt.schema.json` with `result: pass` and `example: false`.
2. For `status == verified-parity`: require `evidenceLevel.L3` (non-example passing receipt whose `host`/provenance records the
   reference plugin version and archive sha equal to `reference.archiveSha256`) and `L1` or `L2` for our side. Keep every
   existing check. This tightens, never loosens.
3. Make `acceptance[].referenceEvidence` and `implementationEvidence` resolve to existing files (today only truthiness is
   checked), and `evidence` entries must hash-match when a `sha256` is recorded.
4. For `partial` rows: allow the levels but forbid promoting; add `--report` printing rows per highest level reached
   (L0/L1/L2/L3) and rows with a recorded build gap, so progress is visible while `355 open gaps` stays true.
5. Tests in `scripts/test-jpa-parity.py`: partial row with L3 stays open; verified row without L3 fails; example receipt is
   refused; reference sha mismatch fails.
6. `check-jpa-receipts.py` already computes per-row coverage; have it emit the `evidenceLevel` L2/L3 skeleton per row so humans
   do not hand-edit JSON (it must keep never writing `requirements.json`; the coordinator applies it).
An L1 runner should write `receipts/L1/<run>.json` in the same schema (scenario = AC id, `host` = host version) so L1 and L2 are
the same artifact type.

## (D) Ordered work plan

Effort: "A" = one focused agent increment (one worktree task, 30-90 min); "H" = one human session (2-4 h at a desktop).

| # | Step | Who | Effort | Command / output |
|---|---|---|---|---|
| 1 | Decide host pin: bump `runtime-smoke` rev to `e702e4d` (v0.1.42), regenerate its lock, check `main.rs`, laravel and every other plugin still build | agent after decision | 1 A | `cargo build --manifest-path scripts/runtime-smoke/Cargo.toml --bins` |
| 2 | Turn `jpa_assist.rs` into a scenario-driven L1 runner: JSON scenario (AC id, fixture, swap, action title, answer script, expected-file oracle) producing `receipts/L1/*.json` with hashes | agent | 3 A | `jpa_assist REPO OUT FIXTURE [--scenario ...]` |
| 3 | Scenarios for the 83 FA rows by family (entity creation, identifier, repository, query methods, DTO/mapper, DDL/entities, migrations, Lombok/Hibernate/Envers), GC-1..4/6/7 each; CRLF copy and denied-grant installs | agent | 6-8 A | one batch per family; oracle must be independent of the plugin where feasible (field/line oracle as in Part 2), otherwise a golden recorded from our own output is change-detection only and must be labelled so |
| 4 | Diagnostics for all fixtures and swaps against every `GOLDEN.md` scenario (base, swap persistence-clean/defects, kotlin, legacy, envers, multimodule) | agent | 2 A | extend the diagnostics check; DF-021/022 inputs |
| 5 | Tree/Designer/diagram models, copy commands, completion, hover/definition probes | agent | 2 A | `render_view`, `run_command`, `provide_*` |
| 6 | Ledger mechanism (section C) + checker tests | agent | 1-2 A | edit `check-jpa-parity.py`, `test-jpa-parity.py` |
| 7 | Close recorded build gaps, batched by module family (about 25 families cover the 69 rows: query_method_action, dto_action, ddl_parse/ddl_entities_action, designer/wizard web, migration/liquibase, validation/index/converter, structure_related, ...). Each gap needs a decision: implement, or accept as permanent divergence (then the row can never be `verified-parity`) | agent | 1-3 A per family, 40-70 A total | `cargo test` + step 3 scenario updates |
| 8 | Obtain and verify the original: marketplace id 15075 archive, `shasum -a 256` equals `8402d2c1...a176`, install in a separate IDE profile | human | 0.5 H | `differential/README.md` steps 1-3 |
| 9 | L2 desktop run of the 21 mandatory rows plus one smoke per action family (about 12 scenarios), then the rest of `ACCEPTANCE.md` if the user wants per-row L2 | human | 4-6 H (21 rows + families); 12+ H for all 96 AC | `ACCEPTANCE.md`; `check-jpa-receipts.py --require-pass` |
| 10 | L3 original capture: 24 DF scenarios (every row maps to at least one, except the 5 without DF: author DF-025+ first), both sides with identical inputs, `compare-jpa-output.py` per variant, behaviours (prompts, defaults, refusals, undo) into the capture sheet | human | 8-12 H | `python3 scripts/compare-jpa-output.py --ours ... --theirs ... --scenario ...` |
| 11 | Per row: fill `acceptance` cases (scenario, referenceEvidence, implementationEvidence, test, scope, result pass), `evidence` files, then promote with the checker | agent, coordinator commits | 2-3 A for all, after step 10 | `python3 scripts/check-jpa-parity.py` (must stay green; `--release` at the end) |

Critical path: 7 (build gaps) and 10 (L3) are the cost; 1-6 are cheap and make 7 and 10 safe to repeat. Run L3 once per
version of the plugin that changes behaviour, so do not start L3 before step 7 stabilises each family.

## (E) Risks

- Original not obtainable or not usable: JPA Buddy is a paid/licensed product; the archive hash is pinned, the legacy
  documentation flag is set, and the repo rules forbid decompiling or copying. If the licence or the archive is unavailable,
  no row can reach `verified-parity` under the current contract (decision for the user, below).
- Behaviour differs by original version, IDE base and JDK: 263.7307.112 on one IDE build. A later JPA Buddy or a different
  host changes defaults. Record IDE build, JDK, Maven/Gradle in every receipt (`receipt.schema.json` has `host`).
- Undocumented original behaviour: 9 DF scenarios have `originalActionDocumented: false` (DF-004, 010, 013, 014, 018, 019, 020,
  021, 022), the 5 rows without DF, and unverified rows such as G121 (schema-error repair), G129 (shared settings) and S005.
  The human must discover entry points from menus; some capabilities may simply not exist in the original, which would make the
  comparison "not present on either side" and needs an explicit rule for such rows (see decisions).
- Observable-only constraint: the kit forbids reading the original's classes or templates, so semantic rules (what it
  flags, in which inputs) are learned only by probing; coverage of the 355 rows is bounded by the fixtures' inputs.
- Non-determinism: HashMap/iteration order, timestamps and UUIDs in generated sources and migrations, IDE-generated names,
  Liquibase/Flyway version stamps, line endings (CRLF), unresolved imports. `compare-jpa-output.py` normalizers and
  `--member-order/--sql-statement-order` cover some; each use must be justified in the receipt. Our own harness avoids the
  problem by hashing whole-tree states, and fixes `Invocation` and project root.
- Harness fidelity: L1 uses the released host's real Wasm dispatch, but a different embedder than the desktop; a pass can hide
  a desktop-only defect (focus, timeouts of long prompts, 16 KiB confirm limit shown differently). Do not let L1 stand in for L2.
- Oracle circularity: expected outputs recorded from our own plugin prove stability, not parity. Only L3 compares to the
  original.
- Fuel/time and host drift: the plugin needs API 1.5; any host bump can change prompts, limits or tree models, so pin the
  host rev in the harness and record it in every L1 receipt.
- Disk: the debug host build is about 3.6 GB; builds on this machine failed once with ENOSPC.

## Decisions the user must make

1. Move `scripts/runtime-smoke` to host rev `e702e4d` (v0.1.42)? Required for jpa-assist; touches the Laravel smoke's host too.
2. Accept the `evidenceLevel` ledger mechanism and the stricter checker (section C), or keep the ledger as is.
3. Who runs the original, with which licence, on which IDE build/JDK; and is a missing original a stop condition (rows stay
   partial forever) or is there an agreed rule for capabilities the original does not have?
4. L2 scope: mandatory 21 rows plus family smokes (about 4-6 H), or all 96 AC scenarios (12+ H)?
5. For the 69 build rows: implement every recorded gap, or declare some permanent divergences (they then cannot be
   `verified-parity` under the current contract; the contract should not be weakened to hide them).
6. Priority and order: build gaps first (cheap to start, expensive in total) versus L3 early on the 32 `ready`
   rows to learn the real divergence rate before investing in 7.
