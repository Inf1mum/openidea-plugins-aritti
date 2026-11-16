# Blade

Laravel Blade templates for OpenIDEA, which has no Blade grammar of its own.
MIT; see `LICENSE`.

## Highlighting, structure and folding

The plugin detects `.blade.php` files and highlights them with a hand-written lexer:
Blade comments, `{{ }}` and `{!! !!}` echoes, directives and their arguments,
`@php` blocks, `<?php ?>` blocks, PHP inside them, and the HTML tags and
attributes around them. The contents of `@verbatim` are not read as Blade, and
`@@directive`, `@{{ }}` and email addresses are left alone. Only the directives
Laravel and Livewire ship are known to the highlighter; directives registered
with `Blade::directive()` stay plain text, since a plugin can't see the PHP
that registers them.

The structure view lists `@section`, `@push`, `@prepend` and `@fragment`
blocks. `@if … @endif` style pairs (about 35 of Laravel's and Livewire's block
directives) and multi-line comments fold.

## Inspections

Two live inspections use the current editor text (each can be switched off in
Settings):

- `unpaired-directive` (an `@endif`, `@endforeach`, ... without its opener, or
  an opener without its closer, including `@php`, `@verbatim` and the block form
  of `@section`);
- `unpaired-parenthesis-in-string` (a directive whose `(` has no matching `)`
  once strings are skipped).

Verbatim blocks, comments, `@@escaped` directives, inline `@section('a', 'b')`
and `@php(...)`, and custom directives are never reported. The
current-document diagnostics provider does not require file events.

## Hover

Hover on a recognized directive name shows short static syntax help and its
block relationships. The same lexer that colours Blade decides whether a
directive exists in that context: comments, `@@` escapes, email addresses,
echo/PHP strings and verbatim contents do not acquire false hovers. `@section`
and `@php` distinguish inline from block forms; bare `@empty` is described as a
`@forelse` branch. The range covers only the directive name, including `@`,
using UTF-16 positions.

Help is bounded to documents of at most 128 KiB and the first 4096 recognized
directives, with at most 2 KiB of static output. It reads no files, settings,
project indexes or network resources. It covers the existing static
Laravel/Livewire directive names only; short descriptions follow the Laravel 12
Blade documentation and the Livewire 3 JavaScript directives, without claiming
version-aware support for every directive. Custom `Blade::directive` metadata,
argument/type resolution, PHP injection, installed framework version detection
and framework-specific documentation are not implemented.

## Navigation

Go to Definition on a recognized matched closing directive opens the nearest
properly nested opener in the current document, and a recognized block opener
navigates to its exact current-text matching closer. The selection covers the
actual token with UTF-16 coordinates. Inline section/PHP forms do not open
blocks; bare forelse `@empty` stays a branch. Crossed/missing blocks, unknown
custom directives, escaped directives, comments, PHP/echo strings and verbatim
contents do not acquire inferred block links. This provider reads no project
files, caches or network data, works on unsaved text, and refuses documents
over 128 KiB or 4096 recognized directives and malformed argument delimiters.
Laravel continues resolving named arguments; the providers cover different
tokens.

## PHP source boundary

Literal closing-tag text inside a PHP quoted string or block comment does not
resume Blade directive recognition; real PHP closing tags in scripting or line
comments still do. Hover, current-document block navigation and diagnostics
share this boundary with colouring. Basic quoted/escaped strings, backticks and
PHP8 attribute string contexts are covered; heredoc/nowdoc, complex string
interpolation and unterminated string/comment contexts keep the remaining
region opaque. This is a conservative lexical subset, without executing PHP or
adding PHP type/runtime resolution.

Directive argument boundaries and argument counts ignore PHP strings and
comments and require properly nested parentheses, brackets and braces. Comments
cannot manufacture directive tokens, inline-section arity or an outline name.
Section/push outlines require one complete ordinary literal first argument,
with its exact source span (at most 256 bytes); dynamic, concatenated, escaped
or interpolated names stay unlabelled.

PHP variable/function highlighting preserves complete UTF-8 identifier
spelling: a Unicode-suffixed variable such as `$thisé` is not attributed the
special `$this` role. These are lexical spans, not published PHP symbols.

### Literal capture roles

Section, push and prepend share one source-local role check across help,
diagnostics, outline/folds and closing-token navigation. An ordinary literal
name with one positional argument captures. With two admitted positional
arguments, section captures for literal null and is inline for an ordinary
string; push/prepend capture for an empty string and are inline for a nonempty
string or null. Comments around arguments preserve source ranges. Dynamic,
named, unpacked, escaped or interpolated forms have unknown roles; subsequent
inferred block relationships and absence diagnostics stop at that boundary.
No PHP values or framework runtime are evaluated.

Malformed argument recovery shares a per-document linear scan budget (eight
times source bytes). Exhaustion makes the remaining source opaque, withholding
inferred absence warnings and closing-token navigation there. Ordinary balanced
argument scans keep their existing behavior.

## Reference ownership

| Reference family | Owning plugin and contract |
|---|---|
| Directive names and block syntax | Blade owns lexical recognition, diagnostics, static hover and matching-directive navigation. Laravel owns completion, including custom registrations. |
| View paths, namespaces and components | Laravel owns framework-aware completion/navigation and configured view roots. A view spelling can map to a file path; it is not a directly replaceable file-name token. |
| Section/yield and push/prepend/stack names | Laravel owns cached inheritance/include relationships and related-file navigation. Equal literal names in unrelated templates do not establish one symbol. |
| Custom directive declarations | Laravel owns PHP registration discovery. Blade's static lexer does not invent registration ownership. |
| Embedded PHP/HTML variables and types | Language-server/native injection ownership; no Blade symbol graph is published. |

No `DocumentIndexer` is registered: API 1.3 uses its definitions/references for
automatic Rename as well as usages. A local section literal cannot prove that
inherited layouts, included views, dynamic names, runtime overrides and custom
registrations have no other references, so usages and Rename are not implemented.

## Embedded JavaScript and CSS

Literal closed, attribute-free script/style bodies publish bounded physical
UTF-16 injection ranges. The host can highlight these ranges and route
configured installed embedded language servers. Blade/PHP holes, ambiguous
tags and unsupported script/style modes remain excluded; directive/PHP
ownership stays with the existing lexer. This does not add full PHP semantics
or embedded formatting/Rename.

## Completion and formatting

Directive, route, view, config and translation completion in Blade files comes
from the `aritti.laravel` plugin (core directives with about 45 snippets such
as `@if(…) … @endif` and `@foreach`, plus custom directives found in `app/`);
no duplicate completion provider is added here, so install both plugins. There
is no formatter: OpenIDEA only runs Prettier and Ruff from a language
definition, and neither parses Blade.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
