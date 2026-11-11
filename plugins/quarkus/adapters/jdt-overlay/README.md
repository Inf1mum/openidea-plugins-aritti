# Optional JDT configuration buffer overlay

This companion bundle lets the Quarkus broker supply currently edited Java-properties
configuration to the official LSP4MP JDT extension. It does not write to the project.
It replaces an existing official `PropertiesConfigSource` in the project's mutable
public source list, preserving its original URI, name, profile, ordinal and list
position. Removing a key removes that source's contribution: lower-priority real
sources can then provide the value. Expressions and source aggregation still use
LSP4MP's own implementation. An additive higher-priority config-source provider
would incorrectly resurrect deleted keys from the saved source, so is not used.

Build against your reviewed tool installation (Java 21 or newer):

```sh
python3 build.py --jdtls /absolute/jdtls \
  --microprofile-jdt /absolute/org.eclipse.lsp4mp.jdt.core.jar --test
```

The output is `build/openidea.quarkus.config.overlay.jar` (bundle
`openidea.quarkus.config.overlay`, version `1.0.0`). Add it to JDT LS's initialization
`bundles` and pin its SHA-256 along with the other profile artifacts. Launch JDT LS
with `-Dopenidea.configOverlay=/absolute/private/config-overlay.json` before `-jar`.
The broker must own this private file and update it atomically; it contains editor
text and must not be placed inside the project or published as a diagnostic receipt.
The parent adapter profile generator supplies these settings when this bundle is
explicitly selected.

Snapshot protocol:

```json
{"version":1,"rootUri":"file:///absolute/project","generation":12,
 "documents":[{"uri":"file:///absolute/project/src/main/resources/application.properties",
               "version":3,"text":"greeting.message=edited\n"}]}
```

Only local `file:` URIs beneath the reviewed real root are accepted. Bounds are
32 documents, 512 Ki UTF-16 code units of text per document and 8 MiB total JSON.
JDT's existing source URI must match the normalized document path. Existing
symlinks escaping the reviewed root are rejected. New/unrecognized source files,
YAML, arbitrary external config locations and sources from unknown providers are
reported unsupported, never given guessed precedence.

Immediately before an opaque MP Java semantic operation, call JDT LS's
`workspace/executeCommand` with
`{"command":"openidea.configOverlay.synchronize","arguments":[12]}`.
The generation argument must match the atomic snapshot exactly and may not go
backwards. A successful receipt is
`{"generation":12,"applied":["file:///..."],"unsupported":[]}`.
The broker must require coverage of every dirty configuration URI and reject any
semantic result if the generation changes while it is in flight. Synchronize an
empty document list after the last buffer closes to restore disk-backed sources.
Every synchronization evicts both source and aggregate/expression caches; it also
restores projects touched by earlier generations before installing replacements.
Upstream cache eviction from a concurrent import/resource event can still happen
after the command returns. This is a barrier for serialized broker requests, not
an upstream atomic transaction or a guarantee for unrelated JDT background work.

Qualified compilation: JDT LS 1.62.0, LSP4MP JDT 0.18.0, Java 26.0.1.
The source parser uses `Properties.load(Reader)` because the broker has already
decoded the editor's current Unicode text; Java escapes, continuation and
profile-specific file filtering match the official properties-source behavior.
Sources retain the official fixed ordinals; this companion does not introduce
new runtime config sources or claim runtime-only configuration behavior.

## YAML, .env, config/ and new files

The broker flattens YAML (`config_sources.py`: literal single-document block mappings; `.` joined keys, `[n]` list indexes, quoted keys kept quoted, quoted top-level `"%profile"` keys, `~` null key; anchors/aliases/tags/flow/block scalars/tabs/multi-document refused) and `.env` into Properties text and adds `format`, `ordinal`, `profile` to each snapshot document. Sources LSP4MP does not model (YAML, `.env`, new unsaved files) are inserted by descending ordinal: `.env` 295, `config/application.properties` 260, classpath `application.properties` 250 (config-reference); YAML 255 and `config/application.yaml` 265 (the YAML guide only fixes YAML over properties and config/ over classpath). `.env` keys are matched by environment-name conversion (`%dev.a.b` is `_DEV_A_B`). Only dirty buffers are added; a saved YAML or `.env` file is not an LSP4MP source, so unsaved edits show while saved ones do not. List scalars are emitted as `key[n]` only (no comma-joined `key`).
