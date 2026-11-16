# Spring Shell Tools

Spring Shell support for Java and Kotlin projects in OpenIDEA, covering the legacy `@ShellComponent`/`@ShellMethod` model, the Spring Shell 3.x `@Command` model and 4.x `name`/`alias` mappings. MIT; see `LICENSE`.

## Command index

- `@ShellMethod` methods are named by `key`, else by the method name in kebab-case (`addUser` → `add-user`).
- `@Command` methods use the combined class/method `command` words; the method name is used only when the combined name is fully empty. A class command with no method command words remains the class command.
- 3.x `command`/`alias` mappings and 4.x `name`/`alias` mappings are indexed for source lookups and description hover. The 3.x class command prefix and class alias prefix remain separate; the 4.x `CommandGroup.prefix` contributes to command names, and aliases retain their declared independent names. `@Command("description")` in 4.x describes the command rather than naming it. Global environment prefixes and effective runtime registration are unknown.
- A command name declared twice in the project is an error at each declaration; duplicate-command messages name the other file.

`@Command` indexing requires the actual `org.springframework.shell.command.annotation.Command` owner (explicit canonical import or fully qualified annotation); an unrelated Spring import does not make picocli's `Command` a Shell command. Inherited conventional `<method>Availability` lookup walks the literal project superclass chain (nearest wins); `@ShellMethod` commands inherited by a `@ShellComponent` subclass over a fully literal chain join the subclass for availability selectors and duplicate-command checks (group still comes from the declaring class).

## Checks

- **Availability**: on an indicator, `@ShellMethodAvailability` names commands in the declaring class; unknown literal selectors are checked locally only when the declaring class has no explicit supertype and all its legacy command keys are statically known. On a command it names exactly one indicator method (more than one/zero literal indicator names, and mixed `*` availability selectors, are reported). Known invalid Java return types and indicator parameters are reported; a command's literal indicator name is checked against local zero-argument methods. Missing modern providers stay unvalidated because external/runtime beans cannot be ruled out by a source cache.
- **Descriptions**: literal missing/empty legacy `@ShellMethod.value` descriptions are reported.
- **Component placement**: missing component placement is reported only on an unannotated class without explicit supertypes; unknown/meta annotations and inheritance stay quiet.
- **Options**: repeated explicit `Option.longNames` and literal `shortNames` characters, plus legacy `ShellOption.value` with the known command prefix, are checked for duplicates within the declaring method (long/short namespaces stay separate; implicit parameter names are never invented). Empty explicit names and legacy forms the supported registrar does not classify are reported. Literal nonnegative `@Option.arityMin` greater than `arityMax` is an advisory on an owned `@Command` method (the supported registrar raises the maximum to the minimum before registration). The 4.x profile checks duplicate explicit singular `longName`/`shortName` values and literal standard primitive defaults on `@Option` and `@Argument`, preserving the separate 3.x plural-name/arity contracts.
- **Defaults**: literal defaults are checked against the standard Spring conversion profile for all eight primitive families and explicitly qualified/imported Java wrappers or Kotlin built-ins — integer signs, decimal/hex, range limits, BMP decimal digits, floating-point decimal/hex exponents, suffixes, NaN/Infinity, boolean synonyms and UTF-16 character width. Plain top-level Kotlin primitive type aliases and chains resolve locally; cycles, type-variable shadowing, nullable/generic/unknown targets and unresolved aliases stay quiet. Blank/unset defaults and legacy `__NONE__`/`__NULL__` markers are not treated as ordinary conversion input. Diagnostics describe standard conversion, not runtime invalidity: custom converters and runtime defaults may differ, and no default is rewritten.
- **Aliases**: modern literal aliases participate in current-document collision checks against known primary commands and other aliases. Class alias arrays form one flattened prefix; each method alias string creates a separate alias; class-only aliases create no routes; unknown inherited/meta prefixes are not guessed. Empty aliases highlight the quotes.

## Completion

Command names and `*` in `@ShellMethodAvailability("…")`, existing group names in `group = "…"` / `@ShellCommandGroup("…")`, and `AvailabilityProvider` beans in `@CommandAvailability(provider = "…")`. Core 4.x provider completion uses the `org.springframework.shell.core.command.availability.AvailabilityProvider` interface and never borrows the legacy provider interface by simple name. Provider-name completion requires exactly one physical declaration among cached named bean facts with exact provider source proof; distinct declarations sharing a name stay quiet. Availability-indicator method completion requires exact written `Availability` return-type ownership; aliased, generic/typealias-shadowed, conflicting or unknown return types are not proposed.

## Hover

Current-document command methods, declared key/command literals and unambiguous legacy availability references show plain-text descriptions — legacy `@ShellMethod.value`, modern `@Command.description`, and modern class descriptions when the method has no explicit override. A legacy indicator selector borrows descriptions only from commands in its own declaring class; on a command, a literal indicator name does not receive an unrelated command-key description. Whole ordinary plain string literals only: escaped values, text blocks, constant expressions, Kotlin templates, missing/empty descriptions and ambiguous references stay quiet. No project reads are needed.

## Navigation and occurrences

Go to Definition from an availability name to the command method, from a provider name to its bean, and from `ShellMethodAvailability("name")` to an indicator inherited from a project superclass chain declared literally once. A gutter marker on a command method opens its availability method or provider (respecting explicit selection before convention and named selectors before wildcard; conventional/explicit name lookup skips argument-taking overloads). Beans come from stereotype classes and `@Bean` methods of the project; providers are `@Bean` methods returning `AvailabilityProvider` or classes implementing it.

Native editor highlights show source-owned command keys/selectors and availability indicator names in the current admitted class; code lenses show local occurrence counts and open the read-only picker independently of the caret, revalidating document text/version/path and project identity (stale or restarted sessions refuse old arguments). **Spring Shell: Find Literal Command or Availability Occurrences** lists literal occurrences in a supported closed Java or Kotlin command class, keeping command keys/selectors and availability indicator method names separate and excluding wildcard selectors.

## Actions

**Preview add explicit ShellComponent annotation…** is a source-local FileAction for a top-level, unannotated, no-supertype legacy command class: it inserts the fully qualified annotation without guessing an import or moving existing bytes. Wildcard imports, an `org` qualifier collision, foreign methods, nested or already annotated/inherited classes offer no fix. The exact insertion, file, document version and UTF-16 location are confirmed; current text/path/language/version, owning project and class proof are rechecked, and the host applies one atomic current-file edit with undo.

**Preview Rename Command Key…** (reviewed FileAction) applies one atomic batch to exact declaration and indicator-selector string contents in one admitted closed Java or Kotlin legacy command class or supported unprefixed modern class. It validates a new lowercase ASCII key (letter first, letters/digits/hyphens, at most 64 characters), shows the old/new literal keys, exact UTF-16 spans and one complete resulting source with the local physical-source/runtime limitation stated first. Explicit `@ShellMethod` aliases and normalized space-separated multiword command keys are supported: selecting one alias changes only its exact declaration and availability selectors; other aliases and command-side indicator method names (including camelCase indicators) keep their identity. An implicit method-derived command name is changed by adding an explicit annotation key/name; the method identifier stays unchanged. Duplicate aliases, ambiguous/duplicate declarations, dynamic keys, noncanonical whitespace, new-key collisions, unresolved command-side indicator methods, foreign/wildcard imports, inheritance, malformed/incomplete source and unsafe source expressions refuse. Existing bytes, comments, CRLF and UTF-16 ranges outside the selected literal contents are preserved. Optional `project.write` is needed to apply.

## How it works

Matching is text-based, using the shared declaration scanner rather than full compiler/type resolution. Checks that need the whole project wait until the project has been read once. Live checks use the API 1.3 diagnostics provider on the current editor text (up to 128 KiB), including unsaved changes, with no project discovery, file reads or network calls; source-local checks work without project access and cross-file checks use only cached source facts. Existing completion/navigation requests can warm the source cache. Local checks are bounded to 128 KiB, 128 types, 256 imports, 128 members per type, 512 total members and 500 diagnostics; malformed/unbalanced or mismatched-language documents stay quiet. Derived names support ASCII method identifiers; literal keys and UTF-16 source ranges support Unicode without guessing framework Unicode case conversion.

Annotation source ownership requires an explicit canonical import or written canonical name; aliased/static/conflicting imports, source type/typealias/generic shadows and shadowed package roots remain unknown and publish no command facts, hover or annotation-derived claims (fully qualified canonical annotations remain supported when only an unrelated short name is shadowed). The same exact written-owner proof applies to indicator return types, provider interface ownership, provider bean return references and built-in boxed/numeric option format checks. Java sources containing structural Unicode escape spellings are withheld from source models (established ordinary string/character literal decoding remains supported when it proves no structural delimiter change); escapes outside such literal tokens remain unknown. Ordinary Java/Kotlin string/character escapes, safe Unicode escapes and Java octal escapes are decoded for the option/alias checks while retaining raw UTF-16 spans; Kotlin interpolation, Java text-block preprocessing and token-structure-altering Unicode escapes remain compiler-owned. These are declared source facts, not classpath, meta-annotation or type resolution, runtime registration or assignability.

Required capability: `project.read`. Optional: `editor.read`.

## Not implemented

Shell script files, an interactive command list, runtime option-value completion, compiler/classpath-based resolution, compiler-evaluated constants, meta-annotation/library activation, inherited/overloaded external resolution, runtime option naming/conversion, runtime bean/provider typing, automatic native Rename/Find Usages (the host inserts arbitrary rename text into anchors unchecked, so the plugin keeps its usages/Rename graph disabled and uses the reviewed action), and the Spring Shell 4.x runtime model beyond the source mappings above (4.x uses a different namespace; no 3.x rule is projected onto it).

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
