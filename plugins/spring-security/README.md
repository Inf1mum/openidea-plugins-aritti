# Spring Security Tools

Spring Security support for Java, Kotlin and Spring XML files in OpenIDEA.
MIT; see `LICENSE`.

Checks are text-based: calls match by method name, and the matcher,
`authorizeRequests` and `hasRole`/`hasAnyRole` call checks only run in files that
import from `org.springframework.security`. Completion is Java-only; the other
checks cover Java and Kotlin sources (including the Kotlin DSL's
`authorizeRequests { … }`) and Spring XML files.

## Checks

- **Spring Security 6 migration**: a class extending `WebSecurityConfigurerAdapter` (removed: declare a `SecurityFilterChain` `@Bean`); `@EnableGlobalMethodSecurity` (deprecated); `.antMatchers(`, `.mvcMatchers(`, `.regexMatchers(` (removed); `.authorizeRequests(` (deprecated). All are source migration advisories without automatic rewrites: the old annotation rewrite could change an omitted `prePostEnabled` from false to true, matcher replacements can change implementations, and role-prefix rewrites depend on receiver/custom prefix semantics, so these findings never modify source.
- **`ROLE_` prefix**: `hasRole('ROLE_ADMIN')`/`hasAnyRole(…)` in `@PreAuthorize`/`@PostAuthorize`/`@PreFilter`/`@PostFilter` and `.hasRole("ROLE_X")`/`.hasAnyRole(…)` calls. Receiver semantics and configured role prefixes require review; no automatic prefix rewrite is offered.
- **Plain-text passwords**: `NoOpPasswordEncoder.getInstance()` (warning), `{noop}` passwords passed to `.password(…)` (info).
- **Matcher order** (Java and Kotlin DSL, setting `springSecurity.checkMatchers`): a rule after `anyRequest()` in the same `authorizeHttpRequests` block (Spring Security rejects it at startup), and a narrower `requestMatchers` pattern shadowed by an earlier broader one (`/**` or `/x/**` before `/x/y`, same or unrestricted HTTP method; regex and dynamic patterns are skipped). The same switch also suppresses XML duplicate/unreachable-pattern diagnostics and the matcher hover/inlay hints.
- **Debug logging** (setting `springSecurity.checkDebug`): `@EnableWebSecurity(debug = true)`, `web.debug(true)` in a `WebSecurityCustomizer` and `<debug/>` in the security XML namespace.
- **Self-invocation** (opt-in, `springSecurity.checkSelfInvocation`, default off): a call from a method of a Spring bean class to another method of the same class that carries `@PreAuthorize`, `@PostAuthorize`, `@PreFilter` or `@PostFilter` skips the security proxy. Wrong with AspectJ mode, hence off by default. Java only.
- **Bean references** (Info, setting `springSecurity.checkBeanReferences`): `@WithUserDetails(userDetailsServiceBeanName = "…")`, `@bean.method()` in `@PreAuthorize`/`@PostAuthorize`/`@PreFilter`/`@PostFilter` expressions, and the `ref` attributes of the security XML namespace (`user-service-ref`, `authentication-manager-ref`, `entry-point-ref`, handler refs, `<password-encoder ref>`, `<jdbc-user-service data-source-ref>`) that no bean of the project answers. Beans are `@Component`-family classes, `@Bean` methods, repository interfaces and XML bean ids; beans of libraries and auto-configuration are invisible, so it stays quiet until the whole project was read and for a few well-known names.

### Security XML namespace

For `http://www.springframework.org/schema/security`, any prefix:

- `<intercept-url access>` expressions: unbalanced quotes/parentheses, `hasRole()` without argument, unknown `has…`/`is…` function names, redundant `ROLE_`. `unknown-security-function` is a warning only and can be a false positive for a custom `SecurityExpressionRoot` (methods such as `isOwner(#id)`); there is nothing to configure, ignore it. Plain role lists are not checked when `use-expressions="false"`.
- Duplicate `pattern`s within one `<http>`, and a `/**` catch-all placed before more specific patterns (not reported when the catch-all has a `method` or `servlet-path`, or `<http>` sets a regex `request-matcher` or a `request-matcher-ref`).
- `use-expressions="false"` and `<global-method-security>` (deprecated; use `<method-security>`).
- Required attributes (`<intercept-url pattern|request-matcher-ref>`, `<user name>`, `<password-encoder ref|hash>`, `<jdbc-user-service data-source-ref>`, `<custom-filter ref>`) and values of `create-session`, `request-matcher`, `requires-channel`, `method`, `session-fixation-protection` and `<global-method-security mode>`, from a hard-coded table.
- `true`/`false` values of `<http>` (`auto-config`, `once-per-request`, `disable-url-rewriting`, `servlet-api-provision`, `jaas-api-provision`), `<method-security>`, `<csrf>`, `<headers>`, `<anonymous>`, `<logout>`, `<form-login>`, `<concurrency-control>`, `<authentication-manager>`; `enabled`/`disabled` annotation switches of `<global-method-security>`; bean references of `<cors>`, `<csrf>`, `<remember-me>`, `<concurrency-control>`, `<session-management>` and more `<http>` attributes. Written by hand from the schema, only for elements and attributes known completely; other elements are not checked.

## Completion

In the four method security annotations: `hasRole('…')`, `hasAnyRole(…)`,
`hasAuthority('…')`, `hasAnyAuthority(…)`, `hasPermission(…, '…')`, `isAuthenticated()`,
`isAnonymous()`, `isFullyAuthenticated()`, `isRememberMe()`, `permitAll`, `denyAll`,
`principal`, `authentication`, `returnObject` (`@PostAuthorize`/`@PostFilter`),
`filterObject` (`@PreFilter`/`@PostFilter`); inside `hasRole('…')`/`hasAnyRole('…')`,
the role names the project already uses (`hasRole`/`hasAnyRole`, `@Secured`,
`@RolesAllowed`, `ROLE_` stripped).

## Hover, inlays and navigation

- **XML Ant matcher hover/inlays** cover only security-namespace `<intercept-url>` under security-namespace `<http request-matcher="ant">`, with a fixed literal path and a known explicit HTTP method or unrestricted method. They describe the written Ant matcher and method; they do not infer precedence, covered controllers, authorization decisions or effective runtime policy. Dynamic/wildcard paths, custom matcher references, servlet-path overrides and implicit/MVC/regex matchers remain quiet. Hover uses half-open literal ranges, so the closing quote at the exact end does not inherit the path hover.
- **PathPatternRequestMatcher Java hover/inlays**: admitted explicit factories (`withDefaults().matcher("api/users")` from the Security 6.5 builder, `pathPattern("api/users")` from the 7.0 direct factory, including the `(null, "path")` and `(HttpMethod.GET, "path")` overloads with an exact unshadowed Spring import or fully qualified spelling) provide source-only hover and inlays for valid bounded path literals. The hover names the explicit 6.5+ builder or 7.0+ direct-factory contract and literal method/path; it does not claim the dependency is installed, the classpath resolves, the declaration is used, or any effective authorization/filter-chain policy. Invalid missing-slash paths retain diagnostics/fixes and do not become matcher facts. Controlled by `springSecurity.matcherHover` and `springSecurity.matcherInlays`; disabling path syntax diagnostics does not silently disable these source facts.
- **Modern Java declaration hints**: literal String-varargs `requestMatchers` inside a direct `authorizeHttpRequests` lambda supplies current-document hover and inlays when its receiver is a uniquely imported `HttpSecurity` method parameter (parenthesized and block lambdas; aliases, qualified receiver chains, local/anonymous types, shadowed binders and dynamic arguments are withheld). The explicit `HttpMethod` plus String-pattern overload also supplies the declared method. Labels say "declared pattern; strategy unresolved": neither the selected Ant/MVC/PathPattern implementation nor matching controller endpoints or effective authorization is inferred.
- **Matcher navigation candidates**: Go to Definition on a security-namespace `<intercept-url pattern>` under an explicit `<http request-matcher="ant">` offers literal controller mapping candidates, filtered by an explicit HTTP method. Only literal path segments, single-segment `*` and a trailing `**` are admitted. Custom matcher references, servlet overrides, implicit/MVC/regex strategies, foreign XML and Java/Kotlin matcher calls stay unresolved. These source candidates do not establish effective authorization or MVC dispatch.
- **XML bean references**: security XML bean-reference attributes from the rule tables (e.g. `authentication-manager-ref` and `entry-point-ref` on `http`, form-login handler refs, `password-encoder ref`, `data-source-ref` and method-security references) receive current-text hover and Go to Definition candidates. The provider requires exact Security/Beans namespaces and a literal name, and targets only one direct same-file `<bean id>` token. Duplicate IDs, name aliases, explicit aliases, competing generated IDs and nested/profile bean collisions suppress the candidate. Hover explains that source identity is not a resolved bean type, effective Spring context or active security policy; bean class/value data is never displayed. Unsaved edits replace these facts immediately.
- **SpEL injection ranges**: canonical PreAuthorize/PostAuthorize/PreFilter/PostFilter Java/Kotlin annotation literals publish exact contiguous `spel` ranges. Embedded provider routing uses the SpEL language provided by Spring Beans Navigator when installed; the released embedded-color renderer does not consume plugin syntax tokens, so embedded coloring is not claimed.

## Quick fixes

The migration, `ROLE_` and matcher findings are advisory and return no edits. The
available actions all preview the exact change, recheck project ownership, language,
document version and current project-visible text around the confirmation, and
return one undoable current-file edit with no file operation, source write or
process (unsaved buffers are supported; this is not a saved-on-disk check):

- **Remove `<debug/>`**: removes an empty, self-closing Security `<debug/>` declaration directly under a Spring Beans root. Namespace identity is checked for both elements, including local prefix overrides. Unknown attributes, paired tags, nested elements, malformed XML, DTD/entity-dependent sources and oversized documents produce no action. Gated by `springSecurity.checkDebug`. Removing the declaration does not prove effective runtime logging is disabled.
- **`@EnableWebSecurity(debug=true)` → `false`**: requires a unique exact Spring annotation import on a top-level ordinary class, no wildcard/static conflict, declared-name shadow or inherited scope, and only one explicit `debug = true` literal argument. A single `true`→`false` edit preserves comments, CRLF, annotation and surrounding bytes.
- **`WebSecurity` parameter `.debug(true)` → `false`**: requires a unique exact Spring import and a direct unchained method parameter receiver; aliases, qualified field chains, receiver reassignment/escape, local/anonymous types, inherited scopes and nonliteral arguments are withheld.
- **PathPattern missing leading slash**: an explicit Java `PathPatternRequestMatcher` factory path that does not begin with `/` (rejected by both upstream implementations) is diagnosed under `springSecurity.checkPathPatterns`, and the fix previews `api/users` → `/api/users` as one edit of that literal's content. Supported forms: an exact non-static import (or the fully qualified Spring class) followed by `withDefaults().matcher("api/users")` (Security 6.5), `pathPattern("api/users")` (7.0 direct factory), or their `(null, "path")` / `(HttpMethod.GET, "path")` overloads. Static path segments, single-star segments and a terminal `**` are supported; wildcard placement is never rearranged. Quotes, comments, whitespace and imports stay unchanged. Ant/MVC/custom builders, intermediate builder variables/base paths, overloaded/dynamic or concatenated arguments, escapes/interpolation/placeholders, Kotlin, wildcard imports, potential local/static shadowing, inherited/anonymous scopes, text blocks, Unicode escapes and generic type-parameter syntax stay quiet. The source spelling is not native classpath resolution.

The suppressed implicit-matcher `requestMatchers` PathPattern warning (gated on
Spring Security 7+ and a non-Ant MVC strategy, which the plugin cannot establish)
publishes no warning or quick fix.

## How it works

Editor checks use API 1.3 current-document diagnostics for Java, Kotlin and XML,
including unsaved text. Activation/file events warm the bean cache outside provider
requests; bean absence checks use only a fresh, complete cache while optional file
events are available. Cold, stale, failed or oversized project facts remain quiet.
Diagnostics, hover and inlay requests perform no project scans or file reads.
Current input is bounded to 128 KiB, results to 500 diagnostics and 200 matcher
hints. Mutation actions use the separately guarded FileActions resolver.

Matcher fact admission requires one balanced XML document with valid namespace
bindings; invalid reserved bindings, unbound prefixes, incomplete/multiple roots,
external top-level content and DTD/reference input stay quiet. XML source-candidate
navigation and atomic XML intentions use a scoped namespace-aware reader plus
decoded reserved-URI checks: illegal `xml`/`xmlns` rebinding, aliases/defaults of
reserved URIs (including escaped aliases), empty prefixed bindings and unbound used
prefixes are withheld; legal implicit/explicit `xml:lang` and ordinary namespace
shadowing remain supported. The legacy definition fallback is guarded too, so a
rejected local model cannot reintroduce a target from a namespace-invalid invoking
document. The guard examines current text only, without I/O, capped at 512 KiB,
8,192 events and a 10 ms loop deadline. This is a conservative XML 1.0 framework
subset: namespace declarations requiring entity normalization and XML 1.1 prefixed
namespace undeclarations can be withheld even if valid in a broader XML model.
XML bean-reference candidates are bounded to 32 KiB text, 4,096 XML events, 512
elements, 128-byte ASCII literal names and a cooperative 10 ms parse budget;
escaped/entity/expression names, malformed/DTD XML, unknown namespaces/attributes
and unopened/foreign project paths are excluded. References are half-open UTF-16
ranges; navigation remains read-only. The modern Java declaration subset is bounded
to 64 KiB with capped tokens/calls/patterns. No provider discovery, filesystem,
process or network operations are added by hover/inlay/navigation.

Settings: each diagnostic group has an independent default-on toggle under
`springSecurity` (`checkMigration`, `checkRolePrefixes`, `checkPasswords`,
`checkExpressions`, `checkXmlAttributes`), plus the separate `checkMatchers`,
`checkDebug`, `checkPathPatterns`, `checkBeanReferences` and opt-in
`checkSelfInvocation` switches and the `matcherHover`/`matcherInlays` pair. Each
toggle changes diagnostic visibility, not the underlying framework facts or the
explicit mutation actions. Settings are read once per group per request, without
filesystem or network work.

Capabilities: `project.read` (required) for cross-file bean facts, `events`
(optional) for live invalidation. Denied capabilities degrade to source-only
behavior without fallback writes.

## Not implemented

- SpEL language support beyond completion, these checks and physical lexical ranges (semantic highlighting, typed permission-root evaluation, navigation).
- OAuth2/SAML configuration assistance; broader quick fixes for the XML checks.
- Modern `requestMatchers` hints beyond the explicit declaration subsets, complete SpEL semantics, runtime filter-chain/authorization analysis, library bean resolution and version-aware migration guidance. Existing migration/name-based source checks are advisory, not proof of resolved library symbols.
- Role/bean Find Usages/Rename anchors, alias traversal, imports, runtime auto-configuration and cross-file bean resolution; the Spring Beans plugin retains its own broader bean model.
- Kotlin for the PathPattern factory fixes and declaration hints; effective JVM assignability and full framework-aware migration rewrites.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
