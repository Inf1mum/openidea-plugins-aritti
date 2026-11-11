# Forwarded constructor and mixin acceptance

The existing `helperCode.classMixins` and
`helperCode.methodsWithConstructorParameters` metadata formats are unchanged.
Mixin donors now materialize public members inherited through known class,
interface and selected trait graphs. Own declarations hide inherited members;
trait preference/alias facts retain real navigation locations. Cycles,
ambiguous traits and source ownership changes withhold affected results.
Public method parameters are excluded from property inventories.

Constructor forwarding selects explicit constructors, selected or aliased trait
constructors, then the actual class parent. Parameter labels retain original
PHP syntax for references, variadics, nullable/union/intersection types and
literal defaults while qualifying imported class types/default class constants.
Promotion modifiers and parameter attributes are removed from call labels.
Trait self/parent types bind to the class using the constructor. Private or
ambiguous constructors do not produce a forwarded signature.

Main provider tests cover completion, definition, return chaining and signature
help, including reordered named arguments, variadic repeated/unknown named
arguments, nested arrays/lambdas, duplicate named arguments and nested calls
whose targets have no forwarded signature. All inference remains bounded and
performs no PHP evaluation.

An optional real PHP reflection test writes only a disposable temporary fixture,
loads its declarations without a framework/application bootstrap, and compares
constructor parameter names/types, references, variadics, optional/default values
and visibility with actual Main signature results. It passed with PHP 8.5.5:

```sh
LARAVEL_NATIVE_PHP=/absolute/path/to/php CARGO_INCREMENTAL=0 \
cargo test --manifest-path plugins/laravel/Cargo.toml \
  eloquent_members::objects::tests::actual_php_reflection_matches_forwarded_constructor_facts \
  -- --ignored
```

The pure Main tests run under `eloquent_members::objects::tests` without the
ignored-test switch. Unknown/dynamic runtime mixins and unindexed inheritance
members are not invented.
