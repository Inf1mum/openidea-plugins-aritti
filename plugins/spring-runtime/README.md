# Spring Runtime Insight

Reads the Actuator endpoints of a Spring Boot application running on `localhost`/`127.0.0.1` and shows what it loaded, in a **Spring Runtime** tool window and as editor markers. MIT; see `LICENSE`.

One shared UI-kit window contains the **Overview**, **Observations**, **Bean Graph** and **Refresh Controls** tabs; commands select their corresponding tab.

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

An endpoint that answers 401 or 403 adds "needs authentication: check actuatorAuth and the actuator-token secret" to the status line. An endpoint that answers 404 or anything other than JSON with status 200 is listed as not available, and the rest are still read. If the first endpoint (`beans`) cannot be reached at all, the refresh stops with an error.

The plugin uses the network only on Refresh (command/button) or through the explicitly opted-in visible controls-view timer with verified project ownership. Rendering, expansion, source markers and cache restoration never fetch or start processes. URLs are normalized and restricted to local HTTP(S) authorities with a valid optional port; credentials, query strings, fragments and ambiguous authority forms are refused. `net:localhost` and `net:127.0.0.1` are optional grants for explicit refresh; a missing grant leaves the last good snapshot in place, and cached browsing and markers need no network grant.

## Snapshots and scope

- Full snapshots have a shared **8 MiB serialized envelope limit** for writing and reading, including ownership metadata. There is no automatic trimming and no deletion of last-good data on overflow; a replacement becomes current only after storage succeeds. Transport, cancellation, JSON/response-size and storage failures leave the previous snapshot intact. Responses are bounded to 4 MiB per endpoint and 16 MiB per refresh.
- A refresh that loses any previously available endpoint is rejected wholesale, so new and old endpoint facts are never silently mixed under one fetch date. If an endpoint was intentionally removed, Clear the current scope before refreshing that reduced endpoint set. Initial partial snapshots still identify unavailable endpoints.
- Cache identity includes the exact project root, normalized base URL and auth **mode**, verified inside the stored envelope. Switching project/URL/auth never reuses another scope's data or navigation targets; Clear deletes only the currently selected scope. Secret values are never read, logged or used in storage keys; rotating a token without changing its mode is not an account-identity signal, so refresh/clear explicitly after changing credentials.
- A refresh is a sequence of HTTP reads, not an atomic application-state capture; no application-instance or debugger-session identity is verified (the same URL can later serve another process). The old unscoped `snapshot` key is not adopted or deleted automatically; refresh once to establish an owned cache.
- Failure metadata is bounded and retained separately. The status shows the last-good fetch date/source and the latest refresh failure rather than presenting old facts as newly fetched. Invalid/oversized caches are not silently replaced; Clear remains available for explicit recovery. Scope records share the host's plugin storage quota; no unrelated scope is evicted — clear the old scope if quota prevents a new snapshot.
- Known-root/container shape checks distinguish **typed empty observations** from unsupported data: `contexts:{}`, `propertySources:[]`, `loggers:{}`, `cacheManagers:{}` and known empty scheduled-task arrays are valid. A plain `{}`, wrong root/container type or malformed required entry is unavailable, not proof of zero beans. If a previously usable endpoint becomes unsupported, the whole refresh is rejected and the previous snapshot retained. Transport exception text and response headers/bodies are not echoed into UI/log errors; 401/403 remain explicit auth guidance.

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

Keys are matched with relaxed binding (`spring.jpa.show-sql` and `spring.jpa.showSql` are the same key; the `SPRING_DATASOURCE_URL` environment variable is `spring.datasource.url`). Java sources are parsed as text, and beans are matched to runtime beans by qualified class name, or by bean name and simple type. YAML block values (`|`, `>`) get no marker. Source markers are for Java only.

## Commands

| Command | Does |
|---|---|
| Spring Runtime: Refresh from Actuator | Reads the endpoints and reports the bean and property source counts and the endpoints not available |
| Spring Runtime: Clear | Drops the snapshot from memory and storage; stops this instance's polling lease and leaves the global autoRefresh permission unchanged |
| Spring Runtime: Inspect Cached Snapshot | Opens bounded read-only native observation/provenance details |
| Spring Runtime: Read JMX (localhost)… | Reviewed, read-only `getAttribute` snapshot of a local JVM started with `-Dcom.sun.management.jmxremote.port=N` (`authenticate=false`, `ssl=false`), including masked InputArguments, Memory, Threading, OperatingSystem and Spring Boot Health/Info MBean attributes; localhost only, port 1024–65535, bounded output, 15 s timeout, shown as a JMX row in the cached snapshot details; needs optional `process:java` |
| Spring Runtime: Cached Bean Dependency Graph | Opens the cached bean dependency graph |
| Spring Runtime: Refresh and Poll Controls | Opens the controls view |
| Spring Runtime: Start Bounded Scoped Polling / Stop Future Polls | Arms / disarms polling for the current scope |
| Tools → Configure Database from Runtime Snapshot | Creates an owned native connection from one exact captured `/env` property source |

The tool window has the same Refresh and Clear buttons, including Clear when a cache cannot be read.

Presentation is bounded to 5,000 rows and a 3 MiB tree message; limits are labelled partial and do not truncate saved evidence. Long labels/details/tooltips are marked truncated, and unbounded source identities are not retained as navigation targets. The plugin reads only the Spring Boot 2.x and 3.x Actuator JSON formats.

## Cached bean dependency graph

The graph loads only the existing project/Actuator-URL/auth-mode snapshot: no network request, source discovery during render, provider scan, CDN or graph library download. Nodes are exact `(context ID, bean ID)` identities from one retained `/beans` result. An arrow means only that this bean's retained string-array dependency list names a unique displayed bean in that same context and snapshot. Duplicate identities suppress every candidate in that group; a parent-context name, alias, uniquely matching type, mapping handler, health component, property source, logger or cache name does not establish another graph relationship. Unknown and ambiguous targets are counted, never invented as nodes or silently attached across contexts; known targets omitted by the node budget are counted separately.

Snapshots record whether each fetched dependency value was a JSON array of strings only. Older snapshots deserialize with this verification false: their observations remain intact, but outgoing edges are withheld until an explicit successful refresh establishes the typed list (a JSON boolean `true` is not proof of an edge to bean ID `true`).

Limits: full identity validation at most 4,096 retained bean records, then 128 displayed unique nodes, 512 edges and the first 64 dependency names per displayed bean; larger identity inventories withhold the graph rather than guessing uniqueness. Context/bean/dependency identities are limited to 512 bytes, type metadata to 4 KiB and scope text to 256 bytes. Cyclic edges within the displayed graph (including self-edges) are highlighted using bounded reachability; this is not cycle enumeration or a claim of runtime failure. Node labels are escaped, and click keys carry epoch/fetch/record fingerprints with scope/membership checks.

Selecting a node explicitly compares its exact reported Java type against a bounded current-project source inventory and shows a read-only cached bean observation with provenance and Back to Graph. Source/runtime correspondence is UNKNOWN: the graph does not prove that this project built the running application, that the current source matches the retained type, that a bean still exists, or that a reported dependency is a particular constructor/field injection. The dated comparison records the exact type observed in one source, ambiguous declarations, or **SOURCE INVENTORY DIFFERENCE** when the reported qualified type was not observed (which may be explained by library/generated/Kotlin/excluded sources). Selection performs at most 2,000 discovered files, 512 KiB/file, 8 MiB total and a cooperative three-second reading/parsing budget; discovery/read failure or any cap yields UNKNOWN, never an absence claim. Source comparison performs no HTTP, source edit or source navigation; select again to refresh it. Back, Clear, scope changes and successful refresh expire it. **Inspect Snapshot List** returns to the endpoint detail view.

## Snapshot details and inlays

**Inspect Cached Snapshot** opens a supporting native `Runtime Snapshot Details` tree with read-only Observation and Provenance tabs (in the shared window, right-click a row and choose **Show Details…**). It never fetches: it presents retained observations from all nine endpoint models, capped to 64 rows per endpoint (health/environment: 32), 512 total, 8 KiB per observation and 16 KiB provenance. Missing fields are explicit; source/condition ownership, current execution state and source/runtime build identity remain unknown. Environment/configprops property values are omitted from these panes; credentials are never shown. Endpoint, project, URL, auth mode, fetch timestamp/age, missing endpoints and last refresh error identify the evidence. Refresh, clear and scope changes expire selections; per-render keys prevent an old selection from silently addressing another row.

Activation of a source-linked bean, configuration-properties type or explicit mapping method opens only a current-project source candidate with the exact reported qualified name. An explicit bounded source scan must be complete; duplicate classes, missing/overloaded methods and capped/oversized source facts refuse navigation (no simple-name/suffix or first-hit fallback). Java files are capped at 512 KiB / 512 types / 4,096 members for this index; candidate paths must belong to the exact current root under the host's innermost project owner. Textual navigation is not proof that this source built the observed application; other endpoint rows have no guessed source target.

The `snapshotInlays` switch (default on) adds properties/YAML labels for exact retained `/env` keys: **snapshot: observed** or **snapshot: redacted**, with tooltips showing the property-source name and fetch provenance, never the value. Inlays also cover exact-key Java/Kotlin `@Value` placeholders at physical closing-string anchors, without presenting annotation defaults as observed values; foreign annotations, concatenations and incomplete source literals are withheld. Inlays read only the already-loaded scoped memory snapshot and perform no network, storage reload, discovery or source scan. Lookup is capped to 256 property sources / 8,192 retained pairs (larger snapshots suppress inlays) and the first 200 current-document keys / 4,096 bytes per key. Relaxed/environment aliases are not used, and the hint does not assert the current effective value, active profile or origin in this editor file.

## Refresh and Poll Controls

The controls view shows the exact owning project, requested Actuator base URL, auth mode, request/batch limits, poll status and cancellation boundary. **Refresh Once** remains explicit; rendering this view (or the main tree, graph, snapshot panes and editor providers) starts no HTTP reads.

Polling requires both `autoRefresh=true` and **Start Bounded Polling** for the current exact project/URL/auth-mode scope. A lease lives only in the current plugin instance, permits at most 1–10 attempts (default 5) no earlier than one minute apart, and expires for new attempts after 10 minutes. Two consecutive failures, native task cancellation, scope changes, Clear, Stop Future Polls or disabling the setting disarm it; a foreign project cannot stop another root's lease. A visible controls page drives a private 60-second timer; the driver validates frame/nonce, increasing sequence, root, current URL/auth mode and lease before any read (wrong frame, nonce, sequence or displayed root/URL/auth messages cannot drive a poll). Hiding or leaving clears the timer and requests Stop; a running request uses the native task Cancel. Reopening never transfers a lease to a new page; an instance accepts at most 64 distinct Start nonces before reclamation. All HTTP remains in Rust, which also checks current open-project membership before and during refresh.

Each batch retains the nine known endpoint order and byte limits. Timeout is clamped to the remaining cooperative wall-time budget, and deadline/scope/cancellation are checked at endpoint boundaries and before commit. Only successful scoped persistence adopts new data. **Stop Future Polls** revokes future attempts only; for an active refresh use the IDE background task's native **Cancel**, which interrupts the active HTTP future on the pinned host (it does not assert that a server immediately stops processing a request).

Requested URLs stay restricted to localhost/127.0.0.1 HTTP/HTTPS with none/bearer/basic auth through host-substituted `actuator-token`. **The redirect boundary is unresolved**: the pinned shared host client follows redirects before checking the final host, so the controls label the *requested* URL, not a verified responding peer.

## Observations: search and HTTP requests

- **Search Observations…** matches cached titles, endpoint categories and retained details without network access, shows the visible/retained row count, keeps the query while browsing the same snapshot scope and clears it on project/URL/auth scope changes. **Clear Search** restores the full list.
- The **HTTP Method** picker limits Observations to mappings with that explicit method in structured Actuator metadata (**All observations** restores every category). It never classifies a mapping by searching its predicate text; mappings with missing or unrestricted method metadata remain unavailable under a specific-method filter.
- **Copy HTTP Requests** (optional `clipboard`) copies an HTTP-client template for a selected mapping with explicit `requestMappingConditions` methods and patterns: each reported method/path pair, an editable application URL and reminders to fill path variables, headers, parameters and bodies. The application address is not inferred from the Actuator management address; no HTTP request is sent; predicate-only, unrestricted-method, malformed and legacy mappings without structured conditions do not invent GET requests.
- **Create HTTP Request File…** reviews the selected mapping's complete template, then creates a new plain `.http` file in the current project root and opens it (existing files are never replaced; a cancelled review creates nothing). Needs the optional `project.write` capability.
- **Send Local Request…**: choose a reported mapping, replace its path variables with a concrete localhost/127.0.0.1 URL, and review the exact method, URL, explicitly entered headers and UTF-8 body before sending. Headers and body each have an 8 KiB editing bound; transport headers such as Host and Content-Length are host-controlled. No Actuator credentials are reused automatically. The **HTTP Response** detail shows status, byte count and up to 64 KiB of plain response text (headers omitted); response data stays in memory and is retired with the observation scope. HTTP 4xx/5xx are shown as responses, not cached snapshot refresh failures.
- **Open GET in Browser…** is available only for an explicitly reported GET mapping. It asks for a concrete local application URL and reviews it before opening the normal browser (whose own cookies and authentication apply); it neither guesses an origin from the Actuator URL nor substitutes unresolved path patterns.
- Structured Actuator request conditions are retained as bounded review clauses: required/negated headers and parameters, consumed and produced media types. Literal positive values supply editable examples; query values are percent-encoded and a single concrete media type supplies a Content-Type/Accept example. Missing fields or negation flags stay labelled unreported/unsupported; negated conditions never become positive defaults; required unspecified header values use an explicit placeholder that must be replaced before sending. Body schemas are not inferred.
- **Related Beans…** on a bean observation chooses a reported injected bean or dependent in the same reported context. Only verified dependency arrays and unique context/name owners produce source candidates; source opening still requires one exact qualified current-project class (no library/parent-context or Java assignability semantics are invented).
- **Tools → Configure Database from Runtime Snapshot** creates an owned native connection from one exact captured `/env` property source. Supported targets are simple PostgreSQL/MySQL/MariaDB TCP JDBC URLs, including bracketed IPv6. The review shows project, capture time, source and target; masked/unresolved fields, URL options, embedded credentials and ambiguous identities require manual setup. Captured nonempty passwords can be stored directly in the native secure store without displaying them; target-bound IDs prevent changed host/database/user targets from inheriting a previous credential. This action does not connect, read schemas or execute SQL, and does not claim to clone the application's currently active connection.

## How it works

This is Actuator browsing, not the debugger's breakpoint/object identity model: no breakpoint/session evaluation, debugger panels or runtime resource navigation. Required capabilities: `project.read`, `storage`; optional: `net:localhost`, `net:127.0.0.1`, `secrets`, `clipboard`, `project.write`, `process:java`, `database.sources`. Denied optional grants degrade to cached browsing without fallback fetches.

Mounted Runtime and Bean Graph pages receive a model-free invalidation after snapshot Refresh/Clear/error and polling control changes; each page re-reads its own project/URL/auth-bound cached model, and no snapshot or credential is broadcast. Missing/denied/invalid current project scope returns an empty-state model instead of reusing another scope's graph.

## Not implemented

- Actuator endpoints behind other authentication than a bearer or Basic token (OAuth flows, client certificates).
- Applications on other hosts; TLS trust/OAuth/client-certificate support.
- SpEL in `@Value` (`#{…}`); starting or stopping the application; changing values at runtime.
- Verifying the responding peer across HTTP redirects.
- Application-instance or debugger-session identity; runtime source-origin/line resolution; live object evaluation.

## Development

```sh
cargo test --locked
node tests/controls-timer.mjs
openidea-plugin build
openidea-plugin link
```
