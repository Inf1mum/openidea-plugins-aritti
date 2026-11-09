# P17.2 configuration key anchor audit

Audited against the pinned API 1.3 / v0.1.39 host and the cached original Spring
Boot plugin 20211 decompilation on 2026-11-08. Status: **audited, pending owner-model
work**. This is feasible plugin-model work, not a demonstrated platform blocker.
There was no existing Boot DocumentIndexer or document-indexer contribution.
None is registered by this change. Completion, definition, diagnostics, hover,
inlay hints and code actions retain their existing owners.

## Original contracts inspected

- `SpringBootApplicationPropertiesPlaceholderReferenceResolver` finds effective
  configuration files, resolves property PSI keys to configuration declarations,
  and uses module/profile/model context. Equal strings alone are insufficient.
- `SpringBootApplicationYamlKeyReferenceProvider` resolves metadata access types,
  splits path/hint references and trims indexed access. It can use an empty PSI
  reference range. That cannot be copied into this host's index: v0.1.39
  `plugins/navigation.rs::valid_range` requires strictly nonempty anchors.
- `SpringBootApplicationPropertiesConfigKeyPathBeanPropertyRenameHandler` accepts
  BEAN_PROPERTY paths only after resolution to a BeanPropertyElement and a simple
  accessor method, then delegates to semantic BeanProperty rename.

Cached sources are under `/private/tmp/port/20211/dec/`, in modules
`intellij.spring.boot.core.ide` and `intellij.spring.boot.configYaml`. The package
and inspected version receipts remain in the repository's parity evidence.

## Current-source findings

| Source | Existing representation | Why an automatic rename anchor is suppressed |
|---|---|---|
| Properties keys | Decoded key plus original byte span | Escaping changes spelling; exact spelling alone still lacks a unique binding owner. |
| YAML mapping | Full dotted path plus last segment span | Replacing `max-size` with `app.pool.min-size` creates a different nested path. Parent/sequence tags are structural, not full-key tokens. |
| Java binding | `prefix` + dashed field/accessor name | `maxPoolSize` and `app.max-pool-size` are different source spellings, requiring semantic coordinated edits. |
| Relaxed forms | Canonical key comparison | `poolSize` and `pool-size` compare equal but cannot share unconditional same-text replacement. |
| Profiles/imported files | File recognition and location discovery | Base/profile/import/root context does not establish one effective owner; equal names can belong to separate projects or modules. |
| Metadata JSON | Explicit name and source type/method hints | Generated versus additional metadata, duplicate declarations and resolved Java owner are not proved. |
| Placeholders/environment | Textual completion and configuration input | Same-file definitions do not prove that Java/XML/other file references and external overrides form a closed rename set. |
| File/class navigation | Location targets, sometimes zero-width file positions | Navigation locations are not replaceable names and must not become index anchors. |

The Properties plugin already models property-bundle declarations and usages
through its private key index and providers (it also has no host DocumentIndexer
registration). A future Boot graph needs an explicit ownership contract with
those property semantics before introducing automatic rename sets. Boot's legacy definition provider remains the navigation fallback for its
currently supported class/resource targets; the later bounded current-file placeholder slice adds source-candidate Go to Definition
for simple standard properties references only. It does not resolve the effective
configuration context or publish rename anchors. See the plugin README for exact limits.

## Required next model

Establish project + module/configuration-context + declaration owner, distinguish
user-owned from framework/generated keys, and define Properties-plugin
co-ownership. Then publish only exact same-spelling, nonempty token references
whose complete rename set is known; reject escaped, derived, composite and
ambiguous cases. Fixtures must cover overlays/imports, sibling/nested projects,
metadata/Java declarations, placeholders and unsaved owner changes before enabling
automatic Rename. No synthetic file tags, blanket `config#name` keys, or
navigation-only spans should be exported meanwhile.

`src/anchor_audit.rs` records parser, profile/project, UTF-16, transformed-token
replacement and no-registration regression evidence. A separate verification
extracted the pinned host's `valid_range` function verbatim and checked five
range cases; it proves range shape only, not semantic ownership. No GUI or real
project refactoring was performed.
