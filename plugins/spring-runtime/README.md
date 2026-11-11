# Spring Runtime Insight

One shared UI-kit window contains the Overview, Observations, Bean Graph, Refresh Controls tabs. Commands select their corresponding custom tab; existing actions, drafts and ownership checks remain in their original handlers. See [tab migration and parity validation](../../docs/custom-plugin-groups-2026-11-10.md).

Reads the Actuator endpoints of a Spring Boot application running on this machine and shows what it loaded, in a Spring Runtime tool window and as editor markers.

## Setup

The application needs Spring Boot Actuator with these endpoints exposed over HTTP:

```properties
management.endpoints.web.exposure.include=beans,env,conditions,health,mappings,configprops,scheduledtasks,loggers,caches
```

Settings › Plugins › Spring Runtime Insight:

| Key | Default | |
|---|---|---|
| `actuatorUrl` | `http://localhost:8080/actuator` | Actuator base URL; host must be `localhost` or `127.0.0.1` (`http://` or `https://`). Change the port or path when the application uses another `server.port`, `management.server.port` or `management.endpoints.web.base-path`. |
| `actuatorAuth` | `none` | `none` sends no credentials. `bearer` sends `Authorization: Bearer <token>`, `basic` sends `Authorization: Basic <token>`, with `<token>` the `actuator-token` secret (store it in the IDE's secrets UI; for `basic`, `user:password` encoded in Base64). The plugin never reads or logs the token; the host substitutes it into the header. Needs the optional `secrets` capability. |
| `timeoutMs` | `2000` | Timeout per endpoint request, clamped to 100–30000 ms and the remaining batch budget. |
| `refreshBudgetMs` | `20000` | Cooperative refresh batch deadline, clamped to 1000–60000 ms. |
| `pollAttempts` | `5` | Explicit polling lease attempt limit, clamped to 1–10; no new attempts after 10 minutes. |
| `autoRefresh` | `false` | Allows explicitly armed, instance-owned polling. Enable it, then Start Bounded Polling for the current project/URL/auth mode. The flag alone never fetches. |

Without `actuatorAuth`, requests carry no credentials. An endpoint that answers 401 or 403 adds "needs authentication: check actuatorAuth and the actuator-token secret" to the status line. An endpoint that answers 404 or anything other than JSON with status 200 is listed as not available, and the rest are still read. If the first endpoint (`beans`) cannot be reached at all, the refresh stops with an error. Both `net:localhost` and `net:127.0.0.1` are optional grants for explicit/opted-in refresh. Cached browsing and markers need no network grant. A missing grant leaves the last good snapshot in place.

The plugin uses the network only on Refresh (command/button) or the existing
explicitly opted-in visible controls-view timer with verified project ownership. Rendering, expansion,
source markers and cache restoration never fetch or start processes. URLs are
normalized and restricted to local HTTP(S) authorities with a valid optional port;
credentials, query strings, fragments and ambiguous authority forms are refused.

Full snapshots now have a shared **8 MiB serialized envelope limit** for both
writing and reading, including ownership metadata. There is no automatic trimming
to beans/environment and no deletion of last-good data on overflow. A replacement
becomes current in memory only after storage succeeds. Transport, cancellation,
JSON/response-size and storage failures leave the previous snapshot intact. A
refresh that loses any previously available endpoint is rejected wholesale, so
new and old endpoint facts are never silently mixed under one fetch date. If an
endpoint was intentionally removed, Clear the current scope before refreshing
that reduced endpoint set. Initial partial snapshots still identify unavailable
endpoints. Responses are bounded to 4 MiB per endpoint and 16 MiB per refresh.

Cache identity includes the exact project root, normalized base URL and auth
**mode**, with the full identity verified inside the stored envelope. Switching
project/URL/auth never reuses another scope's data or navigation targets. Clear
only deletes the currently selected scope. Secret values are never read, logged,
or used in storage keys; rotating a token without changing its mode is not an
account-identity signal, so refresh/clear explicitly after changing credentials.
A refresh is a sequence of HTTP reads, not an atomic application-state capture.
No application-instance or debugger-session identity is verified; the same URL
can later serve another process. The old unscoped `snapshot` key is not adopted or deleted automatically: it lacks
project/auth provenance. Refresh once to establish an owned cache.

Failure metadata is bounded and retained separately when storage permits. The
status shows the last-good fetch date/source and the latest refresh failure,
rather than presenting old facts as newly fetched. Invalid/oversized caches are
not silently replaced; Clear remains available for explicit recovery. Scope
records share the host's plugin storage quota; no unrelated scope is evicted to
make room. Clear the relevant old scope if quota prevents a new snapshot.

## Spring Runtime tool window

The status line shows the project, URL, auth mode, fetch age, unavailable endpoints and last refresh error. This is a cached Actuator source, not debugger state. Activating an item opens it in the project's sources when a matching class, method or key is found.

- Beans (`beans`): per application context, each bean with scope and type, expandable to its dependencies. Opens the bean's class.
- Health (`health`): the overall status, then components and nested components when Actuator includes them (`components`, or `details` on older Spring Boot versions).
- Configuration (`env`): the active profiles (or "default profile"), a Config files group listing the configuration file property sources (opens the file), then every property source in precedence order, highest first, with its properties and values as Actuator returns them (masked values appear as `******`). A property opens its key in the project's `application*`/`bootstrap*` files, preferring the file it came from.
- Auto-configuration (`conditions`): positive and negative matches, each with its condition messages. Opens the class or `@Bean` method.
- Mappings (`mappings`): per dispatcher servlet or handler, each request predicate with its handler. Opens the handler method.
- Configuration properties (`configprops`): each `@ConfigurationProperties` bean by prefix, with its bound properties and values. The bean opens its class, a property its key in the configuration files.
- Scheduled tasks (`scheduledtasks`): each task's target method with its kind (`cron`, `fixedDelay`, `fixedRate`, `custom`) and expression, interval or trigger. Opens the method.
- Loggers (`loggers`): `ROOT` and each logger with a configured level, with its configured and effective level.
- Caches (`caches`): each cache as `manager / cache` with its target type.

## Editor markers

Markers come from the last snapshot only; the editor never triggers a request.

- Java beans: a gutter marker on each stereotype class (`@Component`, `@Service`, `@Repository`, `@Controller`, `@RestController`, `@Configuration`, `@SpringBootApplication`, `@ControllerAdvice`, `@RestControllerAdvice`, `@Endpoint`, `@ShellComponent`, `@TestConfiguration`) and `@Bean` method loaded in the application, with its name, scope and number of dependent beans.
- Not loaded: an info diagnostic on a stereotype class or `@Bean` method missing from the application, when the application has beans in the same package. When a negative auto-configuration match names the class or bean, its condition messages are added.
- `@Value`: on each `@Value("${key:default}")`, the runtime value of every placeholder key and the property source it comes from.
- `@ConfigurationProperties`: on the class, whether the prefix is bound and how many properties it has; on each non-static field or record component, the runtime value and its source, or the bound value when no property is set (the field's default).
- Configuration files (`application*.properties|yml|yaml`, `bootstrap*.properties|yml|yaml`): on each key with a value, "Active (from file)" when this file's value wins, otherwise the winning value and its property source.

Keys are matched with relaxed binding (`spring.jpa.show-sql` and `spring.jpa.showSql` are the same key; the `SPRING_DATASOURCE_URL` environment variable is `spring.datasource.url`).

## Commands

| Command | Does |
|---|---|
| Spring Runtime: Refresh from Actuator | Reads the endpoints and reports the bean and property source counts and the endpoints not available |
| Spring Runtime: Clear | Drops the snapshot from memory and storage |
| Spring Runtime: Inspect Cached Snapshot | Opens bounded read-only native observation/provenance details |

The tool window has the same Refresh and Clear buttons, including Clear when a cache cannot be read. Clear stops this instance’s polling lease and leaves the global autoRefresh permission unchanged.

Presentation is bounded to 5,000 rows and a 3 MiB tree message; limits are labelled
partial and do not truncate saved evidence. Long labels/details/tooltips are marked
truncated, and unbounded source identities are not retained as navigation targets.

Java sources are parsed as text, and beans are matched to runtime beans by qualified class name, or by bean name and simple type. YAML block values (`|`, `>`) get no marker. The plugin reads only the Spring Boot 2.x and 3.x Actuator JSON formats and only applications on `localhost`/`127.0.0.1`. Source markers are for Java only.

P22.1 delivers scoped persistence and read-only cache consumption, not native debugger/runtime equivalence. The inspected LiveBeansPanel/LiveBeanGraphPanelContent and debugger security receipts describe different runtime authority; this plugin does not acquire breakpoint/session evaluation. Desktop refresh/cache acceptance and original screenshot/theme comparison remain unverified.


### Cached snapshot details and inlays (P22.2 source scope)

**Inspect cached snapshot** opens a supporting native `Runtime Snapshot Details`
tree with read-only Observation and Provenance tabs. The existing packaged hub,
Refresh/Clear controls, scoped persistence, authentication and opt-in polling
remain in place. The detail view never fetches endpoints: it presents retained
observations from all nine endpoint models, capped to 64 rows per endpoint
(health/environment: 32), 512 total, 8 KiB per observation and 16 KiB provenance.
Missing fields are explicit; source/condition ownership, current execution state,
cache contents and source/runtime build identity remain unknown. Environment/configprops property
values are omitted from these new panes; retained condition messages and other
server-reported text are displayed as observations. Endpoint, project, URL, auth mode
(never the credential), fetch timestamp/age, missing endpoints and last refresh
error identify the evidence. Refresh, clear and scope changes expire selections;
per-render keys prevent an old selection from silently addressing another row.

Activation of a source-linked bean, configuration-properties type or explicit
mapping method opens only a **current-project source candidate** with the exact
reported qualified name. An explicit bounded source scan must be complete;
duplicate classes, missing/overloaded methods and capped/oversized source facts
refuse navigation. There is no simple-name/suffix or first-hit fallback in this
new view. Java files are capped at 512 KiB / 512 types / 4096 members for this
index. Candidate paths must also belong to the exact current root under the host’s
innermost project owner; a separately open nested project cannot supply a candidate.
Final source-owner/settings checks do not provide an atomic active-window or source-version lock.
Textual navigation is not proof that this source built the observed
application. Other endpoint rows have no guessed source target.

The `snapshotInlays` switch (default on) adds properties/YAML labels for exact
retained `/env` keys: **snapshot: observed** or **snapshot: redacted**. Tooltips
show the property-source name and fetch provenance, never the value. Inlays
read only the already-loaded scoped memory snapshot: cold, disabled, missing-env,
different-scope and duplicate-key facts remain quiet. Lookup is capped to 256
property sources / 8192 retained pairs (larger snapshots suppress inlays), and
the first 200 current-document keys / 4096 bytes per key. They perform no network,
storage reload, discovery or source scan. Relaxed/environment aliases are not
used, and the hint does not assert current effective value, active profile or
origin in this editor file. Existing legacy marker heuristics are unchanged.

Original audited evidence includes `RuntimeBeanClassLineMarker` and
`RuntimeBeanLineMarkerInfoBase` (PSI/UAST, resolved Spring beans and runtime
resource navigation) and `RuntimeProperty` (resource resolution and origin
offsets). Those stronger resource/session authorities are not inferred here.
Breakpoint evaluation, debugger sessions, full source-origin/line resolution,
bean graph interactions, source-linked live object evaluation and desktop
pane/inlay acceptance remain gaps. No proprietary implementation was copied.

Not covered: Actuator endpoints behind other authentication than a bearer or Basic token (OAuth flows, client certificates), applications on other hosts, Kotlin sources, SpEL in `@Value` (`#{…}`), starting or stopping the application, changing values at runtime.

```sh
cargo test --locked
node tests/controls-timer.mjs
openidea-plugin build
openidea-plugin link
```

## Cached bean dependency graph (P22.4 subset)

**Spring Runtime: Cached Bean Dependency Graph** (also **Bean Dependency Graph** in the main
window) opens a supporting packaged graph using the same local SVG/host bridge as the other
Spring diagrams. It loads only the existing project/Actuator-URL/auth-mode snapshot; there is no
network request, source discovery during render, provider scan, CDN or graph library download. Explicit Refresh
remains in the main Runtime view. Selecting a node explicitly compares its exact reported Java type
against a bounded current-project source inventory, then shows a read-only cached bean observation with
Back to Graph and provenance; it does not open guessed source or evaluate a debugger/live object.
**Inspect Snapshot List** returns to the existing endpoint detail view.

Nodes are exact `(context ID, bean ID)` identities from one retained `/beans` result. An arrow
means only that this bean's retained string-array dependency list names a unique displayed bean
in that **same context and snapshot**. Duplicate identities suppress every candidate in that group,
even if one candidate's type metadata is invalid. A parent-context name, alias, uniquely matching
type, mapping handler, health component, property source, logger or cache name does not establish
another graph relationship. Unknown and ambiguous targets are counted, never invented as nodes or
silently attached across contexts. Known targets omitted by the node budget are counted separately.

The parser now records whether each fetched dependency value was a JSON array containing strings
only. Previous snapshots deserialize with this verification **false**: their other observations
remain intact, but outgoing graph edges are withheld until an explicit successful refresh establishes
the typed list. Malformed dependency values retain existing observation behavior without becoming
spurious graph edges (for example JSON boolean `true` is not proof of an edge to bean ID `true`).
This optional metadata is backward-compatible with the current persistence envelope; it does not
adopt legacy global snapshots or change project/URL/auth ownership.

Limits: full identity validation at most 4,096 retained bean records, then 128 displayed unique nodes,
512 edges and the first 64 dependency names per displayed bean. Larger identity inventories withhold
the graph rather than guessing uniqueness. Context/bean/dependency identities are limited to 512
bytes with no controls, type metadata to 4 KiB and scope text to 256 bytes. Unknown/unsupported fields,
omitted nodes/dependencies/edges and legacy partial snapshots remain visible in status. Cyclic edges
within the displayed graph are highlighted using bounded reachability (including self-edges); this
is not cycle enumeration or a claim of runtime failure, and capped/unknown relationships can hide
additional cycles. Node labels are escaped, and click keys carry epoch/fetch/record fingerprints
with full current scope/selected-record membership checks rather than authorizing raw IDs.

Status and details show endpoint URL, project, auth **mode only**, fetch timestamp/age and retained
normalized type provenance. **Source/runtime correspondence is UNKNOWN**: the graph does not prove
that this project built the running application, that the current source matches the retained type,
that a bean still exists now, or that a reported dependency is a particular constructor/field
injection. An explicit node selection records a **dated source inventory comparison**: exact type
observed in one source, ambiguous declarations, or **SOURCE INVENTORY DIFFERENCE** when the reported
qualified type was not observed in discovered current-project Java files. This difference may be
explained by library/generated/Kotlin/excluded sources; it does not prove an outdated application
or a wrong build. Ignored/excluded files and other open project roots are outside the inventory.
Selection performs at most 2,000 discovered files, 512 KiB/file, 8 MiB total and a cooperative
three-second reading/parsing budget; discovery/read failure or any cap yields UNKNOWN, never an
absence claim. Source comparison performs no HTTP, source edit or source navigation. Its timestamp
is shown and render never rescans: source files may have changed since selection. Select again to
refresh that observation. Back, Clear, scope changes and successful refresh expire it.
Scope changes, Clear and successful refresh invalidate graph selections; failed refresh
keeps the last-good snapshot and its recorded age as before. Panes, persistence, auth and opt-in
refresh behavior are otherwise preserved.

Original comparison: decompiled `RuntimeBeanDefinition` retains full bean names, dependencies,
factory/member/runtime flags, and `SpringBeanRuntimePresentationProvider` consults the debugger
logical-info authority before adding a loaded badge. Inspected screenshots `25302-0.png` and
`25302-1.png` show definition badges and a debugger object with Injected into / Depends on and
context/scope details. This adaptation exposes only the smaller Actuator snapshot relationship;
a matching original standalone runtime graph source/screenshot was not found, and debugger/source
origin parity is not claimed. Mock tests cover same-context/duplicate identities, malformed/legacy
dependencies, cycles, all graph bounds, cached rendering and explicit dated source comparison with no HTTP/source navigation, stale
record/root/auth keys, redraw/Clear/missing endpoint, and hostile labels. New native desktop
acceptance remains separate.


## Explicit refresh and bounded polling controls (P22.3)

**Spring Runtime: Refresh and Poll Controls** (or the main window's supporting controls button)
shows the exact owning project, **requested** Actuator base URL, auth mode, request/batch limits,
poll status and cancellation boundary in a private packaged controls view. **Refresh Once** remains explicit. Rendering this view, the main tree, graph,
snapshot panes and editor providers starts no HTTP reads.

Polling requires both `autoRefresh=true` and **Start Bounded Polling** for the current exact
project/URL/auth-mode scope. The setting alone no longer authorizes network work. A lease lives
only in the current plugin instance, permits at most 1–10 attempts (default 5), no earlier than one
minute apart, and expires for new attempts after 10 minutes. Two consecutive failures, native task
cancellation, scope changes, Clear, Stop Future Polls or disabling the setting disarm it. A foreign
project cannot stop another root's lease. A visible controls page drives a private 60-second timer;
Rust validates its random frame/nonce, increasing sequence, root, current URL/auth mode and lease
before any read. Hiding or leaving clears the timer and requests Stop; a running request uses
native task Cancel. Reopening never transfers a lease to a new page. All HTTP remains in Rust.
Pinned v0.1.39 native scheduling only invokes inactive instances without a project, so the old
on-schedule activation was removed and scheduled events never fetch. Delayed/skipped ticks, plugin reclamation and
window shutdown do not create a durable/resumable timer. No lease is stored with the snapshot.

**Stop Future Polls** revokes future attempts only. It cannot signal a currently queued/blocked
refresh from a separate plugin callback. For an active refresh use the IDE background task's
native **Cancel**. Pinned v0.1.39 source establishes that `tasks.begin` captures the invocation
cancellation token, `cancel_task` cancels that token, and `net.fetch`/host HTTP await the same token.
Native task cancellation can therefore interrupt the active HTTP future on that host; explicit
checks before/after each request and before adoption also prevent subsequent reads/adoption.
This does not assert that a server immediately stops processing a request. There is no invented
SDK `Task::cancel` call or mid-request control exposed by the plugin form.

Each batch retains the nine known endpoint order and byte limits. Timeout is clamped to the
remaining cooperative wall-time budget, and deadline/scope/cancellation are checked at endpoint
boundaries and before commit. A synchronous transport/JSON operation cannot be preempted by the
plugin's own clock check; the host's cancellation/deadline and request timeout are separate guards.
Only successful scoped persistence adopts new data. Cancellation, scope drift, failed/oversized
responses and losing a previously usable endpoint leave its last-good snapshot unchanged.
Transport exception text and response headers/bodies are not echoed into UI/log errors, avoiding
accidental credential-bearing error details; 401/403 remain explicit auth guidance.

Known-root/container shape checks distinguish **typed empty observations** from unsupported data:
`contexts:{}` for context endpoints, `propertySources:[]`, `loggers:{}`, `cacheManagers:{}` and
known empty scheduled-task arrays are valid. A plain `{}`, wrong root/container type or malformed
required entry is unavailable, not proof of zero beans/properties/tasks. If a previously usable
endpoint becomes unsupported, the whole refresh is rejected and the previous snapshot retained.
These checks validate only the structures consumed by the existing Boot 2/3 adapters; they do not
add endpoint versions or prove every nested runtime value's semantics.

Requested URLs remain restricted to the existing localhost/127.0.0.1 HTTP/HTTPS forms, and auth
remains none/bearer/Basic through host-substituted `actuator-token`. The plugin never reads the
credential. **Redirect boundary remains unresolved**: the pinned shared host client follows
redirects before checking the final host; the SDK exposes no per-hop redirect policy or final URL.
Controls deliberately label the requested URL, not a verified responding peer. No grant, remote
host, auth mode or arbitrary-target capability is broadened here. TLS trust/OAuth/client-certificate
support and credential/account identity remain outside this implementation.

Original source comparison includes `SpringDebuggerRuntimeApplicationProvider`, which filters
live debugger sessions by project/module. The polling lease reuses the scope-ownership idea but
is not a debugger session lifecycle or a subscription to the running BeanFactory. Prior inspected
runtime screenshots remain debugger observations, not evidence of an equivalent Actuator polling
console. Mocks cover lease opt-in/cadence/attempt/expiry/failure/scope/stop, native task cancellation
at a request boundary, guarded scope changes, supported requested URL/auth headers and timeouts,
secret-safe failures, typed empty/unsupported JSON retention, auth failures and direct 3xx non-adoption.
Mock 3xx does not establish host redirect safety. No real network/process or new desktop acceptance
was used for this slice.

### P22.3 native scheduler correction

The private packaged `runtime-controls.html` / `runtime-controls.js` driver has no HTTP API: it
only posts scoped control messages and schedules one 60-second timeout after an accepted reply.
Wrong frame, nonce, sequence or displayed root/URL/auth messages cannot drive a poll. Rust also
checks current open-project membership before and during refresh. Render/hello never authorize
network work. A new page cannot inherit another page's lease; use the **Stop Future Polls** command
if the old owning page disappeared before Stop was delivered, then explicitly Start again.
An instance accepts at most64 distinct Start nonces before reclamation/reload is required.
Host view-message transport still broadcasts: frame routing prevents wrong-page control/rendering,
not encrypted confidentiality. URL/auth labels contain no credential values.

Deterministic Node VM tests cover real packaged driver timing, hidden/busy/unmounted Stop and
foreign-frame/stale-sequence responses. A hide during an outstanding Start/tick records a pending
Stop even if the dock reopens before its reply; visibility changes preserve that reply sequence,
so the old lease is stopped before any hello can rearm it. Brief hide/reopen races are tested for
both Start and tick; reopening requires another explicit Start. Rust mock tests cover scoped ticks, single-use Start,
settings changes, cancellation and ignored native Scheduled events. These tests are not native
WebView acceptance; native task cancellation remains established by pinned-host source inspection.

Native follow-up: the isolated pinned host proved an owned localhost/none-auth refresh and timed tick. Dock hiding keeps the iframe mounted without changing `document.hidden`; the driver therefore additionally observes actual viewport intersection and fails closed until visible. The corrected native test made no requests while hidden for over90 seconds, and reopening required explicit Start. See desktop23 in the dated parity receipts. This is a bounded native subset; it does not prove protected URLs, redirects, secret revisions or every cancellation path.


Mounted **Runtime** and **Bean Dependency Graph** packaged pages now receive a
model-free invalidation after snapshot Refresh/Clear/error and polling control
changes, including refreshes performed by the separate controls page. Each page
re-reads its own project/URL/auth-bound cached model; no snapshot or credential is
included in the broadcast. Invalidation names its logical destination, so updating
Overview/Observations/Graph does not remount an active polling-controls driver.
Render/hello do not invalidate, poll or fetch. Missing/denied/invalid current
project scope returns an empty-state model instead of reusing another graph.
This is covered by Rust mocks and the existing timer-driver tests; this follow-up
has no new desktop acceptance and does not address immutable packaged-asset URL
caching or make host message transport confidential.

Current packaged-controls validation (2026-11-09): the Node VM harness reads the
current HTML IDs and script, models the Element attribute/class/text methods it
uses, and checks aria-busy and error/status transitions alongside the existing
visibility/in-flight timer races. It has a fake bridge/clock and is not a browser,
actual Actuator refresh or native acceptance. Current asset validation is dated
separately from Runtime55 historical inputs; historical receipts are unchanged.

## Shared-kit runtime window

One native Spring Runtime container now hosts shared UI-kit tabs: **Overview**,
**Observations**, **Bean Graph** and **Refresh Controls**. Existing commands and
buttons select the appropriate logical tab; cached data and command IDs remain
separate. Overview retains the scoped tree, category icons, source navigation,
Refresh, Clear Snapshot and Collapse All. Rendering or navigation never fetches.

Observations retains all bounded endpoint sections. Right-click a row and choose
**Show Details…** for its full **Observation** and **Provenance** (project, URL,
auth mode, endpoint, fetch time, last error and missing endpoints). These read-only
facts are attached only to the authoritative selected row, avoiding a large eager
payload for every observation. Activating a source-linked row still requires a
unique exact current-project qualified source candidate. Credentials and omitted
Actuator property values remain absent. Same-snapshot UI renders retain row keys;
refresh, clear or scope changes retire them.

The graph keeps its existing navigation and controls. Refresh Controls retains the
private frame/nonce/sequence handshake and visible-only 60-second timer. It mounts
inside the hub without remounting after each protocol reply. Leaving the tab
clears its backend lease, including when Start was outstanding; driver disposal
queues Stop before navigation. Hiding the pane stops future polls. Returning
creates a fresh frame and sends Hello, requiring a new explicit Start.

The layout follows the cached observation and source-navigation workflow from the
[official Spring tool window documentation](https://www.jetbrains.com/help/idea/spring-tool-window.html).
This is Actuator browsing, not the debugger's breakpoint/object identity model.

Validation for this migration: 58 Rust tests pass, including real grouped-bridge
observation selection/details/source activation/stale-key checks and private
controls Start/navigation lease retirement. The Node timer harness preserves the
existing cadence/visibility/late-reply cases and adds pending-Start disposal.
Chromium and WebKit run the actual grouped hub and controls driver, covering
pending Start navigation, a fresh frame on return, and hidden-pane Stop. These
checks use deterministic transports; no new live Actuator or desktop acceptance
is claimed.


### Observation search and HTTP request templates

Observations has **Search Observations…** and **Clear Search** controls. Search
matches cached titles, endpoint categories and retained details without network
access. It shows the visible/retained row count, keeps the query while browsing
the same snapshot scope, and clears it on project/URL/auth scope changes.

Select a mapping observation with explicit Actuator `requestMappingConditions`
methods and patterns, then use **Copy HTTP Requests** (optional clipboard
permission). The copied HTTP-client template includes each reported method/path
pair, an editable application URL and reminders to fill path variables, headers,
parameters and bodies. The application address is not inferred from the Actuator
management address. No HTTP request is sent. Predicate-only, unrestricted-method,
malformed and legacy mappings without structured conditions do not invent GET
requests. A stale selection cannot copy another scope's request.

The **HTTP Method** picker limits Observations to mappings with that explicit
method in structured Actuator metadata. **All observations** restores every
category. It never classifies a mapping by searching its predicate text, and
mappings with missing/unrestricted method metadata remain unavailable under a
specific-method filter. Method selection is retained within the same scope and
clears on project/URL/auth changes.


**Create HTTP Request File…** reviews the selected mapping's complete template,
then creates a new plain `.http` file in the current project root and opens it.
Existing files are never replaced; a cancelled review creates nothing. The
optional `project.write` capability is required.

**Send Local Request…** lets you choose a reported mapping,
replace its path variables with a concrete localhost/127.0.0.1 URL, and review the
exact method, URL, explicitly entered headers and UTF-8 body before sending.
Headers and body each have an8KiB editing bound; transport headers such as Host
and Content-Length are controlled by the host and cannot be overridden. No
Actuator credentials are reused automatically. Larger bodies can be composed in
the exported HTTP-client file. The selected observation's
**HTTP Response** detail shows the response status, byte count and up to64KiB of
plain response text; response headers are omitted. Last response data stays only
in memory and is retired with the observation scope. HTTP4xx/5xx are shown as
responses, not cached snapshot refresh failures. No render or editor provider
sends an application request.


**Open GET in Browser…** is available only for an explicitly reported GET mapping.
It asks for a concrete local application URL and reviews it before opening the
normal browser, whose own cookies and authentication apply. It neither guesses
an origin from the Actuator URL nor substitutes unresolved path patterns.


Structured Actuator request conditions are retained as bounded review clauses:
required/negated headers and parameters, consumed media types and produced media
types. Literal positive header/parameter values supply editable request examples;
query values are percent-encoded and a single concrete consumed/produced media
type supplies a Content-Type/Accept example. Missing fields or negation flags
remain labelled unreported/unsupported. Negated conditions never become positive
request defaults. Required unspecified header values use an explicit placeholder
that must be replaced before sending. Body schemas are not inferred.

Select a bean observation and use **Related Beans…** to choose a reported injected
bean or dependent in the same reported context. Only verified dependency arrays
and unique context/name owners produce source candidates. Source opening still
requires one exact qualified current-project class; no library/parent-context or
Java assignability semantics are invented. The observation details list the same
bounded related candidates. Rendering and the chooser never refresh Actuator.

Cached snapshot-labeled inlays also cover exact-key Java/Kotlin `@Value` placeholders at physical closing-string anchors. They reuse current project/URL/auth-mode ownership and redaction/provenance gates, never fetch on editor requests, and do not present annotation defaults as observed values. Foreign annotations, concatenations and incomplete source literals are withheld.

**Tools → Configure Database from Runtime Snapshot** creates an owned native connection from one exact captured `/env` property source. Supported targets are simple PostgreSQL/MySQL/MariaDB TCP JDBC URLs, including bracketed IPv6. The review shows project, capture time, source and target; masked/unresolved fields, URL options, embedded credentials and ambiguous identities require manual setup. Captured nonempty passwords can be stored directly in the native secure store without displaying them. Target-bound IDs prevent changed host/database/user targets from inheriting a previous credential. This action does not connect, read schemas or execute SQL, and does not claim to clone the application's currently active connection.
