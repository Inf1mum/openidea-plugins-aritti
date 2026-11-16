# .env files

`.env` files get highlighting and 11 inspections, with Docker, PHP, PHPUnit, Go, Ruby and Rust support. Environment
variables get completion and go to declaration in `${…}` and in code, with the
keys of `.env` files, Dockerfiles and docker-compose files as declarations.
MIT; see `LICENSE`.

## Files

The `dotenv` language covers `.env`, `.env.*`, `.flaskenv` and `*.env`. The
IDE's own `dotenv` definition still handles detection and comments. This
plugin supplies the highlighting: keys, values, comments, `export`, the `=` /
`:` separator and `${…}` in double-quoted values.

## Inspections

Problems are returned by the registered current-document diagnostics provider.
The host versions these results against the editor text; code actions remain a
separate request. Cross-file facts follow the index lifecycle described under
Limits.

| Check | Severity | Quick fix |
|---|---|---|
| Key containing `-` | warning | none |
| Key not starting with a letter, `_` or `-` (`-` is the check above) | warning | none |
| Key with lowercase letters | warning | none |
| Duplicate key (every occurrence) | warning | none |
| Spaces around `=` | warning | Remove spaces around '=' |
| Unquoted value containing a space | warning | Add quotes (the file's more common quote style, `'` on a tie) |
| Trailing spaces or tabs | warning | Remove trailing whitespace |
| More than one blank line in a row | warning | Remove extra blank lines |
| `${…}` in a value that isn't double-quoted | error | Put the value in double quotes |
| `${KEY}` that no `.env` file, Dockerfile or compose file declares | warning | none |
| Empty `${}` | warning | none |

Unknown cache completeness suppresses absence warnings.

## Completion

- In code, inside a recognised call (below): the project's declared keys.
- In a `.env` file, inside `${…}`: the same keys.
- In a double-quoted `.env` value, typing `$DB` offers `${DB_HOST}`. The popup
  opens on the letters after the `$`, not on the `$` itself, and never after
  `\$` or outside double quotes.

Each key is listed once, with its value as detail (`= value`: the first line,
then `...` if the value spans more lines). The value shown comes from a plain
`.env`, a Dockerfile or a compose file before `.env.example`, `.env.local` and
the other variants.

## Navigation, usages and updates

- Go to Declaration on a key in code or in `${KEY}`: its declarations in plain
  `.env` files, Dockerfiles and compose files. If none of these declares it,
  the declarations in the other `.env.*` files.
- Go to Declaration on a key where a `.env` file declares it: its usages, in
  code and in `${KEY}` of `.env` files.
- After you edit a literal key without saving, an explicit lazy FileAction may offer
  **Update this file's literal ${OLD} references to ${NEW} (source only)**.
  It previews and confirms only current-file interpolation token replacements
  through atomic host edits: it requires cached prior declaration evidence,
  the same value/key count/source line, one current NEW declaration, no
  remaining OLD declaration, and literal ASCII names of at most 128 bytes. It
  uses no provider file discovery and writes no other env, Compose, Docker or
  language file; values never enter the confirmation, which lists only exact
  target/key-token ranges and replacement names. The action is single-use and
  revalidates the exact path/language/version/text, innermost currently open
  project root, cached suggestion and current unsaved text before and after
  the exact confirmation; cancellation returns no edit. Bounds: 12 KiB text,
  256 declarations, 8,192 cached declarations, 128 token replacements and
  eight pending actions; the shared 16 KiB exact-preview bound rejects
  oversized confirmations. CRLF and other source text are preserved. This is
  not runtime-aware Rename or proof that the interpolation uses resolve to the
  edited declaration.

## Where keys are found

Declarations:

- `.env` files: `KEY=value`, `KEY: value`, `export KEY=value`, `#` comments,
  single- and double-quoted values (multi-line too), `\ ` in keys.
- Dockerfiles (`Dockerfile`, `Dockerfile.*`, `*.dockerfile`): `ENV KEY=value
  OTHER="x y"` and `ENV KEY value`, continued with `\`.
- Compose files (`docker-compose.yml`, `docker-compose.yaml`, `compose.yml`,
  `compose.yaml`): `environment:` as a list (`- KEY=value`), a mapping
  (`KEY: value`) or a flow collection (`[A=1, B]`, `{A: 1, B: ~}`, including
  multi-line collections); and the keys of the files named by `env_file:` (a
  scalar, a list or the `- path: x` long form, paths relative to the compose
  file), read when the compose file is indexed. Flow lists of env_file paths
  may span multiple lines and contain quoted commas/comments. A missing file
  is ignored.

Usages in code are matched on the text right before the key, compared
lowercased. The key follows a `"`, `'` or `` ` `` (blanks before the quote
are allowed), or comes unquoted after a member access:

| Form | Examples |
|---|---|
| `…env(` | `getenv(` (PHP, Python `os.getenv`, Java/Kotlin `System.getenv`), Go `os.Getenv(`, Laravel `env(` |
| `…env[` | `$_ENV[`, Ruby `ENV[`, `process.env[` |
| `…env.get(`, `…env.fetch(` | `Deno.env.get(`, `dotenv.get(`, Ruby `ENV.fetch(` |
| `environ[`, `environ.get(` | Python `os.environ[`, `os.environ.get(` |
| `lookupenv(` | Go `os.LookupEnv(` |
| `env!(`, `env::var(`, `env::var_os(`, `env::set_var(`, `env::remove_var(`, `dotenvy::var(` | Rust |
| `$_server[` | PHP |
| `getenvironmentvariable(` | C# `Environment.GetEnvironmentVariable(` |
| `system.getproperty(` | Java, the key may hold dots (`app.name`) |
| `<env name=`, `<server name=` | PHPUnit XML |
| `process.env.`, `import.meta.env.`, `bun.env.` | JavaScript/TypeScript, unquoted key |

Completion and go to declaration work in PHP, Python, Java, Kotlin, Scala,
Groovy, JavaScript, TypeScript (and JSX/TSX), Vue, Svelte, Go, Ruby, Rust,
C# and XML. The usages index reads files with those languages' extensions
plus `phpunit.xml*`.

Other plugins can register additional env declaration files or env readers
through the `aritti.dotenv.sources` extension point (exact file names, role
`declares` or `reads`); changes apply without a restart.

## Hover

Hover on a declared `.env` key, `${VARIABLE}` reference or an
already-supported code reference shows its known declaration paths and
one-based line/column, classified as env, Dockerfile or Compose sources.
Multiple declarations are explicit candidates: hover does not infer runtime
precedence. Current `.env` buffer declarations replace that file's cached
entries, including unsaved edits.

`showValues` controls values in both completion and hover. When false, hover
includes only keys/source metadata and a hidden-values note; this hides
output, it does not remove values from the existing index or hide values
already visible in source files.

Hover never warms the index, probes env files, runs a process or accesses the
network. It only looks up the current project identity, reads an already-built
cache, and parses the current `.env` text. Cold code-reference requests return
no hover until activation/events have populated the cache. Cross-file values
can lag edits/deletions until the existing open/save event indexing runs.
Cache incompleteness and bounded results are labelled; runtime environment
values and interpolation are not evaluated.

Limits: current documents up to 256 KiB, code lines up to 2,000 bytes, keys up
to 256 bytes, the first 8,192 cached declarations and up to 32 matching
sources; displayed keys/values are capped at 256 characters and source paths
at 1,024. The complete response is capped at 16 KiB of UTF-8, including
Markdown fences and provenance notes; whole entries are omitted with a
bounded-results label, rather than clipped inside a value/path. Markdown values
are code spans with control characters escaped.

## Semantic tokens

A SemanticTokens export covers variable declarations and `${KEY}` references
recognized by the established dotenv lexer, with declaration modifiers only
on actual parsed keys. It uses supplied unsaved text only, no
project/index/type lookup. Single-quoted/comment text is not interpreted as a
reference; no runtime ownership or resolved-variable claim is made. Tokens use
real `Variable` semantics, not invented types to colour strings/comments; the
existing lexical fallback remains for the full syntax.

Work is capped at 256 KiB / 4,096 tokens, with sorted nonempty single-line
UTF-16 ranges computed in one forward pass, including long
interpolation-heavy lines. Other languages and oversized documents return no
tokens.

## Compose env_file graph discovery

`env_file` source discovery expands local YAML anchors, aliases and nested
merge graphs, including scalar/list/long-form paths. Explicit mapping entries
retain YAML merge precedence. Missing long-form `required: false` files do not
make discovery incomplete unless another source requires the same path. Paths
remain confined to the project; interpolated, unresolved or unsupported
inputs keep discovery incomplete. This discovers source candidates and does
not infer active Compose services or effective runtime variable precedence.

Compose environment flow collections support multiple lines, comments and
quoted commas/closing delimiters while preserving physical source ranges.
Nested collections are rejected. Local scalar mapping/list anchors are
resolved when used by `environment: *env` or `<<: *common` (including merge
sequences). Explicit environment keys override merged keys; the first anchor
wins duplicate merge keys. Declaration locations point to the physical
anchored key. Forward, cyclic, nested and alias-dependent environment
declarations are not expanded by the physical-range model. Anchor expansion is
capped at 256 KiB, 4,096 lines and 128 names.

The pure YAML parser has explicit limits: 256 KiB source, depth 32, 20,000
nodes, 100,000 events, 128 anchors/aliases, 1 MiB total scalar/retained
anchor bytes, 20,000 replay events and 128 expansions per anchor. Cycles,
undefined aliases, duplicate mapping keys, unsupported tags and budget
exhaustion refuse complete discovery. Filesystem includes and property
interpolation are not enabled in the parser. Keys in Dockerfiles and compose
files are declarations only: `$KEY` there isn't a usage, and Go to Declaration
doesn't work from those files.

## Settings

Settings › Plugins › .env files groups the options under **Completion** and
**Value visibility**, with descriptions of their scope:

| Key | Default | Effect |
|---|---|---|
| `completion` | `true` | Complete environment variables in code (`.env` files always complete) |
| `showValues` | `true` | Show each key's value in completion and hover |

## Limits

- Text-based: this implementation matches the text before the key rather than
  walking each language's syntax tree. A call is recognised anywhere its text
  appears, comments and strings included, and any name ending in `env(`
  counts. A key held in a variable (`process.env[name]`) isn't seen.
- The project index is built by a walk (at most 50,000 paths, about 3 s per
  activation or event batch, declarations first) and updated when a file is
  opened or saved. Until the walk is done, only the root's env files are
  known. A cut-off walk leaves some usages out.
- The walk can't list hidden files, so it looks for a fixed list of names
  (`.env`, `.env.local`, `.env.example`, `.env.dist`, `.env.production`, …,
  `.flaskenv`) in the project root and in every folder holding a project
  marker (`package.json`, `composer.json`, `go.mod`, `Cargo.toml`,
  `Dockerfile`, …). Any other env file is known only once it has been opened
  or saved.
- Semantic Rename/host Find Usages indexing is not implemented: lexical
  matches across env variants, Compose services, Docker build/run stages and
  code do not establish one complete runtime owner set. Navigation returns
  source candidates.
- Lines longer than 2,000 bytes (minified code) aren't searched for usages.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
