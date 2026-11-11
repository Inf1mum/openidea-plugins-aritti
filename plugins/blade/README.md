# Blade

Laravel Blade templates for OpenIDEA, which has no Blade grammar of its own. The
plugin detects `.blade.php` files and highlights them with a hand-written lexer:
Blade comments, `{{ }}` and `{!! !!}` echoes, directives and their arguments,
`@php` blocks, `<?php ?>` blocks, PHP inside them, and the HTML tags and
attributes around them. The contents of `@verbatim` are not read as Blade, and
`@@directive`, `@{{ }}` and email addresses are left alone.

The structure view lists `@section`, `@push`, `@prepend` and `@fragment`
blocks. `@if … @endif` style pairs (about 35 of Laravel's and Livewire's block
directives) and multi-line comments fold.

Two live inspections use the current editor text (each can be switched
off in Settings): `unpaired-directive` (an `@endif`, `@endforeach`, ... without
its opener, or an opener without its closer, including `@php`, `@verbatim` and
the block form of `@section`) and `unpaired-parenthesis-in-string` (a directive
whose `(` has no matching `)` once strings are skipped). Verbatim blocks,
comments, `@@escaped` directives, inline `@section('a', 'b')` and `@php(...)`,
and custom directives are never reported. The current-document diagnostics provider does not require file events.

Directive completion is deliberately not in this plugin: `aritti.laravel`
already completes Blade directives (core ones with about 45 snippets such as
`@if(…) … @endif` and `@foreach`, plus custom directives found in `app/`), so a
second provider here would list every directive twice. Install both plugins.
There is no formatter: OpenIDEA only runs Prettier and Ruff from a language
definition, and neither parses Blade.

Only the directives Laravel and Livewire ship are known to the highlighter;
directives registered with `Blade::directive()` stay plain text, since a plugin
can't see the PHP that registers them.

Directive, route, view, config and translation completion in Blade files comes
from the `aritti.laravel` plugin (it also completes custom directives found in
`app/`).

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Directive hover and completion comparison

Hover on a recognized directive name shows short static syntax help and its block relationships.
The same lexer that colours Blade decides whether a directive exists in that context: comments,
`@@` escapes, email addresses, echo/PHP strings and verbatim contents do not acquire false hovers.
`@section` and `@php` distinguish inline from block forms; bare `@empty` is described as a
`@forelse` branch. The range covers only the directive name, including `@`, using UTF-16 positions.

Help is bounded to documents of at most 128 KiB and the first 4096 recognized directives, with
at most 2 KiB of static output. It reads no files, settings, project indexes or network resources.
It covers the existing static Laravel/Livewire directive names only. Some names receive concise
family help, while others expose their recognized block/closer role. Custom `Blade::directive`
metadata, argument/type resolution, PHP injection, installed framework version detection and
framework-specific documentation are not implemented. Short descriptions were checked against
[Laravel 12 Blade documentation](https://laravel.com/docs/12.x/blade) and
[Livewire 3 JavaScript directives](https://livewire.laravel.com/docs/3.x/javascript); this does not
claim version-aware support for every directive in those frameworks.

Completion remains solely in `aritti.laravel`. The pinned original Blade plugin's
`BladeCompletionWeigher` favours directive items; `BladeDirectiveCompletionProvider` additionally
prioritizes unmatched opening/closing directives and defines section closer order as
`endsection`, `show`, `stop`. The Laravel owner now implements explicit prefix/context ranking and the
section closer priority (see its completion receipt). No duplicate completion provider
is added here. The available original screenshot
`7569-0.png` shows an old `@se` completion list, not directive hover or current host ordering;
no matching original hover screenshot or new desktop acceptance is claimed. Existing colouring,
folding, outline and diagnostics are unchanged.


## Reference ownership and local block navigation (P01.2 partial)

| Reference family | Owning plugin and contract |
|---|---|
| Directive names and block syntax | Blade owns lexical recognition, diagnostics, static hover and closing-to-opening syntax navigation. Laravel owns completion, including custom registrations. |
| View paths, namespaces and components | Laravel owns framework-aware completion/navigation and configured view roots. A view spelling can map to a file path; it is not a directly replaceable file-name token. |
| Section/yield and push/prepend/stack names | Laravel owns cached inheritance/include relationships and related-file navigation. Equal literal names in unrelated templates do not establish one symbol. |
| Custom directive declarations | Laravel owns PHP registration discovery. Blade's static lexer does not invent registration ownership. |
| Embedded PHP/HTML variables and types | Language-server/native injection ownership; no Blade symbol graph is published. |

Go to Definition on a recognized matched closing directive now opens the nearest
properly nested opener **in the current document**. The selection covers the
actual opener token with UTF-16 coordinates. Inline section/PHP forms do not open
blocks; bare forelse `@empty` stays a branch. Crossed/missing blocks, unknown
custom directives, escaped directives, comments, PHP/echo strings and verbatim
contents do not acquire inferred block links. This provider reads no project
files, caches or network data and works on unsaved text. It refuses documents
over 128 KiB or 4096 recognized directives and malformed argument delimiters.
Laravel continues resolving named arguments; the providers cover different tokens.

No `DocumentIndexer` is registered: API 1.3 uses its definitions/references for
automatic Rename as well as usages. A local section literal cannot prove that
inherited layouts, included views, dynamic names, runtime overrides and custom
registrations have no other references. Even exact literal spelling does not
make that scope complete. Usages/Rename remain incomplete pending a common
Laravel-owned resolution model and explicit scope/completeness tests; there is
no host/API blocker asserted for an independently proven subset.

The pinned original `BladeFileViewProvider` and factory samples establish
separate template/HTML file views, not a section Rename contract. Available
`7569-0.png` shows directive completion rather than usages/Rename or block
navigation. The new syntax link is an independently bounded convenience, not
proof of original navigation/visual parity. Native Go to Definition acceptance
for this addition remains unverified. Existing hover has separate desktop evidence.

## PHP source boundary for directive facts

Literal closing-tag text inside a PHP quoted string or block comment does not
resume Blade directive recognition. Real PHP closing tags in scripting or line
comments still do. Hover, current-document block navigation and diagnostics share
this boundary with colouring. Basic quoted/escaped strings, backticks and PHP8
attribute string contexts are covered; heredoc/nowdoc, complex string interpolation
and unterminated string/comment contexts keep the remaining region opaque. This
is a conservative lexical subset, without executing PHP or adding PHP type/runtime
resolution. Named views/sections/components and directive completion remain owned
by Laravel. API14 injection artifacts require their separately validated overlay
refresh after these canonical source changes; earlier native receipts remain dated.

Directive argument boundaries and argument counts ignore PHP strings and comments
and require properly nested parentheses, brackets and braces. Comments cannot
manufacture directive tokens, inline-section arity or an outline name. Section/
push outlines require one complete ordinary literal first argument, with its exact
source span (at most256 bytes); dynamic, concatenated, escaped or interpolated
names stay unlabelled. This is source structure, not Laravel name resolution.
Malformed argument delimiters gain no hover/block navigation; unsupported opaque
PHP states do not establish missing-closer diagnostics. Ordinary malformed-input
recovery and known static checks remain separate from a full PHP parser.

PHP variable/function highlighting preserves complete UTF8 identifier spelling
under the pinned PHP identifier-byte contract. A Unicode-suffixed variable such
as $thisé is not attributed the special $this role. Blade directive-name grammar
and Laravel-owned symbol/type resolution are unchanged. These are lexical spans,
not published PHP symbols or Rename anchors.

### Literal capture roles

Section, push and prepend share one source-local role check across help, diagnostics, outline/folds and closing-token navigation. An ordinary literal name with one positional argument captures. With two admitted positional arguments, section captures for literal null and is inline for an ordinary string; push/prepend capture for an empty string and are inline for a nonempty string or null. Comments around arguments preserve source ranges. Dynamic, named, unpacked, escaped or interpolated forms have unknown roles; subsequent inferred block relationships and absence diagnostics stop at that boundary. Other directive families retain their separate static syntax rules. No PHP values or framework runtime are evaluated.

Malformed argument recovery shares a per-document linear scan budget (eight times source bytes). Exhaustion makes the remaining source opaque, withholding inferred absence warnings and closing-token navigation there. Ordinary balanced argument scans keep their existing behavior; no elapsed-time guarantee is inferred from native unit tests.

## Native embedded JavaScript and CSS (API 1.4)

Literal closed, attribute-free script/style bodies now publish bounded physical UTF16 injection ranges. The host can highlight these ranges and route configured installed embedded language servers. Blade/PHP holes, ambiguous tags and unsupported script/style modes remain excluded; directive/PHP ownership stays with the existing lexer. This does not add full PHP semantics or embedded formatting/Rename.


Matching directive navigation is bidirectional: a recognized block opener now
navigates to its exact current-text matching closer, as well as the existing
closer-to-opener direction. Unclosed/crossed blocks remain unresolved. This reads
no project files and does not publish named Laravel symbol or Rename anchors.
