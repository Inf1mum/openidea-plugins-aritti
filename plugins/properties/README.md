# Properties

Inspections and editing tools for `.properties` files in OpenIDEA.
MIT; see `LICENSE`.

## Checks

Live diagnostics from the current unsaved buffer (API 1.3): duplicate decoded keys, malformed `\u` escapes, trailing spaces, key/value delimiters that don't match the code style, and opt-in three-dots-instead-of-`…` and alphabetic-order checks. Documents over 128 KiB or 4096 properties are skipped; results are capped at 512. Key equality keeps Java UTF-16 units: distinct lone-surrogate escapes stay distinct, and valid escaped surrogate pairs match their literal spelling.

Missing translations: after a bounded warm read of the same file's sibling locales, keys that some known locale lacks are reported, matched to current keys. Warm reads are limited to 128 sibling files, 128 KiB / 4096 keys each and a 250 ms deadline; discovery uses a raw 201st-file sentinel before filtering similarly named bundles. Cold, foreign, erroring, truncated, oversized or otherwise uninterpretable discovery is quiet. Newly typed keys without a warmed fact stay quiet, another locale's unsaved edits may be stale until its next warm event, and ignored locale files are not promised to be discovered.

Missing-key warnings in Java/Kotlin/templates are withheld: text matches and a conventional bundle name do not establish a resolved owner. `reportMissingKeysInCode` is a retained legacy preference.

## Quick fixes

Remove property, unescape, remove trailing spaces, replace delimiter, add the missing translation to another locale, remove a property from every locale, sort the file or the whole bundle.

## Navigation, completion and hover

- Go to Declaration on a key: the same key in the parent locale (`messages_fr_CA` → `messages_fr` → `messages`) and in every other locale of the bundle that declares it.
- Java/Kotlin navigation needs an explicit JDK bundle call: `java.util.ResourceBundle.getBundle("com.acme.messages").getString("app.title")`. The exact `java.util.ResourceBundle` import also permits the short name; `getObject` and `getStringArray` use the same contract; both calls must have exactly one plain string argument. Foreign or wildcard imports, aliases, known shadows, dynamic expressions, escapes/text blocks and Kotlin interpolation stay quiet. The caller must lie under its module's conventional `src/main/java`, `src/main/kotlin`, `src/test/java` or `src/test/kotlin` tree; the bundle base maps only to the corresponding module and main/test `resources/` tree, its named package path and locale variants. This is positive conventional source-candidate navigation, not a verified classpath result; absent keys are never reported.
- Immutable local contexts: `final ResourceBundle bundle = …getBundle("pkg.messages")` (also `final var`, and Kotlin `val` with the same guards) establishes a bounded context for direct literal `bundle.getString/getObject/getStringArray` calls. Constructors, fields, parameters, mutable declarations, unknown type aliases, custom loaders/Control/locale arguments, wrapped initializers, escapes, assignment, aliasing and matching field/parameter names suppress the context; declaration and use must share one unambiguous enclosing function and lexical block ancestry.
- Legacy template candidate navigation on `#{a.b}` in `.html` (Thymeleaf), `.xhtml` and JSP, and on `<spring:message code>` / `<fmt:message key>`.
- Key completion in `simplelogger.properties` (slf4j) and in the explicit JDK bundle calls above. Getter spelling alone, `MessageSource`, custom helpers and unknown `@PropertyKey` contracts do not establish an owner.
- Hover on a key shows the current buffer's decoded key/value, source file and locale (up to 8 declarations with paths, locales and values); Java/Kotlin hovers require the explicit JDK context and filter candidates to that bundle's conventional module resources. Caret line at most 4096 bytes, documents over 256 KiB skipped, output limited to 8 KiB. No project index is built and no file reads happen in hover requests.
- Facts are scoped to the current innermost open project. Cache construction reads at most 5000 discovered files, 128 KiB / 4096 properties per file, 8 MiB total, with a 250 ms deadline; skipped or capped files do not become absence facts. Completion is bounded to 128 files, 4096 distinct keys, 512 results and 64 KiB of labels; value hints are 512 characters, keys needing escaping or longer than 4096 bytes are omitted. Source parsing is limited to 128 KiB / 16384 tokens / 256 imports. Cold or foreign caches return quietly; another file's unsaved values can be stale until its next warm event.
- Automatic key usages and Rename are deliberately withheld: global decoded-key grouping and text heuristics do not prove bundle ownership, and no synthetic, decoded, escaped, Boot relaxed-binding or global key-name anchors are published.

## Resource Bundle editor

Open With › Resource Bundle shows a read-only overview of keys × locales with incomplete-key filtering. Select a property, then use Edit Value or Rename Key in This File; Add Property and Remove from This File provide reviewed changes with a revert control. Other-locale cells open the file (Open Translation) rather than mutating; foreign typed cell submissions are refused, not discarded. Show Incomplete Only keeps the keys some locale lacks.

Native table mutations require a rendered source/layout binding and an exact confirmation naming the current key(s), file, version, UTF-16 ranges and complete before/after text (review over 16 KiB refuses); cancel changes nothing. Callbacks without a matching rendered source/version, callbacks after invalidation, unknown rows, duplicate targets and destination-key collisions refuse. Bundle preparation requires a complete visible inventory of at most 199 files, 256 KiB per source, 1 MiB source text, 4096 distinct keys and 32768 cells; foreign, unreadable or oversized sources are refused. **Revert last reviewed bundle edit…** restores the single last reviewed edit through a separate exact inverse review (memory-only, not native Undo): it requires the identical project, file, full text and version, retains one record of at most 256 KiB + 256 KiB, and disappears after a successful revert or plugin restart.

**Edit All Locales…** switches the native grid into a staged mode: edit any locale value, rename a key across visible bundle locales, or insert/remove property rows while source files stay unchanged. Duplicate keys, incomplete inventories, foreign owners and bounded overflows refuse entry or mutation. Open Context Actions (Alt+Enter) and select **Apply staged bundle changes…** to review the exact replacements and apply one atomic workspace edit (one native Undo reverses it; the extra intention step is required by the API). Key changes update this bundle only; code references are unchanged. Cancel preserves the draft; if any source changes, the grid becomes read-only until **Discard Staged Changes…**. Drafts are memory-only and lost on plugin reload.

## Structure tool window

Properties Structure uses the native tree and toolbar: dotted-prefix or flat grouping, alphabetical/file order, Expand All/Collapse All, Refresh and double-click navigation. Row identities bind the current project, editor path and source version, so delayed actions from another file or an edited source refuse. Rendering requires a concrete project matching the editor's owner and refreshes after observed open/save events and reviewed mutations.

## Bundle and locale actions

All cross-locale intentions are confirmed atomic FileActions (no processes run, existing unsaved text participates, one workspace edit):

- **Add translation**, **Remove from all locales**, **Sort resource bundle files** and **Set property value across selected locales…**: the latter confirms exact paths, UTF-16 ranges and replacement text; duplicate keys must be resolved first; limits are 199 files, 256 KiB each, 1 MiB total and a 16 KiB preview; the 201st-file discovery sentinel makes incomplete discovery refuse the action.
- **Create locale files…**: up to 32 comma-separated locales, each a known ISO 639-1 or ISO 639-2 language, optionally a title-case script, uppercase two-letter country or three-digit region, and Java variant segments (`eng`, `zh_Hans_CN`, `en_US_POSIX`, `en__POSIX`). All paths and ASCII contents are reviewed together; duplicates, invalid names, existing files and open destination buffers refuse; `overwrite: false`; the first locale opens after application. No translation is copied and no encoding selector is supplied; unrepresentable characters fail the edit rather than changing encoding.
- **Rename this locale file…** / **Remove this locale file…**: reviewed, undoable file-level operations on localized files (the default bundle is excluded); source snapshots and destination vacancy are rechecked after confirmation.
- **Change key in this bundle only…**: changes the exact key spelling across the complete visible locale inventory, preserving values and comments; skips locales without the old key; refuses duplicate old keys or destination collisions. Code references and files outside the bundle are unchanged.
- **Change bundle key and local JDK literals…**: scoped source rename that also updates every admitted literal reference in the same module and source set (strict JDK contexts and proven final/val-local adapters only). New keys are nonempty printable ASCII, at most 512 bytes, without double quotes, backslashes or dollar signs; lossless escaping is separate for properties and Java/Kotlin literals. The scan admits at most 400 visible Java/Kotlin files, 128 KiB/file, 8 MiB total, 128 key mentions per file and 128 proven references within a one-second budget; exceeded limits or uncertainty abort the whole operation. The review lists every edited file, UTF-16 ranges and exact before/after spellings; a preview over 16 KiB refuses.
- **Convert property keys and values to Unicode escapes…** / **Convert Unicode escapes to native property text…**: reviewed, version-bound source edits; non-BMP characters use UTF-16 surrogate pairs; native conversion re-escapes delimiters, leading whitespace and control characters and verifies decoded keys/values are unchanged; comments are retained verbatim; malformed or unrepresentable Unicode refuses; the file encoding is unchanged.

Creation, discovery and JDK reference filtering share one locale grammar: known ISO 639-1/639-2 languages (including bibliographic/terminological alternatives), optional script, region and Java-style variants; script-only locales are supported (`sr_Cyrl`); total suffix is limited to 96 bytes and each variant to 32 ASCII alphanumeric/hyphen bytes; the private-use `qaa-qtz` range is not treated as assigned. This is filename handling, not runtime locale negotiation.

## Commands

- **Properties: Find Unused Properties**: a potential-unused count from text search. Ignored, generated or runtime usages may be unknown, so absence is not published as a warning.
- **Properties: Reformat File**: applies the code style settings (delimiter spacing, alignment, blank lines).
- **Properties: Copy Property Key** / **Properties: Copy Property Value**: the selected decoded string must cross the SDK's UTF-8 clipboard boundary losslessly; an unsupported lone-surrogate or malformed escape errors before changing the clipboard, and a valid key or value on the same property remains copyable.
- **Properties: Find Explicit Bundle Key Usages**: a read-only picker that searches visible Java/Kotlin sources for the strict JDK bundle contexts used by navigation. Bounded to 400 files, 128 KiB/file, 8 MiB total, 128 occurrences/file, 128 results and one second; exceeded bounds or read errors refuse the result. Ignored files, framework references and dynamic loaders are not resolved; an empty result never establishes that a property is unused.
- **Properties: Discard Staged Locale Grid…** and **Properties: Inspect Compiler-Proved Java Bundle Usages…**.

## Compiler-proved usages and Rename

The optional **Properties: Inspect Compiler-Proved Java Bundle Usages…** command and **Change key with compiler-proved Java contracts…** intention use an approved Java 17+ JDK source launcher. They analyze actual canonical JDK `ResourceBundle` and `org.jetbrains.annotations.PropertyKey` bindings, including final fields, exactly bound effectively-final local aliases, overloaded parameters, annotated fields/assignments/returns and Java-escaped literals. The compiler receives explicit current editor snapshots with annotation processing, implicit source loading and class generation disabled; application classes and static initializers never run. An optional exact JAR profile supplies dependency classes; its profile and JAR hashes are reviewed and rechecked (see [compiler profile setup](adapters/README.md)).

The declared-source graph includes ignored Java files in the focused conventional `src/main/java` or `src/test/java` tree. A complete graph requires ordinary bounded sources (400 files, 128 KiB/file, 4 MiB total), a complete physical sibling locale inventory and no Kotlin/dynamic/unresolved contracts; missing dependencies, class-based bundle collisions, custom loaders and ambiguous contracts refuse compiler Rename. The lazy intention rechecks inventories, hashes, snapshots and semantic edges before and after an exact bounded review; the host applies the multi-file action atomically with Undo. Reflection, framework conventions, other modules/source sets and runtime classloaders are outside this finite source operation; a denied or missing Java leaves the local tools available.

## Line folds

Current-buffer folds cover multiline property continuations, contiguous comment blocks and consecutive dotted-prefix groups. Single-line value folding needs editor support.

## How it works

The API 1.3 diagnostics provider is the sole editor-diagnostic owner and checks the current buffer only; quick fixes use the same source rules. UTF-16 positions are computed in one pass. Source references, bundle-grid edits and cross-locale actions require a losslessly UTF-8-representable key; malformed or unrepresentable keys refuse the action rather than preparing a partial or conflated edit. Cached facts are keyed by project root and revalidate the current innermost project owner before use (owner inventory capped at 64 projects); a nested project's resources cannot leak into an outer project's caches. The metadata-to-apply window at the host remains: no plugin-owned global transaction is claimed across external filesystem changes.

Capabilities: required `project.read`, `editor.read`, `editor.write`, `events`; optional `clipboard` and `process:java`. No network or storage.

## Not implemented

- Project-wide duplicate values/scopes; type-resolved missing/unused-key inspections; full bundle resolution and a global automatic Rename/Find Usages graph.
- Main/test classpath fallback, custom resource roots, class-based/provider bundles, configured `Control`/class loaders, runtime locale choice and library resources.
- Virtual bundle grouping, a locale-management dialog, encoding selection, single-line value folding and template namespace-aware bindings.
