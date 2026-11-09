# Spring Security Tools

Spring Security support for Java projects in OpenIDEA:

- **Spring Security 6 migration**:
  - a class extending `WebSecurityConfigurerAdapter` (removed: declare a `SecurityFilterChain` `@Bean`);
  - `@EnableGlobalMethodSecurity` (deprecated): source migration advisory; changing defaults/configuration requires manual review;
  - `.antMatchers(`/`.mvcMatchers(`/`.regexMatchers(` (removed): source migration advisories without automatic rewrites;
  - `.authorizeRequests(` (deprecated): source migration advisory without an automatic rewrite.
- **`ROLE_` prefix**: `hasRole('ROLE_ADMIN')`/`hasAnyRole(…)` in `@PreAuthorize`/`@PostAuthorize`/`@PreFilter`/`@PostFilter` and `.hasRole("ROLE_X")`/`.hasAnyRole(…)` calls. Receiver semantics and configured role prefixes require review; no automatic prefix rewrite is offered.
- **Plain-text passwords**: `NoOpPasswordEncoder.getInstance()` (warning), `{noop}` passwords passed to `.password(…)` (info).
- **Matcher order** (Java and Kotlin DSL, setting `springSecurity.checkMatchers`): a rule after `anyRequest()` in the same `authorizeHttpRequests` block (Spring Security rejects it at startup), and a narrower `requestMatchers` pattern shadowed by an earlier broader one (`/**` or `/x/**` before `/x/y`, same or unrestricted HTTP method; regex and dynamic patterns are skipped).
- **Debug logging** (setting `springSecurity.checkDebug`): `@EnableWebSecurity(debug = true)`, `web.debug(true)` in a `WebSecurityCustomizer` and `<debug/>` in the security XML namespace.
- **Completion** in the four method security annotations: `hasRole('…')`, `hasAnyRole(…)`, `hasAuthority('…')`, `hasAnyAuthority(…)`, `hasPermission(…, '…')`, `isAuthenticated()`, `isAnonymous()`, `isFullyAuthenticated()`, `isRememberMe()`, `permitAll`, `denyAll`, `principal`, `authentication`, `returnObject` (`@PostAuthorize`/`@PostFilter`), `filterObject` (`@PreFilter`/`@PostFilter`); inside `hasRole('…')`/`hasAnyRole('…')`, the role names the project already uses (`hasRole`/`hasAnyRole`, `@Secured`, `@RolesAllowed`, `ROLE_` stripped).

Text-based: Java and Kotlin sources (the same migration, `ROLE_` and password checks, including the Kotlin DSL's `authorizeRequests { … }`; completion stays Java-only) and Spring XML files. Calls match by method name; the matcher, `authorizeRequests` and `hasRole`/`hasAnyRole` call checks only run in files that import from `org.springframework.security`.

- **Security XML namespace** (`http://www.springframework.org/schema/security`, any prefix): `<intercept-url access>` expressions (unbalanced quotes/parentheses, `hasRole()` without argument, unknown `has…`/`is…` function names, redundant `ROLE_`), duplicate `pattern`s within one `<http>`, a `/**` catch-all placed before more specific patterns (not reported when the catch-all has a `method` or `servlet-path`, or `<http>` sets a regex `request-matcher` or a `request-matcher-ref`), `use-expressions="false"` and `<global-method-security>` (deprecated; use `<method-security>`). `unknown-security-function` is a warning only and can be a false positive for a custom `SecurityExpressionRoot` (methods such as `isOwner(#id)`); there is nothing to configure, ignore it. Plain role lists are not checked when `use-expressions="false"`.

- **Bean references** (Info, setting `springSecurity.checkBeanReferences`): `@WithUserDetails(userDetailsServiceBeanName = "…")`, `@bean.method()` in `@PreAuthorize`/`@PostAuthorize`/`@PreFilter`/`@PostFilter` expressions, and the `ref` attributes of the security XML namespace (`user-service-ref`, `authentication-manager-ref`, `entry-point-ref`, handler refs, `<password-encoder ref>`, `<jdbc-user-service data-source-ref>`) that no bean of the project answers. Beans are `@Component`-family classes, `@Bean` methods, repository interfaces and XML bean ids; beans of libraries and auto-configuration are invisible, so it stays quiet until the whole project was read and for a few well-known names.
- **More security XML checks**: required attributes (`<intercept-url pattern|request-matcher-ref>`, `<user name>`, `<password-encoder ref|hash>`, `<jdbc-user-service data-source-ref>`) and values of `create-session`, `request-matcher`, `requires-channel`, `method`, `session-fixation-protection` and `<global-method-security mode>`, from a hard-coded table.
- **Wider XML tables**: `true`/`false` values of `<http>` (`auto-config`, `once-per-request`, `disable-url-rewriting`, `servlet-api-provision`, `jaas-api-provision`), `<method-security>`, `<csrf>`, `<headers>`, `<anonymous>`, `<logout>`, `<form-login>`, `<concurrency-control>`, `<authentication-manager>`; `enabled`/`disabled` annotation switches of `<global-method-security>`; required `ref` of `<custom-filter>`; and bean references of `<cors>`, `<csrf>`, `<remember-me>`, `<concurrency-control>`, `<session-management>` and more `<http>` attributes. Written by hand from the schema, only for elements and attributes known completely; other elements are not checked.
- **Self-invocation** (opt-in, `springSecurity.checkSelfInvocation`, default off): a call from a method of a Spring bean class to another method of the same class that carries `@PreAuthorize`, `@PostAuthorize`, `@PreFilter` or `@PostFilter` skips the security proxy. Wrong with AspectJ mode, hence off by default. Java only.
- **Matcher navigation candidates**: Go to Definition on a security-namespace `<intercept-url pattern>` under an explicit `<http request-matcher="ant">` offers literal controller mapping candidates, filtered by an explicit HTTP method. Only literal path segments, single-segment `*` and a trailing `**` are admitted. Custom matcher references, servlet overrides, implicit/MVC/regex strategies, foreign XML and Java/Kotlin matcher calls stay unresolved; the former name-only Ant approximation on arbitrary Java calls has been removed. These source candidates do not establish effective authorization or MVC dispatch.

Editor checks use API 1.3 current-document diagnostics for Java, Kotlin and XML, including unsaved text. Activation/file events warm the bean cache outside provider requests; bean absence checks use only a fresh, complete cache while optional file events are available. Cold, stale, failed or oversized project facts remain quiet. Diagnostics, hover and inlay requests perform no project scans or file reads. Current input is bounded to 128 KiB, results to 500 diagnostics and 200 matcher hints. Line markers no longer publish duplicate diagnostics. Completion and external language-server ownership are unchanged. Matcher navigation uses the conservative explicit XML contract below. Mutation actions use the separately guarded FileActions resolver.

Matcher hover/inlays cover only security-namespace `<intercept-url>` under security-namespace `<http request-matcher="ant">`, with a fixed literal path and a known explicit HTTP method or unrestricted method. They describe the written Ant matcher and method; they do not infer precedence, covered controllers, authorization decisions or effective runtime policy. Dynamic/wildcard paths, custom matcher references, servlet-path overrides and implicit/MVC/regex matchers remain quiet. The existing `springSecurity.checkMatchers` switch also controls these hints; other existing check switches are preserved.

P23.1 is partial. Original local evidence in `spring-security.jar` includes `SpringSecurityMatchersInlayCollector.buildPresentation` over MVC mapping elements and `UrlMatcher.accept/acceptMethod`; that controller-mapping UI remains broader than these declaration hints. The original `SpringRequestPatternFormat` inspection description gates PathPattern findings on Spring Security 7+ and a non-Ant MVC strategy. The plugin cannot establish that version/classpath/strategy contract yet, so live diagnostics suppress the older unconditional `obsolete-path-pattern` warning; its parser remains tested but produces no published warning or quick fix. Modern `requestMatchers` hints, complete SpEL semantics, runtime filter-chain/authorization analysis, library bean resolution and version-aware migration guidance remain gaps. Existing migration/name-based source checks are advisory, not proof of resolved library symbols. Desktop diagnostic scheduling, hint rendering and close/discard lifecycle need native acceptance. No proprietary implementation was copied.


P23.3 adds one bounded source-local FileAction: remove an empty, self-closing
Security `<debug/>` declaration directly under a Spring Beans root. Namespace
identity is checked for both elements, including local prefix overrides. Unknown
attributes, paired tags, nested elements, malformed XML, DTD/entity-dependent
sources and oversized documents produce no action. The existing
`springSecurity.checkDebug` switch applies to listing and resolution.

The action previews the exact tag and empty replacement, then returns one
undoable current-file edit with no file operation, source write or process.
The original path, document language/text/version, concrete project root and
current project-visible source must still match; they are rechecked around confirmation.
Project reads include open unsaved buffers, so this is not a saved-on-disk check.
Host prepare remains after these reads, so
this is not an atomic filesystem/source-version precondition across that final
gap. Surrounding comments, whitespace, declarations and beans are untouched.
Removing this declaration does not prove effective runtime logging is disabled.

Original audited evidence is `SpringSecurityDebugActivatedInspection.visitDebug`
(which flags a resolved Debug DOM element but supplies no LocalQuickFix) and the
empty Security `Debug` model (`@ModelVersion(V_3_1)`). This narrowly scoped removal
is an independent addition. Existing migration/role CodeActions are preserved;
new FileActions do not assume matcher strategy, framework version, role prefix,
bean ownership or Java/Kotlin call resolution. General migration fixes, SpEL and
runtime policy remain outside this slice. Native preview/apply/Undo acceptance
for this new action remains unverified.

Not ported: SpEL language support beyond completion and these checks (highlighting, navigation), OAuth2/SAML configuration assistance, broader quick fixes for the XML checks.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## P23.2 literal role/bean ownership audit and source candidates

Security XML bean-reference attributes from the existing verified rule tables
(e.g. authentication-manager-ref and entry-point-ref on http, form-login handler
refs, password-encoder ref, data-source-ref and method-security references) now
receive current-text hover and Go to Definition candidates. The new provider
requires exact Security/Beans namespaces and a literal name, and targets only
one direct same-file `<bean id>` token. Duplicate IDs, name aliases, explicit
aliases, competing generated IDs and nested/profile bean collisions suppress
the candidate. Hover explains that source identity is not a resolved bean type,
effective Spring context or active security policy; bean class/value data is
never displayed. Unsaved edits replace these facts immediately.

The local path performs no project scan, cache warming, file read or network
operation. Non-matcher definition requests no longer warm the MVC mapping index.
Bounds:32KiB text,4096 XML events,512 elements,128-byte ASCII literal names and a
cooperative10ms parse budget. Escaped/entity/expression names, malformed/DTD XML,
unknown namespaces/attributes and unopened/foreign project paths are excluded.
References are half-open UTF-16 ranges; navigation remains read-only. Java/SpEL
bean ownership, alias traversal, imports, runtime auto-configuration and cross-file
resolution are not added by this slice. Prior live checks and atomic debug edits
remain unchanged.

Original20215 `SpringSecurityRolePsiReference` resolves role strings to synthetic
`SpringSecurityPomTargetRole` objects and collects module/JAM variants; the EL
provider also checks the actual expression-root method model. Those synthetic
targets are not replaceable declarations. Stripping or adding ROLE_ does not
establish a same-spelling complete owner set, particularly with custom prefixes
and role hierarchies. No role/bean automatic host Find Usages/Rename anchors are
published. Spring Beans retains its private broader bean model/core XML refs;
this Security-specific reference provider does not create a competing host graph.
A shared complete Spring context/type/alias ownership contract and semantic
Rename remain P23.2 gaps. Original source contracts were inspected locally; no
matching new hover screenshot or native desktop verification was produced here.

P23.3 now also checks an explicit Java `PathPatternRequestMatcher` factory's
missing leading slash. The supported source forms are an exact non-static import
(or the fully qualified Spring class) followed by
`withDefaults().matcher("api/users")` (API introduced in Security 6.5), or
`pathPattern("api/users")` (direct factory introduced in 7.0). Both upstream
implementations reject a path that does not begin with `/`. The existing
`springSecurity.checkPathPatterns` setting controls this finding and its fix.
The diagnostic uses current text; the FileAction previews `api/users` →
`/api/users`, then returns one undoable edit of that literal's content. Quotes,
comments, whitespace and imports stay unchanged; no file operation, source write,
process or request occurs.

This selects a documented **explicit API contract**, not a guessed project
version, MVC strategy or runtime authorization decision. It does not re-enable
the suppressed implicit `requestMatchers` PathPattern warning. Ant/MVC/custom
builders, intermediate builder variables/base paths, overloaded/dynamic or
concatenated arguments, escapes/interpolation/placeholders, Kotlin and unknown
imports stay quiet. Static path segments, single-star segments and a terminal
`**` are supported; wildcard placement is never rearranged. Closed ordinary
Java class source is required. Wildcard imports, potential local/static
shadowing, inherited/anonymous scopes, text blocks and Unicode escapes are
conservative unknowns. The source spelling is not native classpath resolution.

Listing and resolution retain concrete project/root/path, language, full text
and document-version checks; current project-visible text is checked around the
exact confirmation and candidate/setting ownership is recomputed. Unsaved
buffers are supported, without claiming saved disk bytes or a target-version
CAS across the final read/host-prepare gap. No Rename/role/bean graph is added.
No collision/creation operation is involved: only the selected existing literal
changes. The existing namespace-aware XML debug action remains available.

Comparison evidence: original `SpringRequestPatternFormatInspection` gates its
implicit matcher check on Security 7+ and non-Ant MVC strategy and registers no
quick fix. Its original-specific screenshot remains unavailable. This new
explicit-factory fix is an independent, smaller source-local operation, rather
than a transplantation of that inspection or a guess about its module model.
Primary contracts: [Spring Security 6.5.0 builder](https://github.com/spring-projects/spring-security/blob/6.5.0/web/src/main/java/org/springframework/security/web/servlet/util/matcher/PathPatternRequestMatcher.java)
and [7.0.0 direct factory/builder](https://github.com/spring-projects/spring-security/blob/7.0.0/web/src/main/java/org/springframework/security/web/servlet/util/matcher/PathPatternRequestMatcher.java).
Native follow-up: the explicit imported 6.5+ builder literal passed preview,
Cancel, atomic apply and notification Undo on the pinned API1.3 host. Exact
artifact and receipts: [Desktop40](../../docs/parity-2026-11-08-refresh/implementation/desktop-40-security-matcher/README.md).
Direct 7.0 factory, scheduling and original visual parity remain unverified
natively; source/unit/Wasm checks do not establish those workflows.


Namespace admission review fix: XML source-candidate navigation and atomic XML
intentions use a scoped namespace-aware reader plus decoded reserved-URI checks.
Illegal `xml`/`xmlns` rebinding, aliases/defaults of reserved URIs (including escaped
aliases), empty prefixed bindings and unbound used prefixes are withheld. Legal
implicit/explicit `xml:lang` and ordinary namespace shadowing remain supported.
Legacy definition fallback is guarded too, so a rejected local model cannot
reintroduce a target from that same namespace-invalid invoking document. This
changes source admission only; runtime/type ownership and Rename authority are
unchanged, as are the reviewed FileActions and existing exact atomic previews.

The fallback guard examines current text only, without I/O, capped at 512 KiB,
8,192 events and a 10 ms loop deadline. Larger/over-budget documents are quiet.
This remains a conservative XML 1.0 framework subset: namespace declarations
requiring entity normalization unsupported by the reader, and XML 1.1 prefixed
namespace undeclarations, can be withheld even if valid in a broader XML model.
It does not validate an entire imported framework model or every cached target
file. Prior native receipts identify their earlier artifact hashes; post-fix
package/regression tests do not relabel them as native acceptance.

P23.3 HTTP-method overload extension: the explicit 6.5+ builder and 7.0+ direct
factory also accept `(null, "path")` and `(HttpMethod.GET, "path")` forms (the
standard method constants, with an exact unshadowed Spring import or fully
qualified spelling). The preview changes only the second argument's literal
content. Method arguments, comments, imports and CRLF remain byte-for-byte
unchanged. Arbitrary method expressions, foreign/shadowed `HttpMethod`, static
imports, wrong arity and concatenation stay quiet. The same unsaved-buffer,
UTF-16, owner, source-version, setting and post-confirmation guards apply. This
extends the explicit documented API subset; inferred project/framework versions,
implicit matcher strategy, Kotlin and full semantic resolution remain separate.
The original pattern/debug inspections register no quick fix; the new behavior
uses independently verified public framework contracts and does not transplant
original messages. No new native desktop or original screenshot acceptance is
claimed by these source/MockHost/package checks.

P23.1 explicit Java source facts: admitted `PathPatternRequestMatcher` factories
now provide source-only hover and inlays for valid bounded path literals, including
the reviewed nullable/standard-HTTP-method overloads. The hover names the explicit
6.5+ builder or 7.0+ direct-factory contract and literal method/path; it does not
claim the dependency is installed, the classpath resolves, the declaration is used,
or any effective authorization/filter-chain policy. Invalid missing-slash paths
retain diagnostics/fixes and do not become successful matcher facts. Custom builders, unknown methods, dynamic/escaped syntax and shadowed source stay quiet. Current unsaved Java text, exact UTF-16 positions,
requested inlay ranges and the existing source bounds apply without discovery,
file reads, processes or HTTP. `springSecurity.matcherHover` and
`springSecurity.matcherInlays` independently control matcher facts; disabling path
syntax diagnostics does not silently disable these source facts. XML Ant behavior
is retained and now shares those explicit feature switches. Full semantic model,
Kotlin/implicit strategy, live scheduling/desktop acceptance and original visual
parity remain separate; no new native acceptance is claimed here.

P23.3 legacy-edit safety audit: lexical migration and role findings no longer emit
immediate CodeAction edits. In particular, the old annotation rewrite could change
an omitted `prePostEnabled` from false in `EnableGlobalMethodSecurity` to true in
`EnableMethodSecurity`; matcher replacements can change selected implementations,
and role-prefix rewrites depend on receiver/custom prefix semantics. An unrelated
Security import did not establish receiver ownership. Those findings remain
advisory without modifying source. The unsafe rewrite metadata was removed rather
than routed through a preview that would conceal unresolved semantic changes.
The proven previewed XML debug and explicit Java PathPattern fixes remain; their
owner/version/unsaved/UTF-16 checks and exact confirmation are unchanged. Complete
framework-aware migration and other safe semantic fixes remain P23.3 work; no new
native acceptance is inferred.

P23.1 modern Java declaration subset: literal String-varargs `requestMatchers`
inside a direct `authorizeHttpRequests` lambda now supplies current-document hover
and inlays when its receiver is a uniquely imported `HttpSecurity` method parameter.
Parenthesized and block lambdas are supported; aliases, qualified receiver chains,
local/anonymous types, shadowed binders and dynamic arguments
are withheld. The explicit `HttpMethod` plus String-pattern overload also supplies
the declared method, using a uniquely imported or fully qualified Spring
`HttpMethod` constant (or literal `null` for any method). Unresolved method
expressions, wildcard imports, inherited scopes and shadowed method-type roots
are withheld; no method is inferred from an arbitrary expression. Labels say “declared pattern; strategy unresolved”: neither the
selected Ant/MVC/PathPattern implementation nor matching controller endpoints or
effective authorization is inferred. The source is bounded to 64 KiB with capped
tokens/calls/patterns, and unsaved UTF-16 ranges and existing feature switches apply.
No provider discovery, filesystem, process or network operations are added. The
original MVC mapping popup and framework-aware strategy/model remain partial;
these source tests do not constitute native visual acceptance.

Security87 navigation safety: arbitrary Java/Kotlin matcher method names no longer
select controller targets through an assumed Ant strategy. XML navigation checks
namespace, parent strategy, method, custom matcher/servlet overrides and the
bounded literal/`*`/trailing-`**` subset before reading the mapping cache. Bean
reference navigation remains separately owned. All87 source tests and the pinned
API1.3 package check pass; this is not native endpoint resolution acceptance.

P23.1 diagnostic feature settings: migration, role-prefix, password, XML access
expression and XML attribute checks now have independent default-on toggles
(`checkMigration`, `checkRolePrefixes`, `checkPasswords`, `checkExpressions`,
`checkXmlAttributes` under `springSecurity`). The existing `checkMatchers` switch
also suppresses XML duplicate/unreachable-pattern diagnostics. Debug, path,
bean-reference and opt-in self-invocation switches retain their separate scope.
Each toggle changes diagnostic visibility, not the underlying framework facts or
explicit mutation actions. Hover/inlay switches stay independent. Java, Kotlin
and XML tests exercise switching groups without clearing unrelated findings and
restoring them on the same unsaved document. Settings are read once per group
per request, without filesystem or network work.

Security92 matcher admission follow-up: registered matcher hover/inlays now require
one balanced XML document with valid namespace bindings before emitting explicit
Ant facts. Invalid reserved bindings, unbound prefixes, incomplete/multiple roots,
external top-level content and DTD/reference input stay quiet. The same bounded
validator also protects XML definition fallback. Matcher hover uses half-open
literal ranges, so the closing quote at the exact end does not inherit the path
hover; this applies to XML, explicit PathPattern Java and modern Java declarations.
Both public-provider regressions were reproduced before the fixes. All92 tests
and pinned API1.3 build/package check pass; no new native acceptance is inferred.

Security96 adds a previewed Java `@EnableWebSecurity(debug=true)` action. It
requires a unique exact Spring annotation import on a top-level ordinary class,
no wildcard/static conflict, declared-name shadow or inherited scope, and only
one explicit `debug = true` literal argument. A single reviewed `true`→`false`
edit preserves comments, CRLF, annotation and surrounding bytes. Dynamic flags,
foreign/missing imports, nested/inherited declarations and malformed source stay
quiet. Existing project/path/language/version/source rechecks, cancellation,
`checkDebug` gating and exact confirmation apply. It changes that written flag;
effective runtime logging and other configuration remain unresolved. All96 tests
and pinned API1.3 package check pass; native apply/Undo acceptance is separate.

Security99 adds the corresponding reviewed `WebSecurity` parameter `.debug(true)`
literal action for a unique exact Spring import and a direct unchained method
parameter receiver. Aliases, qualified field chains, receiver reassignment/escape,
local/anonymous types, inherited scopes and nonliteral arguments are withheld.
The action previews one `true`→`false` edit and reuses the same stale/cancel/source
ownership checks; it does not establish effective runtime logging.

Review reproduced generic type-parameter shadowing in modern `HttpSecurity`
declaration hints and the new WebSecurity call candidate. These source subsets
now withhold Java angle/generic syntax instead of treating a type-variable spelling
as the imported framework type. The annotation debug action uses the same bound.
The conservative bound may also withhold source with a comparison `<` token; no
generic or flow/type resolver is claimed. All99 tests and pinned API1.3 package
check pass. Original96/92 evidence remains dated; no new native acceptance.
