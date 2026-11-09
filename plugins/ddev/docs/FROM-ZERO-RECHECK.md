# Fresh source review — 2026-11-09

Baseline: original DDEV Integration **1.2.10**, stable commit `7291f3ea58cf937ae8c6ac7973ed24876915e308`.
A fresh fetch confirms original main `1c5443127c38e392c3c4fc859f381dc758692160`.
Target: OpenIDEA API **1.4**, SDK **v0.1.40**. No SDK upgrade or multi-project
Services dashboard was introduced.

This review started from the source descriptors, Java packages and feature wiki,
then inspected the implementing paths in the port. Passing previous tests was
not treated as proof of a behavioral match. Counts below inventory files, not
executed Java tests or line/branch coverage.

| Original family (main Java files) | Port paths / review result |
|---|---|
| Root initialization/listener contracts (7) | `lib.rs`, `runtime.rs`: project activation, state refresh and integration dispatch; serialized host callbacks replace the message bus |
| Actions (20) | `lib.rs`, `runtime.rs`, manifest: lifecycle argv and availability, installation/help, updates, recovery; confirmed reconfiguration and destructive-action prompts are deliberate adaptations |
| CLI/model/parser/WSL (25) | `model.rs`, `execution.rs`, `lib.rs`: JSON envelopes, error extraction, version parsing, raw argv, command completion and executable alias; local UNC/WSL patch remains platform-specific |
| Database (6), managed index (4) | `database.rs`: owned source identity, changed/unchanged updates, secure credential, port/driver validation, connection requests and legacy-file preservation; no JetBrains/JDBC metadata migration |
| Compose (5), PHP (15), Node (4) | `execution.rs`, PHP hints in `lib.rs`: host-selected mapped run target and explicit tools. Generic Composer hooks/root/cwd and pre-launch validation remain incomplete (#2407/#2408); no JetBrains SDK/server objects or global PHP-level query |
| Service actions (3), status bar (2) | `services.rs`, `model.rs`, `runtime.rs`: HTTPS-first links, running-state guards, current-project tree/detail/actions and window status. Exact popup layout and visibility differ |
| Settings (3) | `support.rs`, manifest: project Save/Reset, watch/update/Database preferences, executable choice via host and unified target offer. Independent PHP/Node selection belongs to host settings |
| State (10) | `lib.rs`, `runtime.rs`: configuration existence, description failures, recovery guards and events; host minimum polling is 60 seconds versus original 10 |
| Notifications (2) | `support.rs`: **fixed** changing error text causing repeated warnings during one outage. Original failure latch now informs port behavior; project-targeted background notifications need API 1.5; delayed unknown-state warning remains absent |
| Terminal (3), two platform-specific adapters | `execution.rs`: keyed native terminal, environment, exit/close handling. Native shell action replaces original terminal profile; typed-command interception and plugin-owned PTY lifecycle remain unavailable |
| Tutorial (2), dependency utilities (3) | `support.rs`: guided Setup, first-use notice and permission guidance replace anchored Got-It/dependency dialogs |
| Version (8) | `version.rs`, `lib.rs`, `runtime.rs`: numeric/HEAD/RC comparison, latest stable release, explicit check and disabling automatic checks. Automatic checks are throttled daily; explicit checks remain usable when disabled |
| Icons (1), error reporting (3) | Licensed package/provider assets and explicit issue link; automatic Sentry reporting intentionally omitted |

Inventory total: **126 main Java files**, plus **two terminal compatibility
adapters**. All **12 main resources**, including the main and five optional XML
descriptors, were inventoried. API-specific extensions are assessed as behavior,
not claimed as identical host objects.

The feature wiki's configuration assistance is inherited from IntelliJ rather
than implemented in its Java extension list. The port supplies offline completion,
hover and diagnostics itself. Both bundled DDEV v1.25.5 schemas retain their
recorded SHA-256 checksums. Root/nested/global values, quoting, conditional database
versions, path scope and bounded invalid input are exercised by native/Wasm tests.
Pinned schema versions and individual override-file validation remain limitations.

The new notification regression first reproduced **three alerts instead of one**.
It now checks changing reasons within an outage, optional-storage denial, idle
reclaim, compatibility with old fingerprint markers and re-arming after a successful
read. Current command errors remain available even when repeat notices are suppressed.

Fresh checks passed: **107 native tests**, strict Clippy, formatting, release
Wasm/package check, **27 command workflows** through the compiled component,
**105 conditions** through the pinned host evaluator, and actual-Wasm editor
providers within 50 ms. These use fixture/mock hosts. This pass adds no live
desktop/container, secure Database connection, debugger or remote/Windows evidence.

The [main audit](../SOURCE-AUDIT.md) retains detailed source mappings and previous
native desktop evidence. Full runtime parity is not established by this review.
