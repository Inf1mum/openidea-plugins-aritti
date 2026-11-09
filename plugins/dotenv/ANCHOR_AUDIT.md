# P04.2 index-anchor and scope audit

No DocumentIndexer, indexing macro export or documentIndexer manifest contribution
is registered. `Index` in index.rs is an internal declaration/usage cache, not a
host Rename graph. Explicit definition navigation, hover, completion and atomic
current-file source updates remain separate APIs.

| Source model | Actual established fact | Missing identity before automatic Rename |
|---|---|---|
| Env variants | Exact parsed key spelling/location, primary/example distinction, current-file overlay | Runtime loader, environment/profile precedence, duplicate key target and complete usages |
| Compose | Parsed environment declarations and explicit env_file sources | Service identity/precedence, aliases/merge resolution and interpolator environment |
| Dockerfile | ENV declaration spans/values, continued syntax | Build-stage inheritance and build/run target scope; no Dockerfile usage-owner graph |
| Supported code | Source token range for known lexical access forms | Language/type/runtime receiver resolution, dynamic keys and complete reference membership |
| Current-file interpolation update | Bounded explicit OLD→NEW token rewrite after prior cached observation | It is intentionally a source rewrite, not symbol Rename or proof of runtime binding |

The project-root cache prevents intentional global pooling, but a shared key name
within one root is still not a symbol owner. Navigation returns source candidates;
primary/example preference is not runtime resolution. An `Index.complete` source
walk flag does not establish semantic completeness for any of these scopes.

Original MIT `DotEnvKeySymbolRenameUsageSearcher` requires a DotEnvKeySymbol,
retains its pointer, searches case-sensitively in an explicit search scope and
validates candidate references before building rename usages. Its nested-variable
reference provider recognizes the dedicated PSI node. The current lexical cache
cannot substitute for those pointer/reference checks by exporting every equal
string under one namespace. Original code was reviewed in the retained MIT source;
no upstream source is copied into this receipt.

Before enabling indexed Rename, prove exact project/file/declaration owner,
reference completeness and collision handling for a coherent supported subset;
unknown loader/service/stage/language facts must withhold that owner. Every
published span must be nonempty UTF16 and replace the exact same spelling.
Transformed strings, structural tags and synthetic file nodes are not replaceable
symbols. Tests must exercise duplicate declarations, scope switches, external or
unsupported references, deletion/current text and host range validation.

The existing explicit source-update FileAction retains its own preview/version/
source/collision/one-use guards. No graph is added as a consequence of semantic
colour tokens: Variable/Declaration is presentation, not reference ownership.
This audit is complete; semantic cross-file Rename remains feasible model work,
not a platform blocker or a delivered feature.
