# Properties compiler-backed declared-source graph and dependency profile

2026-11-11. Native integration: plugins/properties/src/compiler_graph.rs,
atomic.rs/lib.rs/plugin.toml/settings.schema.json. Helper and actual compiler
fixtures: plugins/properties/adapters. No host/API changes.

The explicit Inspect Compiler-Proved Java Bundle Usages command and lazy Change
key with compiler-proved Java contracts intention analyze complete physical
conventional Java roots (including ignored files), overlay all admitted unsaved
source buffers, bind canonical JDK ResourceBundle and canonical PropertyKey
contracts using actual JavacTask/Trees/Elements/Types, and preserve decoded
Unicode/Java escape identity using whole-literal UTF16 compiler spans.

Supported JDK origins: canonical single-string getBundle, constant bundle names,
final fields, final aliases and exactly bound effectively-final local variables.
All assignment/compound/unary writes are collected before resolving local aliases.
PropertyKey supports exact overloaded method/constructor/varargs parameter,
field/assignment/return/type-use contracts; missing explicit inherited contracts
are incomplete, never guessed. Class-backed/source-class/approved-JAR resource
bundle collisions and locale-shaped competing explicit basenames refuse Rename.
A physical default bundle and exact admitted physical sibling family are required.

Optional compilerClasspathProfile is an externally provisioned UTF8 plain profile:
PROPERTIES_CLASSPATH_V1 then exact absolute JAR paths, one per line. No Maven,
Gradle, application bootstrap, annotation processing or generation runs. Every
JAR is streamed into a private immutable snapshot before archive validation and
compiler use, preserving profile order. Original profile and JAR contents and
metadata are rechecked; SHA256 proofs plus exact paths are included in the final
bounded native review. Manifest Class-Path and archive indices cannot introduce
implicit dependencies. Runtime CLASSPATH is not consulted. Private copies retire
on normal exit; native cleanup empties at most128 known private JAR files if a
probe was killed (directory enumeration is cleanup, not absence proof).

Limits:400Java files/128KiB each/4MiB Java+resource source bytes;512dirs/8192entries/
4096entries per directory/depth24;200resource siblings;1024edges/256KiB output;
8s cooperative internal and15s external deadline. Profile64KiB;128JARs;128MiB/JAR;
512MiB compressed total;100000entries/64MiB expanded per entry/1GiB expanded total;
compression-ratio/path-traversal/implicit-classpath checks. Source, profile and JAR
reads use NOFOLLOW and before/after content/metadata guards.

Native Rename analyzes initial physical inventory, re-analyzes unsaved snapshots,
prepares exact source edits, re-runs/compares graph+physical inventories+all source
snapshots before review, and repeats after confirmation. Bundle snapshots are
rechecked after the final compiler run. Duplicate/collision keys, complete review
above16KiB, ownership changes, source/dependency drift and cancellation refuse.
Host applies the reviewed multi-file FileAction atomically with Undo. No automatic
provider starts a process; no unchecked generic Rename index anchor is published.

Verification:137 native Rust tests including actual Java source-launch result
consumed by production decoder and exact planner; source static System.exit(91)
never executes. Separate23 actual-JDK fixtures cover external annotation/API JAR
binding, effectively-final aliases, unsafe jar/profile cases and dependency bundle
collisions, plus earlier19 source/inventory/escape/lifecycle checks. Latest log
/tmp/properties-graph-classpath-tests.log (23pass15.112s under parallel fullbuild).
JDK26 source17/--release17 compatibility tested; no actual JDK17 runtime installed.

Current all-six integration receipt:
/tmp/non-laravel-parity-implementation/editor-complete-20261111/results.json
137Properties tests, releaseWasm, actual released API1.4 component dispatch and
full installer schema/assets all pass. Latest source-only clean Quarkus receipt
is /tmp/non-laravel-parity-implementation/editor-final-clean-20261111/results.json.

Concrete remaining implementation: Kotlin compiler/type contracts; directory and
modulepath dependency profiles; ownership of locale/Control/Module/custom loader
getBundle overloads; safe unique constant-key carrier propagation; inferred
inherited PropertyKey contracts; automatic live semantic references beyond the
manual source graph; reflection/framework conventions and other source-set/module
boundaries. These are not all host blockers. Native installed-host compiler
FileAction prompts/apply/undo and broader Java17/Windows profiles need separate
acceptance; Rust/Java/package tests alone do not prove those journeys.
