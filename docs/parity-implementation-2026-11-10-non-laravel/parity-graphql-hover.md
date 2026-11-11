# GraphQL annotation and SDL hover delivery — 2026-11-11

## Assigned ownership

Only modules/graphql/src/mapping_docs.rs and the hover portion/tests of live.rs were edited by this task. Parent concurrently owns lib.rs and corrected fixed shortcut-parent semantics versus custom SDL root request drafts. No lib.rs changes were made here.

## Public annotation contracts

- https://docs.spring.io/spring-graphql/docs/current/api/org/springframework/graphql/data/method/annotation/MutationMapping.html: composed SchemaMapping contract uses parent Mutation, and name/value alias its field attribute.
- https://docs.spring.io/spring-graphql/docs/current/api/org/springframework/graphql/data/method/annotation/SubscriptionMapping.html: corresponding fixed parent Subscription, with name/value field aliases.

The fixed composed parent is documented as annotation source behavior. It is not replaced by a custom schema operation root and is not presented as proof of installed framework version, schema merge, resolver registration or runtime behavior.

## Implemented

- Query/Schema mapping docs extended to MutationMapping and SubscriptionMapping.
- Current Java/Kotlin source parsing uses spring_text::parse_source with language/path agreement.
- Imported annotation owners require exactly one unambiguous, nonstatic, nonaliased canonical import; fully qualified canonical Controller/mapping owners are also admitted without imports when the root namespace is unshadowed.
- Foreign, wildcard, aliased/conflicting imports, local type/generic/typealias shadows, inherited controllers, multiple mapping annotations, conflicting field aliases, dynamic/escaped/textblock values and invalid GraphQL names stay quiet.
- Kotlin hover dispatch now uses the same explicit annotation source model.
- SDL field hover renders the exact declared type_reference and argument_types, including list/non-null wrappers, instead of claiming wrappers are uninferred.
- SDL hover requires valid complete type references and uses bounded per-name/type/count/total-signature sizes. It does not infer resolver/schema semantics.

## Tests

Five new regressions cover:
1. Java and Kotlin Mutation/Subscription fixed parents and field-name aliases, including contradictory class-level typeName source without changing the shortcut contract.
2. Kotlin foreign/aliased Controller/mapping imports, generic/typealias shadows, dynamic/interpolated/escaped names and conflicting aliases.
3. Fully qualified Controller/SubscriptionMapping ownership and package-root shadow rejection.
4. Exact SDL field/argument list and non-null wrappers; no-argument fields; no network/process work.
5. Malformed and excessive signatures withheld rather than printed as complete.

Command: cargo test --locked --manifest-path plugins/spring-web/modules/graphql/Cargo.toml
Result after parent lib.rs updates:44 tests passed,0 failed. Log /tmp/parity-graphql-hover-test.log. Scoped git diff --check passes. Parent is running final merged Spring Web tests and release Wasm build.

## Residual boundaries

This is explicit source documentation and exact written SDL rendering. It does not resolve meta-annotation compositions, inferred class/source-argument SchemaMapping parents, arbitrary aliases/typealiases, dynamic field/type expressions, classpath types, merged schemas or runtime fetcher registrations. No native desktop hover acceptance was performed for the new artifact.
