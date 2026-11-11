# Ideolog

One shared UI-kit window contains the Ideolog, Log Snapshot tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

Ideolog offers Patterns and Log Snapshot, plus optional JVM Observations and Live Log Tail surfaces. Pattern forms keep their draft/save/conflict controls. The optional captured snapshot exposes Previous/Next Events in 100-row pages with exact source-line activation, off-page rejection and retained position across tab switches. Capturing remains explicit; changing tabs never reads or tails the log.

OpenIDEA port of JetBrains' [Ideolog](https://github.com/JetBrains/ideolog) (MIT), from tag
`pycharm/263.7307.101` (`41beb53`), the version on the JetBrains Marketplace.

- Detects the log format from the first 25 lines: IntelliJ IDEA, TeamCity build log, Logcat,
  Loguru, pipe-separated, Docker/ISO timestamps, Python `logging`, nginx error log, syslog (level
  from keywords), Laravel/Monolog (`[date] channel.LEVEL:`), Serilog text, or your own (yours win
  ties); other logs are split on the first space, like Ideolog does.
- Colours each event by level (error, warning, info), including its stack trace lines. The time
  column of a coloured event gets its own colour.
- The status bar names the detected format (`Log: IntelliJ IDEA`, or `Log: plain text`); click it to open the Patterns view.
- Setting _Mark time gaps_: the time of an event that comes at least N seconds after the previous one
  gets its own colour (ISO-like, Logcat, syslog and `HH:MM:SS` times; zones are ignored).
- Folds multi-line events (stack traces).
- Lists errors and warnings in the structure view.
- **Ideolog: Jump to Source** (F7): opens the stack frame on the caret line (Java/Kotlin, Python,
  .NET, PHP, Ruby, Dart (`file://`, `package:`), Swift, Node, Go, Rust…), or else the source line whose string literal best matches the
  event's message.
- **Ideolog: Go to Next Error** (Shift+F7), **Go to Next Warning**, **Go to Next Exception** (an event
  whose text names an exception or a Python traceback), **Go to Previous / Next Entry**.
- Cmd/Ctrl+click on a stack frame opens its file and line.

Colours come from the theme's syntax colours (keyword, tag, string, number…), which are close to
Ideolog's red, orange and green in the default schemes.

## Custom formats

Settings › Plugins › Ideolog › _Custom log formats and highlighting_: paste the
`<LogParsingPattern …/>` and `<LogHighlightingPattern …/>` elements of an Ideolog settings export,
or a file from Ideolog's [custom log formats](https://github.com/JetBrains/ideolog/wiki/Custom-Log-Formats)
(Laravel, Symfony, Serilog, Xdebug…). Patterns use Java-like regular expressions, lookaround
included. Highlighting colours map to the nearest theme colour; `HIDE` patterns are ignored.
**Ideolog: Custom Formats and Highlighting** opens the native Patterns view: a table of the patterns you
added, with Log Formats / Highlighting section controls. Select a row before Edit, Enable / Disable, Move Up / Down or Remove; double-click opens Edit. Add Format and Edit open a structured settings form with name, Enabled, event regex,
event-start regex, a bounded numeric timestamp format (empty: automatic) and 0-based timestamp/level/category columns (`-1`: none). Add Highlighting
opens a form with line/field/captured-match action, optional column/format restriction and named
colour choices. Native text fields commit on blur or Enter; press Tab before Preview/Save after editing. Highlighting format restrictions use a named format selector with Any log format; unknown imported restrictions remain labelled unavailable. The table shows enabled state and explicit native Enable / Disable acts on the selected row. Controls revalidate the rendered storage/layout before mutation.

A sample event and **Preview** show the timestamp, level, category and message captures, or the
pieces coloured by the draft's highlighting rule. The preview tests the rule even when its Enabled
checkbox is off. Sample text stays in the draft and is never written to pattern storage. **Save**
validates regex syntax, capture-column bounds, actions and colour values; invalid input stays open
with an explanation. **Cancel** discards the draft. Editing preserves the UUID and other exported
attributes, including `timePattern` and `fullmatch`; the timestamp preview implements a documented numeric subset rather than
full Java `SimpleDateFormat`. The selected supported format also drives actual timestamp-gap calculation. A changed storage snapshot
blocks Save so another window's intervening edit is preserved. The SDK has no storage compare-and-swap,
so this check cannot make concurrent writes atomic across plugin instances.

The form bounds regex/attribute values to 4096 bytes, format names to 128 bytes, samples to
8192 bytes with 4096-byte lines, and saved storage to 128 entries / 128 KiB. Samples use a bounded
backtracking precheck; regex compilation and the existing parser are not preemptible. These global
pattern settings also work without an open project; saving needs the optional storage permission.
Legacy pasted XML in `customPatterns` remains supported alongside the form.
**Ideolog: Highlight Selection** (editor menu) adds the selected text, escaped, as a highlighting pattern; **Remove Custom Highlightings** deletes them all. They are kept in the plugin's storage next to the setting. Invalid
regexes are refused with a notification.
Open logs pick up a change when they're edited or reopened.

## `.log` files

OpenIDEA's built-in `Log` file type now owns `.log` by default. The plugin no
longer shadows it with a filename glob. **Log (Ideolog)** remains available as a
manual file-type choice for its existing custom-language lexical highlighting,
folding and outline. Commands and stack-frame links support both types. API1.4 folding now supplies multiline-event folds for the built-in Log editor.
Built-in document outline remains a separate feature.
The API 1.3 provider returns Ideolog's existing event,
field and match highlighting tokens, including captured timestamps, levels, messages and
stack-trace continuation lines. **On v0.1.39 the built-in editor does not render the
keyword/string/number/comment semantic kinds used by many rules.** Use the manual
**Log (Ideolog)** file type for full custom theme-mapped highlighting. Native builtin log
colours alone are not evidence that custom rules rendered; see the source/native
[boundary audit](../../docs/parity-2026-11-08-refresh/semantic-log-boundary.md). The same built-in/custom formats,
`customPatterns` setting, saved Patterns entries and `timeGapSeconds` setting apply. Uncoloured
entries retain the editor's existing styling; this does not infer extra fields in unknown formats.

The built-in editor overlay processes at most the first 64 KiB / 512 complete lines, stops before
any line longer than 4096 bytes and returns at most 4096 single-line UTF-16 tokens. It checks a
15 ms cooperative deadline during format attempts, physical-line grouping and lazy event parsing; individual regex calls/rule passes cannot be preempted. Configuration is warmed outside the semantic request during activation, log focus/open and explicit pattern/capture work; cold requests return no tokens. External settings changes apply after that out-of-provider refresh, not by recompiling during typing. It performs no project discovery, file reads, pattern-storage reads or network requests. Theme
semantic colours approximate the lexical colours: warnings and marked time gaps use `enum-member`
because LSP semantic tokens have no `tag` or `constant` kind. The `ideolog` custom language keeps
its existing lexical highlighting, folding and outline path, with its 512 KiB limit.

## Not ported

No OpenIDEA API for these yet:

- Hiding, whitelisting or highlighting values and columns, hiding lines above/below or containing
  a substring in the native editor: native line visibility/whitelist hooks remain unavailable.
  The owned Live Log Tail supplies literal text filtering and inverse filtering.
- Native scrollbar stripe and arbitrary font/background styling remain separate from source decorations.
  API1.4 selection ranges and Extend Log Selection are implemented below.
- Error stripe heat map: API1.4 source decorations do not expose the original scrollbar heatmap contract.
- Large file editor, terminal and TextMate integrations.

Stack frame links, syntax, views and semantic tokens come from one component registered with
the API 1.3 SDK macro, see `src/lib.rs`.

Navigation needs the log inside the open project (the IDE only opens project files for plugins).
Jump to Source scans up to 5,000 source files, as there is no index to query. The custom language highlights only the first 512 KiB
of a log to bound editor work; the built-in Log semantic overlay uses the tighter limits above.

Draft fields and Preview/Save/Cancel controls now carry a fresh edit-session
identity. Controls retained by another window after Cancel/reopen are rejected
instead of editing or saving the replacement draft. This binds actions to a
reviewed session; it does not provide private per-window storage or storage CAS.

## P07.3 optional paged log snapshot

**Ideolog: Capture Active Log Snapshot** explicitly captures the focused built-in
Log or manual Log(Ideolog) editor into the optional native **Log Snapshot** bottom
view. **Open Captured Log Snapshot** reopens the retained snapshot. The editor
remains the main log interface. Capturing requires editor.read/project.read and
an existing file in the invoking owning project; no outside-project grant, process
or network is introduced. Rendering never rereads the source or starts capture.

The snapshot reuses current built-in/custom format detection, event grouping,
timestamp/category extraction and existing Error/Warning classification. Literal captured
levels such as INFO/DEBUG/custom names are retained; otherwise severity is Error/Warning/Unclassified. Columns show event source line, level,
timestamp, category and the first message line; messages are limited to512
characters and timestamp/category to128 each. Multiline events remain one row;
activate it to open its original log event line. Existing Jump to Source and the
editor's stack-frame navigation remain separate; this viewer does not invent
resolved Java/other source links or full editor decorations.

Native Viewport requests page at most100 rows (default50). A capture accepts at
most256KiB current text and8192 physical lines, retains the first2048 parsed events
and reports retained versus parsed count, source version, age and format. Oversized
files/buffers are refused, not presented as complete. Discard releases the instance
snapshot. Project switches do not expose or discard another root's snapshot. Row
activation is restricted to the issued page and revalidates current project/source
text before opening; changed/deleted sources require a new capture.

Capture has a cancellable native Task and a cooperative two-second parse deadline;
checks surround reads and event parsing, while failures/cancellation retain the
last good snapshot. Cancellation does not promise interruption inside a synchronous
regex/read call. There is no running tail job to stop; Discard is not a substitute
for native Task Cancel during capture. Regex handling keeps the existing bounded
backtracking behavior, rather than adding a new unbounded parser.

This is **snapshot paging**, not byte-offset streaming, filesystem tailing, appended
record following, rotation handling or durable/background viewing. Pattern changes
apply on the next capture; retained rows keep their capture-time parsing. Pinned SDK
editor.document/project.read_file transfer whole text; stat preflight bounds saved
files, and oversized unsaved buffers or file races are rejected after retrieval.
No bounded-transfer guarantee is made for that SDK call. Parser snapshots and
rendered rows are bounded as above; no log text is persisted or sent to a service.

Original MIT source `largeFile/LargeLogFileEditor.kt` wraps IntelliJ LargeFileEditor,
clears Ideolog context on dispose/settings changes, and installs editor highlighting.
That platform large-file/editor behavior is not reproduced by this optional view.
Known native semantic-renderer acceptance limitations remain separate; this change
adds no new editor-coloring claim. Tests/package checks are not matching native
viewer screenshots or desktop acceptance; native verification remains separate.

Native follow-up (dated desktop30): the isolated pinned host captured120/121 owned events, rendered a later viewport, followed an exact source line, refused a changed source and discarded the cached snapshot. Literal INFO/DEBUG/custom levels are retained in the table. Native wheel scrolling and in-flight Task Cancel remain unverified; renderer-independent snapshot paging does not fix the editor semantic-overlay gate.

## External global Patterns refresh

Successful Highlight Selection and Remove Highlightings now notify only the
Patterns view through native refresh when persistent pattern
text actually changes. Native refresh is retained; cancelled/error/no-op commands
emit no false change. The page obtains its own correlated model, and stable
render/preview paths emit no invalidation loop. Native Log Snapshot/capture views
and their existing source ownership remain separate and unchanged.

Pattern definitions and manual pattern-editor drafts/samples are intentionally
global settings. The editor's sample has exactly two input paths: the bundled
Example failure initializer and the explicit manual Sample field. Highlight
Selection converts editor text into a user-approved global highlighting rule;
it does not import that text into the draft sample. No editor-derived preview
sample is retained by this external path, so no guessed project owner or new
frame protocol is introduced. Manual global drafts are preserved, and external
storage changes still make Save refuse its stale base until Cancel/reopen. Sample
text remains preview-only and is never saved with the pattern definition.

Notifications contain no definition/sample payload. This does not make global
definitions or drafts frame-private; same-instance sharing is deliberate. Tests
cover the distinction, actual change/no-op behavior, stale-save conflict and
feedback-free project-independent render. No native GUI, process/network or full
original-settings parity is claimed. MIT/JetBrains attribution remains unchanged.

## Cooperative snapshot parsing checkpoints

Ideolog76 extends the optional snapshot Task cancellation/deadline checkpoints through format detection (between individual regex attempts) and event grouping (before every physical line), including long continuation runs. The same parser grouping/ranges serve existing editor behavior. A before-fix regression observed only three cancellation checks for300 physical lines; checked grouping now visits every boundary, and cancellation during detection/grouping returns no partial snapshot. Prior accepted snapshots remain intact.

Eight focused viewer tests, full76 tests and pinned API1.3 build/package checks are recorded separately. This does not interrupt a single synchronous regex, configuration compilation or SDK read, guarantee bounded text transfer, add streaming/tailing/rotation, or change the native Log semantic-renderer gate. No new native Task Cancel or viewer acceptance is claimed.

Ideolog78 keeps semantic preparation inside its cooperative deadline: expired/zero-token requests skip preparation, detection/grouping use checked parser loops, and event field parsing is lazy until the remaining cap/deadline. Existing complete-line/UTF16/nonoverlap bounds and format/highlight rules remain. Six focused semantic tests and full78/pinned build are recorded separately; they prove checkpoint/config ownership behavior, not native50ms rendering acceptance. The pinned built-in Log literal-kind renderer limitation is unchanged.

Ideolog79 bounds the newline search itself to a UTF8-safe64KiB prefix before splitting lines. A source line extending far past that prefix therefore cannot cause scanning of its entire tail merely to reject it. A prefix ending inside a line or Unicode scalar does not feed that partial line to format detection; a real final document line without newline remains supported. The new regression covers byte-cap line fragments, an astral scalar across the cutoff, a very long line after a valid prefix, and complete no-newline source. This bounds processing of already supplied text, not the SDK's document transfer or native rendering time.

Stack-frame definitions now require an owned Log document (512KiB maximum), parse at most8KiB per frame line and retain at most32 frame observations. Zero/overflow line numbers are refused. Filename lookup remains a suffix heuristic: only a unique highest suffix score in an uncapped, project-owned regular-file candidate set is navigable. Ambiguity returns no definition, rather than choosing a shorter path. UTF16 positions remain document-local; host-confined stat may follow links within its root and does not establish physical path identity. No native semantic colours, runtime acceptance or renderer mapping is implied.

Custom-language syntax requests admit only the `ideolog` language before configuration preparation. A512KiB cutoff that contains no complete line returns no custom syntax events, folds or outline entries; complete final document lines remain supported. This does not add a deadline to a single custom regex operation or prove native timing.

Explicit Jump to Source requires the editor owner and invoking project to match. Both command and definition positions require an exact UTF16-to-byte-to-UTF16 roundtrip, so surrogate-interior or out-of-document carets cannot silently select byte zero.

The explicit message-based source search now excludes files owned by another open project, including nested project roots. Equal best message/file ranks return no source jump instead of choosing the first file. Search remains a bounded heuristic over up to5000 source candidates, not exact source provenance or complete project indexing.

Time-gap parsing normalizes supported ISO `Z`, `±HH:MM` and `±HHMM` offsets, so changing timezone spelling alone cannot create or hide a gap. Malformed offset suffixes yield no parsed time. Naive timestamps retain an assumed common local scale; this does not establish timezone provenance or validate every calendar field. Built-in semantic colour approximations and native renderer limits are unchanged.

Severity navigation and outlines keep numeric custom-level identifiers intact: `error2` is unclassified rather than silently normalized to `ERROR`. Existing recognized aliases and punctuation-wrapper handling remain; raw custom levels stay visible in the snapshot.

API 1.4 adds optional **Ideolog: Mark Current Log Source** and **Clear Log Source Marks** commands. Mark paints a bounded snapshot of the focused built-in Log file using native error/warning/time-gap theme decorations. The log must belong to an open project and `editor.read`/`project.read`/`events` must be granted. It examines at most the first complete 64 KiB/512 lines (4096-byte line limit) and 4096 spans. Marks are cleared by the next semantic request (including edits), observed events, explicit Clear or plugin shutdown; run Mark again to refresh. The existing semantic provider remains active. These snapshot hints do not fix the semantic renderer, filter or redact log text, or provide arbitrary custom colors. Native theme/lifecycle visual acceptance remains pending.


### Native pattern UI and original comparison

The Patterns panel now uses the IDE's native table and form renderer, without a packaged web view. It preserves separate parsing/highlighting forms, regex validation, enabled state, named colour/action choices, sample preview and explicit Save/Cancel. Draft-bound controls and storage snapshots reject stale edits. Native fields commit on blur or Enter.

Compared with JetBrains' public `LogParsingPatternSettingsDialog` and `LogHighlightingPatternSettingsDialog` in [Ideolog](https://github.com/JetBrains/ideolog/tree/master/src/main/kotlin/com/intellij/ideolog/highlighting/settings): existing capture/regex controls map to native fields rather than copying Swing layouts. Full Java `SimpleDateFormat` support, regex-aware editor widgets, full colour picking and screenshot parity are not claimed; the original Marketplace listing has no gallery in the retained audit. No filtering/tailing or semantic-renderer fix is implied. Native desktop keyboard/light/dark/dock-size acceptance remains pending.


### Further original-dialog parity recheck

The native Patterns table now exposes recognizable Log Formats and Highlighting sections, selected-row editing/removal/reordering, an explicit enabled toggle and named log-format restrictions. Format UUIDs remain internal identities; duplicate display names are disambiguated. The regex widgets remain native text fields, not IntelliJ regex editors.

Timestamp formats support fixed numeric `yyyy`, `MM`, `dd`, `HH` (or `hh` with `a`), `mm`, `ss`, `SSS`, quoted literals and `Z`/`X`/`XX`/`XXX` offsets. A pattern requires hour/minute/second fields; absent/empty preserves automatic parsing. Preview confirms whether the sample timestamp matches the selected format; the same parser is used for actual gap computation. Unsupported tokens such as locale month/day names are rejected for new edits; imported unsupported formats remain readable but produce configuration errors and no computed gap. This is not full Java SimpleDateFormat support.

Background/bold/italic/stripe widgets and arbitrary color semantics remain missing. Original MIT Ideolog parsing/highlighting dialog fields were rechecked against retained Marketplace263.7307.101 decompilation; supplementary public source is commit aa342f00d580c074c27ad32e67b46c7670df9b02. Native UI adaptation does not establish full visual or behavior parity.

### Twelve-hour timestamp formats (2026-11-10)

Custom timestamp formats accept `hh:mm:ss a`, including case-insensitive ASCII
`AM`/`PM` markers, dates, milliseconds and numeric zone offsets. Midnight/noon
and time gaps use the same normalized timestamp scale as 24-hour formats.
Ambiguous `hh` without `a`, `HH` with `a`, invalid twelve-hour values and extra
text are rejected. Preview and imported custom format processing use the same
parser. Locale-specific day-period words, month names, named zones and general
Java SimpleDateFormat semantics remain unsupported.

### Numeric width and fixed-zone timestamp formats (2026-11-10)

Single-letter `M`, `d`, `H`, `h`, `m`, and `s` accept one or two digits; double-letter
forms retain fixed two-digit parsing. `k`/`kk` accepts 1–24 (24 means midnight),
and `K`/`KK` accepts 0–11 with `a`. `S` reads one to three millisecond digits;
`SS` and `SSS` read exactly two and three (these are millisecond numbers, not decimal
fraction scaling). Variable-width adjacent numeric fields are rejected unless
separated to avoid ambiguous ownership of digits. `z` through `zzzz` accepts UTC,
GMT and GMT offsets in `GMT+02:00` form; region names/ambiguous abbreviations are
not guessed. RFC822 `Z` through `ZZZ` keeps numeric `+0200` spelling.

### Optional JVM regex and date observations (2026-11-10)

**Ideolog: Capture JVM Regex and Date Observations** runs an approved Java17+
executable for an explicitly selected custom `LogParsingPattern`. It accepts raw
imported patterns from `customPatterns`, including Java regex constructs and
SimpleDateFormat fields unsupported by the local editor parser. Choose an explicit
BCP47 locale and Java timezone ID, or leave either empty to use the JVM default.
The review names the source, format, regex, date pattern, scope and local tool.
No Java tool or application is installed or bootstrapped automatically.

The native **JVM Observations** table shows event source lines, epoch milliseconds,
time gaps, timestamp/severity/category/message captures and the actual Java
version/locale/timezone. Java `Pattern.find()` capture order, non-overlapping
groups and multiline final-message extension follow the original parser behavior.
Java SimpleDateFormat retains its default lenient parse behavior, including its
locale-aware names and timezone rules. After explicit capture, the built-in Log semantic provider can reuse exact-source
observations for uniquely located timestamp, recognized severity and category
fields. Syntax and semantic providers never launch a subprocess.

Capture requires an owned project log no larger than64KiB/512lines, with4096bytes
per line. One bounded JVM child handles the whole snapshot, with a64MiB heap,
15second deadline, cancellation and256KiB stdout/8KiB stderr limits. Display cells
are truncated to512UTF16 code units. Helper source and a bounded input file live
in plugin-private storage; input contents are cleared on every normal return or
error. Empty input-file metadata may remain until plugin uninstall. No project
build, project classpath or framework bootstrap is invoked. The approved tool
inherits the host's process environment. Missing/denied Java, unsupported versions,
invalid patterns, timeouts and oversized output retain previous observations.

Observation activation rechecks project ownership, the exact current source,
version where available and stored pattern configuration before navigating.
Snapshots remain memory-only and do not imply live-tail, editor renderer or
arbitrary custom-highlighting-rule parity.

### Exact-source JVM highlighting cache (2026-11-10)

For a successfully captured built-in Log document, cached JVM observations supply
semantic timestamp, recognized error/warning severity and category tokens. Actual
JVM time gaps drive the existing gap-color category. A field is admitted only when
it occurs exactly once in its original first physical line; repeated, multiline,
potentially truncated and overlapping fields are withheld rather than guessed.
Ranges use UTF16 coordinates, including astral characters before a capture.

Cache use requires the same document path, full text, version, deepest project
owner, warmed custom-format configuration and current customPatterns setting.
Delivered file/settings/focus lifecycle events withdraw live cache eligibility
while retaining the explicit observation table. **Mark Current Log Source** can
re-admit an unchanged snapshot after warming current stored configuration and
subscribing to lifecycle events; it uses the same cached JVM fields for native
source marks. Re-capture when the source or custom format changes.

Providers read neither files nor pattern storage and run no Java process. Stored
format changes in another plugin instance become visible after a configuration
refresh or delivered lifecycle event; the live cache does not claim independent
cross-instance storage observation. The custom `ideolog` language retains its
existing syntax provider. Java custom-highlighting-rule colors and full-line
style rendering remain outside this cached-field integration.

## Owned live tail, filtering and API1.4 editor features (2026-11-11)

Open a saved project UTF8 log, choose **Ideolog: Open Live Log Tail**, and press
**Start** to review an approved Java17+ file-following session. The shared-kit panel
shows complete raw lines, a case-insensitive literal text filter, inverse **Hide
matches**, **Follow** scrolling and **Stop**. Leaving/hiding the panel requests Stop.
**Ideolog: Stop Live Log Tail** also stops a remaining session after a panel reload.
Filtering is confined to the owned panel; it does not hide native editor lines.

The Java helper uses secure directory-relative read handles with symlinks refused.
It begins within the final64KiB of the selected file and follows appends for at
most5minutes/1MiB per explicitly started session. Each helper read is4096bytes;
only complete UTF8 lines up to4096bytes are admitted. The view retains at most
512lines/128KiB. File identity changes and observed truncation reset the displayed
generation; a missing replacement gets a2second rotation grace before stopping.
Changes that truncate and regrow between polls without changing file identity or
the retained prefix are not promised to be detected.

Visible correlated panel messages drain output at1second intervals. The backend
checks project ownership and unsaved-source state, bounds frames/output, rejects
stale controller messages and stops its child on cancellation, source invalidation
or plugin shutdown. The child self-expires even if the panel loses its controller.
Helper source is written only to plugin-private storage; no project/application
bootstrap or automatic tool installation occurs. Source file contents are never
written. Platforms without SecureDirectoryStream or usable file identity refuse
the profile; a Windows live-tail profile is not supplied.

The builtin **Log** language now receives generic API1.4 folding ranges for
multiline events/stacktraces. Exact-source JVM observations can supply fold groups
when the local parser cannot express their custom format. Providers reuse warmed
configuration and current text with size/line/deadline limits and no subprocesses.
**Ideolog: Extend Log Selection** and the generic selection-range provider expand
from a captured field through its physical line, event and document. UTF16 carets
are validated; explicit selection changes recheck source version/text and use the
current editor API rather than file edits.

Verification includes actual Java file append, truncation, delayed rotation,
outside-root symlink rejection, process cancellation and lease expiry; a real
headless Chromium panel fixture checks filter/invert, safe text rendering, visible
polling, hide-stop and380px layout. These are runtime helper/browser receipts,
not installed-desktop acceptance.
