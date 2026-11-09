# Spring Shell Tools

Spring Shell support for Java and Kotlin projects in OpenIDEA, for the legacy and newer Spring Shell 3.x annotation models:

- **Command index**: `@ShellMethod` methods (named by `key`, else by the method name in kebab-case: `addUser` → `add-user`) and `@Command` methods (the combined class/method `command` words; the method name is used only when that combined name is empty).
- **Availability checks**: on an indicator, `@ShellMethodAvailability` names commands in the declaring class; unknown literal selectors are checked locally only when the declaring class has no explicit supertype and all its legacy command keys are statically known. On a command it instead names one indicator method. Known invalid Java return types and indicator parameters are reported. Missing modern providers stay unvalidated because external/runtime beans cannot be ruled out by a source cache.
- **Duplicate commands**: a command name declared twice in the project is an error at each declaration.
- **Completion**: command names and `*` in `@ShellMethodAvailability("…")`, existing group names in `group = "…"` / `@ShellCommandGroup("…")`, `AvailabilityProvider` beans in `@CommandAvailability(provider = "…")`.
- **Description hover**: current-document command methods, declared key/command literals and unambiguous legacy availability references show plain-text descriptions. Legacy `@ShellMethod.value` and modern `@Command.description` are supported, including modern class descriptions when the method has no explicit override. Constant expressions, Kotlin templates, missing/empty descriptions and ambiguous references stay quiet. No project reads are needed.
- **Navigation**: Go to Definition from an availability name to the command method and from a provider name to its bean; a gutter arrow on a command method opens its availability method or provider.

Matching is text-based. `@Command` indexing requires the actual `org.springframework.shell.command.annotation.Command` owner (explicit import or fully qualified annotation); an unrelated Spring import no longer makes picocli's `Command` a Shell command. Beans come from stereotype classes and `@Bean` methods of the project; providers are `@Bean` methods returning `AvailabilityProvider` or classes implementing it. Checks that need the whole project wait until the project has been read once.

Not ported: shell script files, an interactive command list, runtime option-value completion and compiler/classpath-based resolution.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

Live checks use the API 1.3 diagnostics provider on the current editor text (up to
128 KiB), including unsaved changes. This request performs no project discovery,
file reads or network calls. Source-local checks work without project access;
cross-file checks use only cached source facts. Closed-class legacy selector checks use current text; unknown inherited/dynamic keys
and external provider types remain unvalidated. Existing
completion/navigation requests can warm the source cache. Gutter requests no
longer manually publish a second set of diagnostics.

P24.2 is partial: declared-description hover is implemented; indexed usages and
rename remain unregistered. Existing command anchors serve navigation/diagnostics,
not unconditional text replacement:

| Source form | Why the existing anchor is insufficient for rename |
|---|---|
| Derived `listUsers` → `list-users` | Method identifier and command spelling differ. |
| Modern class prefix + method words/array | A full command can span several literals and declarations. |
| Whitespace-normalized or escaped key | Decoded command text differs from the source bytes. |
| Wildcard/dynamic availability value | It does not identify one exact replaceable command name. |
| Equal keys in multiple classes/files | Resolution falls back from the declaring class to project matches; a unique owner and project scope need separate index fixtures. |

Even exact single literal subsets need owner-aware index declarations/references,
duplicate/alias tests and project separation before usages/rename can be published.
The anchor audit has regression fixtures; no navigation span is exported as a
rename symbol. Additional generator parity remains separate work.

Description fields were checked against Spring Shell v3.4.0 annotation sources:
[`ShellMethod`](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-standard/src/main/java/org/springframework/shell/standard/ShellMethod.java)
and [`Command`](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/Command.java).
The original `SpringShellCommandInspection` decompiled sample confirms legacy
`value` description handling; `SpringShellReferencesInspection` excludes polyadic
expressions from its reference check. These samples do not establish a visual
hover contract. Original-specific screenshots and desktop hover verification
remain unavailable.


### P24.1 verified source-local rules (bounded)

The existing live diagnostics owner now also reports literal missing/empty legacy
`@ShellMethod.value` descriptions; mixed `*` availability selectors; more than
one/zero literal indicator names when availability is placed on a command; and
arguments on explicitly annotated availability indicators. Command methods are
not required to return `Availability`. Explicit indicator references navigate and
complete local zero-argument availability methods, and gutter lookup respects
explicit selection before convention and named selectors before wildcard.
Conventional/explicit name lookup skips argument-taking overloads. Unknown return
types, Kotlin inferred/aliased return types and dynamic selector expressions stay
unvalidated. New annotation rules require supported annotation ownership.

For the 3.x modern model, literal nonnegative `@Option.arityMin` greater than
`arityMax` is an advisory on an owned `@Command` method: the supported 3.4 registrar raises the maximum to the minimum before registration. Constants, expressions,
unset negative sentinels and unknown/foreign Option annotations are skipped.
Every literal entry in `@CommandAvailability.provider` is now available to
source navigation against the existing bean cache. Absence warnings have been
removed because a source-only cache cannot exclude external/runtime beans.
No diagnostics are published from gutter requests, and the source-local checks
perform no project reads, scans or network requests.

Rules were compared with Spring Shell **v3.4.0** primary sources:
[StandardMethodTargetRegistrar](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-standard/src/main/java/org/springframework/shell/standard/StandardMethodTargetRegistrar.java)
(availability roles, wildcard and no-arg/exact return contract),
[Option](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/Option.java)
and [CommandRegistration](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/CommandRegistration.java)
(arity bounds), plus the existing original `SpringShellCommandInspection`
decompilation (description inspection) and `SpringShellReferencesInspection`
(expression exclusions). Description checks do not evaluate constants or infer
whether a custom/meta annotation supplies a ShellComponent.

**Spring Shell 4 is not covered by this slice.** Its
[official migration guide](https://github.com/spring-projects/spring-shell/wiki/v4-migration-guide)
moves annotations to `org.springframework.shell.core.command.annotation`, removes
legacy annotations and class-level Command, and renames `command` to `name`.
The new 3.x checks are not applied to those different owners. This does not
supply a 4.x model, runtime option/converter validation, inherited availability resolution or
runtime provider typing. Checks use the shared declaration
scanner rather than full compiler/type resolution. Desktop diagnostics and
navigation remain unverified for this change.


### P24.1 final bounded local-rule audit

The supported **literal, explicit-owner, closed-class 3.x subset** now covers the
three audited original command-inspection concerns: description presence,
component placement and availability specification/reference shape. Missing
component placement is reported only on an unannotated class without explicit
supertypes; unknown/meta annotations and inheritance stay quiet. A command's
literal indicator name is checked against local zero-argument methods (with
implicit Object names excluded from absence inference). Conversely, an
indicator's literal selectors are checked against declared local legacy keys
without waiting for project discovery. Empty references highlight their quotes.
Multiple named indicators for one command are reported only when no explicit or
conventional local indicator, or inherited uncertainty, takes precedence.
Wildcard mixtures, indicator arguments/known wrong return types and modern 3.x
literal arity range checks remain covered.

Ownership now applies consistently to legacy/modern command indexing, groups,
references, descriptions, bean declarations and typed provider completion.
Foreign, wildcard-only, aliased/unknown ownership and dynamic key/group
expressions do not become framework facts. Positive fixtures use explicit
imports. Provider absence is deliberately quiet; known source beans remain
navigable without claiming runtime availability or type assignability. Local
checks use the current text with no discovery/I/O, bounded to 128 KiB, 128 types,
256 imports, 128 members per type, 512 total members and 500 diagnostics.
Malformed/unbalanced or mismatched-language documents stay quiet. Derived names
are supported for ASCII method identifiers; literal keys and UTF-16 source
ranges support Unicode without guessing framework Unicode case conversion.

Primary sources rechecked: v3.4.0
[StandardMethodTargetRegistrar](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-standard/src/main/java/org/springframework/shell/standard/StandardMethodTargetRegistrar.java),
[ShellComponent](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-standard/src/main/java/org/springframework/shell/standard/ShellComponent.java),
[Option](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/Option.java),
[AvailabilityProvider](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/AvailabilityProvider.java)
and [Utils](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/Utils.java),
plus the official [3.4 availability guide](https://docs.spring.io/spring-shell/reference/3.4/commands/availability.html).
The current official [v4 migration guide](https://github.com/spring-projects/spring-shell/wiki/v4-migration-guide)
confirms a different namespace and model; no 3.x rule is projected onto it.
Original receipts contain no Spring Shell-specific screenshot, so no visual
parity is claimed.

This completes the audited local concerns **within the subset above**, not all
P24.1/framework parity. Remaining cases are compiler-evaluated constants,
meta-annotation/library activation, inherited/overloaded external resolution,
runtime option naming/conversion/type resolution and runtime bean/provider typing and the 4.x model. Those are implementation/type
resolution gaps, not newly claimed SDK blockers. No Rename/usages provider or new process/network behavior was added; the later
annotation action below is separately previewed. Native
diagnostic/hover/navigation acceptance remains unverified.


### Literal options, aliases and a guarded annotation fix (P24.1/P24.3)

Source-local option checks now cover explicit modern `Option.longNames` and
literal `shortNames` character arrays, plus legacy `ShellOption.value` and the
known command prefix. Repeated names are identified within the declaring method
before runtime naming modifiers. Empty explicit names and legacy forms the
supported registrar does not classify are reported. Long/short namespaces stay
separate; implicit parameter names are not invented when compiler parameter-name
metadata is unknown. Constants, expressions and foreign,
wildcard-only or unresolved annotation owners stay quiet.

Literal defaults are checked against the standard Spring conversion profile for
all eight primitive families and explicitly qualified/imported Java wrappers or
Kotlin built-ins. Integer signs, decimal/hex, range limits and JDK 17 BMP decimal
digits are modeled; floating-point decimal/hex exponents, suffixes, NaN/Infinity,
boolean synonyms and UTF-16 character width are covered. Plain top-level Kotlin
primitive type aliases and chains are resolved locally; cycles, type-variable
shadowing, nullable/generic/unknown targets and unresolved aliases stay quiet.
Blank/unset defaults and legacy `__NONE__`/`__NULL__` markers are not treated as
ordinary conversion input. Diagnostics explicitly describe **standard
conversion**, not runtime invalidity: custom converters and runtime defaults may
differ, and no default is rewritten automatically.

Modern literal aliases now participate in current-document collision checks
against known primary commands and other aliases. Class alias arrays form one
flattened prefix; each method alias string creates a separate alias. Class-only
aliases create no routes, and unknown inherited/meta prefixes are not guessed.
Full-name hashing is collision-guarded by exact streamed comparison, with shared
prefixes rather than repeated expanded strings; all admitted literal entries are
examined within the existing document/output budget. Empty aliases highlight the
quotes. These are declared-route checks, not Rename/usages anchors or proof of
runtime registration. Corrected primary naming follows the actual 3.4 factory:
a class command with no method command words remains the class command; only a
fully empty combined command falls back to the method name. Reversed literal
arity bounds are an advisory because that factory normalizes them upward.

**Preview add explicit ShellComponent annotation…** is a source-local FileAction
for a top-level, unannotated, no-supertype legacy command class. It inserts the
fully qualified annotation without guessing an import or moving/replacing any
existing bytes. Wildcard imports, an `org` type/type-variable/alias qualifier
collision, foreign methods, nested or already annotated/inherited classes offer
no fix. The exact insertion, file, document version and UTF-16 location are
confirmed. Current text/path/language/version, owning project and class proof
must still match, with visible-source freshness checks before/after confirmation.
The host applies one atomic current-file edit with Undo. Unsaved source is
supported; the final read/prepare gap remains, and component scanning,
construction, classpath and actual runtime registration are not certified.

The contracts were verified against the actual
[CommandRegistrationFactoryBean](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/support/CommandRegistrationFactoryBean.java),
[CommandAnnotationUtils](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-core/src/main/java/org/springframework/shell/command/annotation/support/CommandAnnotationUtils.java),
[ShellOption](https://github.com/spring-projects/spring-shell/blob/v3.4.0/spring-shell-standard/src/main/java/org/springframework/shell/standard/ShellOption.java),
[option naming documentation](https://docs.spring.io/spring-shell/reference/3.4/options/naming.html),
[NumberUtils](https://github.com/spring-projects/spring-framework/blob/v6.2.0/spring-core/src/main/java/org/springframework/util/NumberUtils.java),
[StringToBooleanConverter](https://github.com/spring-projects/spring-framework/blob/v6.2.0/spring-core/src/main/java/org/springframework/core/convert/support/StringToBooleanConverter.java)
and [Character conversion](https://github.com/spring-projects/spring-framework/blob/v6.2.0/spring-core/src/main/java/org/springframework/core/convert/support/StringToCharacterConverter.java).
JDK numeric details were checked against its
[Character contract](https://docs.oracle.com/en/java/javase/17/docs/api/java.base/java/lang/Character.html),
[Unicode data](https://github.com/openjdk/jdk17u/blob/master/make/data/unicodedata/UnicodeData.txt)
and [floating-point parser](https://github.com/openjdk/jdk17u/blob/master/src/java.base/share/classes/jdk/internal/math/FloatingDecimal.java).

Ordinary Java/Kotlin string/character escapes, safe Unicode escapes and Java
octal escapes are decoded for the new option/alias checks while retaining raw
UTF-16 source spans. Kotlin interpolation, Java text-block preprocessing and
Java Unicode escapes that alter token structure remain compiler-owned. Existing
navigation/index anchors retain their stricter literal contract.

The remaining local uncertainties are compiler-evaluated constants and structural escapes,
import/type-alias/generic or inherited metadata that cannot be established from
the current source model, runtime naming modifiers/converters and external
registration/provider context. No 4.x migration is inferred from 3.x owners.
The new annotation fix's native preview/apply/Undo acceptance is unverified.

P24.2 availability hover ownership follow-up: a legacy indicator selector only
borrows descriptions from commands in its own declaring class. It no longer
falls back to another class's equal key. On a command, ShellMethodAvailability's
literal names an indicator method and does not receive an unrelated command-key
description. Existing definition behavior and same-class/unsaved descriptions
remain. Both cross-class and command/indicator-role hover defects were reproduced
before fixes. All47 tests and pinned API1.3 build/package check pass. Exact
scoped host usages/Rename remains partial; no new native acceptance is inferred.

P24.2 reviewed local command-key Rename: an explicit FileAction validates a new
lowercase ASCII key (letter first, letters/digits/hyphens, max64) and shows the
old/new literal keys, exact UTF-16 spans and one complete resulting source, with
the local physical-source/runtime limitation first. It applies one atomic batch to exact declaration
and indicator-selector String contents in one admitted Java class. The class
must be the only top-level ordinary class, explicitly ShellComponent, without
inheritance/nested/generic/unknown annotation context. Every command key must be
an explicit single exact unescaped literal, and every indicator selector must be
known literal input. Aliases/derived/dynamic/duplicate names, malformed/incomplete
source, unresolved command-side indicator methods and foreign/wildcard imports
refuse. New-key collisions refuse. Wildcard selectors and command-side indicator
method-name strings remain unchanged. Existing bytes/comments/CRLF/UTF16 ranges
are preserved outside the selected literal contents.

The operation checks current project/path/class/version/source and candidate
ownership before/after prompts, uses bounded disposable opaque tokens, supports
project-visible unsaved source and refuses oversized complete previews. Pinned
host automatic indexed Rename performs unchecked raw new-name insertion; this
plugin therefore keeps its host String usages/Rename graph disabled and uses the
validated action instead. Runtime/global command registry, other files/classes,
Kotlin and indexed/global usage resolution remain outside this supported source
operation. P24.2 stays partial. All53 tests and pinned API1.3 build/package check
pass; native preview/apply/Undo on this new action remains unverified.

Shell54 compact review follow-up: Native57 reported the final runtime caveat in
the53 before+after dialog was visually clipped even after scrolling. The54 dialog
puts that essential local physical-source/runtime limit first, lists the old/new
literal and exact span count with1-based end-exclusive UTF-16 ranges, then shows
one complete resulting source. Mutation/ownership rules are unchanged and
confirm_exact still bounds the entire body. The oversized-preview fixture was
expanded to remain over that same byte bound after removing duplicate source.
All54 tests and pinnedAPI1.3 build/package check pass. Native53 stays dated;
a separate54 visual review/Cancel acceptance is required.

Annotation source ownership requires an explicit canonical import or written
canonical name. Aliased/static/conflicting imports, source type/typealias/generic
shadows, and shadowed package roots remain unknown and do not publish command
hover or annotation-derived claims. Fully qualified canonical annotations remain
supported when only an unrelated short name is shadowed. This is bounded source
ownership, not classpath/meta-annotation/type resolution. Native54 local rename
review/Apply/Undo evidence belongs to its frozen artifact; later ownership guards
have separate source validation and no new native claim.

Availability-indicator method completion uses the same exact written type owner
proof for its return type. Aliased, generic/typealias-shadowed, conflicting or
unknown `Availability` return types are not proposed as known Spring indicators;
an explicit canonical return type remains supported despite a short-name shadow.
No inferred return type, classpath assignability or runtime indicator check is
performed.

Description hover accepts whole ordinary plain string literals. Escaped values
and text blocks stay quiet because this source scanner does not implement their
full Java/Kotlin value semantics. Dynamic/concatenated descriptions and Kotlin
templates remain unknown; plain descriptions retain their exact source spans
and Markdown escaping. This does not evaluate annotation expressions.

Java sources containing structural Unicode escape spellings are withheld from source models:
Java preprocesses eligible escapes before comments and tokens, and this scanner
does not provide that preprocessing with an exact source offset map. Established
ordinary string/character literal decoding remains supported when the existing
decoder proves no structural delimiter change. Escapes outside such literal
tokens (including comments/identifiers) remain unknown. This quiet
boundary applies to current hover/navigation/completion/diagnostics and indexed
source replacement. It avoids treating compiler-commented text as declarations;
no Unicode-expression evaluation or approximate source rewrite is performed.

AvailabilityProvider bean candidate completion also requires exact written
Spring interface ownership for method return types and class implements clauses.
Generic/typealias/import shadows and aliased imports do not establish that
interface; an explicit canonical type remains supported under a short shadow.
This remains declared source evidence, without effective assignability/runtime
registration proof.

A provider bean method returning a local implementing class requires an exact
unambiguous reference to that top-level source declaration. A foreign qualified
return type or generic/typealias/import shadow cannot borrow another class's
same simple name. Plain and fully qualified local source return types remain
supported; indirect inheritance and arbitrary type assignability remain unknown.

Provider-name completion requires exactly one physical declaration among the
cached named bean facts and exact provider source proof for that declaration.
Distinct declarations sharing a name stay quiet, including mixed known/unknown
return types; repeated aliases on the same physical declaration are deduplicated.
This is uniqueness within the bounded source inventory, not the runtime registry.

Availability-provider gutter targets share the unique cached physical name and
exact provider source proof used by candidate completion. Duplicate names or
unknown provider types produce no guessed first target. Ordinary definition
navigation remains a list of matching declared bean locations rather than a
runtime binding/type assertion.

Built-in boxed/numeric option format checks use exact written type ownership,
including Kotlin import alias and package-root guards. A foreign aliased type
cannot borrow `Integer` or `kotlin.Int` spelling to trigger a built-in converter
warning. Established plain Kotlin typealiases and safe literal decoding retain
separate source tests; custom/effective converters remain unknown.

Wrong-return diagnostics also leave short generic/typealias return names unknown.
A generic called `String` can have an Availability bound; its spelling alone is
not proof of `java.lang.String`. Explicit canonical wrong return types retain
their known source checks. No generic erasure/type inference is performed.
