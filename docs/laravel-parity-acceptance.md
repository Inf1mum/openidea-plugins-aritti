# Laravel parity acceptance ledger

Baseline: public Laravel Idea 13.4 documentation and applicable earlier behaviors
listed in its [changelog](https://laravel-idea.com/docs/changelog). Initial source
inventory: checkpoint `79f13dc`, 2026-11-10. This ledger is an implementation and
acceptance checklist, not a declaration of full parity. The current recheck below supersedes earlier status claims. Historical ID
matrices remain frozen checkpoint evidence; the working-tree receipt retains
earlier implementation/artifact results without promoting them to full parity.

The user authorized wizard and non-wizard implementation using our UI kit and the
current OpenIDEA host. Native custom-generator templates are an accepted format
difference; importing Velocity/Laravel Idea settings is not required. Other
approximations are open gaps, not exclusions.

## Current parity recheck — 2026-11-10

**Verdict: full parity is not achieved, even after excluding confirmed missing APIs.**
This recheck inspected the current working tree, not only checkpoint `79f13dc`.
The public Laravel Idea 13.4 baseline was re-opened. No grouped baseline row is
promoted to fully verified parity: scoped implementations/tests do not establish
all the behavior in a broad row. No defensible percentage is assigned.

The following current assessment supersedes old missing/implemented claims below.
**P** means implemented in part with known gaps; **IU** means an implemented scoped
workflow with outstanding acceptance; **M** means absent/unqualified; **D** is the
approved native-template compatibility difference. Neither IU nor P means full parity.

| IDs | Current assessment | Concrete remaining behavior / source evidence |
|---|---|---|
| G01–G03 | P: real model/key/field/relation/index wizard and related-file previews | Native model flow now selects factory/resource fields and resource relations independently. Nova/Filament outputs, inferred factory mappings and indexed resource wrappers remain absent. `generator_plan.rs`, `web/generators.js`, `generate.rs`. |
| G04 | P: atomic existing-model migration/field action | Graphical field/index editing now prepares a migration plus casts and fillable/guarded changes atomically. Keep preserves dynamic/attribute mass assignment; editing those expressions and broader model layouts remain partial. `existing_model_fields.rs`, `generator_workspace.rs`. |
| G05 | P: explicit modules, native per-kind layout/availability and Composer proof | Per-kind directories/namespaces, Application/module availability and module overrides are implemented with owned parent/module-local Composer evidence. Add Module still creates configuration, not complete nWidart scaffolding. `generator_presets.rs`, `generator_workspace.rs`, `generator_plan.rs`. |
| G06–G07 | P / D: project/user presets, CRUD, calculations and multi-file templates | Indexed model selection and live calculated values with explicit override/reset are implemented. Declared shared partials and boolean sections are implemented with bounded scoped expansion. Advanced preset/module definitions still use JSON; remaining template capabilities stay partial and native syntax remains D. |
| G08 | IU: reviewed project installer flow | Real installation, all starter options, failure recovery and desktop completion remain unverified. `project_wizard.rs`. |
| G09–G10 | P: configured native plans and atomic related generation | Base classes, postfixes, type hints/dependencies/promotion, per-kind paths/namespaces and module overrides are implemented. Factory/resource existing-model entry points now select fields and explicit declared mappings before preparing reviewed FileActions. Existing-model generation honors shared per-kind settings/availability and configured bases; resource choices additionally include bounded source-readable and proven captured fields; factory choices remain writable schema columns. Other Artisan-backed catalog workflows are not native atomic generation; all package versions/catalog settings remain unverified. |
| E01 | P: typed grouped Models tree with related classes | Cast/accessor/inherited relation/scope facts and proven factory/resource/migration/policy/observer/seeder associations are displayed with source checks. Literal primary-key properties and separate captured indexes avoid guessing inherited defaults. Captured schema must have proven matching model connection; unknown/default binding is withheld. General field usages/rename remains partial. `model_overview.rs`, `eloquent_related_facts.rs`. |
| E02–E03 | P: bounded Eloquent chains, casts and relations | No general factory-result shape, custom-factory result inference, higher-order collection proxies or arbitrary PHPDoc/query flow. `eloquent_members.rs:1252–1569`. |
| E04–E05 | P: reviewed helpers and persistent project parameters | Storage mode, optional proven relation counts and authored date annotation class persist per project. Project regeneration opens a source anchor and hands off to a reviewed editor action, not a direct one-click transaction. General proxies, runtime-field merging, automatic on-save refresh and post-generation actions remain open. `model_actions.rs`, `model_helper_parameters.rs`. |
| E06 | P: consumed exclusions, mixins, constructor forwarding and signatures | General interface/complex type metadata, donor static methods and adapted-trait forwarding remain excluded. `metadata_inference.rs`, `eloquent_objects.rs`, `metadata.rs`. |
| E07–E08 | P: fillable/model-rule insertion and captured-table wizard handoff | Configurable validation-array/model selection is absent; schema adapter preserves proven decimal/enum/set/unsigned/string metadata; insufficient or conflicting metadata, composite/keyless keys and unsupported domains remain refused. `lang_actions.rs`, `model_from_schema.rs`. |
| E09–E10 | P: SQL CREATE/custom Blueprint and ordered rename/drop | Explicit migration Change events replace existing known columns, with rename/change/drop regression coverage. SQL ALTER/constraints are not modeled; dynamic omissions do not universally mark schema incomplete. `migrations.rs`, `schema_sources.rs`. |
| E11–E13 | P: selected attributes, relations and binding names | Attribute navigation is not builder/factory/collection typing; external/fluent/polymorphic/package relations and binding-target/contextual/facade return inference remain incomplete. |
| M01–M02 | P: direct literal custom call arguments and selected domains | Named arguments, class/function import aliases (including namespace-qualified functions), current validation-rule segments and proven typed/literal instance calls are supported. Arbitrary receivers, subtype conditions, array/return places, transformations and remaining completion domains remain partial. `metadata.rs`, `metadata_inference.rs`. |
| M03 | P: parsed package overrides | Effective package version/major accessors and package gates now cover Livewire/Inertia inventories, Filament fallback, directives and legacy generator launchers. Version-specific generation remains partial. `metadata.rs`, `index.rs`, `generate.rs`. |
| M04–M06 | P: explicit metadata domains, Vite facts and dependency invalidation | Added gates/middleware, FQN modelTables, declared custom Route methods and four Blade component sources are consumed. Unsupported/malformed declarations and precedence conflicts surface diagnostics. Enum config keys, response contexts, general metadata places and dynamic Vite remain incomplete. |
| R01–R03 | P: route inventory, literal/static recognition and completion | Dedicated Route Search now ranks fuzzy name/URI/method/controller matches and revalidates the source before navigation. The inventory filter remains substring-based. Attribute routes, namespace/resource-action/parameter-name completion and general registration semantics remain open. |
| R04–R06 | P: PHP route hints, request actions, static endpoints, hover and forwarded signatures | Captured source-verified controller/invokable/static routes now publish through the host endpoints provider, preserving original methods. Unknown-location registrations remain omitted. Bare invokable controller strings also complete/navigate. Type definitions, inline Blade/Livewire/Inertia route hints and general signatures remain open. `route_runtime.rs`, `lib.rs`. |
| R07 | P: related navigation and literal matches | Reviewed typed rename actions cover proven route literals, config/PHP/JSON translations and explicit Blade aliases. Native graph anchors remain withheld because host indexed Rename inserts raw replacement text without validation/escaping. Reviewed filename-based view/anonymous-component moves update proven PHP/Blade references. PHP class-component declaration/import rename and native Find Usages remain open. `view_rename.rs`, `semantic_refs.rs`, `indexer.rs`. |
| R08–R09 | P / M: Ziggy completion and bounded Wayfinder navigation | Wayfinder stops at generated exports, not full controller/route intelligence. Frontend Inertia completion remains incomplete. Terminal autocomplete is absent; exact hook availability remains unqualified. |
| V01–V02 | P: rules, model-field rules and more Rule builders | Rule arguments after ':' and general request/message fields are incomplete; builder conversion is limited to recognized literal lists and supported versions. |
| V03–V05 | P: guarded intentions and broader controller conversion | Scope requires supported locked version, enabled setting and proven ancestry with case-insensitive collision/direct-call guards. Controller strings preserve imported/qualified/absolute identities and import collisions. Ordinary supported Request aliases/post-validation statements are handled; external/mutated bindings and Request DTO/Pest remain open. `actions.rs`, `class_actions.rs`, `request_promotion_ordinary.rs`. |
| V06–V09 | P: partial extraction/localization/vendor overrides | Proven ordinary layout wrappers now permit pure markup/simple-variable extraction, including anonymous components with props. Selected directives, active control/PHP scopes and complex captures remain withheld. Locale/vendor patterns remain narrow; Request DTO/Pest workflows are missing. `view_extract.rs`, `vendor_overrides.rs`, `lang_actions.rs`. |
| B01–B04 | P: Blade/Livewire/Inertia names, selected props and component metadata | viewPaths, phpNamespaces, explicit list and literal fromConfig feed owned component/view inventories, namespaces/bare tags and props consumers. Dynamic config and arbitrary component ancestry remain withheld. Vendor directives, full PHPDoc values, PHP injection and complete Livewire/Volt/version behavior remain partial. `component_metadata.rs`, `components.rs`. |
| B05–B09 | P: selected package launchers/templates and Passport scopes | Nova/Filament still delegate to Artisan without native package field/settings workflows. Tenancy/Pennant/Folio/Envoy/Native/extended relations and cross-plugin Twig behavior lack baseline-complete fixtures. Templates are not package intelligence. |
| T01–T02 | IU / P: captured Artisan forms, additional argv and bind-proven Docker | Quoted options are merged before the structured -- boundary and trailing values follow it; no shell expansion. Job/history retain extras for matching captured preview/run/rerun. Zero-parameter dialog skipping, terminal autocomplete, volume-only/remote interpreter layouts and native/Docker acceptance remain open. |
| T03–T05 | P: About, managed Processes and structured local errors | About has explicit app refresh; Processes owns session jobs; restart/reconnect equivalence is unqualified by the public baseline. Error format/timezone coverage and exact original Blade line mapping remain incomplete. |
| T06–T07 | P: Sentry token reads and Cloud app/environment lists | Explicit validated pagination is implemented with 20-page/2000-resource limits and stale resource rejection. OAuth, regional/self-hosted endpoints and connected-account acceptance remain open; Cloud management/deployment equivalence is unqualified. `external_services.rs`. |
| T08–T09 | IU / P: named runtime connection/schema and owned Database lifecycle | Explicit connection selection is recaptured before registration; local/Docker mapping is reviewed with target/network/published-port evidence. Schema connection survives empty results and model/migration generation. Native list/connect/remove uses project-bound rows and secure credential flow. Unsupported drivers/layouts and real Database/Docker acceptance remain open. `runtime_database.rs`, `model_from_schema.rs`, `generator_plan.rs`. |
| T10–T11 | P / M: real MCP harness and seven tools | Project Laravel-MCP route understanding and precise baseline integration scope remain unqualified; seven plugin-owned tools do not establish those features. Qodana/git/other hook coverage remains open. |

### Rich resource fields and New Model output selections — 2026-11-10

This batch implements the two approved follow-ups. Existing-model resources now
combine schema columns with current owned casts, inherited typed classic
accessors, attached `@property`/`@property-read` PHPDoc and literal class `$appends`.
The extra fields are readable resource inputs; virtual values do not become
factory write fields. Selected source dependencies are rechecked, including
unchanged-output annotation edits. Comment/string lookalikes do not create fields.

Resources can also select fields from an explicit cached Runtime schema capture
when a current literal model connection matches and exactly one table is proven.
Unknown/default connections, dynamic table resolution and schema ambiguity are
withheld. Capture identity, runner, target and application inputs are checked
again before/after review. Generation reads captures without executing PHP;
changed captures/configuration invalidate prepared actions.

New Model now has independent factory-field, resource-field and resource-relation
selectors, with applicable primary-key controls and reset-to-default behavior.
The optional `factoryFields`, `resourceFields` and `resourceRelations` arrays are
backward compatible: absent/null preserves defaults and an empty array selects
none. Native validation rejects unknown/duplicate members. UI rename/delete/new
field behavior preserves selection intent, and Preview and Finish send the same
choices. Model and migration definitions remain independent of output filtering.

Validation: **828 native tests passed, 10 ignored**. Both explicit generator PHP
lint suites passed; **12 exported existing-model PHP files passed lint**. Real
Laravel/SQLite resolves selected accessor/cast/documented/captured fields as well
as the prior factory/resource configuration and relation cases. New wizard UI
fixtures passed Chromium and WebKit. The compiled smoke test now checks empty
factory output, selected resource fields and omitted primary-key output.

Final API 1.4 build/check, compiled Wasm/real Laravel smoke (including the new
per-output selector case) and real HTTP MCP acceptance passed for **3,998,769
bytes**, SHA-256
`d9616639d6e7ecfb266fffa84485605c7934088089e462544e489fc2f87df6b1`.
Shared UI synchronization and whitespace checks passed.

Remaining boundaries: arbitrary/dynamic PHPDoc/appends/accessor shapes, modern
unproven accessors, automatic relation/factory inference and New Model indexed
resource wrappers/factory mappings are not implemented. Factory fields still use
writable schema facts. Native desktop picker/transaction acceptance remains open.

### Related-generator configuration follow-up — 2026-11-10

This supersedes the configuration mismatch in the earlier recheck below.
Existing-model factory/resource actions now use the same native settings and
per-kind option resolver as the wizard. Application/module availability,
module-local overrides, directory/namespace placeholders, configured class bases,
postfix defaults and optional return hints are honored. Explicitly entered names
are preserved. Existing models do not require model generation to be enabled.

Configured destinations and module contexts reuse the wizard's owned
ancestor/package Composer proof, including longest-prefix/path validation and
absent-file evidence. Settings, availability or Composer changes invalidate
prepared actions. Generated class/base/model identity collisions are refused.
Default legacy application destinations still preserve their existing root
Composer inference; the complete catalog is not converted to native generation.

Validation: **821 tests passed, 10 ignored**, including four new configuration,
availability, module-local mapping and stale-action regressions. **10 exported
PHP classes passed lint**. Real Laravel/SQLite acceptance still verifies mapped
factory persistence and loaded resource output, and now loads configured base
classes with optional return hints disabled. The reproduction commands in the
existing-model follow-up below use the extended fixture.

API 1.4 release build/check, compiled Wasm/real Laravel smoke and real HTTP MCP
acceptance passed for **3,962,564 bytes**, SHA-256
`cdc85fa3ae52237afb60a94cf04b54e74ed44bc5ca26a7426433cb2c9fb58711`.
UI synchronization and whitespace checks passed; no desktop acceptance is inferred.

At this earlier receipt, remaining generation work included richer fields and
New Model output selectors; the follow-up above implements those. Indexed
wrappers/inferred mappings, broader factory discovery and package/version/catalog
workflows remain partial. No host API exclusion is added.

### Earlier recheck after generation selection — 2026-11-10

Verdict unchanged: **full parity is not achieved**, excluding only the already
qualified narrow host restrictions. Reopened the public
[generation](https://laravel-idea.com/docs/generation),
[Eloquent](https://laravel-idea.com/docs/eloquent) and
[13.4 changelog](https://laravel-idea.com/docs/changelog) baseline and inspected
current source against artifact `4f62047f…`.

The existing-model selection gap is now implemented within its documented
bounds; it must not be repeated as entirely absent. Two remaining gaps need
more explicit tracking:

1. **Generator configuration differs by entry point.** `bulk.rs::output` reads
   module roots/Composer settings but never applies `Bundle::apply_settings` or
   `apply_generator_options`. Its factory/resource bodies still use fixed bases
   and type signatures, and its target layout bypasses per-kind directory,
   namespace and availability settings. A generator disabled for the selected
   Application/module can therefore still be prepared through Generate for
   Model. Name/postfix defaults also need consistent entry-point handling.
2. **Field selection is schema-column-only.**
   `related_generation_selection.rs::fields` reads `index.columns`; it does not
   offer accessor/appended/PHPDoc-only model attributes or captured Runtime
   fields. Relations use typed source facts separately, but the richer Models
   tree is not yet a shared complete field source for generation.

The New Model wizard still emits all draft fields into factory/resource output
and raw `whenLoaded` relations without the new per-output selectors/wrappers
(`generator_plan.rs:1087–1115`). Factory mapping remains explicit for `*_id`
columns and only declared owned classes; no automatic relation/factory inference
is established.

Other confirmed groups remain open: complete helper regeneration/source merging
and proxies; array/return and additional-domain metadata; default connection
binding; semantic usages/refactoring; package/validation/routing depth; terminal
completion; Sentry OAuth/original Blade mapping; project Laravel-MCP routes and
native/connected acceptance. The current assessment table retains those details.

Recheck validation: **11 focused tests passed**; re-exported generated fixtures
again passed the real Laravel/SQLite factory and loaded/unloaded resource checks.
Artifact SHA-256 is unchanged. No production source changed in this audit.

### Existing-model generation follow-up — 2026-11-10

This implementation supersedes the existing-model factory/resource selection
finding in the preceding `bb57d5bc…` audit. Generate for Model now offers all or
selected supported columns using native pickers. Selected `*_id` fields may map
to an explicitly chosen, owned declared factory class (`FactoryClass::new()`),
without guessing a relationship from the column name. Unmapped foreign keys keep
explicit review placeholders. Resources can include proven inherited relations
only when loaded, with raw output or an associated declared resource wrapper;
one/many cardinality controls constructor versus collection output.

Choices are prepared without writes and resolved through the existing reviewed,
one-shot non-overwriting FileAction. Field and mapping membership, model/source
ownership, selected class source and relation identity are rechecked before and
after confirmation. Cancellation clears superseded pending generation. Unknown
picker values, changed dependencies and newly foreign scopes are rejected.

Scope remains bounded to existing-model generation: the New Model wizard's
related-file templates do not yet share these selectors; automatic relationship
inference, arbitrary foreign-key names, general factory discovery, dynamic and
polymorphic relations and full package generators remain partial.

Validation: **817 native tests passed, 10 ignored**, including four added
selection/mapping/atomicity regressions. Generated fixtures passed PHP lint.
A real Laravel/SQLite fixture verified foreign-factory persistence and one/many
resource output with unloaded relations omitted. Reproduce with:

```sh
LARAVEL_RELATED_FIXTURES_DIR=/tmp/laravel-related-selection-check CARGO_INCREMENTAL=0 cargo test --manifest-path plugins/laravel/Cargo.toml --lib bulk:: --quiet
php plugins/laravel/tests/fixtures/related-selection-runtime.php /path/to/laravel/vendor/autoload.php /tmp/laravel-related-selection-check
```

Final API 1.4 release build/check, compiled Wasm/real-Laravel smoke and real HTTP
MCP acceptance passed for this follow-up. Artifact: **3,957,636 bytes**, SHA-256
`4f62047f6b756ccd3e4a2190b406f4db433767a725d2ca815a14bbbef8a022b4`.
Shared UI synchronization and `git diff --check` passed. Desktop picker/application
acceptance remains unverified; no full-parity claim is made.

### Earlier post-batch source recheck — 2026-11-10

Read-only implementation audit against the final `bb57d5bc…` artifact and current
source. Public generation, Eloquent, commands, routing, completion metadata and
13.4 changelog documentation were reopened. The artifact hash is unchanged.
The existing 813-pass native/build/compiled receipts remain evidence for their
stated scope; this audit does not imply another desktop or connected-service run.

| Priority | Confirmed remaining workflow | Current source evidence |
|---|---|---|
| High | Helper regeneration requires a model picker, opening the editor and a second editor action. Persistent parameters cover storage/count/date only; database+migration merging, collection proxies, on-save generation and post-generation commands remain absent. | `model_actions.rs::begin_project`, `model_helper_parameters.rs::Parameters`; [Eloquent baseline](https://laravel-idea.com/docs/eloquent), [changelog](https://laravel-idea.com/docs/changelog). The handoff is usable but does not complete the baseline helper workflow. |
| High | Generated factories/resources lack selectable field/relation mappings. Existing-model factory foreign keys deliberately emit `LogicException` placeholders and need user implementation before use. | `bulk.rs::faker`, `generator_plan.rs`; [Eloquent generation](https://laravel-idea.com/docs/eloquent). Generated PHP lint establishes syntax, not factory runtime completeness. |
| High | Completion metadata rejects documented array/return places, parent/interface filters and transformations. Unsupported categories include routeAction, moduleName, directoryFiles, artisanCommandName/Parameter, classFields, eloquentFields/Casts, bladeIcon and cssClassName(s). External staticStrings object-to-location maps and stringsWithLocation remain unsupported. | Explicit allowlists and `dataFile` array branch in `metadata.rs`; [completion contract](https://laravel-idea.com/docs/ide_json/completion). Valid unsupported object data is a compatibility gap, not malformed baseline input. |
| High | Models cannot bind ordinary implicit-default or dynamic model connections to a named capture. Only matching current explicit literal connections, including supported inheritance, receive overlays. | `eloquent_related_facts.rs::connection`, `model_overview.rs::inventory_with_schema`. Raw captures remain available through Runtime; this guard prevents incorrect attribution but leaves common model workflows incomplete. |
| Medium | Native model wizard settings do not extend to the complete generator catalog. Module creation configures native module entries; it does not scaffold nWidart. Nova/Filament resource entry points still delegate to Artisan. | `generator_workspace.rs`, `generate.rs`, `generator_presets.rs`; [generation baseline](https://laravel-idea.com/docs/generation). |
| Medium | Terminal completion remains absent, and no-parameter commands still enter the workspace flow. PHPDoc-driven component value completion, Laravel-MCP route understanding and source-safe general field/component Find Usages/Rename remain partial or missing. | `artisan_workspace.rs`, `components.rs`, `indexer.rs`, `mcp.rs`; [commands](https://laravel-idea.com/docs/commands), [13.4 changelog](https://laravel-idea.com/docs/changelog). Plugin-owned MCP tools do not establish project MCP route support. |

The current assessment table remains the complete grouped backlog, including
validation/DTO/Pest, routing/package depth, Sentry OAuth, original Blade error
mapping, localization and acceptance gaps. None of the above is newly accepted
as a missing-host-API exclusion. The narrow API qualifications below are unchanged.
One focused native regression, `unsupported_conditions_are_not_broadened`, was
rerun and passed. No production code changed during this recheck.

### Latest implementation follow-up — 2026-11-10

This follow-up supersedes the fixed findings in the historical audit immediately
below. These are scoped implementations with tests, not a promotion of broad
baseline rows to complete parity.

| Audit area | Implemented now | Remaining boundary |
|---|---|---|
| Legacy model-related generation | Factory/resource creation is prepared in memory and returned as one reviewed, one-shot FileAction. Real model FQNs and Composer/module outputs replace hardcoded namespaces; generated keys/property accesses are escaped. Unicode/empty foreign-key segments no longer panic. | Existing-model selection now supports explicit declared factory/resource mappings; unmapped foreign keys keep LogicException placeholders. New Model inferred mappings/wrappers and other catalog workflows remain partial. |
| Scope/import/dependency safety | Scope conversion proves current Eloquent identity (including aliases/FQNs and owned indexed ancestry), rejects case-insensitive method collisions/direct legacy calls, and checks ambiguous declarations. Imports/local class names compare case-insensitively. Constructor injection protects other-method property uses and uneditable constructors. Blade class transformations require a trusted supported framework version. | Dynamic/ambiguous source shapes are withheld. Documented broader controller conversion and dependency inference remain partial. |
| Metadata correctness | Namespace-import-qualified functions resolve through import mappings. Validation lists replace only the current rule name with UTF-16-safe ranges; singular rules refuse pipe lists. Malformed external data and global budgets surface diagnostics; unsaved metadata validates using cached dependencies. | Regex parameters conservatively suppress remaining-literal suggestions/navigation. New external paths need save/reindex; broader metadata domains and receivers remain open. |
| Component/view consumers | Exact namespaced Blade/Livewire tags now work through completion, definition, attributes/slots, opening/closing/self-closing references and owned source lookup. Class precedence and external namespaces remain guarded. `extendsFirst`/`includeIsolated` references and first-existing fallback layout ancestry are supported without false missing-view diagnostics for optional candidates. | Volt/version-specific intelligence, vendor directives, full PHPDoc values and generalized dynamic references remain partial. |
| Error navigation | Standard Monolog JSON origin/trace `file:line`, structured frames, PHP traces and bounded previous chains are parsed. Displayed class/message summaries redact recognizable credential assignments and omit call arguments/arbitrary context. Tests use an actual installed-Monolog fixture. | Not a general secret scrubber. Processor/context formats and exact original Blade line mapping remain incomplete. |
| Dynamic model layouts | Keep mode avoids interpreting or changing dynamic/attribute mass assignment. Pure PHPDoc edits and declared-key display use class layout independently of literal fillable parsing. | Updating dynamic mass-assignment/cast expressions remains withheld; general helper proxies and runtime-field merging remain incomplete. |
| Composer/templates | Owned bounded ancestor manifests, longest-prefix/parent and module-local Composer proofs include absent-file evidence; native per-kind Application/module layout and availability are applied. Shared/boolean templates bound scope, skipped branches, cycles and aggregate work. | nWidart scaffolding, complete catalog/version settings and remaining template capabilities stay open. Native syntax/import remains D. |
| Command/controller entry points | Console signatures/AsCommand select captured metadata without PHP. Artisan additional quoted options/arguments roundtrip preview/run/history/rerun. Controller rewrites preserve known namespace/import/FQN identity and handle invokables. | Indirect console ancestry, zero-parameter dialog skipping, general semantic controller refactoring and terminal autocomplete remain partial. |


### Historical audit findings before this implementation follow-up


This section retains the preceding read-only audit against artifact `ed06fbfc…`.
Its source positions describe that earlier working tree. The follow-up table above
supersedes resolved findings; they must not be repeated as current missing fixes.
No runtime fixes were made in that audit itself.
The Rule-import collision additionally has a PHP-lint reproduction of the
predicted invalid output, not a newly executed end-to-end editor refactoring.

| Area | Newly concrete correctness/coverage finding | Current source evidence |
|---|---|---|
| G03/G10 | Legacy factories emit `Related::factory()` for every `*_id`, even without a matching model. UTF-8-leading or empty underscore segments can panic at byte slicing. This path remains reachable from Generate for Model. | `bulk.rs:103–105,131–144`; `generate.rs:304–312` |
| V05 | Scope conversion checks the substring ` extends Model`, not proven Eloquent identity: unrelated `Modelish`/imported `App\Model` can qualify, while aliases/custom Eloquent bases are missed. The fixed framework-version gate does not solve this. | `class_actions.rs:247–250`; `lib.rs:1543–1549` |
| V02/V04 | Class-declaration collision checks are case-sensitive although PHP class names are not. A local `class rule` can be followed by a conflicting generated `use Illuminate\Validation\Rule`; import insertion checks imported aliases but not declarations. | `actions.rs:298–312`; `php.rs:288–303` |
| V05 | Blade `@class` transformations have no framework capability/version guard, unlike the documented intention requirement. | `actions.rs:26–37`; [intentions](https://laravel-idea.com/docs/intention-actions) |
| M01 | Namespace-import-qualified functions are misresolved: `use Vendor\Views as V; V\render()` is qualified as `Site\V\render` in namespace Site. Correct `functionFqn` metadata can miss, and an incorrect condition can match. | `metadata_inference.rs:680–703` |
| M01/V01 | Custom `validationRule` and `validationRules` completion use the same inventory and replace the entire literal. Completing `required\|em` may erase `required` rather than replace only the current segment. | `metadata.rs:1022,1054–1059` |
| M03/B03 | Livewire namespace inventory/literal support is not end-to-end tag support: `<livewire:blog::counter>` is excluded by name/context scanners despite an indexed `blog::counter` key. | `refs.rs:782–784,830`; `components.rs:110–114` |
| T05 | Standard Monolog JSON exception frames can be lost: `trace` strings pass through a parser accepting only `#N /path(line): call`, while installed Monolog emits `/path:line` and a separate origin `file`. Previous exceptions/context metadata are also dropped. | `runtime_errors.rs:23–30,42–50`; installed Monolog `NormalizerFormatter.php:287–319` |
| G07 | Native compound irregular inflection is not equivalent: `AccountPerson\|snake\|plural` produces `account_persons`, while the baseline example yields `account_people`. | `generator_presets.rs:753–774`; [template reference](https://laravel-idea.com/docs/template) |

Additional implementation gaps now made explicit:

| Area | Remaining behavior / corrected scope | Evidence |
|---|---|---|
| G02/G09 | Settings cover the native model/related-file planner, not every Artisan/catalog generator. Per-kind source directories, stock-template replacement and full dedicated generator/action coverage remain missing. | `generator_workspace.rs:560–582`; `generate.rs:35–87,257–312`; `generator_plan.rs:894,974,996,1022,1031` |
| G05 | Valid parent-prefix PSR-4 child namespaces and modules using their own Composer packages can be rejected: generation demands an exact namespace key in root composer.json. Per-generator module availability and configurable application-module availability are absent. | `generator_workspace.rs:361–376`; [module systems](https://laravel-idea.com/docs/modules) |
| G06–G07 | Native syntax is the accepted difference, not an exemption for conditional template sections, shared headers/includes, generic PHP-class inputs, input filters/optional fields, UTC/date and trim/replace equivalents. | `generator_presets.rs:40–78,684–705,796–894`; [custom generations](https://laravel-idea.com/docs/custom_generations), [template reference](https://laravel-idea.com/docs/template) |
| G04 | Keep mass-assignment configuration still rejects dynamic/attribute-based models before reaching the keep branch. Relationship editing is not a proven requirement of the documented Create Fields action and is no longer counted as its missing behavior. | `existing_model_fields.rs:328–340`; [Create Fields](https://laravel-idea.com/docs/eloquent) |
| E01/E04–E05 | Model overview still shows migration/direct-member facts rather than richer cast/accessor/inherited helper facts. Helper batches are selected-model batches of 1–32, with per-run parameters, hardcoded date class and no project-level command, automatic helper-file overview or persisted helper settings. PHPDoc edits are unnecessarily rejected for dynamic fillable layouts. | `model_overview.rs:37`; `eloquent_members.rs:468,490`; `model_actions.rs:19,580,628–672`; `model_helper_codegen.rs:36` |
| M02/M06 | Explicit `auth.additionalGates`, `eloquent.modelTables`, `routing.routeFacadeMethods` and `routing.additionalMiddleware` remain unsupported. External Passport/static-string data can silently drop malformed entries; unsaved metadata diagnostics and visibility of global budget warnings remain incomplete. | `metadata_validation.rs:96,127`; `metadata.rs:524,670,713,794`; `live.rs:13` |
| R03/V05 | Bare invokable route strings have no completion/navigation counterpart; controller-string conversion misses documented fully qualified/invokable forms. `extendsFirst` and `includeIsolated` are also missing reference contexts. | `lib.rs:211–218`; `controllers.rs:38`; `actions.rs:160–171`; `refs.rs:560–575` |
| T01 | Console-command Run gutters, zero-parameter dialog skipping and a dedicated additional-argv input are missing. The legacy free-form terminal path splits whitespace, losing quoted argument grouping. | `lib.rs:1057,2213–2216`; `generate.rs:188–191`; `artisan_workspace.rs:323–329`; [Artisan commands](https://laravel-idea.com/docs/commands) |
| B05/B09/T11 | Package-specific file templates remain unconditional; Laravel view discovery does not bridge Twig files. Japanese/Korean/Chinese plugin-UI localization from the 13.4 baseline has no corresponding localized UI catalog in this plugin; this is separate from Laravel translation-file support. | `plugin.toml:455`; `index.rs:308,645`; hardcoded UI strings; [13.4 changelog](https://laravel-idea.com/docs/changelog) |

A further dependency-injection collision needs a dedicated regression test:
constructor property checks omit other method bodies (`class_actions.rs:162–168`),
while naming checks inspect the selected method only (`:578–581`). Existing dynamic
`$this->cache` use elsewhere may conflict with a newly injected typed property.
This is a source-supported concern, not a newly reproduced editor transaction.

Baseline corrections: the [routing documentation](https://laravel-idea.com/docs/routing)
explicitly excludes route-name refactoring; our reviewed route-literal rename is
an extra capability. Controller class/method refactoring and documented usages
remain relevant. Restart/reconnect supervision, every Cloud deployment operation
and an unspecified MCP tool/resource superset were not established by the public
baseline reviewed here; earlier wording must not count them as confirmed parity
deficits. The plugin's own seven-tool MCP server is not evidence for project
Laravel-MCP route understanding.

### Remaining priorities after this implementation

1. Complete selectable field/relation resource/factory mapping, actual module
   scaffolding, remaining catalog/package-version settings and template behavior.
2. Expand factory/proxy/query inference, runtime-field helper merging, broader
   semantic usages/rename and optional automatic/post-generation helper behavior.
3. Finish remaining metadata places/types, validation/request contexts,
   zero-parameter command behavior and package intelligence. Explicit
   gates/middleware/modelTables/customroutes/component sources, additional argv
   and persistent helper parameters are implemented and are not missing features.
4. Complete Sentry OAuth; qualify the exact Cloud/MCP/integration baseline and
   terminal hooks. Named connection discovery/schema preservation and bounded
   Docker mapping now have source implementations; actual connected execution
   and unsupported layouts remain open acceptance, not absent APIs.
5. Qualify real desktop apply/undo, project installation, Docker and connected services.

These implementation/acceptance gaps are not automatically host-API exclusions.

### Earlier verification (before the latest capability batch)

- Final integrated Laravel suite: **777 passed, 0 failed, 9 ignored** (786 total). Ignored tests include captured inventories and opt-in PHP/real-framework fixtures. Relevant opt-in checks were run explicitly below.
- Companion Blade suite: **59 passed**, and API 1.4 Blade build/check passed for the new directive names.
- Generator-focused checks: **49 passed**, existing-model fields **8 passed**; both the all-related/key PHP lint and shared/conditional true/false PHP lint tests ran explicitly and passed. Chromium and WebKit Generators fixtures passed, preserving the stepper, gap, sibling connectors and relation autocomplete.
- Legacy generated output: **7** focused bulk tests and **5** generated PHP lint artifacts passed, including cancellation/staleness/ownership/collision/one-shot atomic creation.
- Refactoring safety: focused tests and both explicit rendered-edit PHP lint fixtures passed, including case-insensitive declarations/methods, actual Eloquent ancestry and property/constructor collision checks.
- Metadata **35** focused tests and live unsaved-diagnostics regression passed. Runtime **26** focused tests passed; the installed-Monolog replay fixture ran explicitly and passed. Namespace provider tests exercised actual metadata-built inventories and source ownership.
- Independent read-only review found no unresolved blocker in the inspected Scope, atomic related-generation, console, namespace/layout, Monolog, Composer or shared-template boundaries.
- UI synchronization and whitespace checks passed. Final Laravel API 1.4 build/check, compiled Wasm/real-Laravel smoke and real HTTP MCP acceptance passed on **3,634,475 bytes**, SHA-256 `0e77eac37ae011561a8a8ae3510be5309378419ccd6ae355cb9d7ccfe2537ee0`; the artifact hash was unchanged after both harnesses.
- No new real desktop apply/undo, project installation, Docker or connected-account acceptance was performed.

## Historical checkpoint status and evidence rules

- **IU**: implementation exists in checkpoint source; this review has not verified
  the complete stated behavior in the desktop host.
- **P**: some behavior exists; a concrete part remains missing.
- **M**: no corresponding workflow established in checkpoint source.
- **A**: an exact host API restriction has been established for the stated part.
  It does not exclude the rest of that feature.
- **D**: intentional user-approved compatibility difference.

Rows group related scenarios for readability. Every scenario in a row must pass
before marking it verified. No row is presently marked desktop-verified by this
review. Parent-session unit-test results are evidence for that tested source only,
not acceptance of a complete workflow. Prepared harnesses are not passed tests.

## Historical checkpoint: wizards, projects and settings

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| G01 | [Model field/relation/index wizard](https://laravel-idea.com/docs/generation) | P: `generate.rs` exposes Artisan forms/prompts and model flags, not the complete editor | Create fields, relations, composite/unique indexes and coordinated related files from one preview |
| G02 | [All documented class generators](https://laravel-idea.com/docs/generation) | P: `generate.rs` and manifest templates cover many classes | Account for every baseline generator, flags, names, namespaces, paths and individual command entry points |
| G03 | [Model-related resource/factory generation](https://laravel-idea.com/docs/eloquent) | P: `bulk.rs` fills selected migration-derived fields | Select model fields/relations; generate typed resources and factories with explicit unresolved mappings |
| G04 | [Existing model field creation](https://laravel-idea.com/docs/eloquent) | M: no model overview field editor | Create a new migration and reviewed fillable/guarded changes; preserve existing declarations |
| G05 | [Modules](https://laravel-idea.com/docs/modules) | P: Composer/source roots do not constitute a module wizard | Simple and configured nWidart roots; module creation and related-file placement |
| G06 | [Custom generator capabilities](https://laravel-idea.com/docs/custom_generations) | M: custom Artisan commands are not template-defined generators | Inputs, calculated parameters, multiple output files, project/global presets, preview and persistence |
| G07 | [Custom template syntax](https://laravel-idea.com/docs/template) | D: native declarative format selected by user | Document supported transformations and migration guidance; do not claim Velocity compatibility |
| G08 | [New-project wizard](https://laravel-idea.com/docs/changelog) | M | Explicit destination, runner and starter/package selection; nonempty target rejection; inspectable failure output |
| G09 | [Generator settings](https://laravel-idea.com/docs/generation) | P: selected roots/settings only | Application namespaces, base classes, postfixes, type hints and promotion settings affect generated output |
| G10 | Native multi-file application | P: selected existing FileActions are atomic; legacy direct writes are not | Full preview, no-overwrite creates, version/ownership revalidation and one undo; no sequential-write fallback |

## Historical checkpoint: Eloquent, schema and helpers

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| E01 | [Model overview](https://laravel-idea.com/docs/eloquent) | M: source facts exist without overview | Table/key, fields/types, relations, scopes and related classes; every supported item navigates correctly |
| E02 | [Field/relation/query/collection types](https://laravel-idea.com/docs/eloquent) | P: `eloquent_*` handles bounded patterns | Chained and closure queries, traits/base classes, aliases, nullable returns, casts, collection proxies and counts |
| E03 | [Factory result types](https://laravel-idea.com/docs/eloquent) | P: general/custom factory inference remains incomplete | Single/multiple creation, configured custom factories and original-source navigation |
| E04 | [Generated helpers](https://laravel-idea.com/docs/eloquent) | M: cleanup exists; full helper producer absent | Generate from migration/database/both, proxies/counts/date types, refresh and diagnostics |
| E05 | [Helper storage modes](https://laravel-idea.com/docs/eloquent) | M | Helper-only, preserve handwritten model PHPDoc, explicit overwrite mode and cleanup |
| E06 | [Helper metadata](https://laravel-idea.com/docs/ide_json/helper_code) | M | Ignore classes/namespaces, class mixins and constructor-parameter forwarding |
| E07 | [Add all model fields](https://laravel-idea.com/docs/eloquent) | P: column completion exists | Add all fields in supported fillable/validation contexts without duplicates or unrelated edits |
| E08 | [Database-backed model creation](https://laravel-idea.com/docs/changelog) | M | Owned connection/schema selection; model-from-table; table/model navigation |
| E09 | [Schema files and custom Blueprint methods](https://laravel-idea.com/docs/ide_json/database) | M | SQL dumps, configured directories/files and single/multi-column custom methods |
| E10 | Migration schema evolution | P: `migrations.rs` collects create/table columns | Ordered table/column rename/drop/change, rollback-body exclusion and incomplete/dynamic schema handling |
| E11 | Laravel attributes and custom builders | P: selected source patterns only | Baseline model attributes, custom builders/collections, factory/policy mappings and accessor declarations |
| E12 | External and extended relations | P: bounded source inference | `resolveRelationUsing`, through relations and baseline package-provided relation types |
| E13 | Container/facade intelligence | P: literal container names and known classes only | Aliases, contextual/declared bindings, real-time facades and original implementation navigation |

## Historical checkpoint: metadata and ordinary completions

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| M01 | [Custom call completions](https://laravel-idea.com/docs/ide_json/completion) | M: `ide-blade.json` roots are much narrower | Functions/methods/constructors, argument positions/names, array keys/values, return positions, transformations and links |
| M02 | [Metadata domains](https://laravel-idea.com/docs/ide_json/overview) | P: view-root subset only | Inventory each documented completion/routing/views/Blade/auth/database/Eloquent/helper/package declaration |
| M03 | [Package configuration](https://laravel-idea.com/docs/ide_json/packages/packages) | M | Enable/disable/version overrides and nonstandard layout handling |
| M04 | [Config, gates, env and responses](https://laravel-idea.com/docs/other) | P: inventories and literal completions exist | Check all documented call sites, values, enum keys, package config, grouped env and response-specific contexts |
| M05 | [Vite](https://laravel-idea.com/docs/other) | P: `index.rs` scans resource extensions | Interpret config/manifest and configured directories; resolve entries/assets to owned source |
| M06 | Metadata resolution and lifecycle | P: selected root watchers exist | Documented relative paths, project/package precedence, contradictions, malformed/unsupported entries and invalidation |

## Historical checkpoint: routing and editor integrations

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| R01 | [Route search](https://laravel-idea.com/docs/routing) | P: searchable route inventory | Fuzzy name/URL search, method/name display and correct declaration navigation |
| R02 | [Route recognition](https://laravel-idea.com/docs/routing) | P: `routes.rs` plus runtime route capture | Configured roots, bootstrap/providers, groups/prefixes, attribute routes and package conventions |
| R03 | [Route completions](https://laravel-idea.com/docs/routing) | P | Controllers/invokables, namespaces, middleware, resource only/except and route-parameter names |
| R04 | [Route/controller hints](https://laravel-idea.com/docs/routing) | P: tables/links exist; rich hints absent | Inlays plus supported actionable gutters/code lenses; browse/copy/HTTP actions and settings |
| R05 | Endpoints/Search Everywhere | IU: `lib.rs` implements `EndpointsProvider` | Supported static/runtime route identity, method, handler and location in actual host search/HTTP client |
| R06 | Hover, signature, type definition and code vision | P: hover exists; other available exports not fully used | Verify SDK-supported editor integrations rather than retaining historic API exclusions |
| R07 | Semantic usages and rename | P: navigation graph and literal search exist | Publish proven anchors, preview exact scope, update proven references, distinguish unresolved candidates |
| R08 | JS/TS/Vue route/Inertia/Wayfinder | P: `js.rs` handles selected Ziggy calls | Baseline call contexts, configured pages and Wayfinder navigation |
| R09 | Artisan terminal completion | M: native terminal dispatch is not autocomplete | Establish terminal hook availability in host; implement or document exact unavailable hook |

## Historical checkpoint: validation, refactoring and localization

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| V01 | [Rules and request fields](https://laravel-idea.com/docs/request_validation) | P: rules and selected model-column completions | Request properties, messages, configurable contexts/base classes and Add Model Fields |
| V02 | [Rule-object conversion](https://laravel-idea.com/docs/intention-actions) | P: `actions.rs` covers exists/unique/in/not_in | Add array/file/image/dimensions; import collisions, unsupported framework versions and regex safety |
| V03 | [FormRequest promotion](https://laravel-idea.com/docs/intention-actions) | P: strict literal one-method subset in `request_promotion.rs` | Broader supported controller bindings; source semantics, authorization and `safe()`/`validated()` behavior reviewed explicitly |
| V04 | [Facade alias/dependency transformations](https://laravel-idea.com/docs/intention-actions) | P: selected facade imports/rewrites | Method-wide aliases, interface selection, method/constructor injection and matching-method scope |
| V05 | [Other intentions](https://laravel-idea.com/docs/intention-actions) | P: rules, controller, scopes, braces and class rewrites exist | Single/bulk cases, imports, version gates and preservation of comments/formatting |
| V06 | View/component extraction | P: `view_extract.rs` supports reviewed bounded fragments | Binding-aware captures, correct props/slots, atomic create/edit and unsupported cases explained |
| V07 | [Localization](https://laravel-idea.com/docs/localization) | P: inventory, selected extraction/key actions | PHP/Blade extraction, PHP/JSON/vendor targets, locales, duplicates and complete multi-file preview |
| V08 | Vendor view/translation overrides | M: resolution is not override generation | Create owned override at correct namespace/locale path with no vendor modification |
| V09 | Test/debug productivity | P: templates and related navigation only | Pest subject/test navigation, request DTO, supported postfix/surround helpers and shortcuts |

## Historical checkpoint: Blade and package behavior

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| B01 | [Blade directives/layouts/components](https://laravel-idea.com/docs/blade) | P: core/project directives, layout and component inventories | Vendor/custom directives, parent layouts, sections/stacks, component props/slots/boolean attributes and PHPDoc value types |
| B02 | [Embedded languages](https://laravel-idea.com/docs/blade) | P: Blade supports bounded JS/CSS injection ranges | Supported PHP attribute/expression ranges, language ownership and non-overlapping injections |
| B03 | [Livewire](https://laravel-idea.com/docs/livewire) | P: tags/directives and selected methods/attributes | Version-specific attributes, slots, Volt, missing-component/method fixes and navigation/rename |
| B04 | [Inertia](https://laravel-idea.com/docs/inertia) | P: selected page patterns | Configurable multiple roots, page generation, diagnostics, references/usages |
| B05 | [Nova](https://laravel-idea.com/docs/nova) | P: package-gated generators/templates | Model field mapping, base classes/paths and title-translation modes |
| B06 | [Filament](https://laravel-idea.com/docs/filament) | P: package-gated generator | Baseline version structures, separate schemas/tables, relation managers and integrated model generation |
| B07 | [Passport](https://laravel-idea.com/docs/ide_json/packages/passport) | M | Declared/runtime scope contexts, inline scopes and external scope files |
| B08 | Other documented packages | P: broad coverage not established | Separate cases for tenancy, Pennant, Folio, Envoy, Dusk, Native/EDGE, Action packages and extended Eloquent packages; use changelog references per package |
| B09 | Related views/Twig/package configuration | P: existing plugins own separate languages | Verify cross-plugin ownership, namespaces, package view roots and documented navigation without duplicate providers |

## Historical checkpoint: runtime, services and external integration

| ID | Behavior and public reference | Status / checkpoint evidence | Required acceptance |
|---|---|---|---|
| T01 | [Artisan commands](https://laravel-idea.com/docs/commands) | IU: `artisan_workspace.rs` forms/output/history; local/Docker runners | Catalog arguments/options, additional argv, cancellation, stale target, terminal and nonterminal execution |
| T02 | Interpreter/container support | P: verified bind-backed local Docker targets | Compose/Sail/DDEV mappings, target changes and clear unsupported volume/remote layouts |
| T03 | Dashboard | P: application snapshots exist, no full dashboard | Project information, useful disconnected state and relevant quick actions |
| T04 | Long-running Processes view | P: command jobs/history and observed workers are not a managed processes UI | Start/output/status/stop managed processes; distinguish discovered workers; project closure/reload behavior |
| T05 | Structured Errors view | P: bounded selected-log tail only | Structured Monolog formats, timezone, metadata, original Blade frames, malformed/large input and navigation |
| T06 | Sentry integration | M | Account/connection lifecycle, error listing/details/source links, OAuth flow and credential handling |
| T07 | Laravel Cloud integration | M | Define documented account/project/environment workflows and test with explicit connected/disconnected fixtures |
| T08 | Queue/schedule/events/logs | IU: snapshots and selected failed-job retry exist | Driver availability, bounded reads, retry review, stale ownership/target and no false running-job claims |
| T09 | Database connection integration | M in Laravel; SDK supports it | Create owned sources, secure credentials, connection request and removal; do not label missing API |
| T10 | MCP/LLM integration | P: `mcp.rs` has inventory/runtime/job tools | Compare baseline routes/resources/tools, lifecycle, exact review semantics and real server harness |
| T11 | Qodana/git and other integrations | M/unqualified | Establish documented behavior and required hooks before deciding implementation or API exclusion |

## Current-host capability qualification

Pinned SDK is `openidea-plugin` v0.1.40 at revision
`37c2a6048eea89d4f6db645622612358b643862b`. Evidence below names its public Rust
source, not assumptions derived from older PARITY tables.

| Capability | Exact evidence | Consequence |
|---|---|---|
| Hover/inlays | `hover.rs::Hover`, `inlay_hints.rs::InlayHints` | Available; no blanket exclusion |
| Code lenses/actionable gutters/signature/type definitions | `language_features.rs::LanguageFeatures` methods `code_lenses`, `gutter_actions`, `signature_help`, `type_definition` | Available; labels need implementation and desktop checks |
| Embedded ranges | `language_features.rs::Injections` | Available; full PHP semantics are separate work |
| Search/HTTP endpoints | `endpoints.rs::EndpointsProvider`; `lib.rs` already registers `endpoints` | Route search integration is not blanket API-blocked |
| Definitions/references graph | `indexing.rs::Edges` / `DocumentIndexer`; pinned host `ops/lsp.rs::merge_indexed_rename` | Graph APIs exist, but native indexed Rename substitutes raw text with no plugin validation/escaping hook. Literal anchors are withheld; reviewed typed FileActions provide scoped rename |
| File transactions | `file_actions.rs::FileAction` | Multi-file operations/edits apply atomically with one undo through an editor action |
| Direct webview transaction apply | `host.rs` exposes editor text edits, but no public general `FileAction` apply function | **A**, only for applying the same transaction directly from the workspace button; editor-action workflow remains supported |
| Owned Database sources | `host.rs::Project::{upsert_data_source,remove_data_source,data_sources,set_data_source_credential,connect_data_source}` | Available with declared capability/trust; no blanket database exclusion |
| Credential writes | `host.rs::{secret_set,secret_delete,secret_is_set}` and secret-backed HTTP headers | Available; OAuth/session transport requirements still need separate verification |
| Process and terminal lifecycle | `host.rs` execution/terminal calls and `execution.rs` lifecycle events | Available for supported executions; arbitrary terminal interception/autocomplete is not established |
| Shared host PHP type system | No plugin PHP type-provider export established in pinned SDK | Does not excuse plugin-side inference; mark only a concrete host integration unavailable after reproducing the requirement |

Arbitrary virtual injection prefix/suffix context is also a narrow **A**: the pinned `language_features.rs::Injection` carries only `range` and `language`. This does not exclude physical PHP attribute/expression ranges. No other feature is currently accepted as **A**. An absent implementation, a
missing third-party account, complexity, failed probing or an unreviewed API is
not evidence of an unavailable host API.

## Verification receipts to attach

For every completed ID record the implementation revision, fixture/framework and
package versions, native test result, compiled component result and desktop
journey. Shared UI changes also require Chromium/WebKit, light/dark, keyboard and
narrow-dock checks. Native generation/refactoring needs actual apply/undo and
collision/stale/unsaved ownership cases. External integrations need deterministic
fixtures plus a real opt-in connected acceptance receipt.

The initial source inventory intentionally retains open rows. A planner, parser,
test fixture or prepared acceptance script alone does not close a user workflow.

## Working-tree source receipt (2026-11-10)

This appendix records implementation after checkpoint `79f13dc`. The initial
matrix above is intentionally frozen as checkpoint evidence; an IU/P/M there
must not be read as a claim that no later source exists. This receipt describes
the integrated working tree, not a released artifact or completed desktop
acceptance. Final commit/build identities must be attached before promotion.

| Acceptance IDs | Added source and integration | Remaining acceptance boundary |
|---|---|---|
| G01–G04, G10 | Generators workspace, typed deterministic model/field/relation/index and related-file planner, complete preview and prepared editor FileAction bridge; model field/migration actions | Native built-in catalog/version details and every planned related-file behavior need individual comparison; actual desktop apply/undo remains unverified |
| G05–G07 | Native project/user presets, reviewed project CRUD and private user persistence, calculated parameters and module roots integrated with generation | Advanced definitions use JSON editors; exhaustive module/nWidart conventions and full baseline parameter capabilities remain open; Velocity import stays D |
| G08 | New-project command with destination checks, installed CLI-help option gating and reviewed keyed-terminal launch | Disposable real project/starter-kit installation, cancellation/failure/recovery, all baseline combinations and desktop verification remain open |
| E01, E04–E07 | Models inventory, captured-schema overlay, helper PHP and preserving model PHPDoc actions; field/migration changes | Reviewed selected-model batches and overwrite mode exist; complete proxies/custom-factory helpers and on-save/post-generation workflows and field semantic usages/rename remain partial |
| E09–E10 | Migration evolution parser, bounded SQL schema-source ingestion and custom Blueprint metadata | Arbitrary dynamic migrations/SQL dialects, constraints and every schema evolution pattern remain outside established coverage |
| E08, T09 | Explicit runtime schema probe/snapshot, model-from-captured-table wizard handoff and local owned Database source registration | Unsupported/composite/keyless schema shapes and database/native desktop acceptance remain open; only verified bounded Docker published-port mappings are supported; real Docker acceptance remains outstanding |
| M01–M03, M06 | Supported `ide.json` roots, declared literal call completion/navigation, static values, package settings and diagnostics | Consumed helper exclusions, class mixins and constructor forwarding now include completion/navigation/signatures; other metadata categories, return/array contexts and general type-sensitive receivers remain partial |
| M05 | Vite literal config/manifest facts integrated into indexing | Dynamic JS/TS configuration/plugin evaluation and exhaustive source/manifest shapes remain partial |
| R01–R06 | Existing endpoints retained; route editor integrations use available inlay/LanguageFeatures APIs | Full route registration semantics, every inline action/setting and desktop search/hint behavior still need acceptance |
| R08, B07 | Passport middleware scope completion/navigation, subscriber/attribute navigation and bounded JS/TS Wayfinder references | Broader frontend resolver/alias/type behavior, generated variants and scope usages remain partial |
| V02 | Additional array/file/image/dimensions Rule-object intentions and safer method-scoped facade/dependency transformations | Framework-version behavior, every documented fluent conversion and full real-editor application require acceptance |
| V07–V08 | Reviewed PHP/JSON translation extraction plus conventional owned vendor view/translation override creation | General expression extraction, every vendor layout/locale and semantic reference transformations remain partial |
| T04 | Processes workspace with explicit preview/run/poll/stop over session-owned Artisan jobs | Lifecycle across plugin/window restart, arbitrary processes, reconnect and multi-platform native acceptance remain open |
| T05 | Bounded Monolog text/JSON record and stack parsing, runtime display integration | Full timezone/processor/format coverage and exact compiled-Blade-to-original line mapping remain open |
| T06–T07 | External Services tab/actions, declared host secrets, Sentry issues/detail/latest-event and Cloud applications/environments using official read-only endpoints | OAuth is unimplemented; token workflow is an alternative, not OAuth parity. Explicit pagination is bounded to 20 pages/2000 resources per inventory; fixed SaaS hosts, Cloud deployment/management, connected-account acceptance and richer navigation remain open |
| T10 | Runtime schema integration extends existing project-owned MCP/runtime paths | Real loopback MCP acceptance passed; full baseline tool/resource comparison remains open |

### Capability batch following the earlier receipt

The current assessment at the top supersedes earlier source-gap descriptions;
existing build/test counts and hashes below intentionally retain their original
artifact scope pending the integrating agent's new final checks.

- Native generator Application/module/per-kind paths, namespaces and availability
  now flow through reviewed plans; model-related output still needs proven owned
  Composer mappings.
- Explicit auth gates, route middleware, model/table FQNs and custom Route method
  declarations feed inventories and applicable completion/editor/runtime paths.
- All four documented component metadata sources are consumed: view directories,
  PHP namespaces, explicit lists and bounded literal config arrays. Namespaced and
  bare-tag identities retain source ownership and class precedence.
- Additional Artisan options/arguments are parsed without shell evaluation,
  previewed as exact argv and preserved through captured execution/history/rerun.
- Named database/schema capture is curated; connection identity is retained in
  generated models and migration create/drop calls. Model overview attachment
  must prove the matching connection and withholds unknown/default provenance.
- Typed Models facts now include casts/accessors/inherited members and proven
  related classes. Helper storage/count/date preferences persist per project;
  the project regeneration command opens an anchor for the reviewed editor
  action and does not directly apply an unseen transaction.
- Controller rewrites cover proven aliases, explicit namespaces and absolute
  class identities, preserving collisions and source ownership guards.

This remains partial baseline coverage. Models shipped UI fixtures passed in
Chromium/WebKit; current artifact validation is recorded below. No native desktop
or real Docker/connected-service acceptance is inferred from these browser fixtures.

### Current batch validation — 2026-11-10

- Final frozen-source native suite: **813 passed, 0 failed, 10 ignored** (823 total), with `CARGO_INCREMENTAL=0`.
- Focused source checks cover all four component metadata sources, exact additional Artisan argv/history, helper preferences, connection-bound schema overlays and stale related-class rejection.
- Generated model/helper PHP lint and expanded generator PHP lint checks passed explicitly. Generators, Artisan and Models fixtures passed in Chromium and WebKit. Shared UI synchronization and whitespace checks passed.
- API 1.4 release build and manifest/capability checks passed using the rebuilt v0.1.40 CLI. The compiled Wasm smoke harness passed against real Laravel/PHP. Artifact: **3,932,133 bytes**, SHA-256 `bb57d5bc7af06cbe1e826577674a9a747940d2b9c85f8f5eaba6632e663c5eb7`.
- Real HTTP MCP acceptance passed schema validation, Restricted Mode, window/project confirmation ownership, process grants and live-disable withdrawal. The artifact hash was rechecked unchanged after both compiled harnesses.
- The following older receipts retain their original artifact scope.

### Verified checks and their limits

- The external-services focused native run completed with **8 passed, 0 failed**:
  secret substitution, bounded/strict response parsing, sanitized error bodies,
  explicit list/detail/environment retrieval, project separation, no HTTP while
  rendering, disconnect and previous-snapshot retention after failures. Pagination
  checks cover Sentry cursor/Cloud next-page contracts, cross-origin/resource
  rejection, duplicate/cyclic links, merging, stale row keys and failed-load retention. Command:
  `cargo test --manifest-path plugins/laravel/Cargo.toml --lib external_services:: -- --nocapture`.
- Final working-tree native run: `cargo test --manifest-path plugins/laravel/Cargo.toml --lib --quiet`: **629 passed, 0 failed, 4 ignored**. The ignored generated-PHP lint check was also run explicitly: **64 generated PHP files passed**.
- Built and validated with the **0.1.40 / API 1.4** checker. Artifact `plugins/laravel/dist/plugin.wasm` SHA-256: `dd6ccfc3304dddd5a2b1651ba85e99fb9ffa636db42bfc9eaf6c68b429eed074`.
- Against that artifact, the compiled Wasm smoke harness passed real Laravel/PHP execution, cached completion, model wizard preview, registered routes, large catalogs, Config inventory, cancellation/replay/permission/staleness checks and compiled MCP flows.
- The real MCP HTTP server harness passed initialization, seven tool schemas, schema rejection, Restricted Mode, confirmation ownership/denial, process grants and live disable withdrawal using an isolated profile/project.
- Generators and Processes shipped UI fixtures passed in **Chromium and WebKit**. Existing Runtime, Components, Config, Translations and Views fixtures also passed in WebKit. `scripts/sync-plugin-ui.py --check`, runtime PHP lint and `git diff --check` passed.
- These receipts describe the uncommitted implementation after checkpoint `79f13dc`; they do not establish native desktop apply/undo, Docker or connected-service acceptance, nor full parity.
- Documentation validation checks the 67 unique matrix IDs, column structure,
  local cross-links and `git diff --check`. It does not verify remote behavior.

### Remaining implementable gaps

Do not promote these to host API exclusions simply because bounded alternatives
now exist:

1. General PHP declaration/binding analysis and the remaining Eloquent/container/
   facade/custom-factory/helper behavior, including semantically correct usages
   and rename. The absence of a shared host PHP engine does not eliminate this
   plugin-side work.
2. Complete documented metadata behavior, richer visual preset editing,
   module conventions, full generator catalog/settings and
   version-specific related-file generation.
3. Package depth beyond selected Passport/Wayfinder and existing Livewire/Inertia
   patterns: full Nova/Filament generation, tenancy, Pennant, Folio, Envoy, Native
   and extended relation packages still require individual baseline fixtures.
4. Broader binding-aware FormRequest/component/injection refactoring, all request
   field/messages contexts, source-safe field/model rename, complete translation
   behavior and all vendor override layouts.
5. Full process supervision, structured error navigation/original Blade mapping,
   Sentry OAuth/refresh lifecycle, regional endpoints and the
   documented Cloud management workflows. A registered OAuth client/account is
   a connection prerequisite, not evidence of a missing host API; the SDK has a
   loopback authorization helper.
6. Terminal autocomplete, Qodana, git/LLM hooks and exact inline interaction
   equivalence need concrete SDK qualification and implementation/acceptance.
   The available endpoints, code-lens, gutter and injection APIs must be used
   before any narrower exclusion is accepted.

External API references used for implemented contracts:
[Sentry authentication](https://docs.sentry.io/api/auth/),
[issue listing](https://docs.sentry.io/api/events/list-an-organizations-issues/),
[issue details](https://docs.sentry.io/api/events/retrieve-an-issue/),
[issue event](https://docs.sentry.io/api/events/retrieve-an-issue-event/),
[Cloud authentication](https://laravel.com/cloud/docs/api/authentication),
[Cloud applications](https://laravel.com/cloud/docs/api/applications/list-applications)
and [Cloud environments](https://laravel.com/cloud/docs/api/environments/list-environments).
