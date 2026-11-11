# Version-aware Boot/Cloud metadata implementation — 2026-11-11

Owned files changed here: plugins/spring-boot/src/jar_metadata.rs; new plugins/spring-boot/src/jar_versions.rs (private nested module); README explicit metadata sections. Parent integrated jar State::bundled / version_scoped across lib.rs and live.rs and updated manifest descriptions; those integration files were not edited here.

Implemented source model:
- Root POM direct JAR versions accept literal values, exact root-property aliases (up to8 expansion steps, cycles/duplicates refused), and literal/property root dependencyManagement entries.
- Explicit importer derives official org.springframework.boot / org.springframework.cloud cached parent/imported BOM paths from declared coordinates. Reads only those paths, recursively bounded64 files,256KiB/file,2MiB combined,512 catalog candidates. No cache enumeration, Maven/Gradle/compiler/project execution, network or artifact extraction in product code.
- Each cached POM is strict namespace/wellformedness validated and its declared group/artifact/version must match the exact requested cache coordinate. Literal inherited parent version is supported. Current POM and every source POM digest revalidated after review and before installing metadata.
- Managed catalog entries are explicitly labeled dependency presence unverified, never effective Maven resolution or active dependency proof. Arbitrary inherited properties/profiles, Gradle resolution and alternate repositories remain unsupported.
- Up to16 artifact metadata snapshots/project coexist (8 project roots,32768 aggregate property limit). A successful import replaces only the same group/artifact. Conflicting retained key/hint facts refuse the new import; identical facts may coexist.
- Actual Boot/Cloud archives contain repeated normalized records across generated/additional metadata. Equal cross-file records deduplicate; same-document duplicates and cross-file conflicts still error. Existing ZIP integrity/size/CRC limits retained.
- State::version_scoped and ::bundled with parent lib/live integration exclude bundled3.5 fallback for observed official version-qualified selection and withhold completeness-dependent negatives. Positive facts expire after600s. Bounded version-awareness marker persists until source invalidation/reload so expiration does not revive3.5 fallback. Editor requests use retained/static data only, no POM/JAR re-read or process.

Meaningful regression coverage includes property alias/cycle/duplicate cases; root managed versions; declared Boot-parent+Cloud-BOM multi-GAV coexistence with explicit provenance; equal/generated/additional dedup versus conflict; public provider regression that server.port bundled key disappears from completion/hover, selected property remains available, unknown-property absence warning withheld; expiry retains fallback exclusion. Existing rejected archives/namespace/source/cancel/invalidation tests still pass.

Fresh verification:
- cargo test --locked --manifest-path plugins/spring-boot/Cargo.toml:172 passed,0 failed,1 ignored. Log /tmp/parity-boot-metadata-test.log.
- cargo build --locked --release --target wasm32-wasip2 --manifest-path plugins/spring-boot/Cargo.toml:pass. Log /tmp/parity-boot-metadata-build.log.
- Explicit ignored real-byte fixture invocation: SPRING_METADATA_REAL_FIXTURE=/tmp/spring-versioned-metadata-real cargo test --locked --manifest-path plugins/spring-boot/Cargo.toml real_versioned_boot_cloud_metadata_fixture -- --ignored --nocapture:1 passed. Log /tmp/spring-versioned-metadata-real/import-test.log.

Real disposable official-byte evidence (download performed outside product solely for fixture):
- Official Spring docs currently list stable Boot4.1.2 and state that Boot jars supply contextual configuration metadata: https://docs.spring.io/spring-boot/specification/configuration-metadata/index.html
- Exact source URLs,byte sizes,SHA256 for20 POMs and2 JARs: /tmp/spring-versioned-metadata-real/receipt.json. Temporary fetcher /tmp/fetch-spring-metadata-fixture.py. No downloaded artifacts committed to repo.
- Declared Boot parent4.1.2 + Cloud train2025.1.4 ->426 managed catalog entries from20 cached POM source snapshots.
- org.springframework.boot:spring-boot:4.1.2:105 normalized metadata properties; JAR SHA2561fbf6443a301eb9a098bd024149c0edc6578b073f75da8c98bc1af84c43a8f59.
- org.springframework.cloud:spring-cloud-context:5.0.4:30 normalized metadata properties; JAR SHA2563c19afc1934270689bb1a1f29c397b1e08b5fcf7c08d53ebea60e0ab89b5233a.
- Both retained concurrently; real spring.main.banner-mode and spring.cloud.refresh.on-restart.enabled available. No process/network requests from importer.

Limits: real archive bytes exercised through SDK MockHost importer, not desktop/native filesystem grant acceptance. Source catalog selection is incomplete effective runtime/classpath knowledge and does not claim latest-complete property inventory. No optional compiler/profile executor was necessary or added. No host/API changes.
