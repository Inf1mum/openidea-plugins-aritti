# Optional Spring Batch CLI adapter — 2026-11-11

## Implemented surface

New `src/batch_runtime.rs`, owned `Runtime` state, five contributed commands and four runtime settings. Parent wired the module into `Structure::run_command`; no host/API changes or extra native view were introduced.

- Profiles: **disabled** by default, or exact **runner-5.2.6** / **operator-6.0.6**. Core artifacts are pinned by official Maven SHA256, manifest version, filename and launcher entry.
- Explicit project-relative classpath directories/JARs (1–32 entries, no wildcard/parent traversal/foreign project). Java/Kotlin source must declare the exact configured top-level JVM context. Runner additionally admits an exact XML classpath resource with identical source/resource bytes. No implicit build, dependency installation or framework-version guess occurs in the plugin.
- `batch.runtime-validate` probes actual bounded artifacts without loading the application context. `BatchRuntimeProbe.java` checks core identity, one launcher/context, compiled binary name/version, canonical project confinement, duplicate physical entries and JVM-option contamination. Manifest-added classpaths are refused. Runtime-version-aware JAR access matches effective multi-release context entries. Length-framed SHA256 binds directory resource names and contents, JARs, source and compiled context; directory enumeration is capped before sorting.
- `batch.runtime-operate` accepts the documented exact operations: Runner start/next/stop/restart/abandon; Operator start/startNextInstance/stop/restart/abandon/recover. Operator accepts parameters only for start, avoiding ignored arguments. Execution IDs are positive decimal values. Names and parameter values retain native argv boundaries, whitespace and Unicode; Runner option prefixes/control characters are refused.
- Every launch shows cwd, actual argv, context source and compiled/classpath identity, explicitly states startup/datasource and build-provenance limits, then reloads source/settings and reprobes before `host::start`.
- Up to four owned CLI sessions retain root/profile/context/operation identity; at most three launch operations reserve a control slot. A running start can receive a separate stop request without losing its process handle.
- `batch.runtime-poll` drains all owned sessions into the native Output channel. The display payload is capped at256 KiB per session with one omission notice; business execution is **not killed** by a display cap, and exit polling/control remain available.
- `batch.runtime-cancel` explicitly ends one selected owned CLI process. It is not a Batch stop request. A detected project switch ends old owned CLI processes with a visible notice before accepting the new project's runtime command; old output is not attributed to the new project.
- `batch.runtime-inspect` separately reviews context startup and queries an exact execution through **JobExplorer.getJobExecution(Long)** for Runner5 or **JobRepository.getJobExecution(long)** for Operator6. Original `BatchRuntimeQuery.java` reports typed execution/instance/job/status/start/end/observation metadata; missing results remain local to the selected reader. No automatic query occurs during typing or normal output polling, and no generic Actuator endpoint is assumed.
- Helper sources are embedded with `include_str!`, materialized as content-hash-named Java source files in the plugin's private `fs::plugin_dir()`, and reread/verified before use. They are not presumed to be installed package resources. The only new optional capability is `process:java`; the private data-directory API provides helper read/write access.

## Public contracts and exact profile sources

- https://docs.spring.io/spring-batch/docs/current/api/org/springframework/batch/core/launch/support/CommandLineJobRunner.html
- https://github.com/spring-projects/spring-batch/blob/v6.0.6/spring-batch-core/src/main/java/org/springframework/batch/core/launch/support/CommandLineJobOperator.java
- https://github.com/spring-projects/spring-batch/blob/v5.2.6/spring-batch-core/src/main/java/org/springframework/batch/core/explore/JobExplorer.java
- https://github.com/spring-projects/spring-batch/blob/v6.0.6/spring-batch-core/src/main/java/org/springframework/batch/core/repository/JobRepository.java
- Official Maven core5.2.6 SHA256: `283db480ba39f1808c766efd314e070090de055915f219523995bf1aa3c58d7c`.
- Official Maven core6.0.6 SHA256: `2b043159df359d4ef29952131db58fc2bc6c74b333c4202c97d3a8d80a21a19f`.

The exact6.0.6 artifact and CLI switch were freshly verified;6.0.5 search-cache results were not treated as the latest release. `javap` against both actual pinned artifacts additionally confirmed the typed repository-reader signatures and metadata getters. No framework or proprietary IDE implementation was copied into these helpers.

## Repeatable actual qualification

Run `python3 plugins/spring-integration/tests/runtime-cli/verify.py` with Java17+ and Maven3.9+. This always creates a new **/tmp** root, never reads an existing project's settings or datasource configuration, downloads pinned official Maven artifacts as needed and retains logs/receipts for inspection. Original fixture Java templates and POM generator are tracked; jars, build outputs and H2 state are not.

Latest full successful receipt: **`/tmp/openidea-batch-cli-ty5vrita/receipt.json`**.

| Actual check | Result |
|---|---|
| Runner5.2.6, source-mode private/hash-named probe, Java context, real in-memory job | Pass, CLI exit0 and completion marker |
| Runner5.2.6 XML context/resource byte identity and real XML-launched job | Pass |
| Operator6.0.6 exact artifact and real in-memory job | Pass, CLI exit0/COMPLETED |
| Modified core artifact with unchanged manifest/version | Rejected by pinned SHA256 |
| Wrong binary class name at selected classfile path | Rejected |
| Manifest Class-Path adding an undeclared dependency | Rejected |
| Multi-release application JAR with invalid base entry and valid version17 context | Actual runtime-selected hash and actual job launch pass |
| Disposable Runner/H2 file repository: start→separate CLI stop | CLI acknowledgement exit0, independently observed SQL STOPPED |
| Same repository: restart stopped execution | CLI exit0, independently observed SQL COMPLETED |
| Original typed execution helper on stopped/completed records | STOPPED/COMPLETED observed through actual JobExplorer |
| Typed missing execution | Runner missing response and Operator empty in-memory repository response pass |
| Artifact identity across state-changing operations | Unchanged; H2 state is outside classpath directory |

H2 uses only its generated temporary database and may open a local AUTO_SERVER listener. Fixture contexts hold one connection to keep the server lifetime stable. A successful stop acknowledgement alone is never credited as STOPPED; SQL and typed metadata observations provide separate evidence.

## Automated Rust/build checks

- Thirteen runtime tests plus Windows/UNC separator coverage are present (the latest whole-package suite has **169 passing tests**, including parallel parent additions).
- Tests cover unavailable/malformed profiles, source identity, Java/Kotlin/XML admission, native argv boundaries, stale reprobe, denied execution, review cancellation, process-vs-job stop semantics, concurrent start/control ownership, project switch cleanup, bounded output without process termination, private helper materialization, operator recover/no ignored parameters and strict typed inspection identities/status/encoding.
- The backend seam preserves production `host::start`/Child behavior; MockHost managed Child does not execute process handlers, so tests inject a controlled launcher for proof/state transitions rather than claiming MockHost runs Java. The real Java/Maven/H2 harness supplies the actual tool integration evidence.
- `cargo test --locked --manifest-path plugins/spring-integration/Cargo.toml`: pass, log `/tmp/batch-cli-all-test.log`.
- `cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/spring-integration/Cargo.toml`: pass, log `/tmp/batch-cli-wasm.log`.
- Source whitespace checks pass.

## Precise residual boundaries

- Native OpenIDEA desktop command/prompt/output/permission acceptance remains unperformed by this worker. Host-managed lifecycle behavior is covered by Rust tests and the actual OS tool contract, not by a fresh native session.
- Positive persisted cross-process stop/restart/query acceptance is qualified for the Runner5.2.6 H2 fixture. Operator6.0.6 real start and empty-reader behavior pass, but its positive persisted stop/restart/recover/abandon/next scenarios are not all exercised. Kotlin source admission has a Rust regression; no actual Kotlin compilation fixture is claimed.
- Existing profiles intentionally reject other/patched core artifacts; a new distribution needs its own manifest/ABI/artifact/fixture qualification. Windows/UNC separator logic is tested, but no real Windows execution is claimed.
- Hashes identify reviewed compiled artifacts and their stability. They do not establish source-to-bytecode build provenance, effective datasource identity, job reachability or arbitrary application behavior. Startup is explicitly reviewed because opening a context can itself perform configured actions.
- Listing all job histories, live scheduling, runtime debugger inspection, native Run Dashboard integration and other broader original capability families are not silently claimed by these commands. No whole-plugin parity ledger task was promoted from this finite delivery.

## Files to retain

New source: `src/batch_runtime.rs`, `runtime/BatchRuntimeProbe.java`, `runtime/BatchRuntimeQuery.java`, and `tests/runtime-cli/*`. The helper sources are compile-time inputs to the Wasm; no ad-hoc package-resource declaration is required. `Cargo.toml`/`Cargo.lock` add sha2, settings preserve the parent's XML compatibility selector, manifest adds five commands and optional approved Java execution. Existing Laravel and shared crates were not edited by this worker.
