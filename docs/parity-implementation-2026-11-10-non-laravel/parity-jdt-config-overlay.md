# Quarkus/JDT unsaved configuration companion, 2026-11-11

Owned implementation: `plugins/quarkus/adapters/jdt-overlay/` only. The broker/profile/controller integration belongs to parent agent and is not modified here.

## Public interface evidence

Official LSP4MP source checkout `/tmp/parity-microprofile-ls-source`, actual extension `org.eclipse.lsp4mp.jdt.core.jar` version0.18.0.20260824-1311, JDT LS1.62.0, Java26.0.1.

- `IConfigSourceProvider` extension point is additive; adding an overlay at the same or a higher ordinal retains stale deleted keys in the original saved source. Therefore additive provider SPI alone cannot honestly close the gap.
- Exported `org.eclipse.lsp4mp.jdt.core.project` contains public `JDTMicroProfileProjectManager.getInstance().getJDTMicroProfileProject(IJavaProject)`, `JDTMicroProfileProject.evictConfigSourcesCache()` and mutable public `getConfigSources()`.
- Companion JDT command `openidea.configOverlay.synchronize` uses those public project/source APIs to replace an existing `PropertiesConfigSource` in its original list position, preserving URI/name/ordinal/profile. It uses standard JDT LS delegate-command extension registration, same `IDelegateCommandHandler` interface used by LSP4MP itself.
- Official project aggregate and expression-expansion implementation remains responsible for effective values. Entire source replacement handles key deletion correctly; original source is absent rather than shadowed.

## Files and contract

`src/openidea/quarkus/overlay/OverlaySource.java`: immutable Properties.load(Reader) snapshot source. Real Unicode editor text, Java escapes, line continuation, integer trimming and profile-file filtering. Source identity metadata preserved; no disk writes.

`OverlayCommand.java`: private snapshot validation, exact expected-generation argument, canonical identity and duplicate-path rejection, workspace confinement, bounded admission, official-source replacement, cache eviction on every barrier and rollback on installation exception. Empty snapshot restores previously touched disk sources.

`plugin.xml`, `META-INF/MANIFEST.MF`: bundle `openidea.quarkus.config.overlay`, version1.0.0; command `openidea.configOverlay.synchronize`.

System property: `-Dopenidea.configOverlay=/absolute/private/config-overlay.json` before Java's `-jar`.

Snapshot: `{version:1,rootUri,generation,documents:[{uri,version,text}]}`. Bounds:32 docs,512Ki UTF16 code units/document,8MiB JSON. Existing paths canonicalized with toRealPath; aliases to one physical file reject duplicate document entries. No external URI sources.

Command: JDT workspace/executeCommand `{command:"openidea.configOverlay.synchronize",arguments:[generation]}`. Exact generation required. Receipt `{generation,applied:[original snapshot URI strings],unsupported:[URI strings]}`; no content/value logging.

Parent must require every dirty source URI applied, serialize barrier with MPJava callbacks and reject/retry results when generation changes while in flight. Optional bundle must be included in JDT initializationOptions.bundles and hash-pinned in reviewed tool profile.

## Validation completed here

Command:
`python3 plugins/quarkus/adapters/jdt-overlay/build.py --jdtls /tmp/parity-qute-adapter-tools/jdtls --microprofile-jdt /tmp/parity-qute-adapter-tools/microprofile/extension/jars/org.eclipse.lsp4mp.jdt.core.jar --test`

Passed real javac --release21 against actual reviewed JDT/LSP4MP jars, Java assertions for removed-key absence, escapes, Unicode, continuation, integer conversion/trimming, unchanged expression text, source identity, profile filtering and information, immutable keyset, URI admission and canonical symlink identity. Built actual loadable Eclipse bundle with deterministic entry timestamps. Generated binaries/classes ignored by .gitignore; no binaries committed.

Final local jar: `plugins/quarkus/adapters/jdt-overlay/build/openidea.quarkus.config.overlay.jar`.
SHA256: `b621bdebdd315f72c1e4f2b7af40a7880b4398c3bb3f290f9c009135d200e4b1`.

Parent is performing actual broker fixture with unsaved configuration value7→42, deletion fallback and restore. Unit assertions alone are not proof that Eclipse bundle initialization or configured Java inlays work in a live project; parent must append real result separately.

## Precise admitted limitations

- Only existing official PropertiesConfigSource instances matching exact physical source paths are replaced. New/unimported config sources and provider-specific YAML/custom/external sources return unsupported; broker must withhold dirty-config MPJava assists for them.
- No new precedence rules or runtime environment/config source emulation. Fixed original ordinals/list ordering/profile semantics retained.
- Upstream resource/classpath import may evict caches after synchronize; public API has no atomic transaction combining barrier with every unrelated background consumer. Serialized broker callbacks plus post-result generation validation bound normal editor operations; no claim of universal background atomicity.
- Requires explicit user-installed reviewed JDT/LSP4MP profile and optional compiled bundle. No native host/API changes or project writes.
