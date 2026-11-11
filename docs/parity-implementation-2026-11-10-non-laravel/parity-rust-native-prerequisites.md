Native Darwin OSV-Scanner2.7.1 prerequisite investigation (2026-11-11 local)

Official source inspected at pinned release:
https://github.com/google/osv-scanner/blob/v2.7.1/internal/sourceanalysis/rust.go
Exact commit02f96a40691c2d155fdda799af436e111231e365.

rustBuildSource invokes cargo build --workspace --all-targets --release with explicit optimization/debug flags. Compilation succeeds. functionsFromDWARF invokes Go debug/elf.NewFile directly; it does not invoke LLVM, binutils, xcrun dwarfdump or cargo-auditable. An arm64 Mach-O executable starts CF FA ED FE and therefore fails the ELF parser before debug extraction. Code also discovers top-level target/release .d paths, rather than Cargo-emitted current artifact messages, and does not discover Darwin's adjacent dSYM.

xcrun --find dwarfdump resolves installed Xcode toolchain. xcrun dwarfdump --uuid reads the compiled executable's UUID. Cargo-generated adjacent call-fixture.dSYM contains valid DWARF Rust linkage information; native dwarfdump reads it. Re-run official scanner with Xcode tools first on PATH remains exit127 with same bad magic diagnostic; receipt /tmp/parity-osv-scanner-tools/rust-xcode-qualification.json and log rust-xcode.log. This is a binary-reader/platform limitation in exact source, not missing tool installation. Original failed result remains Unknown, not a success/clean report.

Root authorized a separately named, reproducibly pinned-source Mach-O/dSYM adapter profile, distinct version2.7.1-openidea-macho.1 and executable osv-scanner-macho. In progress: exact current Cargo compiler-artifact ownership, UUID-matching bounded dSYM discovery, failure on missing/malformed/empty debug, no partial false facts, actual called/uncalled and malformed fixtures. This is not unchanged official binary acceptance.
