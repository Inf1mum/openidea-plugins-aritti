# PHP/template parity delivery — current qualification, 2026-11-11

This report is authoritative for this worker and replaces earlier chronological claims. Scope: Symfony, Twig and Blade only. Laravel and shared files were not edited. Existing Blade injection/Cargo work was preserved. No package version bumps or publication.

## Frozen latest-original baseline

Official Marketplace update APIs queried2026-11-10:

| Package | Original | Frozen latest version | Update |
|---|---|---|---|
| Symfony |7219 Symfony Plugin|2026.2.314|1205017|
| Twig |7303 Twig|263.7307.101|1207069|
| Blade |7569 Blade|263.7307.112|1207324|

Sources: `https://plugins.jetbrains.com/api/plugins/<id>/updates?size=1`; official PhpStorm Twig template/Symfony navigation documentation; Twig3 macro/template documentation; official Symfony7.4 FileProfilerStorage source. Proprietary originals were compared through public behavior only.

## Critical API1.4 Rename boundary — verified, mitigated

Inspected released OpenIDEA v0.1.40 / commit37c2a6048eea89d4f6db645622612358b643862b:

- `src/editor/lsp/rename.ts:255` (`commitInplace`) checks only nonempty/different name; `finishRename` at266 likewise only nonempty/different.
- `crates/ide-host/src/ops/lsp.rs:1193` (`merge_indexed_rename`), particularly1239, creates `TextEdit {range, new_text:new_name.to_owned()}` directly for every indexed range.
- SDK `indexing.rs` Anchor has key/range only; no read-only flag, replacement encoding, collision/new-name validation or Rename callback.
- Released LanguageFeatures provides read-only highlights/lenses, but no Rename policy hook.

Therefore exact source ownership does NOT make public indexed replacements safe. Quoted block references could be corrupted by a quote/interpolation; even plain loop/macro identifiers can be invalid/colliding keywords. All public Twig DocumentIndexer output now returns empty, including older loop anchors. The registered empty export lets host reindex retire earlier rows. No read-only/native-writable graph is claimed.

The useful source models are retained privately. Native highlights/lenses, existing/extended Providers::definition and reviewed FileActions supply the supported workflow. Global native Find Usages without implicit unchecked Rename, or name-validated native Rename, is a concrete host/API gap excluded by the user.

## Implemented current behavior

### Symfony

- Route presentation regression assertions retain handler methods (`Controller@method`) and exact navigation targets.
- Seven-column legacy and eight-column current profiler CSV; request virtual_type retained, command profiles excluded. Source-owned alphanumeric/underscore/dash tokens, including short storage tokens; duplicate-case identities withheld conservatively.
- Selection-aware local raw/GZIP profile inspection with Back to Requests. Fresh root/source/token membership checks before/after adapter execution; inert rendering; details invalidation on profile filesystem changes, including already-stale snapshots.
- Optional approved PHP adapter: `php -n`,64MiB/two-second limits; no application bootstrap/Composer/autoload; unserialize allowed_classes=false/max_depth32. OneMiB compressed/decoded bounds,128collectors. Only collector types and allowlisted nonnegative numeric metrics/counts enter UI; payloads, SQL, messages, arbitrary strings withheld.
- Find Source Occurrences command: fresh bounded physical route/service/template candidate review and exact navigation. Source-pattern scope is explicitly not runtime usages or automatic Rename. Comments/raw/dynamic/escaped/foreign contexts excluded. Current editor and selected source revalidated.

### Twig

- Exact private loop models support key/value loops, nested conditional bodies and shadowing. Dynamic context, assignments/arrow scopes, duplicate/crossed/unclosed scopes stay quiet.
- Six configurable delimiters across actual-source Lexer, structure/folds, completion/snippets, imports/macros, hover, diagnostics/brackets, local models and injections. No text normalization/range drift;1–16byte valid unequal-width/Unicode tokens tested. Explicit invalid settings diagnostic; per-request settings restoration.
- Configured Toggle Comment command: versioned current-editor edits, one range/caret line, Unicode/CRLF preservation; incomplete/nested/raw contexts refuse.
- Physical JavaScript injections for documented complete javascript blocks/script tags, plus existing HTML script/style islands. Template holes, comments/raw/nested/unclosed bodies excluded.
- Static macro/block/import source models: unique macro declarations/named endmacro, `_self`/literal imports, separate local aliases vs exported names, literal extends override families, named endblock and block('name'). Duplicate/dynamic/shadowed/unsupported scopes withheld. Source cache16files/1MiB,128bindings,8parents; final imported source/root revalidation. Models are PRIVATE, never native writable anchors.
- Native document highlights and source-occurrence lenses use those private models. Opaque bounded snapshots recheck root/path/text/version/settings/key/range. Command works from a lens or directly at an admitted caret symbol. Go to Definition retains local-loop/macro/block functionality without public anchors.
- Preview Rename Static Twig Symbol FileActions: Unicode identifier grammar/reserved-word validation (64UTF8bytes max), no quote/whitespace/interpolation replacements, bounded fresh inventory, explicit exact-span preview and scope exclusions, cancellation, fresh-source/membership/ownership/settings checks, and atomic host workspace edits. A virtual rewritten source graph must preserve every expected symbol occurrence with no added capture/collision/disconnection. Local loops/aliases and admitted cross-file macro/block families are covered. No silent partial edits on inventory/review bounds. Exact previews are capped15KiB because released wit_host.rs526 clips native confirmation bodies at16KiB; an oversized-review regression proves refusal before confirmation.
- Macro named-argument completion (root contribution) integrated with delimiter settings and known-installed pre3.15 gate. Signature help reuses physical owners, tracks named/positional/partial strings, and respects old-version evidence. Exact parameter definition navigation; no parameter Rename fiction.
- Opt-in pinned official Twig-CS-Fixer4.1.1 spacing formatter via LanguageFeatures::format. Official CLI fix writes files, so adapter calls official in-memory Fixer::fixFile only, without config resolver/linter/cache/application configuration. Active current source and literal HTML/string/Twig token identity checked before returning one full-document host edit. No external writes.

### Blade

- Matching-directive navigation is bidirectional in current unsaved text with exact UTF16 ranges. No named Laravel symbols or Rename anchors published. Existing injection implementation/Cargo changes left intact.

## Actual formatter qualification

Official GitHub release: Twig-CS-Fixer4.1.1, commit label53c1234. MIT LICENCE read (Vincent Langlet2021+). Artifact acquired only to `/tmp/twig-cs-fixer-4.1.1`; not vendored.

SHA256: `4d099a3be74ea41b0c8f1b2a7033c53a77e7d0dfba36725c2bbaaa19453ed2e5`.

Formatting is disabled until explicitly enabled/configured and PHP approved. Only standard Twig spacing rules are invoked; identifier renaming, template conversion and literal HTML indentation changes are excluded. Custom delimiter/range formatting is unavailable in this reviewed tokenizer and is an adapter/formatter residual, not a host blocker.

## Verification

- Twig88 Rust tests passing, fresh release Wasm build passes after mitigation.
- Symfony557 and Blade63 tests previously passed with fresh release Wasm; no subsequent changes in those source families by this worker.
- PHP profiler adapter:3 actual PHP8.4 integration tests pass (raw/GZIP, sanitized content, malformed/oversized/symlink escape).
- Official Twig formatter adapter:2 actual-tool tests pass (multiple goldens/idempotence, no file writes/cache/config execution, wrong hash/malformed source refusal).
- New mitigation tests prove public index empty, preserve loop GoToDefinition/highlights/lenses, reject quote/interpolation/keywords/capture/collisions, return atomic imported-macro/block edits, and refuse cancellation/stale snapshots/replay.
- Logs: `/tmp/parity-twig-{test,build}.log`, `/tmp/parity-symfony-{test,build}.log`, `/tmp/parity-blade-{test,build}.log`, `/tmp/parity-symfony-adapter-test.log`, `/tmp/parity-twig-formatter-adapter-test.log`.
- `/tmp/parity-php-template-artifacts.json` records hashes. Parent final build/runtime/package/native acceptance supersedes worker artifacts if shared sources change.
- No new desktop acceptance, publication or API1.4 installer qualification claimed by this worker. Parent owns final integration validation.

## Current residual ledger

| Feature family | Remaining scope | Classification |
|---|---|---|
| Native global usages/Rename | Read-only index or validated replacement hook; unchecked host indexed Rename deliberately not exposed | Concrete released host/API gap |
| Twig static source refactoring | Trait/embed/namespaced loader models, complete macro-parameter Rename, further admitted source dialects | Plugin/model work; NOT host-blocked |
| Twig formatter | HTML indentation/ranges/custom delimiters and original-specific style equivalence beyond qualified spacing | Qualified adapter/formatter work; NOT host-blocked |
| Twig language/runtime | Effective dynamic context, extension types, debugger template mapping/custom typing hooks | Split semantic adapter work vs specific missing native hooks; no blanket closure |
| Symfony references | Complete effective service/route/template/domain symbol models and reviewed Rename beyond read-only source candidates | Plugin/model implementation remains |
| Symfony profiler | Full sanitized collector detail renderer/pagination/remote modes beyond local overview | Plugin/adapter integration remains |
| Symfony framework semantics | Complete Doctrine/forms/PHP/container type information | Trustworthy semantic adapter and/or specific host query hooks needed |
| Blade named references/completion | Existing documented Laravel ownership; excluded owner cannot be changed in this task | Explicit scope/ownership constraint, not an API excuse |
| Desktop acceptance | Exact-package light/dark/narrow/native workflow validation | Parent validation still required |

No full feasible/original parity claim. Passing finite tests does not close the residual ledger. In particular, do not promote P01.2/P27.2/P27.4/P29.2 wholesale from this report.
