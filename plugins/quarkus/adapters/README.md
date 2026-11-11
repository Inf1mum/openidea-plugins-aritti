# Optional typed Qute adapter

This companion runs the official Qute language server and a private Eclipse JDT
language server, with the official Qute and MicroProfile JDT extensions. It
translates Qute's documented Java-model callbacks to JDT workspace commands.
It does not modify the OpenIDEA host/API or replace generic HTML/Java languages.

The plugin contributes **Qute HTML** only for `*.qute.html`. An exact project
file-type override can enable it for a conventional template such as
`src/main/resources/templates/Templates/hello.html`.

## Reviewed tool profile

The currently tested combination is:

- Quarkus VS Code extension 1.24.0-923: Qute LS/JDT 0.25.0.
- MicroProfile VS Code extension 0.18.0-844: LSP4MP JDT 0.18.0 and its bundles.
- Eclipse JDT LS 1.62.0-202610131109.
- Java 26.0.1, Python 3.14, macOS aarch64. The generator requires Java 21+ and
  Python 3.11+; other runtime/platform combinations need their own acceptance.

Exact official download URLs and archive SHA256 are in
[qualified-tools.json](qualified-tools.json). VSIX files are ZIP archives: extract
both and retain their `extension` directories. Extract the JDT distribution.
No extension is installed into VS Code. Keep the upstream licenses supplied in
those archives. The adapter downloads nothing and does not execute a shell.

Create a local profile from your explicit tool paths:

```sh
python3 adapters/create_profile.py \
  --java /absolute/path/to/java \
  --jdt-home /absolute/path/to/extracted/jdtls \
  --quarkus-extension /absolute/path/to/extracted/quarkus/extension \
  --microprofile-extension /absolute/path/to/extracted/microprofile/extension \
  --output /absolute/path/to/new/qute-profile.json
```

Run this from the Quarkus plugin directory, or use absolute script paths. Output
must be a new file; an existing profile is never overwritten. The generator checks
the selected versions and pins the Java executable and runtime version, complete selected JDT plugin
JAR inventory, Qute server, JDT extension bundles and configuration by hash. Hashes
bind the selected local artifacts; verify the archive hashes against the recorded
official artifacts before generating a profile. Changed tools refuse startup until
a newly reviewed profile is generated.

## OpenIDEA setup

Merge [openidea-settings.example.toml](openidea-settings.example.toml) into the
opted-in project's `.openidea/settings.toml`, replacing every placeholder path.
Keep existing settings. The custom server must serve **both `java` and
`qute-html` under the same server name and root**. OpenIDEA API1.4 reuses one
process per `(server name, workspace root, target)` across those language IDs.
The example disables the normal `jdtls` only in that project, since this broker
already runs JDT for Java. All ordinary Java requests route to JDT; Qute requests
route to Qute LS. This avoids two Java sessions with different unsaved text.

A separate broker configured only for templates cannot observe unsaved Java
buffers and must not be described as typed-parity integration. No generic
`*.html` association is installed. For a normal `.html` template, use the explicit
project-relative `files.fileTypeOverrides` entry shown in the example. The new
language retains HTML UI mode and tree-sitter grammar. Automatic framework-scoped
HTML reassociation and native desktop rendering are not claimed by this setup.

Use a canonical local workspace path, not a symlink alias. One local workspace
per broker is supported; project switching needs a new broker pair. Each pair
gets its own private JDT data/configuration directory under the explicit state
directory, so separate IDE windows do not share a locked JDT workspace. Owned
session files are removed after both children stop. Maven artifacts may be
resolved by JDT while it imports the selected project; the broker does not run
the project, its tests, a database or a development server.

## Lifecycle and verification

The broker waits for JDT `ServiceReady` before satisfying Qute model callbacks.
Java open/change/save/close and watched Java-file changes synchronize through a
JDT document-symbol barrier before Qute cached model/type facts are reset. Qute
requests wait behind that barrier. JDT `qute/dataModelChanged` events also pass
through. Server/client IDs, cancellation, watched notifications, errors and
shutdown remain distinct between the two backends. Frames, callback queues and
request lifetimes are bounded. Source content is not logged by the broker;
`QUTE_BROKER_TRACE=1` logs method names/IDs for diagnosis, and backend logs go to
stderr.

Run protocol checks without tools:

```sh
python3 -m unittest discover -s adapters -p 'test_*.py' -v
```

Run the opt-in actual-tool test:

```sh
QUTE_BROKER_PROFILE=/absolute/path/to/qute-profile.json \
QUTE_BROKER_RECEIPT_DIR=/absolute/path/to/receipt-directory \
python3 -m unittest discover -s adapters -p 'test_live_qute.py' -v
```

The actual test creates a disposable Maven Quarkus 3.40.1 project, checks inferred
`@CheckedTemplate` Java fields in Qute completion, changes a Java field **without
saving**, verifies the replacement field and absence of the old field, restores
and closes the buffer, and checks disk edit/restore through watched-file events.
It also requires clean shutdown. It never edits the user's settings or project.
Tool-level stdio acceptance does not establish native desktop acceptance or all
original-plugin features.

Remaining adapter acceptance includes multi-module/Gradle imports, classpath
changes, inherited/generic/custom-extension models, all editor refactoring and
client-only command flows, source/library navigation, and native UI acceptance.
The bridge canonicalizes repeated path separators in conventional physical-source
model paths, fixing the upstream 0.25.0 explicitly empty `basePath` double-slash
case observed during probing. Both default nested paths and explicitly empty
base paths run the actual lifecycle test. Custom locator IDs and traversal paths
are never rewritten.

## Optional MicroProfile and Quarkus configuration model

The same broker can additionally run official **LSP4MP LS 0.18.0** with the
**Quarkus LS 0.25.0** extension. Select `--with-microprofile` when creating the
profile. It shares the single JDT session and implements all 15 custom
MicroProfile Java/project callbacks by forwarding their exact supported JDT
workspace commands. Configuration completion, validation, hover, navigation and
completion resolution use the official language server. Java hover, definition,
actions, lenses and inlays merge the official MicroProfile and ordinary JDT
results; opaque resolve data returns to its originating backend. Java configuration
key completion also remains available from the plugin's local source provider.

Add `quarkus-properties` to the custom server's `languages` list and explicitly
associate the project's configuration file in `files.fileTypeOverrides`:

```toml
[files.fileTypeOverrides]
"src/main/resources/application.properties" = "quarkus-properties"
```

The contributed alias has no automatic suffix ownership. Generic Properties,
HTML and Java language associations remain unchanged. This adapter admits
`application*.properties` and `microprofile-config.properties`; YAML continues
through the plugin's existing source provider and is not routed to this LS.

For current unsaved configuration values in Java semantic assistance, build the
companion public-JDT extension and select its output explicitly:

```sh
python3 adapters/jdt-overlay/build.py --jdtls /absolute/jdtls \
  --microprofile-jdt /absolute/microprofile/extension/jars/org.eclipse.lsp4mp.jdt.core.jar --test
python3 adapters/create_profile.py --java /absolute/java \
  --jdt-home /absolute/jdtls --quarkus-extension /absolute/quarkus/extension \
  --microprofile-extension /absolute/microprofile/extension --with-microprofile \
  --config-overlay-bundle "$PWD/adapters/jdt-overlay/build/openidea.quarkus.config.overlay.jar" \
  --output /absolute/new-microprofile-profile.json
```

The profile pins the companion JAR as well as official tools. The broker sends
bounded UTF-16-aware buffer snapshots to a private session file, synchronizes the
exact generation immediately before each MP Java callback, requires coverage of
all dirty configuration sources, and rejects results from superseded snapshots.
The companion replaces existing official properties sources with current text,
preserving source identity, precedence and profile rules. Closing/saving restores
the official disk-backed sources. No project files are written. Existing official
source recognition is required: new/unrecognized files, external sources, YAML,
ambiguous aliases or oversized/invalid snapshots cannot supply these Java facts.
Without the companion, Java MP assistance is withheld while config buffers are
dirty; ordinary Java and configuration editor assistance remain available.
See [the companion contract](jdt-overlay/README.md) for precise cache/concurrency
limits. MP Java diagnostics are conservatively suppressed while configuration is
dirty because unsolicited upstream diagnostics have no snapshot-generation tag.

Children receive a small OS/runtime environment by default. Original MP completion
includes environment variables; explicitly opt into additional names with repeated
`--inherit-environment NAME` profile flags. Avoid passing secrets. Review backend
stderr before sharing it; official servers may log project content.

Enable settings under `microprofile.tools`, including
`validation.enabled`, `inlayHint.enabled`, `inlayHint.converters.enabled` and
`inlayHint.defaultValues.enabled`. The example settings contain this optional
configuration. Existing `qute` and `java` settings continue to work.

```sh
MICROPROFILE_BROKER_PROFILE=/absolute/new-microprofile-profile.json \
MICROPROFILE_BROKER_RECEIPT_DIR=/absolute/receipt-directory \
python3 -m unittest discover -s adapters -p test_live_microprofile.py -v
```

The actual fixture checks source-derived mapping keys, unsaved Java mapping
rename/restore, unsaved properties converter diagnostics/fix, Java semantic
hover/inlays and resolve routing. With the companion it additionally checks Java
inlays from unsaved configuration, deleted-key removal and disk restoration on
close. Receipts contain selected fixture facts, not complete environment-variable
completion candidates. Multi-module/Gradle, source/library refactoring, all
configuration providers, background upstream cache races and native desktop
acceptance remain additional acceptance work.
