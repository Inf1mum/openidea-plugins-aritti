# Experimental native Rust call analysis on macOS

This source-built profile is **`osv-scanner-macho2.7.1-openidea-macho.1`**.
It is a separately modified Apache-2.0 OSV-Scanner2.7.1 adapter, qualified on
Darwin arm64 with Go1.27.4 and rustc1.99.1. The unchanged official2.7.1 Rust
profile cannot parse this platform's MachO binaries. Other platforms/toolchains
remain unqualified; Go analysis uses the separate unchanged official profile.

Run the reproducible builder explicitly in a new disposable directory:

```sh
python3 build.py --work-dir /tmp/my-new-macho-build --go /absolute/path/to/go
```

The builder verifies the pinned official source archive and patch SHA256,
checks downloaded Go modules against upstream go.sum, applies the patch, runs
reader regressions and builds a separately named executable. It does not install
anything globally or replace the official scanner. Approve the resulting
`osv-scanner-macho` executable in OpenIDEA before choosing its separate profile.
`source.json` records the exact upstream commit, archive and patch digests.
The archive digest pins reviewed HTTPS content; it is not an independently
verified publisher signature. Build receipts record the resulting binary digest.

The profile builds reviewed Rust workspace/all-target release artifacts, including
tests, with upstream optimization/LTO/debug flags. Rust build scripts/macros and
configured compilers/linkers execute; OSV receives package coordinates. Current
Cargo JSON artifacts replace stale `.d` guesses. Analysis admits only bounded
owned artifacts, embedded DWARF or one adjacent Cargo dSYM bundle, including
Cargo's exact relative `deps/<hashed>.dSYM` link. Debug UUID must match; the
bounded bundle has exactly one matching file. Legacy spellings are retained
alongside structured demangler NoTemplateParams v0 aliases and receiver normalization. Only plain Rust advisory
paths admit negative call facts; unsupported formats remain unknown.

Malformed/missing/empty debug, mismatched UUIDs, malformed linkage, truncated
DWARF after a valid symbol and incomplete artifacts produce errors/unknown,
never a fabricated uncalled result. All artifacts must parse before any call
facts are published. The reader tests include captured legacy/v0/qualified-trait
nested generic spellings and the formerly unsafe nil-entry/error ordering.

Actual safe fixture qualification uses smallvec0.6.9 (a deliberately pinned
yanked historical advisory version): integer-only grow without panic/drop attack
conditions. Calling grow gives RUSTSEC-2019-0009 called=true; removing it with
the same dependency gives false; advisories without function data remain unknown.
This is optimized-build function evidence, not exploitability proof or a guarantee
that every execution target/dynamic dispatch path is modeled. Vulnerabilities
remain visible regardless of call state. Native IDE process controls and live
project acceptance remain separate from disposable runtime qualification.

Reproduce the actual positive/negative OSV fixture with the built executable:

```sh
python3 qualify.py --scanner /tmp/my-new-macho-build/osv-scanner-macho --work-dir /tmp/my-new-macho-qualification
```

This explicit qualification contacts OSV and downloads the pinned historical
crate while compiling a disposable fixture. It does not run the fixture program.
`qualification.json` retains dated package-level call facts and the exact source
profile metadata. Full logs/output live in the selected disposable directory.
The builder isolates Go caches/environment inside its new work directory,
disables automatic toolchain/workspace/environment overrides and CGO, fixes the
Darwin/arm64 target and readonly module mode, and verifies modules after build.
