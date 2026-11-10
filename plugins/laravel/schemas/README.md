# Native Laravel generator presets

The Generators workspace can inspect, preview and prepare project definitions,
or save user definitions in private plugin storage. Project changes are applied
through **Apply prepared Laravel preset changes…** in the opened PHP editor; one
undo reverses the complete config/template edit. Existing files are shown with
their full before/after contents. Definitions larger than the host's complete
confirmation limit must be split into smaller reviewed changes.

Project definitions live in `.openidea/laravel-generators.json`. For example:

```json
{
  "version": 1,
  "presets": [
    {
      "id": "service",
      "label": "Service",
      "parameters": [
        { "name": "name", "label": "Name", "type": "text", "default": "ReportService" },
        { "name": "description", "label": "Description", "type": "text", "default": "Reports" }
      ],
      "calculated": [
        { "name": "class", "expression": "${name|studly}" }
      ],
      "files": [
        { "path": "app/Services/${class}.php", "template": "service.php" }
      ]
    }
  ]
}
```

Save the referenced template as `.openidea/laravel-templates/service.php`:

```php
<?php

declare(strict_types=1);

namespace App\Services;

final class ${class}
{
    public const DESCRIPTION = ${description|php};
}
```

Inputs support `text`, `boolean`, `choice` (with a `choices` list), and `model`
(a qualified PHP class name). Inputs without a default must be supplied.
Calculated values contain one expression referring to an input or an earlier
calculation. Forward references, cycles and unknown transforms fail validation.

Expressions use `${name|transform}`. Available transforms are `className`,
`namespace`, `snake`, `kebab`, `studly`, `camel`, `plural`, `singular`, and `php`.
The inflection transforms cover regular English names and selected common
irregular names; provide an explicit input when a project needs a different
spelling. `${description|php}` emits an escaped, quoted PHP string literal.
Unescaped content substitutions accept only identifier/namespace/path-like
characters. `$${` emits a literal `${`; Blade's `{{ ... }}` is untouched.
No expression executes PHP, shell or Velocity.

Optional `modules` entries have `id`, `label`, `namespace`, `sourceRoot`,
`migrationRoot`, and `viewRoot`. Selecting a module exposes
`${module.namespace}`, `${module.sourceRoot}`, `${module.migrationRoot}` and
`${module.viewRoot}` to presets. Model generation also adopts the selected
module's explicit namespace/source/migration roots. This consumes configured
layouts; it does not run a package-specific module installer.

Project presets/modules override user definitions with the same id. User
templates are isolated from project template names. Removing a preset does not
implicitly delete shared templates; template deletions must be selected
explicitly and cannot remove a template still referenced by the replacement
configuration.

The accompanying `laravel-generators.schema.json` describes version 1. A local
`$schema` reference may be added to the config; the plugin validates its string
shape without fetching it. Config and template files are limited to 128 KiB
each, with a 2 MiB bundle limit. Paths must stay within the owning project;
nested open-project ownership is respected.

### Native generator defaults and module database classes

The optional `settings` object configures native generator base classes and class
postfixes, generated return type hints, and constructor property promotion.
`controllerDependencies` is an array of `{ "name": "cache", "class":
"Illuminate\\Contracts\\Cache\\Repository" }` entries. Disabling promotion
generates ordinary typed properties plus constructor assignments instead. The
Generators settings panel can override these defaults for one draft; project
defaults take precedence over user defaults. These settings affect the native
model/related-file planner; captured Artisan commands retain their own stubs.

A module can explicitly set `factoryRoot`, `factoryNamespace`, `seederRoot` and
`seederNamespace`. By default a source root ending in `/app` places factories and
seeders in its sibling `database/factories` and `database/seeders` directories,
with `<module namespace>\\Database\\Factories` / `Seeders` namespaces. Other
source roots use their own `database` subdirectory. Composer must map these
classes to their exact generated files. The model's generated `newFactory()`
method points to the configured factory, including its configured postfix.

Existing models expose a graphical field/index editor in Generators. It previews
a new reversible migration and optional `$fillable`, literal `$guarded`, and cast
updates. Literal `$casts` properties and literal-return `casts()` methods are
supported. Keep mass assignment leaves dynamic declarations and Fillable/Guarded attributes intact; editing mass assignment requires a supported literal declaration. Dynamic cast declarations are refused when casts are being changed. Migration/schema/config changes
invalidate prepared edits.

### Indexed model choices and live calculated values

A parameter with `"type": "model"` uses the current project's indexed Eloquent
model picker, with searchable fully qualified names. Typed or default class names
that are absent from the owned model inventory are rejected. Preview and apply
recheck the bounded model-source snapshot; missing, changed or re-owned source
files invalidate the prepared generation. Rendering and live calculations do not
execute PHP or inspect source files.

Calculated declarations accept optional `label`, `hidden` (default false) and
`editable` (default true). Visible values follow their inputs until manually
edited. **Use calculated value** clears an override and resumes automatic updates.
An overridden calculation feeds subsequent calculations. Hidden or read-only
values cannot be overridden. Project/user preset definitions use the same behavior.

The workspace's `evaluate-preset`, `preview-preset` and `prepare-preset` messages
accept a separate optional `overrides` object keyed by calculated-variable name.
Ordinary `values` contain only declared parameters. Unknown override names,
hidden/read-only overrides, mismatched scalar types and executable expressions
are refused. Native template syntax remains an intentional difference from
Velocity templates.

### Shared templates and boolean sections

Declare shared files in the top-level `"partials": ["header.php"]` list. They
live beside ordinary templates and use the same owned-file, size and freshness
checks. `${#include header.php}` inserts a registered template from the same
project or user scope. Includes cannot cross scopes, escape the template root,
form cycles or exceed eight nested files. Unregistered includes are refused,
including in skipped branches.

Native `${#if save}...${#else}...${/if}` sections require a declared boolean
parameter or boolean calculated value. The else branch is optional; `${#if !save}`
inverts the predicate. Nesting is limited to eight sections and a render permits
4096 directives/placeholders. Expanded files remain limited to 128 KiB, with
2 MiB total generated output. These native capabilities provide reusable headers
and conditional methods; they do not import Velocity syntax or execute expressions.
`$${` escapes a literal placeholder, and substituted parameter text is never
reinterpreted as a directive.

Name inflection affects the final word and preserves compound spelling and case:
`UserPerson` → `UserPeople`, `user_child` → `user_children`, `USER_ACCOUNTS` →
`USER_ACCOUNT`. Name transforms handle Unicode character boundaries safely.

Composer proof accepts a matching parent PSR-4 namespace prefix and owned module
`composer.json` mappings relative to that manifest. It records absent ancestor
manifests too, so adding a mapping invalidates a prepared review. Conflicting
package namespaces, source-root escape and separately owned nested projects are
refused. Module scaffolding and per-generator module availability remain separate
workflows.

### Native generator paths and application/module availability

The preset editor exposes **Native generator paths & availability**, with a row
for model, migration, factory, seeder, controller, policy, request and resource.
Choose default settings or a specific module override. Paths and flags are saved
through the existing reviewed project configuration action (or private user
settings). Each preset also exposes Application, Modules and allowed module ids.

The equivalent native configuration is:

```json
{
  "version": 1,
  "generators": {
    "model": {
      "directory": "${sourceRoot}/Entities",
      "namespace": "${namespace}\\Entities"
    },
    "controller": { "availableInModules": false }
  },
  "modules": [],
  "presets": []
}
```

A module may carry its own `generators` map, overriding individual default fields.
Directories are relative to the project; `${sourceRoot}` expands to the selected
application/module source root and `${moduleRoot}` to the module source root
without a trailing `/app`. `${moduleRoot}` is empty in Application mode; a path
that becomes absolute or empty is refused. Namespaces accept `${namespace}`.
Unknown placeholders, generator kinds, invalid class namespaces and traversal
are rejected. Migrations have directories but no class namespace.

`availableInApplication` and `availableInModules` default to true. Native model
availability controls the context picker; unavailable related generators are
disabled and removed from the current selection. Preview and prepare independently
reject unavailable generators, even if a client constructs a request manually.
A preset accepts the same flags plus optional `moduleIds`; selecting it filters
its context choices. Application availability remains independent of that list.

Effective directories/namespaces drive both generated declarations and references
(factory model type, seeder model call and model factory binding). Composer proof
must map every generated class to its chosen path. User defaults apply when the
project has no definition for that generator kind; project entries take precedence.

These settings govern the coordinated native model workflow and native presets'
context availability. Artisan catalog generators continue to use the flags their
installed command schemas expose. Arbitrary native directories/base classes are
not translated into unsupported Artisan flags. Actual module scaffolding remains
a separate workflow.

The native model draft also accepts optional `connection`. Blank means the default
connection; a supplied exact configured name is preserved as a PHP-escaped model
`$connection` property. Related migrations use the same `Schema::connection(...)`
for both create and rollback. Names are limited to 128 characters without control
characters. Schema-derived models preserve the captured resolved connection name.

### New Model output selections

The Fields step exposes independent **Factory fields** and **Resource fields**
checkboxes. The Relations step exposes **Resource relations**. These choices
control entries only in selected factory/resource outputs; the model, migration,
request rules and other outputs retain their normal complete field definitions.
A resource relation remains a raw `whenLoaded` entry.

Optional native draft properties `factoryFields`, `resourceFields` and
`resourceRelations` carry arrays of declared names. Omitted or null retains the
historical default; an explicit empty array emits no corresponding entries.
Factory defaults include a custom manually generated primary key; automatically
incremented keys and UUID/ULID model-generated keys are excluded. Resource fields
include the primary key by default, and it can be unchecked separately. Unknown,
stale or repeated names are rejected, regardless of whether an output is selected.

**Use default selections** resets a customized list. The UI preserves explicit
choices when a declared field/relation or primary key is renamed, removes deleted
names, and excludes newly added definitions from an already customized list until
selected. Preview and Finish send the same complete draft and selections.
The supplemental `laravel-model-output-selections.schema.json` describes only
these optional draft properties; it is not the project configuration schema.

Indexed resource wrappers and explicit factory mapping expressions remain separate
workflows. This draft accepts neither arbitrary PHP mappings nor unproved wrapper
class references.
