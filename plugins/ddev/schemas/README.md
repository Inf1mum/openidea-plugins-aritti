# Bundled DDEV schemas

Unmodified project and global configuration schemas from DDEV **v1.25.5**,
bundled for offline assistance. Apache-2.0; see [LICENSE-DDEV](LICENSE-DDEV).
These schemas are separate from the IntelliJ plugin source (BSD-3-Clause).

| File | Pinned upstream source | SHA-256 |
|---|---|---|
| `project-v1.25.5.json` | [pkg/ddevapp/schema.json](https://raw.githubusercontent.com/ddev/ddev/v1.25.5/pkg/ddevapp/schema.json) | `435f11701cf971b66d811f9ef0eb8ac23ffbd0df212a78a675323472792d6106` |
| `global-v1.25.5.json` | [pkg/globalconfig/schema.json](https://raw.githubusercontent.com/ddev/ddev/v1.25.5/pkg/globalconfig/schema.json) | `0a98eea7ae5a008bbc3577ffb4fd934bf779011bf1e363eef81d94a7a10c0cb1` |

The editor never downloads schemas or resolves external file/network references.
Update both files from an explicit DDEV release, update `configuration/schema.rs`
and this provenance, then run the configuration tests and Wasm build/check.
Schema warnings identify this pinned version; a newer DDEV may introduce valid
properties/values that this snapshot does not recognize.
