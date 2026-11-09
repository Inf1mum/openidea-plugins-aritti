# Ideolog

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
**Ideolog: Custom Formats and Highlighting** opens the Patterns view: a table of the patterns you
added. Add Format and Edit open a structured settings form with name, Enabled, event regex,
event-start regex and 0-based timestamp/level/category columns (`-1`: none). Add Highlighting
opens a form with line/field/captured-match action, optional column/format restriction and named
colour choices. Pattern UUID is selectable for copying into a highlighting restriction. Move Up
and Remove keep their existing ordering workflow.

A sample event and **Preview** show the timestamp, level, category and message captures, or the
pieces coloured by the draft's highlighting rule. The preview tests the rule even when its Enabled
checkbox is off. Sample text stays in the draft and is never written to pattern storage. **Save**
validates regex syntax, capture-column bounds, actions and colour values; invalid input stays open
with an explanation. **Cancel** discards the draft. Editing preserves the UUID and other exported
attributes, including `timePattern` and `fullmatch`; the raw-capture preview does not implement
Java `SimpleDateFormat` or change the parser's existing date handling. A changed storage snapshot
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
folding and outline. Commands and stack-frame links support both types. The
built-in semantic overlay does not itself publish custom folds or outline.
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
  a substring: the editor has no API to fold, hide or re-colour on demand.
- Extend selection: plugins can't set the selection.
- Error stripe heat map: no editor decoration API.
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
packaged Patterns view with model-free `ui-kit-refresh` when persistent pattern
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
