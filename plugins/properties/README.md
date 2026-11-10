# Properties

Inspections and editing tools for `.properties` files in OpenIDEA, ported
from JetBrains' [Properties plugin](https://plugins.jetbrains.com/plugin/11594-properties)
(`com.intellij.properties`, Apache-2.0, source in
[intellij-community](https://github.com/JetBrains/intellij-community/tree/idea/263.7307.112/plugins/properties)).

- Live problems from current unsaved text: duplicate keys, invalid `\u` escapes,
  trailing spaces, a key/value delimiter that doesn't match the setting, and
  keys missing from other locales of the resource bundle. Three dots instead of
  `…` and unsorted files are reported when enabled in the settings.
- Quick fixes (⌥⏎): remove property, unescape, remove trailing spaces, replace
  delimiter, add the missing translation to another locale, remove a property
  from every locale, sort the file or the whole bundle.
- Properties: Find Unused Properties reports a potential-unused count from text search.
  Ignored, generated or runtime usages may be unknown, so absence is not published as a warning.
  The optional open/save candidate cache remains available without fading editor keys.
- Properties: Reformat File applies the code style settings (delimiter
  spacing, alignment, blank lines).
- Properties Structure tool window: keys grouped by dotted prefix, flat or
  sorted. Double-click a key to go to it.
- Open With › Resource Bundle: a read-only overview of keys × locales. Select a
  property, then use Edit Value or Rename Key in This File; Add Property and
  Remove from This File provide reviewed changes with the existing revert control.
  Open Translation chooses a locale and opens its source at the selected key.
  The overview never presents other locales as editable or discards typed cell submissions.
  Native intentions perform confirmed atomic
  cross-locale updates; no process permission is required and unsaved buffers are
  not replaced by shell writes. Show Incomplete Only keeps the keys some locale lacks.
- Go to Declaration on a key: the same key in the parent locale
  (`messages_fr_CA` → `messages_fr` → `messages`) and in every other locale
  of the bundle that declares it. Java/Kotlin navigation is limited to the
  explicit JDK bundle-call subset below; arbitrary literals are quiet. Legacy
  template candidate navigation remains on `#{a.b}` in `.html` (Thymeleaf),
  `.xhtml` and JSP, and on `<spring:message code>` / `<fmt:message key>`.
- Missing-key warnings in Java/Kotlin/templates are withheld: text matches and a conventional
  bundle name do not establish a resolved classpath/MessageSource owner. Navigation, completion
  and labelled candidate hovers remain available; `reportMissingKeysInCode` is a retained legacy preference.
- Key completion in `simplelogger.properties` (slf4j) and in explicitly owned
  direct JDK ResourceBundle key calls. Getter spelling alone, MessageSource,
  custom helpers and unknown `@PropertyKey` contracts do not establish an owner.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```


Cross-locale intentions now use API 1.3 FileActions: Add translation, Remove from
all locales, Sort resource bundle files, and **Set property value across selected
locales…**. The latter chooses bundle members and a new value, then confirms the
exact paths, UTF-16 edit ranges and replacement text. Existing unsaved project
text participates in the snapshot and edits; unrelated buffer content is retained.
Duplicate keys must be resolved before setting a value. All files are applied as
one workspace edit and undone together by the host. No `tee` or other program runs.

Actions bind source path/version and project ownership, re-read bundle membership
and text after confirmation, reject stale translations and membership changes,
and are limited to 199 files, 256 KiB each, 1 MiB total and a 16 KiB preview.
Mutation planning also retains the raw 201st-file discovery sentinel before
filtering similarly named bundles; incomplete discovery refuses the action rather
than returning a partial “all locales” edit.
The SDK does not expose explicit per-target editor versions in FileAction; target
changes after resolution use the host's workspace-edit preconditions rather than
an extra plugin-owned lock. There is no atomic guarantee across an external
filesystem change racing the host application.

**Create locale file…** adds an empty locale beside the current bundle, with a
preview of its path and ASCII comment. It accepts a known two-letter language,
optionally an uppercase two-letter country or three-digit region (`fr`, `fr_CA`,
`es_419`). Existing paths and unsaved destination buffers are refused; create
uses `overwrite: false`. New files contain ASCII text; the host applies its new-file encoding defaults.
No translation is copied or invented, and no encoding selector is supplied. Existing-file edits retain host encoding/BOM behavior;
unrepresentable characters fail the workspace edit rather than changing encoding.
This does not implement virtual bundle grouping or a locale-management dialog.
Desktop multi-file application and undo remain unverified for this change.


Original-aligned scope: the pinned Properties 263.7307.108 evidence identifies
`AddNewPropertyFileAction` and `CreateResourceBundleDialogComponent`, whose dialog
validates file names/existing destinations and creates chosen locale files in one
write command. This implementation supplies native intention prompts for one new
locale and atomic selected-locale writes; it does not reproduce that full dialog
or claim original-specific bundle-grid screenshot parity.

## Property hover and Rename ownership

Hover on a `.properties` key shows the **current buffer's** decoded key/value, source file and
locale, including unsaved changes. Java/Kotlin hovers require the explicit JDK bundle
context below and filter candidates to that bundle's conventional module resources.
Supported template hovers retain their labelled cached candidates. Up to eight declarations
show source paths, locales and values; runtime/classpath resolution remains unknown.
Code/template hover checks the caret line (at most 4096 bytes); Java/Kotlin ownership
also scans the bounded current document for imports and shadowing. It never builds
the project index and performs no file reads or scans. A cold, denied or foreign-project cache
returns no candidate hover. Cache identity now includes its project root; saved/changed/deleted
property files invalidate it. Other files' unsaved values are only as fresh as the warm index.
Documents over 256 KiB are skipped; key discovery is capped and output is limited to 8 KiB.
Markdown metacharacters are escaped and source ranges retain exact UTF-16 coordinates.

**Properties: Find Explicit Bundle Key Usages** provides an explicit, read-only picker
from a property key. It searches visible Java/Kotlin sources for the same strict JDK
bundle contexts used by navigation, and only includes calls owned by this exact
conventional resource bundle. Results retain exact UTF-16 key-literal coordinates;
selection rechecks the invoking document, project ownership and selected source
text before opening. Search is bounded to 400 files, 128 KiB/file, 8 MiB total,
128 occurrences/file, 128 results and one second; exceeded bounds/read errors refuse
the result. Ignored files, framework references and dynamic loaders are not resolved;
an empty result never establishes that the property is unused. No provider scanning,
file mutation, automatic graph or Rename is added by this command.

**Automatic key usages and Rename remain deliberately withheld.** The original
`PropertyReference` resolves a named bundle through the code context, while
`RenamePropertyProcessor.prepareRenaming` expands a resolved ResourceBundle across locales and
checks destination collisions. This port's existing `KeyIndex` groups decoded names globally,
and Java/template call detection is a text heuristic. Neither proves the classpath/bundle owner.
Publishing it to the host graph would also enable automatic Rename, including incomplete groups
when escaped spellings were omitted. No synthetic, decoded, escaped, Boot relaxed-binding or
global key-name anchors are published. Existing related-locale and explicitly scoped code navigation, legacy template candidate
navigation and the confirmed atomic bundle actions remain available; this slice does not claim indexed usages/Rename
parity. Safe bundle/type resolution plus escape-aware replacement and collision handling are
still required before that graph can be enabled.

## Live diagnostics

API 1.3 DiagnosticsProvider is the sole editor-diagnostic owner. It checks the current buffer for
duplicate decoded keys, malformed Unicode escapes, trailing spaces and code-style delimiter
mismatches, plus opt-in three-dots/ellipsis and alphabetic-order checks. Existing quick fixes use
the same source rules. Calls from events, commands, action providers and hover do not publish
editor diagnostics. Documents over 128 KiB or 4096 properties are skipped; results are capped
at 512, and UTF-16 positions are computed in one pass rather than rescanning each prefix.

Known missing-translation facts may supplement local checks after a successful bounded warm
read of the same file's sibling locales. They are keyed by project and current file, matched to
current keys rather than stale offsets, and invalidated on property open/save/create/change/delete.
Cold/foreign/error/truncated/oversized/uninterpretable discovery is quiet. Warm reads are limited
to 128 sibling files, 128 KiB / 4096 keys each and a 250 ms cooperative deadline; discovery uses
a raw 201st-file sentinel before filtering similar bundle names. Current newly typed keys without
a warmed fact remain quiet. Ignored locale files are not promised to be discovered, and another
locale's unsaved edits may remain stale until its next warm event. This is known-locale coverage,
not a proof of every possible bundle or runtime usage.

Original comparison: pinned Properties inspection descriptions and implementations cover duplicate
properties, trailing spaces, key/value delimiters, ellipsis, sorting and incomplete translations.
This slice retains those established source-local rules; project-wide duplicate values/scopes,
type-resolved missing/unused-key inspections, full bundle resolution/Rename and richer bundle-editor
work remain outside it. The earlier locale creation and atomic edits are unchanged. Verification
uses actual provider calls with current unsaved/fixed buffers, Unicode/CRLF ranges, limits and quiet
unknown discovery; no new desktop acceptance is claimed.


## Explicit Java/Kotlin reference ownership correction (P14.2 partial)

The former definition provider accepted every Java/Kotlin literal and returned
all properties with the same decoded key. That incorrectly created Properties
links on framework-owned annotation/configuration literals, including Quarkus
`@ConfigProperty(name="owned.quarkus.key")`. That behavior is removed. Completion
and cached source hover now share the same current-text ownership gate rather
than treating a getter's spelling as a resource-bundle contract.

Supported calls include a direct chain such as:

```java
java.util.ResourceBundle.getBundle("com.acme.messages").getString("app.title")
```

The exact explicit `java.util.ResourceBundle` import also permits the short
`ResourceBundle` name. `getObject` and `getStringArray` use the same literal key
contract. Both calls must have exactly one plain string argument. Foreign or
wildcard imports, aliases, known local qualifier/type/value/type-variable
shadows, explicit inheritance, dynamic expressions, escapes/text blocks,
Kotlin interpolation/nested-comment uncertainty and unbalanced source stay quiet.
Java structural Unicode preprocessing is not guessed. Ordinary strings,
Quarkus/Boot annotations, `MessageSource`, unproved variables named `bundle`, helper
`message(...)` methods and compiler/classpath-resolved `@PropertyKey` contracts
do not acquire a Properties definition/completion/hover.

The caller must lie under its module's conventional `src/main/java`,
`src/main/kotlin`, `src/test/java` or `src/test/kotlin` source tree. The explicit
bundle base is mapped only to the corresponding module and main/test
`resources/` tree, including its named package path and known locale variants.
Another module, source set, base name or `application.properties` file with the
same key is excluded. Explicit base names ending in locale-like suffixes retain
their own default file identity. This is positive **conventional source candidate**
navigation, not a verified build/classpath/module-loader result. Main/test
classpath fallback, custom resource roots, class-based/provider bundles,
configured Control/class loaders, runtime locale choice and library resources
remain unestablished. No absent key is reported from this partial model.

All three Java/Kotlin editor surfaces consume an already warmed same-project
cache; cold requests return quietly instead of building a project index.
Existing source-open events and explicit/bundle workflows can warm it. Cache
construction reads at most 5000 discovered files, 128 KiB/4096 properties per
file, 8 MiB total with a cooperative 250 ms deadline. Skipped/unreadable/capped
files do not become absence facts. Per-file key membership lets completion
inspect only the named family, bounded to 128 files/4096 distinct known keys and
512 results within 64 KiB of labels/details; value hints are limited to 512
characters. Keys requiring string escaping/interpolation or longer than 4096
bytes are omitted rather than inserted as unsafe raw source. Source parsing is limited to 128 KiB/16384 tokens/256 imports;
UTF-16 ranges retain the exact literal contents. Saved/changed/deleted properties
invalidate the cache; another file's unsaved values/ranges can remain stale until
its next warm event. Locale-parent navigation, current-buffer diagnostics and
all atomic bundle actions retain their separate existing contracts.

Original ownership evidence was rechecked in the pinned decompiled
`PropertyReference`, `I18nUtil` and `PropertiesCompletionContributor`:
a reference carries a bundle name, lookup uses the caller's module, and
completion requires a PropertyReference and defers to a more-important reference.
This source subset does not reproduce PSI/classpath resolution. The JDK's
[ResourceBundle contract](https://docs.oracle.com/en/java/javase/17/docs/api/java.base/java/util/ResourceBundle.html)
confirms that the base name and locale/module/provider loading matter, and that
the supported getter methods read keys; filename equality alone is insufficient.

No DocumentIndexer or Rename graph is registered. Global decoded-key, relaxed
binding, escaped and synthetic anchors remain withheld. Legacy HTML/JSP candidate
navigation still uses its existing heuristic and is not promoted to resolved
ownership or Rename by this change. Complete scoped usages/Rename, custom
bundle APIs and namespace-aware template bindings remain separate work. The
Quarkus collision and scope corrections have focused MockHost regressions;
new native desktop acceptance has not been run.

### P14.2 follow-up: immutable local ResourceBundle contexts

Java `final ResourceBundle bundle = ResourceBundle.getBundle("pkg.messages");`
and `final var bundle = java.util.ResourceBundle.getBundle("pkg.messages");`
now establish a bounded source-local bundle context for direct literal
`bundle.getString`, `getObject` and `getStringArray` calls. Kotlin `val` supports
the same inferred initializer and an explicit JDK ResourceBundle type in named
class-member or simple top-level block functions. Short names require the exact
non-wildcard JDK import; package/type/value shadow guards remain in force.

The declaration must start a simple statement after an opening/closing brace or
semicolon, and the initializer is precisely a one-literal JDK getBundle call.
Kotlin may end that initializer at a newline; constructors, fields/properties,
parameters, mutable declarations, unknown type aliases, custom loaders/Control/
locale arguments, wrapped/postprocessed initializers and guessed data flow remain
unsupported. Non-braced control-flow initializer positions are not inferred.
This deliberately excludes some valid Kotlin declarations after other newline-
terminated statements; it is not a compiler-wide local-variable resolver.

One unambiguous enclosing named function and lexical block ancestry must contain
the declaration and use. Sibling/closed blocks and other methods cannot donate a
bundle identity. Every same-name function-body occurrence must be the declaration
or a direct one-literal getter: assignment (even after the selected use), aliasing,
argument/return escape, rebinding and shadow declarations suppress the whole
context. Matching field/parameter names also suppress it. Lambda receivers,
anonymous/local classes, nested functions and unrecognized scope blocks stay
quiet; ordinary braced control blocks retain lexical ancestry.

The existing explicit bundle/module/main-or-test resource scope and warmed-cache
lookup are reused by definition, completion and hover. Source facts follow the
current unsaved buffer; requests perform no file discovery/read or cache warming.
Cold/foreign caches remain quiet, and no missing-key or complete-usage conclusion
is added. No host Rename graph is registered. Bounds retain128KiB/16384 tokens/
256 imports and add64 nested delimiters plus4096 tokens per supported local function.
The original PropertyReference's explicit bundleName and bundle-file lookup contract
informed this extension; no runtime classpath, loader, active locale or semantic
Rename equivalence is claimed. The earlier62-test/atomic locale/live slice and the
immutable batch22 receipt are preserved; this is a later follow-up with focused
mock tests/package validation, pending native local-variable acceptance.


The packaged Properties Structure pane now receives model-free invalidation after
observed Properties open/save/file batches and editor-focus events, plus successful
native bundle-cell edits, locale-source navigation and Reformat mutations. The
receiving page requests its own current frame/project model; no source data or
foreign model is broadcast in the invalidation message, and render never sends
another invalidation. The native Resource Bundle editor retains native refresh and uses a reviewed
source/layout binding for its numeric table callbacks, as described below. Resolving a FileAction is not an applied
mutation: locale creation/cross-locale actions rely on actual host apply events.
Missed events, host scheduling and final event/render races remain possible; this
is not a durable watcher or an immutable web-asset cache fix.

Structure and bundle rendering now require a concrete Invocation project matching
the current Properties editor's project owner. Unknown/foreign editors show no
source model. Packaged structure row/group keys bind exact root/path/document
version; stale editor/version keys refuse activation instead of targeting an equal
key in a different file. Native structure keys remain compatible. Bundle cache
identity includes root and source path, and cached member project owners are
rechecked before rendering/navigation. These are current project-visible source
checks, not resolved runtime/classpath bundle identity or semantic Rename.

Original source comparison: decompiled `PropertiesFileStructureViewModel` binds
its editor and Properties PSI file and offers prefix grouping/alphabetical sorting;
`PropertiesFileStructureViewElement.getChildrenBase()` enumerates that file's
properties. This follow-up preserves the equivalent grouped/flat/sorted source
workflow and makes packaged freshness/owner scope explicit; it does not invent a
new global bundle model. Retained official documentation imagery concerns property
values/encoding, not a verified structure/bundle-grid layout. Desktop34 remains
dated evidence for the earlier69-test immutable-local ResourceBundle navigation;
new packaged invalidation/row-identity native acceptance remains pending.


Cached code/template property facts now use a bounded current open-project owner
snapshot before hover/navigation/completion. Opening a resource directory as a
nested project cannot leave its cached values/keys available to the outer project.
Initial indexing also skips current foreign-owned sources before reading them.
The metadata-only owner inventory is capped at64 projects; unsupported paths,
unavailable identity or an over-cap inventory are quiet. Explicit JDK bundle
contexts with a reassigned matching cached source are withheld, while template
candidate lists keep unaffected owned positives. No source read/discovery is
added to the cached provider path; snapshot-to-use metadata races and cached target
text freshness remain distinct limits. No host String Rename graph is published.


Key equality for duplicate inspection now retains Java UTF-16 units instead of
merging distinct lone-surrogate escapes into the Unicode replacement character.
Valid escaped surrogate pairs still match their literal Unicode spelling. Source
reference facts and explicit usages require a lossless UTF-8 key; malformed or
unrepresentable keys cannot authorize a bundle-grid edit or cross-locale action.
Those operations refuse the unsupported bundle rather than preparing a partial
or conflated key edit. Source-local inspection can still distinguish repeated
identical Java surrogate keys; an unrepresentable key hover shows its raw spelling.
Display-value conversion and copy-only helpers are separate from this key-identity
proof. This does not add indexed Rename or a full JVM resource-loader model.


Copy Key/Copy Value now require the selected decoded String to cross the SDK's
UTF-8 clipboard boundary losslessly. An unsupported lone-surrogate or malformed
Unicode escape returns an error before changing the clipboard; a valid key or
value on that same property remains copyable, including valid surrogate pairs.
This closes the separate copy-helper boundary after the key-identity repair;
source text is never rewritten to manufacture a replacement character.


Native Resource Bundle table mutations now require a rendered source/layout
binding and an exact confirmation naming the current key(s), file, version,
UTF-16 ranges and complete before/after text for every edit. Cancel changes
nothing. Callbacks without a matching rendered source/version, callbacks after filter or
reload invalidation, unknown rows and reclaimed bindings refuse; duplicate value/
key targets and destination-key collisions also refuse. Existing function tests
now render a current model and answer this review, matching the native callback
protocol. Other-locale cells still navigate rather than mutate.

The pinned native event payload has no render-generation token. A late callback
can therefore refer to an older table after a newer model was returned; the
mandatory exact target review prevents that positional mismatch from becoming
a silent write. This is a reviewed current-file edit, not semantic Rename of
code references or other locales. After review the source/path/root/version are
checked again, then one versioned editor apply performs the edit. The final
metadata-to-apply window remains; no plugin-owned global transaction is claimed.

Native bundle preparation requires a complete visible inventory, at most199
files,256KiB per source,1MiB source text,4096 distinct keys and32768 cells. Foreign,
unreadable or oversized locale sources are refused before cache adoption; stat
checks precede locale reads and returned text is bounded again for unsaved buffers.
These checks do not establish a bounded transfer for an SDK read racing file growth.
Oversized16KiB exact edit reviews refuse before confirmation. New native review,
rapid-edit, cancellation and viewport acceptance remains unverified.

Explicit source-usage search now admits only canonical paths owned by a bounded
current project snapshot, including the invoking resource file. Source stat kind
and byte limits are checked before reading; returned text remains bounded for
unsaved buffers and racing growth. This is positive-only conventional JDK usage
search, with ignored/dynamic/framework closure still unsupported.

The confirmed FileAction **Change key in this bundle only…** changes exact key
spelling across the complete visible conventional locale inventory. It preserves values and
comments, skips locales without the old key, and refuses duplicate old keys or
any destination-key collision. New keys are bounded, losslessly escaped and
reviewed. Code references, ignored files and files outside this bundle are unchanged;
this is not complete source-aware Rename. Atomic bundle snapshots now validate
bounded current owners and source metadata before locale reads.

Navigation-index warmup also checks source metadata before reads. A source whose
on-disk size exceeds admission is withheld even if a smaller unsaved buffer is
open; admitted sources still use unsaved text and are bounded again after reads.

After a reviewed native bundle edit, **Revert last reviewed bundle edit…** can
restore that single edit through a separate exact inverse review. This is a
plugin memory-only control, not native Undo. It requires the identical resulting
project, file, full text and document version; any unrelated change refuses.
Only one record is retained (at most256KiB resulting text plus256KiB inverse old
text), and it disappears after successful revert or plugin restart. The16KiB
exact-review limit still applies. Other files are never reverted.

## Scoped bundle and local JDK literal Rename

From a key in a conventional `src/main/resources` or `src/test/resources` bundle, **Change bundle key and local JDK literals…** updates the visible locale declarations and every admitted literal reference in the same module and source set. It uses the existing strict JDK `ResourceBundle.getBundle`/getter and proven final/val-local adapter; other bundle names, modules, source sets, dynamic/framework/unknown receivers and ignored files are unchanged. This is a scoped source Rename, not global classloader identity or a host Rename graph. The separate **Change key in this bundle only…** retains its existing declarations-only behavior.

New keys must be nonempty printable ASCII, at most512bytes, without double quotes, backslashes or dollar signs. Properties keys are losslessly escaped separately from Java/Kotlin plain literal interiors. Locale destination collisions and decoded duplicate old keys refuse the whole operation. The complete review lists the project, decoded old/new key, every edited file, UTF-16 ranges and JSON-escaped exact before/after spellings. Cancel changes nothing; resolving the action only returns one workspace edit for the host to apply.

The scan admits at most400visible Java/Kotlin files,128KiB/file,8MiB total,128key mentions per file and128proven references, with a one-second budget checked between parsing calls. Every discovered admitted source must retain its current project owner and regular-file/byte admission; uncertainty, unreadable sources or exceeded limits abort instead of preparing a partial source update. Full source text/metadata, including nonmatching files, and exact admitted inventory are compared again after confirmation. Visible locale inventory/text/owners and the invoking document's request version/text are also rechecked. A complete preview over16KiB refuses without consuming an unshown approval.

Ignored/unresolved framework/runtime references are explicit exclusions, so the action does not establish complete program-wide Rename. Per-target live editor version tokens and final check-to-host-apply CAS remain SDK/host boundaries; no native acceptance for this new action follows from the earlier bundle-only69/70 or Revert110 receipts. No safe-delete/move, source compilation/runtime, process, storage or network work is added.

## Native structure and bundle UI

Properties Structure now uses the IDE's native tree and toolbar: dotted-prefix
or flat grouping, alphabetical/file order, Expand All/Collapse All, Refresh and
Enter/double-click source navigation. Native row identities bind the current
project, editor path and source version, so delayed actions from another file or
an edited source refuse rather than navigating a different property.

The Resource Bundle custom editor already uses a native editable key-by-locale
table; it retains incomplete-key filtering, locale navigation and reviewed edits.
No HTML structure bridge is required. These workflows follow the original
PropertiesStructureViewModel/ResourceBundleEditor roles and the official
[Properties files documentation](https://www.jetbrains.com/help/idea/properties-files.html).
The documentation reference is not evidence of exact bundle-grid screenshot
parity; final keyboard/theme/dock and native editor acceptance remains separate.

## Native bundle controls (2026-11-10)

The overview separates browsing from editing because the native table API cannot
mark individual locale columns read-only. Current-file changes use explicit
selected-property controls, exact review and current-source/project guards;
other locale sources open only through Open Translation. The existing atomic
Set property value across selected locales intention remains the cross-file
editing path. This adapts the original key/value/destination-selection workflow
without claiming the full original internationalization dialog or inline
all-locale editor. Toolbar glyphs use the released native icon registry.
