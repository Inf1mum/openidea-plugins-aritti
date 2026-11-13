# Scenario runner (L1, released OpenIDEA host, headless)

`scripts/runtime-smoke/src/bin/jpa_assist.rs` drives the freshly built `plugin.wasm` inside the released v0.1.42 host
(pin `e702e4da7b0333e81bc818e26be72db1d157ec5d`, plugin API 1.5): scripted prompt answers, native workspace-edit apply and
grouped Undo, provider requests (diagnostics, completion, hover, markers, code actions), commands with clipboard capture and
native view render models. Each scenario works on its own disposable copy of a parity fixture. No GUI, no network.

This is L1 evidence only (see `../CLOSURE_PLAN.md`). A passing scenario shows that OUR output has the structural property a
documented claim states. It is documentation-derived evidence at most (`referenceType: documentation`), never observed
reference behaviour of the original plugin, and it is not a parity claim. No ledger row is changed by running it.

## Build and run

```sh
export PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=/tmp/jpa-runner-target
(cd plugins/jpa-assist && openidea-plugin build)          # v0.1.42 CLI -> dist/plugin.wasm
cargo build --manifest-path scripts/runtime-smoke/Cargo.toml --bin jpa_assist
BIN=$CARGO_TARGET_DIR/debug/jpa_assist

$BIN REPO --validate [--scenario FILE|DIR ...] [--only RS-001,...]   # check scenario JSON against claims/*.json, runs nothing
$BIN REPO --list     [--scenario FILE|DIR ...] [--only ...]          # scenarios and the claim ids each covers
$BIN REPO OUT [FIXTURE_DIR] [WASM] [--smoke] [--scenario FILE|DIR ...] [--only ...] [--json-receipt-dir DIR]
```

* `OUT` must be outside the repository. Receipts go to `OUT/receipts/<id>.json` plus `OUT/receipts/summary.json`
  (or `--json-receipt-dir`); artifacts to `OUT/artifacts/<id>/`.
* No `--scenario` (and no `--list`/`--validate`): runs only the built-in 31-check smoke set, as before
  (`OUT/jpa-assist-results.json`, `FIXTURE_DIR` defaults to `boot-jpa-java-jakarta`).
* `--scenario` runs the scenarios (default directory `parity/runner/scenarios` when only `--only` is given). `--smoke` adds
  the smoke set (its output goes to `OUT/smoke/`).
* Scenarios are validated before any run; an invalid scenario aborts the whole run. Exit code 1 on any failure.
* Receipts are deterministic: no timestamps, the disposable-copy path is written as `$PROJECT`, and the plugin's clock-based
  date proposal is replaced by `<clock-proposal>`. Two runs of the same wasm give identical receipt bytes.

## Scenario file (`scenarios/RS-NNN-<slug>.json`)

```jsonc
{
  "id": "RS-001",                       // RS-NNN, file name starts with it
  "rows": ["G040", "G042"],             // ledger rows whose claims this covers
  "claimIds": ["G040-C1"],              // each must exist in claims/<row>.json, be explicit and verifiableHeadless
  "verifies": {"G040-C1": "which expectation checks it, and what part of the sentence is NOT checked"},
  "fixture": "boot-jpa-java-jakarta",   // directory under parity/fixtures
  "setup": [ ... ],                     // applied to the disposable copy before the host starts
  "preActions": [ ... ],                // code actions applied (natively) after the host starts, before the main action
  "postSetup": [ ... ],                 // setup steps applied after preActions (open buffers are re-synced)
  "kind": "fileaction|diagnostics|completion|hover|command|tree|lineMarkers|designer|manifest",
  "target": { "file": "...", ... },     // see below
  "action": { "title": "exact code-action title" } | { "command": "id", "arg": "..." } | { "view": "structure" },
  "prompts": [ answers in the order the plugin asks ],
  "expect": { ... },
  "checks": { "preview": true, "undo": true, "cancelEach": true, "stale": true },
  "warm": false,                        // optional: skip the cache priming (default: prime like the IDE does)
  "kindExerciser": true,                // only for scenarios that cover no documented claim (rows and claimIds empty)
  "note": "what the scenario does"
}
```

`claimIds` lists only claims the scenario genuinely verifies. If a claim needs a surface the headless host cannot show
(tool-window layout, gutter drawing, colours, settings pages, drag and drop, the original plugin) it is not covered; if the
plugin offers only part of a claim's sentence, the scenario says so in `verifies` and the claim is either left out or
documented as option-level only. `--validate` rejects unknown claim ids, claims whose row is not in `rows`, claims that are not
explicit or not `verifiableHeadless`, rows without a covered claim, and claim ids without a `verifies` sentence.

### target

`file` (relative to the copy) plus one locator: `position {line, col}` (1-based), `selection {startLine,startCol,endLine,endCol}`
or `selection {fromAnchor, toAnchor}`, or `anchor` text (first line containing it; cursor at its start, or its end with
`"at": "end"`). `language` overrides the language derived from the extension (java, kotlin, xml, sql, properties).

### setup / postSetup steps (one key each)

`{"copy": {"from", "to"}}` (file or directory, both relative to the copy), `{"swap": {"file", "with"}}` (overwrite `file` with
`with`), `{"write": {"path", "content"}}`, `{"replace": {"file", "from", "to"}}` (first occurrence, must exist),
`{"delete": "path"}`, `{"settings": {"key": value, ...}}` (plugin settings, written to the private global `settings.toml`; an
`object` item such as a `liquibaseChangeSetDefaults` row must carry every property of its schema, exactly as the settings page
stores it). `settings` is allowed in `setup` only.

`preActions` entries: `{title, target:{file, anchor...}, prompts:[...]}`; the action is resolved and applied through the native
edit engine (used for "Create schema baseline…" before a migration diff). The state after `preActions` and `postSetup` is the
baseline for `noFilesChanged`, `undo` and the receipts' `setupTreeHash`.

### prompts (answers)

`{"pick": key}`, `{"pickLabel": "label substring"}`, `{"pickMany": [key-or-label, ...]}`, `{"input": "text"}`,
`"default"` (accept the prompt's proposed input value), `{"confirm": "yes"|"no"}`, `"cancel"` (also `{"cancel": true}`). A
missing or mismatching answer cancels the prompt and fails the scenario (`scripted answer accepted`).

### expect (structural, from the claim; every key is optional but at least one is required)

| key | meaning |
|---|---|
| `promptSequence` | list in order of `{kind: pick\|pickMany\|input\|confirm, titleContains, messageContains, optionsInclude, optionsExclude, itemDescriptions:{key: text}, defaultIs, name}`; options match a key exactly or a label by substring |
| `promptCount` | exact number of prompts shown |
| `actionOffered` | `false` asserts the title is not among the offered code actions |
| `resolveErrorContains` | the action refuses with a message containing this text (and nothing was applied) |
| `editsContain` | regexes over the resolved edit and file-creation texts |
| `resultFile` | object or list `{path, contains, notContains, matches}` read from the disposable copy after the apply |
| `createdFiles` | `{path, contains}`: absent before, present after |
| `noFilesChanged` | tree and applied-edit count unchanged |
| `diagnostics` / `diagnosticsAbsent` | `{rule, file (suffix), line?, severity?}` (line is 1-based; severity is Error, Warning, Info or Hint, case-insensitive) |
| `pluginPackage` | `manifest` kind (no host action; reads `plugin.toml` and `settings/` of the plugin source): `{settingsPage, includesSchemaKeysWithPrefix?, includes?}` the page is declared in plugin.toml and lists those keys (every `settings.schema.json` key with the prefix, and the named keys); `{manifestView: {id, location?}}` a `[[contributes.views]]` block with that id and location |
| `completionsInclude` / `completionsExclude` | labels (substring) |
| `hoverContains`, `clipboardContains`, `treeNodes`, `markersInclude` | substrings of hover markdown, clipboard text, tree label+detail, marker tooltip |
| `treeChildren` | list of label chains `["Customer", "Repositories", "CustomerRepository"]`: each label is a child of the previous (the first is a root item) |
| `treeAbsent` | substrings that must not occur in any tree label+detail |
| `designerReplies` | `designer` kind: checks on the plugin's reply to one message, see "designer scenarios" |
| `goldenFile` | `{file, actual, ownOutputSnapshot: true}` byte comparison with a stored copy of our own output. It must say `ownOutputSnapshot: true` and is never documentation evidence. No shipped scenario uses it. |

### tree scenarios with view events

`action.viewEvents` (tree kind) sends `{"button": id}`, `{"select": key}` or `{"activate": key}` to the view after one priming render
and before the checked render; `$PROJECT` in keys and in a command's `action.arg` is the disposable copy. `target.file` gives the view
an active editor (Locate Open File). `prompts` answer what the events open. `"grants": ["project.write"]` consents to the optional
`project.write` capability for the scenario (the project-creating commands need it; no other scenario is granted it). The headless host
has no file watcher, so the tree rendered after a created file does not show it: check the created file, not the new node.

### designer scenarios (web-view messages)

`kind: designer` drives the plugin's packaged web view through the released host's real message path: each message is posted
with `PluginHost::view_event(plugin, view, ViewEvent::Message{text})`, the host entry the web view's `openidea.postMessage`
reaches (it is size-capped and dispatched to the plugin's `handle`, exactly as in the IDE). The plugin answers with
`host::post_view_message`; the runner's event sink collects those `PluginEvent::ViewMessage` events and parses each as JSON. The
view is rendered once first (as mounting it would). `target` gives the view an active editor (the class `inspect` reads).

```jsonc
"kind": "designer",
"action": {"view": "designer", "messages": [
  {"operation": {"kind": "snapshot"}},                                   // channel, protocol 1, frame, sequence 1..n, root are filled in
  {"operation": {"kind": "reverse-inventory", "path": "$PROJECT/db/x.sql", "dialect": null, "schema": "shop", "table": null}},
  {"operation": {"kind": "select", "attribute": "label"}, "context": "auto"},          // "auto": path/version/attribute of the previous reply's model
  {"operation": {"kind": "apply", "token": "$last:/model/pending/token"}, "context": "auto"},   // $last:/json/pointer = a value of the previous reply
  {"setup": [{"replace": {"file": "db/x.sql", "from": "a", "to": "b"}}]}               // harness step between messages (not sent, not counted); same steps as setup
]},
"expect": {"designerReplies": [
  {"message": 2, "path": "reverse.relations", "anyItem": {"name": "shop.orders", "group": "table"}},
  {"message": 3, "path": "model.selected", "equals": "label"},
  {"message": 1, "handlerError": "Stale"}
]}
```

Messages are the JSON objects the web view posts (`web/*.js`); any field given explicitly (`frame`, `sequence`, `root`, `protocol`) wins
over the default, which is how stale or malformed frames are sent. `$PROJECT` is substituted in every string. Messages are numbered
from 1 in the order sent (`setup` entries do not count). A `designerReplies` entry names a `message` (default: the last) and a dotted `path`
into the reply (`reverse.relations`, `migration.changes.0.location`, numbers index arrays), plus exactly one operator: `equals`,
`exists` (a non-null value), `contains` (substring of a string, or an equal element of an array), `matches` (regex over a string),
`length` (array, string or object), `anyItem` / `noItem` (an array item whose listed fields all equal the given ones), `countWhere`
`{where, count}`, or `handlerError` (the handler refused the frame, e.g. a stale sequence, and the text it returned contains the string; no `path`).
The reply is the last message of the frame the plugin posted for that request. The settings the plugin reads on each reply
(`jpaAssist.designer.*`, `schemaChangeLocations`, ...) are set with `setup` `settings`. Receipt artifact: `designer-exchange.json` (each
request as sent, the handler result and the parsed reply).

Exercised: the plugin's request validation, frame registry, inspector model, prepare/apply (an edit of the open buffer), SQL inventory,
migration plan and layout reply, all in the real wasm. NOT exercised: the web view's HTML, JavaScript, CSS, controls, colours and
layout (no browser runs; that is `scripts/check-jpa-designer.mjs`), the host's size cap on messages, and anything that needs a live database.
Limits of the headless host: it has no file watcher, so a file ADDED after start-up is not listed by `reverse-sources`; edits of existing
files made by a `setup` step are re-synced into open buffers.

### checks (fileaction only unless noted)

* `preview`: every inserted/created text appears in the confirm preview (`{"preview": {"paths": ["V3__x.sql"]}}` limits it to
  those targets when the preview shows a secondary file only by name).
* `undo`: native grouped Undo returns the copy to the exact setup tree hash.
* `cancelEach`: cancelling at every prompt resolves with an error, applies nothing and leaves the tree unchanged.
* `stale`: a buffer edit after the first prompt and at the last prompt makes the action refuse with an out-of-sync error.

## Receipt (`receipts/<id>.json`)

`scenario, kind, rows, claimIds, kindExerciser, scenarioSha256, wasmSha256, hostRev, fixture, fixtureTreeHashBefore` (pristine
copy), `setupTreeHash, fixtureTreeHashAfter` (state the scenario left), `treeHashAfterRestore` (after Undo, when requested),
`changedFiles, results:[{check, pass, detail}], status: pass|fail, artifacts:[{path, sha256}], scope`. The artifacts are the
prompts as shown, the preview text, the resolved edits, diagnostics, completion labels, hover, markers, clipboard or tree
labels, and the result files that were read.

## Scenarios

`--list` prints the current table. RS-001..RS-010 cover documented claims; RS-011..RS-015 are kind exercisers
(`kindExerciser: true`): they prove the runner can drive completion, hover, command, tree and line-marker surfaces but cover no
claim, because no explicit claim states those surfaces' behaviour in a headless-verifiable way.

## Limits of this runner

* Only what the plugin exposes through the host is observable: prompts are scripted (no widget layout, order of controls or
  keyboard flow), views are render models (packaged web views render a fallback), no process or database grants.
* Expectations check properties, not reference bytes; documented sentences that describe options the plugin does not offer are
  not covered rather than approximated.
* The headless host has no file watcher: after `postSetup` the runner re-syncs buffers it had opened.
* One main action per scenario (plus `preActions`); the plugin's cross-file cache is primed by opening every source file,
  running diagnostics once and sending `FileOpened` events, as the IDE does.

## Reliability: run on a quiet machine, at least three times

The released host drops editor-provider results that exceed its time budget (50 ms). On a loaded machine (a load average
of about 80 was observed while several builds ran) scenarios that need code actions fail intermittently: the same scenario
failed once and passed once with no code change. A single passing run therefore proves nothing, and a failing run under load
is not a product failure.

Receipts used as ledger evidence must come from at least three separate runs on an otherwise idle machine
(`scripts/update-jpa-ledger.py --runner-out RUN1 --runner-out RUN2 --runner-out RUN3`). A scenario counts as passing only if
it passed in every run and its receipts are byte-identical; anything else is recorded as `flaky` and keeps the row open.
