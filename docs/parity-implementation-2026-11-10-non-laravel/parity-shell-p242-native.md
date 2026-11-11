# Shell P24.2 native occurrence delivery — 2026-11-11

## Released API qualification

Reviewed exact released OpenIDEA v0.1.40 / commit37c2a6048eea89d4f6db645622612358b643862b:

- sdk/rust/openidea-plugin/src/indexing.rs: DocumentIndexer returns Edges; Anchor has key/range only. There is no read-only mode, rename opt-out, replacement encoding or new-name-validation callback.
- crates/ide-host/src/ops/lsp.rs::lsp_prepare_rename / lsp_rename / merge_indexed_rename: indexed strings participate in Rename; merge_indexed_rename inserts new_name literally at every indexed range. Native preview and source preconditions do not validate Java/Kotlin string encoding, Shell name grammar or registry collisions.
- sdk/rust/openidea-plugin/src/language_features.rs: document_highlights and code_lenses are released read-only native language surfaces.

Therefore exporting raw Shell string anchors through DocumentIndexer would enable unvalidated automatic edits. This delivery implements the requested fallback: native read-only local occurrences and existing reviewed edits. It does not claim global native Find Usages or export an unsafe Rename graph. A rename-policy/read-only indexing hook would require host/API work excluded by the user.

## Implemented

- native_occurrences.rs implements LanguageFeatures::document_highlights over the existing source-owned current-class key/availability graph.
- Separate explicit aliases, command-side availability indicator method names, selector references and implicit command identities preserve their existing ownership. Implicit method spellings can differ from command spellings; highlights show the exact distinct physical ranges rather than suggesting raw Rename is safe.
- Registered native code lenses show local occurrence counts. Each opens the existing read-only occurrence picker for its own definition independent of caret location.
- Lens arguments are opaque tokens into bounded plugin-held snapshots (eight document snapshots;64 definitions per snapshot). Invocation rechecks root, path, text, version and definition key/range; stale/forged/restarted contexts refuse.
- Existing reviewed FileActions retain grammar, collision, source-snapshot and confirmation checks. No global text replacement or document-indexer registration was added.
- Manifest registers document-highlights/code-lenses and language_features export; README/change-notes describe the exact scope.

## Tests

native_occurrences::tests:
- aliases vs command selectors vs indicator names remain separate and exact source ranges;
- lens identity independent of caret, correct picker destination, stale source/refreshed session refusal;
- escaped strings, foreign ownership, inherited and ambiguous scopes stay quiet.

native_occurrences::kotlin_identity_tests:
- implicit Kotlin runTask / run-task highlight exact different physical ranges after Unicode/CRLF source;
- lens tokens do not cross paths or project ownership.

Existing ownership, FQN/import/typealias guards and reviewed-edit tests also pass.

Latest verification:94 Shell tests passed; fresh release Wasm build passed; scoped git diff --check passed. Logs /tmp/parity-shell-test.log and /tmp/parity-shell-build.log. No desktop acceptance or global Find Usages claim.

## Residual

Native global Find Usages cannot safely be enabled through this API without exposing automatic string Rename. The supported alternative is local native highlights/lenses plus the explicit occurrence picker and reviewed key editing. Complete runtime/inherited command ownership and cross-file registry semantics remain separate plugin/adapter work.
