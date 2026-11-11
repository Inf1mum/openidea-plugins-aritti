# Twig

Twig templates for OpenIDEA, which has no Twig grammar of its own. The plugin
detects `.twig` files and highlights them with a hand-written lexer: Twig
comments, prints and tags (inside HTML attributes too) and the HTML tags and
attributes around them. The contents of `verbatim` and `raw` are not read as Twig.
The structure view lists blocks, macros and embedded template heads. Recognized block pairs
and multi-line Twig comments fold; arbitrary custom `{% x %}…{% endx %}` names are not inferred.

API 1.4 native injections expose closed, attribute-free `<script>` and `<style>`
bodies as JavaScript/CSS ranges. Comments, Twig expressions/tags/raw regions,
template holes, external/typed scripts and unclosed bodies are excluded. A configured
installed embedded language server can serve those ranges; embedded formatting and
Rename remain host-owned. This does not add PHP or full template language semantics.

Completion offers Twig's tags, filters, functions and tests by position
(after `{%`, `|`, `is`, or a bare name), and in HTML text thirteen snippets
that expand after typing a prefix: `blk` block, `mcr` macro, `esc`
autoescape, `emb` embed, `ext` extends, `for`, `from`, `if`, `imp` import,
`inc` include, `sbx` sandbox, `spl` spaceless, `verb` verbatim.

Macros: after `{% import 'forms.html.twig' as forms %}` typing `forms.` lists the
macros that template defines (with their arguments), `{% from 'x' import ` lists
them too, and `_self` works for the current file. Go to Declaration on
`forms.input`, on a name imported with `from … import`, and on the template
name in `extends`, `include`, `embed`, `import`, `from`, `use`, `include()`
and `source()` opens an existing conventional `templates/<name>` file.
Arbitrary suffix matches and `@namespace/` guesses are refused. Configured template
navigation and macro completion stay with `aritti.symfony` in Symfony projects;
external macro definition lookup is quiet there until that owner supplies its
configured namespace/path resolution. Explicit `_self.name` navigation remains
source-local in Twig.

Outside Symfony projects, filters, functions and tests registered in the
project's PHP (`new TwigFilter('x'`, `new TwigFunction('x'`, `new TwigTest('x'`
under `src/`, `app/` and `lib/`) join the completion lists, and Go to
Declaration on them opens the registration (setting `projectExtensions`).
Extensions from `vendor/` are not read.

Live current-document inspections (setting `diagnostics`) report block tags
without their `{% end… %}` (`unclosed-tag`), `{% end… %}` without an opening
tag (`unexpected-end-tag`) and a `{{`, `{%` or `{#` never closed
(`unterminated-delimiter`). A stray `}}` is not reported: it is common in
inline JavaScript and CSS. Off by default (setting `expressionErrors`): brackets
that do not pair inside `{{ }}` and `{% %}` (`unbalanced-bracket`); strings,
comments and `verbatim` blocks are skipped. Other expression errors (unknown
operators, wrong argument counts) are not detected.

In a Symfony project (`symfony.lock`, or `symfony/framework-bundle` in
`composer.json`) the `aritti.symfony` plugin owns name completion, so this
plugin leaves it out there. Route, template and translation completion also
comes from `aritti.symfony`.

The opt-in API 1.4 formatting provider below supplies reviewed Twig spacing formatting. The language-definition Prettier/Ruff fallback does not understand Twig.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Token classification and structure bounds

Twig already owns the `syntax` export for its custom language. This slice improves those lexical
categories and does **not** add a second SemanticTokens overlay. Decimal/scientific numeric
literals and digit separators follow the verified [Twig 3 lexer grammar](https://github.com/twigphp/Twig/blob/3.x/src/Lexer.php);
`1..3` is now number / range operator / number. Callable members use `function-call`, ordinary
members use `property`, macro declaration names use `function`, and block declaration/closing
names use `property`. UTF-8 identifier bytes stay in whole-name tokens, so non-ASCII names no
longer produce invalid token boundaries. These are source-shape roles, not resolved PHP/Twig types.
Strings/comments/raw contexts keep their lexer ownership; string interpolation is not a new injected
language or independently classified expression layer.

The pinned original `TwigHighlighter`/`TwigHighlighterData` map lexical categories to theme colours.
Its `TwigStructureViewFactory` lists block/embed/macro statements, so this port now includes a
literal or identifier embed head as a module entry and parents nested blocks beneath it. Its
`TwigFoldingBuilder` uses recognized structural block statements and excludes separate else/elseif
folds. Our folds now follow the established known opener set (including legacy raw/filter/spaceless
and the port's existing apply/cache names), with inline block/set exceptions. Unknown custom tag
pairs are quiet. Raw/verbatim closers must be exact tags, not prefixed identifiers or tags with
extra arguments; unterminated raw bodies remain literal through the end of the document.

Syntax highlighting/outline/folding skip documents over 256 KiB. Highlight output is limited to
16,384 tokens. Structure is suppressed above 4096 tags and limited to 2048 symbols/folds, keeping
retained parent indices valid. Line offsets are precomputed once for structure rather than rescanned
from the start per tag. The SDK receives byte ranges for syntax tokens/symbols and line ranges for
folds; Unicode/CRLF tests verify round trips through UTF-16 editor positions. Malformed inputs use
the existing recovery rules; this is not a complete Twig AST, custom fold-placeholder implementation,
type resolver or injected PHP/HTML/CSS/JavaScript parser. Completion, diagnostics, Symfony ownership
and extension discovery retain their existing behavior and limits.

Original/decompiled evidence inspected: `7303/.../TwigHighlighter{,Data}.java`,
`folding/TwigFoldingBuilder.java`, `structure/TwigStructureViewFactory.java` and
`parser/TwigBlockStatements.java`. No matching Twig editor screenshot was available in the local
parity screenshot set (only icon images elsewhere); no screenshot parity or desktop acceptance
is claimed. Tests cover numeric/member/declaration roles, Unicode and every valid malformed prefix,
raw suppression/closer boundaries, embed hierarchy, branches/custom tags and output caps.


## Reference ownership and scope audit (P29.2 partial)

The Twig owner now uses its lexer to recognize macro declarations/imports and
reference contexts. HTML text, HTML/Twig comments, raw/verbatim bodies and quoted
expression text cannot manufacture imports or macro calls. Template navigation
requires the actual recognized literal token and a supported template argument;
member calls such as `object.include(...)` do not become global include references.
Escaped/dynamic/interpolated template expressions stay unresolved.

Explicit `import`/`from` bindings resolve within the current lexical block,
macro or embed container. A known block-local binding shadows a template-level
binding without leaking into sibling blocks. Embed and macro bodies require an
explicit local binding; top-level imports are not assumed to execute when a macro
is called externally. Conditional/control-flow imports, duplicate same-scope
aliases, malformed imports, unknown scope-defining custom tags and crossed or
unclosed scopes stay quiet. Outer nested-container imports are conservatively
unresolved rather than treated as globally visible. This is declaration navigation,
not proof of effective runtime registration, import execution order or variable types.

`_self.name` resolves one unique current-text macro declaration without project
access, including unsaved Unicode names and exact UTF-16 selection ranges.
Imported macro targets are read afresh and duplicate/malformed declarations do
not produce an arbitrary target. Optional registered PHP extension navigation
rechecks cached source anchors against current visible PHP text and distinguishes
calls from plain variables; it still does not establish classpath/runtime extension
ownership. Existing PHP registration discovery remains a separate heuristic feature.

Provider input and imported source are bounded to 128 KiB; declaration/import
parsing is suppressed above 4096 recognized tags. Conventional non-Symfony template
navigation accepts only literal project-relative `templates/<name>` targets;
namespace, traversal, glob and arbitrary directory fallback paths stay unresolved.
A file navigation location is not an indexed rename definition. Completion of
imported macros is now delegated along with other names in Symfony projects to
avoid duplicate providers. Symfony's configured roots/namespaces, PHP/Twig variable
context and cross-template framework references remain that plugin's responsibility.

Historical reference audit (superseded for the current static-source subset below): no document indexer or Rename graph was published. A macro can be imported through
aliases in other templates; block names participate in inheritance and `use`
traits; variables have lexical bindings and context supplied externally. A
current-document spelling cannot establish all those references. Safe indexed
usages/Rename still requires a common template identity, exact declaration/reference
anchors and complete scope/index fixtures. This is an implementation/resolution
gap, not a new SDK blocker. Blocks/variables and namespace-aware cross-template
Rename remain unimplemented, and native navigation acceptance for this change
has not been run.

The actual scope rules were checked against the primary
[Twig 3 macro documentation](https://twig.symfony.com/doc/3.x/tags/macro.html)
and [block trait documentation](https://twig.symfony.com/doc/3.x/tags/use.html).
The pinned original `TwigFileViewProvider` decompilation delegates template data
and clones the selected template language; it does not prove a Rename contract.
The available original receipts contain no Twig editor screenshot establishing
usages or Rename behavior, so neither visual nor complete original parity is claimed.

## Live source documentation and bounds (P29.1 extension)

Twig hover now supplies a verified Twig3 language documentation catalogue for
core tag/filter positions and unique current-template macro declarations or
`_self.name()` calls. Macro argument text comes from the current unsaved document;
it is escaped/bounded source, not evaluated values or argument/type checking.
Tag/filter hover links to public language documentation and does not establish
that this project's installed Twig version or custom overrides implement it.
Comments, HTML, raw/verbatim bodies, strings, member lookalikes, unknown/extra
filters, duplicate macros and external/imported macros stay quiet.

Ownership is explicit without detecting Symfony through I/O in hover: Twig owns
its language/source docs; Symfony owns routes/services/translations, configured
template namespaces and framework extension references. Framework `path`/`trans`
lookalikes do not receive core Twig hover. Existing Symfony completion/navigation
ownership remains unchanged. The `hover` switch is independent of diagnostics.
Hover performs no project reads, discovery, process or network requests.

Versioned diagnostics now admit only Twig-language `.twig` input up to128KiB,
4,096 recognized tags and16,384 tokens, with at most500 findings. Disabled,
wrong-language and oversized documents return an empty current result. Fixed
current text replaces old findings; no marker publication or project scan is
added. This bounds the existing local delimiter/tag/bracket checks, not full Twig
parser/type diagnostics. Native scheduling and new hover rendering remain
unverified; prior outline acceptance does not cover this artifact.

Original retained highlighter/folding/structure contracts were rechecked; no
matching documentation-provider implementation or Twig editor screenshot exists
in the retained inventory. The fixed Twig3.10 CoreExtension catalogue and public
Twig3 macro/include/default/escape contracts informed the independent docs; no
proprietary bodies, original messages or new native/visual parity are claimed.

P29.3 lexical-role extension: the existing Twig `syntax` owner now distinguishes
built-in test names after `is`/`is not`, the `same as`/`divisible by` connectors,
the initial `apply` filter, and named argument keys inside admitted call-shaped
parentheses. These use existing function/keyword/property highlight categories;
ordinary variables/constants, hash entries, grouped comparisons and macro parameter
declarations retain their roles. Parenthesis context is bounded to128 levels and
malformed context suppresses named-argument classification. Strings, comments and
raw/verbatim still belong to the existing lexer. No second semantic-token provider
or unrelated view is introduced. Folds/outline keep their current contracts and
UTF-8 byte/UTF-16 host range behavior.

The Twig3 docs distinguish `=` named arguments from `:` (3.12+) and macro/member
named arguments (3.15+). Classification identifies source shape only; it does not
infer installed versions, validate supported argument names or resolve runtime
calls/types. Tests cover those shapes, ambiguous non-call contexts and every
Unicode source prefix, retaining existing fold/outline fixtures. New native token,
folding and outline rendering remains unverified; prior desktop15 is separately
dated and does not prove this artifact's complete original editor parity.

P29.2 ownership follow-up: conventional template references now stat a concrete
file and require the target's innermost open project to match the invoking
project. A separately opened nested template project cannot supply an outer
project's include/import/macro target. Template existence probes no longer read
the entire target just to return a file location.

PHP extension discovery and fresh definition checks require same-project concrete
source files under `src/`, `app/` or `lib/`, up to128KiB each; vendor/outside-source
save events cannot expand that scope. Discovery is capped at1,500 paths/12MiB/3s
across its globs/projects, checked between host calls. Definition lookup observes
at most16 matching cached registrations, then rechecks source ownership, file
kind/size and current registration text. Disabled `projectExtensions`, projectless
requests and separately opened nested PHP projects cannot reuse an outer cache.
Completion remains cached positive heuristic data and can be stale until refresh;
these bounds do not establish complete PHP/runtime registration or a closed index.

Historical navigation pass: a real nested-target regression was reproduced and fixed. No host document indexer
or replaceable native Rename anchors were added in that historical pass: aliased macros/named external arguments,
block inheritance/traits, included context variables and interpolation still lack
a complete common declaration/reference identity. P29.2 remains partial for its
indexed usages/Rename scope. This source navigation hardening is not closure of
that requirement or new native acceptance.

Twig46 literal self-template imports use the supplied current Document for macro names and target ranges, matching `_self` behavior instead of reading the saved copy. The regression includes an astral prefix and checks the UTF16 target column. External macro sources receive a 128KiB stat preflight before reading and retain the post-read bound. No indexed usages/Rename or native acceptance is claimed.

Twig47 macro completion withholds duplicate declaration names rather than offering ambiguous owners, matching macro navigation. Structurally malformed external template sources are withheld. Current-buffer completion retains recovery for the unfinished call being typed; duplicate current-buffer names remain excluded. No full Twig compiler/runtime validation is inferred.

Twig48 import/from macro bindings establish navigation/completion ownership only after their declaration ends. A later import cannot own an earlier expression. This follows sequential symbol registration in Twig's ImportTokenParser/FromTokenParser and does not infer runtime control-flow execution.

Twig49 template navigation requires the literal to be the complete template expression: function argument close/comma or a supported tag terminator/modifier. Literal prefixes of concatenation, filters and coalescing expressions no longer borrow the prefix file as a false target. Include modifiers/arguments remain supported. Unsupported expression shapes stay unresolved.

Twig50 PHP extension extraction admits registration candidates only in PHP code outside strings/comments/backticks/HTML, at supported constructor or attribute boundaries, and with a complete literal first argument. Explicit foreign qualified class names are excluded; recognized Twig qualified constructors/attributes and legacy Twig_ constructors remain. Heredoc/nowdoc files are withheld conservatively, and input is capped at128KiB. Unqualified PHP names/import aliases, namespace semantics, reachability and runtime registration remain heuristic; no complete PHP semantic ownership is claimed.

Twig51 subscribes to typed PHP files-changed notifications and invalidates matching cached registrations on creation/modification/removal without discovery or reads. Saved-file notification rechecks/warm source as before. At most256 events/paths and100ms are processed before conservative cache clearing. Native event delivery and unsaved PHP synchronization remain unverified; unavailable event grant retains existing activation/saved-event fallback behavior.

## Unreleased follow-up

Local for-loop variables now have document-scoped index anchors, including nested shadowing and outer-variable references in an inner loop iterable. The supported subset is a complete template containing only `for`/`endfor` statement tags; assignments, other tag scopes and dynamic context access disable this graph.


## Current local references and JavaScript blocks

The private local loop binding model now supports `for key, value in items` and nested
`if`/`elseif`/`else` bodies, keeping each binding scoped to its owning loop.
Iterable references resolve to outer bindings before the new loop bindings enter
scope. Duplicate binding names, crossed/unclosed scopes, assignments, loop `else`
and dynamic context access still withhold the graph. This does not close macro,
block-inheritance or cross-template usages/Rename ownership.

Automatic JavaScript injections also recognize complete `{% block javascript %}`
and `{% script %}` bodies, matching the documented original workflow. The physical
body must contain no Twig holes/nested tags or HTML script/style wrapper; comments,
raw/verbatim text, empty and unclosed bodies stay excluded. Existing closed HTML
script/style islands remain supported. Injected language services still require
an installed/configured server and matching host support.

Twig 3.15+ literal imported macros and current-buffer `_self` macros also offer named
argument completion inside calls. Suggestions preserve existing `:`/`=` separators,
exclude supplied positional/named arguments, and use exact current-document ranges.
Duplicate declarations, ambiguous imports, dynamic receivers and malformed signatures
withhold argument suggestions. A known older installed Twig version in composer.lock
disables this assistance. External template resolution remains owned by Symfony
in Symfony projects; `_self` assistance remains available there.


## Custom Twig delimiters

Settings `tagOpen`/`tagClose`, `variableOpen`/`variableClose` and
`commentOpen`/`commentClose` configure the application’s Twig lexer syntax.
Defaults are `{%`/`%}`, `{{`/`}}` and `{#`/`#}`. Values must be nonempty,
1–16 bytes and contain no whitespace, controls, quotes or backslashes; opening
values must be distinct without one being a prefix of another. Invalid settings
withhold syntax/reference providers and produce an explicit settings diagnostic.

Lexer tokens, folds, structure, diagnostics, completion/snippets, macro
navigation/named arguments, hover, local binding anchors and injected ranges all
consume actual source delimiters. Original physical byte/UTF-16 coordinates are
preserved even with unequal-width or Unicode delimiters; no normalized copy is
used as edit coordinates. Settings are snapshotted for each request and restored
when it returns. Configure the same delimiters in Twig itself. Framework-aware
Symfony providers and the host’s static comment-toggle language declaration
remain separate owners; the static declaration still uses `{#`/`#}`.


**Twig: Toggle Comment** provides a configured-syntax comment action independently
of the host’s static comment shortcut. It wraps one selected range or the current
line, or removes one enclosing Twig comment. Edits use the current document
version, preserve Unicode/CRLF text and reselect the content. Incomplete comments,
nested-closer selections, multiple selections and raw/verbatim contexts refuse
rather than produce misleading comment markup. The command needs optional
`editor.read`/`editor.write`; no process or project writes are used.


## Read-only source occurrences and reviewed Rename

`staticTemplateReferences` builds private exact source models (default on).
Unique macro declarations, matching `endmacro name`, `_self.name`, literal
namespace/from imports, local aliases and literal block inheritance retain
physical-source identities. Namespace aliases and explicit `from … import
original as local` bindings remain separate from exported macro names.

**The public DocumentIndexer returns no anchors.** Released API 1.4 uses those
anchors for unchecked native Rename: the frontend accepts any nonempty new name,
and the host inserts it literally without a plugin name-policy callback. A quote,
interpolation sequence, keyword or colliding name could corrupt Twig or change
binding ownership. Source ownership alone does not make that replacement safe.
This includes the older local-loop anchors; they are also withheld. The empty
registered indexer lets the host retire earlier published records after reindex.

Go to Definition remains available through the definition provider and private
models, including local loop bindings, macros, aliases, named block ends and
literal block references. Native read-only highlights and occurrence-count lenses
use the same models. Lens actions recheck opaque source snapshots and open
**Twig: Review Static Source Occurrences** independently of caret position.
The command also works directly at an admitted symbol under the caret.

**Preview Rename Static Twig Symbol…** is a reviewed FileAction for local loop
bindings/import aliases and admitted macro/block source families. It validates a
new non-reserved identifier (letters/digits/underscore; leading letter/underscore;
maximum64 UTF-8 bytes), rejects quotes/interpolation, and constructs a virtual
rewritten source graph before returning edits. If occurrence identity changes,
a name collides, a reference is captured or a binding disconnects, it refuses.
The preview lists exact file/line/column replacements and other spellings left
unchanged outside the admitted scope. Cancellation returns no edits. Source text,
project ownership, inventory membership and settings are rechecked after review;
the host applies the returned workspace edit atomically with undo. Optional
`project.write` is needed to apply the action.

The reviewed scope is explicitly static, not all runtime usages. Dynamic imports
or members, ambiguous/duplicate names, shadowing assignments, malformed/control
imports and unsupported trait/embed/namespaced block models remain excluded.
Conventional cross-template resolution remains outside Symfony configured-loader
ownership. Review scans at most64 owned sources/2 MiB,128local import bindings,
16affected files and256spans; the complete exact preview must fit15KiB (below the host’s16KiB confirmation clipping limit). Each source is at most64KiB, parent chains at most8,
and each graph callback caches at most16sources/1MiB. Exceeding a bound refuses
rename rather than silently truncating edits. Native global Find Usages with
read-only anchors and plugin-validated native Rename require a host/API hook.

Macro signature help uses unique current-source `_self`/literal-import owners,
tracks positional/named parameters and incomplete quoted values, and withholds
named-argument assistance for a known installed Twig older than3.15. Parameter
Go to Definition resolves current macro-body reads and named arguments to exact
physical parameter spellings. Parameter Rename remains a separate scope.

## Reviewed optional formatter

The formatting provider is off until `formatterEnabled` is enabled and
`formatterPhar` is set to an absolute official **Twig-CS-Fixer 4.1.1** PHAR path.
Acquire the release from
[the official project](https://github.com/VincentLanglet/Twig-CS-Fixer/releases/tag/4.1.1).
The adapter requires this exact reviewed SHA-256:
`4d099a3be74ea41b0c8f1b2a7033c53a77e7d0dfba36725c2bbaaa19453ed2e5`.
It needs the optional approved `process:php`, plus `editor.read` to establish the
active current document; it does not download/install a tool automatically.

The official CLI’s `fix` command writes files, so the adapter instead calls its
in-memory `Fixer::fixFile` using the official Twig spacing rules inside that
pinned PHAR. PHP runs with `-n`, 128 MiB and a two-second execution limit. No
project configuration, application autoload, formatter cache or external source
file is read/written by the adapter. Formatting receives current text over stdin
and returns one full-document edit; the host applies it transactionally. Before
returning an edit, the plugin rechecks active source text, project, settings and
all literal HTML/string/Twig tokens. Any semantic/literal-data change is refused.
Unicode and CRLF text are preserved.

This adapter formats standard Twig delimiter/operator/punctuation/macro-argument
spacing; it does not rename identifiers, convert templates, or rewrite rendered
HTML indentation. Range formatting and custom-delimiter formatting are explicitly
unavailable because this reviewed official tokenizer does not support those
contracts. PHP/tool denial, wrong release bytes, malformed input, stale source or
unexpected output returns an error with no edit.

Run the explicit real-tool test with:
`TWIG_CS_FIXER_PHAR=/absolute/path/twig-cs-fixer.phar python3 tests/formatter-adapter.py`.
