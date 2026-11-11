# Properties / Ideolog delivery — 2026-11-10

## Refreshed original baseline

Fresh official JetBrains Marketplace API `updates?size=1` responses:
- Properties ID11594: **263.7307.108**, `11594/1207313/properties-263.7307.108.zip`.
- Ideolog ID9746: **263.7307.101**, `9746/1207116/ideolog-263.7307.101.zip`.
- Raw-response hashes and exact URLs: `/tmp/parity-properties-ideolog-baseline.json`.
- Public behavior references: https://www.jetbrains.com/help/idea/properties-files.html ; https://plugins.jetbrains.com/plugin/9746-ideolog ; https://github.com/JetBrains/ideolog/wiki/Custom-Log-Formats . No proprietary artifacts were downloaded.

## Implemented

Properties now creates **multiple locales in one reviewed atomic FileAction**, closing its prior single-locale-at-a-time restriction. Existing action accepts up to32 comma-separated locale names (`de, fr_CA, es_419`); validates every name, rejects duplicates, existing targets and unsaved target buffers, previews all exact paths/content, and rechecks source snapshot and all vacancies after confirmation. Each Create operation has `overwrite:false`; no direct file writes or process executions occur. First created file opens. Locale creation remains empty/ASCII and uses host new-file encoding defaults.

Ideolog custom timestamp formats now support **hh with a**, with case-insensitive ASCII AM/PM markers and correct noon/midnight normalization. Dates, milliseconds and numeric offsets compose with this format. Existing imported-pattern processing, native pattern preview and actual event timestamp/time-gap computation use this same parser. Ambiguous hour/day-period combinations, invalid 00/13 hours, repeated fields and trailing text refuse parsing.

Touched files only within plugins/properties and plugins/ideolog: implementation, README and manifest change-notes. Release-managed versions unchanged. No shared files, scripts, Laravel, host or API changes.

## Verification

- `cargo test --locked --manifest-path plugins/properties/Cargo.toml`: **122 passed**, zero failures (including four multi-locale transaction regression tests).
- `cargo test --locked --manifest-path plugins/ideolog/Cargo.toml`: **99 passed**, zero failures (unit + real imported-pattern timestamp tests).
- Fresh `cargo build --locked --release --target wasm32-wasip2` passes for both.
- Logs `/tmp/properties-parity-{tests,build}.log`, `/tmp/ideolog-parity-{tests,build}.log`.
- `git diff --check` passes for both plugin directories.
- No new native desktop, package installation or real service acceptance.

## Residuals: full feasible parity is NOT established

Properties: automatic host-graph key usages/Rename still withheld because complete bundle ownership, escaped-key edits and source closure remain unproved; current reviewed scoped rename is retained. Arbitrary @PropertyKey contracts and framework/classpath resource ownership remain absent. Full native bundle grid, locale management, encoding picker and virtual bundle grouping remain absent; multi-locale FileAction adapts only creation behavior. Some of these are plugin source-model work, not evidence of API blockers.

Ideolog: locale month/day names, named zones, variable-width date fields and general Java SimpleDateFormat semantics remain plugin-side parser gaps; twelve-hour addition does not close them. Arbitrary color/font/background/stripe widgets and exact original style rendering remain absent. Native line filtering/hiding, extended selection and heatmap remain host-surface limitations. This batch does not close the semantic renderer acceptance task or establish visual parity.

## Continued delivery after first checkpoint

Properties adds current-localized-file **Rename this locale file** and **Remove this locale file** FileActions. They protect the default bundle, review source path + full current content, validate new locale destination (nonoverwriting and no open buffer), and recheck full bundle/source snapshot after confirmation. No key/reference refactoring is implied by changing a locale suffix.

Properties also adds reviewed **Unicode escapes/native source conversion** for property keys and values. Encoding emits UTF16 surrogate escapes for non-BMP characters. Native conversion re-escapes structural key delimiters, leading whitespace and control characters; decoded property identities/values are verified equal before edits are offered. Comments remain verbatim and file encoding is unchanged. Invalid/lossy Unicode refuses edits.

Ideolog now supports variable-width M/d/H/h/m/s, S (up to3 milliseconds digits), fixed SS, k/kk (1–24 hours), K/KK (0–11 with AM/PM), RFC822 Z/ZZ/ZZZ, z/zz/zzz/zzzz for UTC/GMT and explicit GMT±HH:mm. Variable numeric fields must have separators to avoid guessing adjacent field allocation. Values and offsets are validated. Full SimpleDateFormat is **not** implemented: original current public source calls Java `SimpleDateFormat(pattern)` in `util/IdeologDocumentContext.kt`, inheriting default JVM locale/timezone and Java parsing semantics. No locale context exists in this plugin parser, so hardcoded English or guessed abbreviations would not faithfully close that contract.

Final verification supersedes earlier counts: **Properties125, Ideolog100** unit tests pass; fresh release wasm builds pass for both. Tests cover native/escaped Unicode identity (including emojis and escaped delimiters), transaction cancellation, protected default bundle, rename/delete operation shape, exact offset normalization, midnight/noon, alternate hour conventions and malformed/ambiguous format refusal.

### Concrete remaining contracts

| Contract | Current implementation | Remaining class |
|---|---|---|
| Properties three-letter ISO languages, script/variant locale creation | Existing bundle discovery accepts known two-letter-language suffixes; creation accepts language + region | Plugin locale-model work, not host-blocked |
| Properties type/classpath-aware arbitrary ResourceBundle/@PropertyKey/framework ownership | Exact direct JDK getBundle calls and reviewed current-module local rename only | Plugin semantic resolver/optional semantic adapter; no generic proof from literal equality |
| Properties automatic graph usages/rename across all runtime bundles | Explicit reviewed bounded operations; no globally enabled graph | Complete source/model ownership + escape-aware graph work |
| Properties virtual bundle combine/dissociate / native file-tree grouping | No host contribution for this synthetic native tree | Host integration required |
| Properties all-locale inline native grid edits | Explicit atomic selected-locale edits exist | Native table lacks per-cell editable policy; alternate custom grid feasible but absent |
| Properties resource-bundle encoding management | Host encoding retained; no plugin encoding selection | Host encoding API/integration absent |
| Ideolog full Java SimpleDateFormat fields/default locale/timezone/leniency | Numeric dates/clocks + fixed UTC/GMT/offsets only | Plugin timestamp engine/explicit locale/timezone model; not a generic host API blocker |
| Ideolog Java regex fidelity | fancy-regex subset, validation and named formats | Optional regex-engine adapter/model work; full Java semantics absent |
| Ideolog arbitrary background, bold/italic, stripe settings | Native symbolic foreground/action choices | Shared/native style surface lacks these exact render contracts; settings-only cosmetics would not implement rendering |
| Ideolog filtered live file tail / hide lines | Owned read-only bounded log snapshots exist | Native file editor line hiding/tail hooks absent; owned custom editor alternative feasible but absent |
| Ideolog extended selection, heatmap, arbitrary terminal filtering | No contributions available | Host integration required |

Generated `plugins/ideolog/web/resources/*` changed concurrently via root's shared sync; this worker did not edit those generated files directly.

## Final locale-model closure

Shared `props::valid_locale` now admits known ISO639-1 and ISO639-2 languages, scripts, regions and Java legacy variants; it drives bundle suffix discovery, creation, locale-file rename and existing reference membership through `split_locale`. Examples: `eng`, `zh_Hans_CN`, `eng_Latn_US_POSIX`, `en__POSIX`, `sr_Cyrl`. Unknown languages, invalid case, dangling separators, path syntax and invalid segments refuse. The supported suffix grammar is bounded (96bytes total,32bytes per variant), preserves code identity rather than silently aliasing `eng` to `en`, and excludes unassigned/private-use qaa-qtz range.

The506 ISO639-2 identifier facts (bibliographic and terminological codes) are pinned to MIT wooorm/iso-639-2 revision1bd9b9e1b29f3bda2c8e72a16ff09d284f07caa6, source2.js; license retained in existing plugin NOTICE. Official LOC text endpoint returned403, so no claim it was fetched. Fresh tests verify the same admitted names participate in atomic bundle inventory; a similarly named unknown-language file is not falsely grouped.

**Final results: Properties128 / Ideolog100 unit tests pass; fresh release Wasm builds pass; scoped diff check passes.** Full Java SimpleDateFormat/regex semantics, semantic resource ownership and exact inline bundle-grid editing are still not implemented; none is automatically relabeled API-blocked. Existing explicit cross-locale native editing remains functional, while exact inline grid interactions would need a custom grid implementation. Root's shared sync owns concurrent Ideolog generated web assets.

## JVM adapter superseding date/regex engine absence

See `/tmp/parity-ideolog-jvm.md`: optional reviewed Java17+ custom-format observation capture is now implemented, backed by actual JVM Pattern and SimpleDateFormat engines, explicit/default locale/timezone provenance and source-guarded native observation navigation. Final Ideolog104 tests and release Wasm build pass, including actual Java26 localized-date/regex integration. This closes the **explicit bounded faithful engine** gap, while automatic editor syntax remains a distinct local-parser surface. Parent now owns further Properties grid changes and its newer counts.

Final Ideolog JVM follow-up: actual Java observations now supply exact-source semantic timestamp/severity/category fields and reviewed source marks in the main built-in Log editor, under strict source/version/config/owner/unique-range admission.107 Ideolog tests and fresh Wasm build pass. See final section of `/tmp/parity-ideolog-jvm.md`; parent owns current Properties grid tests/counts.
