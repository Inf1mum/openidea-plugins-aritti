# Boot owned native runs — 2026-11-11

Implementation: `plugins/spring-boot/src/run_profiles.rs`, plus README section. Parent integrated `mod run_profiles`, macro `run_configurations`, manifest `contributes.runConfigurations` marker files, opt-in `springBoot.nativeRunProfiles=false`, optional process:mvn/process:gradle grants.

Released contract examined: SDK v0.1.40 (37c2a604) execution.rs PluginRunConfiguration/RunConfigurations and native host plugins/run_detectors.rs. The provider returns argv/cwd; discovery never executes process or accesses network. Native host requires approved/pinned Process(program), owns start/stop/rerun/logs and marks these non-debug configurations. No manual saved configurations are modified.

Source admission: bounded128KiB root POM or one root build.gradle/.kts, registered project identity, explicit opt-in. Maven requires exactly one direct build/plugins declaration of canonical Boot plugin; pluginManagement/profile/nested/foreign/malformed/DTD/entity/namespace borrowing and duplicate coordinate/version cases do not establish it. Full QuickXML0.42 namespace validation includes unbound attributes/reserved namespaces, complete roots and nesting limits. Gradle requires exactly one direct applied literal Boot plugin and source Java/java-library/application/org.jetbrains.kotlin.jvm plugin; Kotlin java/application/kotlin("jvm") accessors only in .kts without local/import alias shadow. Inactive/dynamic/ambiguous/alias/convention plugins and slashy-string ambiguities withheld. Both Gradle files simultaneously withhold Gradle because no unique build owner. Maven and Gradle source profiles may coexist as separately named native choices.

Emitted commands (no shell/project wrapper):
- mvn --batch-mode --file pom.xml spring-boot:run
- gradle --console=plain bootRun

Commands run project build lifecycle/application only after user invokes native Run. Optional process approvals pin the actual native executable. App/JVM/profile configuration stays in project build. Discovery reflects saved source at refresh; no effective parent/plugin/classpath/main-class proof is claimed. Source changes/setting opt-out withdraw configurations on native refresh. No compiler/run work in editor, no native debugger attachment, no fake debug/test configuration.

Regression tests cover opt-in/withdrawal/root ownership, no process/network side effects, exact argv/cwd/non-debug data, Maven namespace/malformed/duplicate/pluginManagement exclusions, Gradle enabled JVM plugin/accessors vs aliases/foreign source/inactive/missing Java/dynamic/helper shadow/malformed imports/nested/slashy forms. Actual package focused2 tests pass, full locked package and release wasm32-wasip2 outputs in `/tmp/parity-boot-run-test.log` and `/tmp/parity-boot-run-build.log` (latest counts in logs; package changes may be concurrent from parent).

Real disposable fixtures: `/tmp/parity-boot-native-run-fixture` Maven; `/tmp/parity-boot-native-gradle-fixture` Gradle. Both use pinned Spring Boot4.0.0, a real @SpringBootApplication with WebApplicationType.NONE, start SpringApplication then close context, print BOOT_NATIVE_RUN_FIXTURE_OK. Exact emitted argv executed from each root, both exit0. Maven system binary /opt/homebrew/bin/mvn, Gradle cached9.8.0 distribution made available on fixture PATH (not a wrapper or installation). Gradle not initially on standard host PATH, so actual user native launch requires available approved Gradle binary. Logs `/tmp/parity-boot-native-run-maven.log` (BUILD SUCCESS) and `/tmp/parity-boot-native-run-gradle.log` (BUILD SUCCESSFUL), hashed fixture receipts `/tmp/parity-boot-run-fixture-receipt.json`. These demonstrate real Maven/Gradle command behavior, not desktop Run UI acceptance. Parent retains native exact-artifact checks.

Official latest documentation verified public run contract (currently docs4.1.2):
https://docs.spring.io/spring-boot/maven-plugin/run.html
https://docs.spring.io/spring-boot/gradle-plugin/running.html
Fixture4.0.0 is an explicitly pinned compatibility fixture, not a claim of latest Boot version.

Residuals: custom args/UI editor, arbitrary inherited/convention/alias build plugin resolution, multi-module run root selection, runtime active profile/environment/main class facts, host Java debug support. These are separated from the supplied explicit owned run profiles; broad original dedicated Spring run configuration parity is not claimed.
