# Laravel13.4 remaining baseline qualification — 2026-11-11

This resolves behavior questions; it is not an implementation/native acceptance receipt.
Public docs were checked alongside the already audited, publicly distributed
Laravel Idea **13.4.1.261** artifact. Its JAR SHA256 is
`d074563e4d819f4baf0d238555b08ed4e24ab378a92222905cfd2fe5d137552f`.
The patch artifact is supporting primary product evidence; do not represent it as
13.4.0 source. Public changelog dates independently establish the relevant feature
families before the13.4 baseline.

Artifact provenance: [Marketplace](https://plugins.jetbrains.com/plugin/13441-laravel-idea),
[versioned distribution](https://plugins.jetbrains.com/files/13441/1181127/laravel-idea-13.4.1.261.zip),
repository original receipt `docs/parity-2026-11-08/evidence.json`, and local original
`/tmp/laravel-idea-audit-20261110/laravel-idea-13.4.1.261.jar`.
Recovered class/member names below identify evidence for independent inspection;
no recovered implementation is copied into production source.

## Metadata completion transformers: qualified

The [12.3 changelog](https://laravel-idea.com/docs/changelog) confirms transformers
for configKey, translationKey, viewName and staticStrings. The public
[completion page](https://laravel-idea.com/docs/ide_json/completion) does not explain
the expression language. The artifact resolves it: `CompletionsModel.transformers`
is an ordered string array on each completion entry, alongside complete/options/condition.
`CompletionTransformerFactory` recognizes these eight names:

| Expression family | Behavior |
| --- | --- |
| `filter:hasPrefix:ARG` | Retain lookups starting with any alternative. |
| `filter:hasPostfix:ARG`, `filter:hasSuffix:ARG` | Retain lookups ending with any alternative. |
| `filter:contains:ARG` | Retain lookups containing any alternative. |
| `filter:doesntContain:ARG` | Retain lookups containing none of the alternatives. |
| `transform:removePrefix:ARG` | Retain matching lookups and strip the literal prefix. |
| `transform:removePostfix:ARG`, `transform:removeSuffix:ARG` | Retain matching lookups and strip the literal suffix. |

Matching is case-sensitive. Filter alternatives split on `|`; a backslash escapes
only the delimiter. Other backslashes remain literal. An empty argument gives no
alternatives; positive filters reject all, negative contains retains all. Leading,
trailing and consecutive delimiters retain empty alternatives. Removal arguments
are single literals, not alternative lists. Removal also filters out nonmatching
lookups. Navigation/source metadata is retained while the visible lookup changes.
Forward transformations run in array order; navigation reverses them in reverse
order, prepending stripped prefixes or appending stripped suffixes. Filters have
identity inverse behavior. Unknown expressions are ignored; an empty or
unknown-only list is identity. The original factory recognizes a known name by
prefix before stripping name+colon, so a missing colon leaves the original whole
expression as the argument; malformed-prefix fixtures document this product quirk.
There is no additional transformer grammar validation in CompletionsModel.

Authored test example: inventory `app.name`, `app.env`, `other`; transforms
`filter:hasPrefix:app.` then `transform:removePrefix:app.` produce `name`, `env`.
Navigation of `name` resolves `app.name`. For suffix removal of `.php`, `foo.php`
becomes `foo`, while `bar.txt` is filtered out. Filter argument `a\|b|c` means
alternatives `a|b` and `c` after JSON string decoding.

Original member evidence: `extension/customConfig/models/completion/CompletionsModel`,
`common/completion/transformers/CompletionTransformerFactory`, its Prefix/Postfix/
Contains/DoesntContain filter and Prefix/Postfix/Multi transformer classes,
`common/StringHelpersKt.getDelimitedListWithEscaping`, and
`common/completion/CommonStringCompletion` navigation. Recovered files are under
`/tmp/laravel-idea-audit-20261110/java-recovery/com/laravel_idea/plugin/`.
The configKey `options.prefix` branch in the original registrar captures only prefix/recursive and uses ConfigRootBasedCompletion; unlike its no-prefix branch, it does not capture the custom transformer. Combining that built-in option with the new pipeline is therefore an optional native extension, not required original behavior.

Native generator preset mutators are **not an equivalent implementation**: they
render generated files, whereas this API transforms metadata completion and
reverse navigation inventories at editor call sites.

## Enum configuration keys: qualified valid array-key expressions

The [12.0 changelog](https://laravel-idea.com/docs/changelog) names enum config keys.
Original `config/ConfigKeyCollector.collectKeys` sends array-key expressions through
`configStringValueResolver` (`ConfigUtilKt` creates DeepStringValueResolver).
`common/stringValues/DeepStringValueResolver` explicitly resolves enum-case `name`
and backed `value` property references, retaining enum-source dependencies.
The concrete valid contract is therefore config array keys such as
`Key::Enabled->value` or `Key::Other->name`, not an invented raw Enum-object
argument to Laravel's Config Repository API. A fixture with string-backed case
Enabled='enabled' must make `foo.enabled` available from config/foo.php; `Other->name`
must yield `foo.Other`. Integer-backed values use PHP array key semantics. PHP>=8.1
and owned, unambiguous enum class/case source are prerequisites. Unknown dynamic
case/value expressions must not establish complete absence. Current static
config_arrays literal/integer parser does not resolve these enum key expressions,
so this is a narrowly qualified plugin-side implementation gap, not an API exclusion.

## Laravel Cloud: qualified read-only product contract

The [13.1 changelog](https://laravel-idea.com/docs/changelog) announces the Cloud
tab. The artifact defines its actual scope:

- Token setup, per-project selected application, change application/token,
  disconnect and manual refresh.
- Environment list and selected environment details: domain, environment status,
  latest deployment status/failure reason, branch, relative last deployment and
  PHP version; an external Manage link uses organization/application/environment slugs.
- Read-only environment variables with masked default, explicit reveal/hide and copy.
- Build/deploy log phases with step description/status/time/output.
- Refresh unfinished deployments approximately every5seconds; log fetching has
  in-flight de-duplication/30-second throttle and selected unfinished retry scheduling.
- Import available Cloud database/schema connections through the existing reviewed
  Add Database dialog. Credentials are handled as secrets, not ordinary UI logs.

No native deploy/create/delete operation is established by these product paths.
“Manage” opens the external Cloud environment page. Do not invent write workflows
from the broad integration heading.

Original evidence: `toolwindow/laravelCloud/api/LaravelCloudApiClient`,
`EnvironmentListParser`, `ui/EnvironmentDetailPanel`, `EnvironmentValueCellRenderer`,
`EnvironmentListPanel`, `ui/model/EnvironmentsViewModel`, `settings/LaravelCloudSettings`,
and `integrations/database/AddFoundDatabaseConnectionDialog`.

Callable official contracts: [application list](https://laravel.com/cloud/docs/api/applications/list-applications),
[environment list](https://laravel.com/cloud/docs/api/environments/list-environments),
[deployment list](https://laravel.com/cloud/docs/api/deployments/list-deployments),
[deployment logs](https://laravel.com/cloud/docs/api/deployments/get-deployment-logs),
and the [official API endpoint index](https://laravel.com/cloud/docs/llms.txt).
The original database path uses `/api/databases/clusters?include=databases,schemas`;
new implementation must bind only a currently verified official contract, including
its connection/schema response, rather than inventing private endpoints.
Our current application/environment lists are partial against these now-qualified
read-only details/logs/variables/import/refresh requirements. UI-kit native views
can implement the behavior; a generator preset cannot substitute for it. Real
account/keychain/network/native interaction remains a distinct acceptance gate.

## Git hook: qualified Cloud freshness, implementable

Original `integrations/git/LaravelCloudGitPushListener.onCompleted` ignores the
push result. It checks Laravel enabled, Laravel toolwindow visible and Cloud tab
present, then schedules `LaravelCloudPanel.updateEnvironments` after5seconds.
It is not a generic commit inspection or a deployment-triggering write.
A native implementation using `VcsChanged(root)` plus project-bound visible Cloud
scheduled refresh can preserve the observable read-only freshness. This is an
implementation adaptation, not a new accepted scope exception. Exact push-success
authority is unnecessary because the original does not inspect success either.
Actual visible refresh/network-lifecycle fixtures still need acceptance.

## LLM hook: exact agent-finished boundary is API-excluded

Original `integrations/llm/HelperCodeAgentFinishedListener` gathers Add/Remove/Edit
paths and both Move paths, resolves existing files to owning Laravel projects,
and invokes silent helper regeneration after an agent finishes. Its separate
application setting is user-toggleable and defaults **true** in
`LaravelIdeaLlmSettings`; do not confuse this with our on-save preference. Native
execution must still honor helper preferences, project ownership and capabilities.

Pinned API1.4 has no agent-finished event: [SDK EventKind/Event](https://github.com/l0ng-ai/openidea/blob/37c2a60/sdk/rust/openidea-plugin/src/types.rs),
[event ABI](https://github.com/l0ng-ai/openidea/blob/37c2a60/wit/events.wit),
and [manifest EVENT_KINDS validation](https://github.com/l0ng-ai/openidea/blob/37c2a60/crates/ide-extension/src/manifest.rs).
Unknown subscribed kinds fail; the generic Other payload is forward compatibility,
not permission to invent an agent-finished producer. `ActionPerformed` reports an
IDE action invocation, and `VcsChanged` reports repository changes; neither identifies
agent completion or its changed-file batch. The **precise agent-finished trigger**
is the exclusion. Reviewed manual/on-save/project helper workflows remain implemented
and required under their own leaves; this does not exclude helper generation.

## Generic Pest subject navigation: withdrawn invented requirement

The [5.1 changelog](https://laravel-idea.com/docs/changelog) establishes Pest
unit/feature test template choice;5.6 establishes Livewire Pest completions.
Original `packages/pest/PestPackage` likewise describes unit/feature Pest templates,
with `UsePestTemplateOption` and the two Pest file templates as concrete integration.
No reviewed primary source establishes generic subject-to-Pest-test navigation as a
Laravel Idea feature. That broad requirement is withdrawn from V09.pest; this is
neither acceptance, an API exclusion nor a user scope exception. Documented Pest
code-generation and Livewire contexts remain in G02/B03 leaves. This does not claim
that the surrounding IDE or a separate Pest plugin cannot offer navigation.
