# Persistence core

Shared Rust contracts for source-owned persistence identities, provenance and reviewed edits. This crate has no host/SDK dependency and performs no discovery or runtime execution.

The initial consumers are JPA Assist's Designer and persistence-role generators. Source facts are distinct from compiler/provider facts; an incomplete positive inventory does not establish absence. Byte-based edits reject invalid UTF-8 boundaries and overlaps before the SDK adapter converts them to UTF-16.

`generation` defines validated class/identifier drafts, namespace/type-compatible
generation strategies and sequence/table generator parameters. These describe
requested source output, not a resolved persistence unit or database capability.

`SourceIdentity::is_confined` is only a lexical check for canonical slash-rooted paths; it is not host project-ownership proof or a symlink resolver. Consumers still verify current host ownership and document versions. Broader platform/module/schema/query contracts remain on the parity plan.

```sh
cargo test --manifest-path crates/persistence-core/Cargo.toml
```
