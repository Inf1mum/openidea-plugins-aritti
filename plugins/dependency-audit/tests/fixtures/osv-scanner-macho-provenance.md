The Rust called/uncalled JSON fixtures normalize the source path to /project and
retain actual source-built OSV-Scanner2.7.1-openidea-macho.1 package/group/call facts
from a safe smallvec0.6.9 experiment on Darwin arm64/rustc1.99.1, 2026-11-11 local.
Large advisory descriptions and unrelated top-level metadata were omitted.
The full original output, exact argv, tool version, date and logs are recorded in
/tmp/parity-osv-scanner-macho/qualification.json. Reproduction/build patch and
upstream Apache2 license: ../../adapters/osv-scanner-macho/.

A small integer SmallVec grow call gives RUSTSEC-2019-0009 called:true. The same
locked dependency without a grow call gives called:false. Advisory groups lacking
explicit function evidence and aliases lacking a direct call map remain Unknown.
This exercises retained-symbol evidence without triggering the advisory's unsafe
conditions; no exploit input is used. It does not prove exploitability or safety.
