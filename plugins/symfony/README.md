# Symfony

Partial port of the [Symfony Plugin](https://github.com/Haehnchen/idea-php-symfony2-plugin)
for IntelliJ (`fr.adrienbrault.idea.symfony2plugin` 2026.2.314, revision
`b6c1034f`, MIT, by Daniel Espendiller and Adrien Brault). The port uses only
the free code of the public repository.

## Completion and go to definition (⌘B)

| Names        | Indexed from                                                                                                       | Referenced in                                                                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Routes       | `#[Route]` / `@Route` in `src/**/*Controller.php`, `config/routes*` YAML/XML, compiled `var/cache/dev/url_generating_routes.php` | PHP `generateUrl`, `redirectToRoute`, `->generate`; Twig `path()`, `url()`                                                                                     |
| Templates    | `templates/**/*.twig` (`templates/bundles/XBundle/` as `@X/`; `twig: paths:` of `config/packages/twig.yaml` as `@namespace/`)                                                       | PHP `render*`, `stream`, `htmlTemplate`, `textTemplate`, `#[Template]`; Twig `extends`, `include`, `embed`, `import`, `from`, `use`, `include()`, `source()` |
| Blocks       | the parent templates (`extends` chain)                                                                              | Twig `{% block name %}`, `block('name')`                                                                                                                       |
| Variables    | keys of the array passed to `render()` by controllers                                                              | Twig expressions (completion only)                                                                                                                             |
| Services     | `services:` in `config/**` YAML, `<service>` in XML, compiled `var/cache/dev/App_KernelDevDebugContainer.xml`      | PHP `$container->get/has`, `service()`, `ref()`; YAML `'@id'`, `'@?id'`; XML `type="service" id=`, `alias=`, `parent=`                                          |
| Classes      | `composer.json` PSR-4 autoload directories                                                                         | YAML service keys, `class:`, `parent:`; XML `class=`                                                                                                           |
| Translations | `translations/**` YAML (nested keys joined with `.`) and XLIFF, by domain                                          | PHP `->trans`, `t()`, `new TranslatableMessage`; Twig `'key'\|trans`; domain from the call, `{% trans_default_domain %}`, else `messages`                       |
| Assets       | files under `public/`                                                                                              | Twig `asset()`; PHP `->getUrl()`                                                                                                                               |
| Entries      | `webpack.config.js` `addEntry`/`addStyleEntry`, `importmap.php`                                                    | Twig `encore_entry_link_tags()`, `encore_entry_script_tags()`, `importmap()`                                                                                    |
| Roles        | `ROLE_*` in security config and controllers, plus built-in attributes (`IS_AUTHENTICATED_*`, `PUBLIC_ACCESS`, …)  | PHP `isGranted`, `denyAccessUnlessGranted`, `#[IsGranted]`; Twig `is_granted()`                                                                                |
| Tags         | Symfony's built-in service tags plus the tags used in `config/**` YAML and XML service files                      | YAML `tags:` of a service (`- { name: x }`, `- name: x`, `['x']`, `- x`); XML `<tag name="">` (completion only)                                              |

The former host-index association graph for event listeners, Twig macros,
voters, constraints, transformed class/template names and related gutter links
is now gated. Its global equality keys and synthetic/transformed anchors were
navigation associations, not proven complete Rename symbols. API 1.3 uses that
graph for automatic usages/Rename too. Explicit Go to Definition from the table
above and source-derived tool-window navigation remain; the graph-only related
gutter links are unavailable until a safe read-only provider replaces them.

## Twig names and structure

In `.twig` files of a Symfony project: completion of tag names (`if`, `for`,
`block`, `extends`, … and their `end…` forms) after `{%`, filters after `|`,
functions in expressions and tests after `is` / `is not`, from static lists
taken from Twig's public documentation plus the filters, functions and tests of
the project's own Twig extensions (`getFilters()` and friends, `#[AsTwigFilter]`,
`#[AsTwigFunction]`, `#[AsTwigTest]`, read from the document index; extensions in
`vendor/` are not read). After `alias.` of `{% import 'tpl' as alias %}` (or
`_self`) the macros of that template are offered. Template paths after `extends`, `include`, `embed`, `import`,
`from` and `use` come from the cached `templates/` index.

## Problems, quick fixes and gutter

- Problems: missing template, missing translation key (in a domain that has
  keys), missing route (once the compiled routes exist), missing service (once
  the compiled container exists), route defined twice in a file, service id defined twice in an XML file, unquoted YAML values starting with `@` or `%` in `config/` and `translations/` (Symfony's parser rejects them), Twig block tags (`block`, `if`, `for`, `macro`,
  `embed`, `apply`, `autoescape`, `with`, `verbatim`, `cache`, `sandbox`) never closed. Files are
  checked when opened or saved. _Settings › Plugins › Symfony_ turns the
  checks off.
- Text inspections (`src/checks.rs`, written from the Symfony documentation;
  each is silent when the class, service or file it needs is not in the
  index, and when a name is dynamic or comes from a library). Three groups,
  each with a setting:
  - _Route controllers_ (`controllerChecks`): `controller-method-missing`, a
    route in `config/routes*` (YAML/XML) naming `Class::method` where the
    project class has no such public method, an invokable class without
    `__invoke`, or a class-level `#[Route]` with neither `__invoke` nor routed
    methods; `deprecated-route-setting`, only these: the
    `Bundle:Controller:action` notation (removed in 4.0), `pattern` (removed in
    3.0), `type: annotation` on imports (deprecated in 6.4; only reported when composer.json requires `symfony/framework-bundle` 6.4 or newer), and the
    `_method` / `_scheme` requirements (removed in 3.0). Classes that extend a
    class other than `AbstractController`, use traits or define `__call` are
    not reported.
  - _Services_ (`serviceChecks`, YAML/XML under `config/`):
    `deprecated-service-class`, a `class:` naming a project class with
    `@deprecated` or `#[Deprecated]`; `duplicate-parameter-key`, one
    parameter twice in a file; `named-argument-missing`, a `$name` argument the
    class constructor has no parameter for (not for variadic constructors,
    services with `parent` or a factory, `calls`, or classes without a
    constructor).
  - _PHP calls_ (`phpCallChecks`): `form-type-string`, `$builder->add('f',
    'text')` with a core type alias removed in Symfony 3; `missing-asset`,
    `->getUrl('x')` / `asset('x')` of a file absent from the public directory
    (not for URLs, absolute paths, dynamic names or a second package
    argument); `missing-translation-domain`, `->trans(…, domain)` / `t()` with a
    domain that has no file in `translations/` (only lower-case project-style
    domain names are reported: `messages`, `validators`, `security`, dotted or
    capitalised names such as `EasyAdminBundle` never are). Assets under
    `build/`, `bundles/`, `assets/` and in directories with no indexed file
    are not reported; `->getUrl()` is only checked in files that mention
    `Packages` or `Symfony\Component\Asset`. Class files are read at most once
    per run (256 KB and 500 files caps); beyond that the checks stay silent.
    A parent named `Controller` / `AbstractController` counts only when it
    resolves to the framework's. Services with `constructor`, `parent` or a
    factory are not checked for named arguments.
  - _Twig_ (`twigChecks`): `twig-missing-route`, `path('x')` / `url('x')` of a
    route absent from the compiled routes (silent without them, for
    `_`-prefixed routes and dynamic names; replaces `missing-route` in Twig
    files); `twig-missing-asset`, `asset('x')` of a file absent from the public
    directory, with the same exceptions as in PHP; `twig-route-compare`, a
    literal compared with `app.current_route` or
    `app.request.attributes.get('_route')` (`==`, `!=`, `is same as`, `in [...]`)
    that is not a compiled route; `twig-missing-translation-domain`, the domain
    of `'k'|trans(args, 'domain')` (or `domain:`) without translation files.
    They run on `.twig` files from their extension, so they work without the
    `aritti.twig` plugin. Comments and `{% verbatim %}` bodies are skipped.
  - _Twig syntax_ (`twigSyntaxChecks`): `twig-for-condition` (warning), the `if`
    condition of `{% for %}` (deprecated in Twig 2.10, removed in 3.0; use
    `|filter`); `twig-component-block-syntax` (error), a `{% block %}` inside a
    `<twig:Name>` element; `twig-component-self-import` (error), `_self` as
    the source of `{% from %}` / `{% import %}` inside a component. An
    unbalanced `<twig:...>` file is left alone.
  - _YAML syntax_ (`yamlSyntaxChecks`, files under `config/` and
    `translations/`): `unquoted-yaml-scalar` (also a value starting with a
    backtick, or with `|` or `>` that is no block scalar header),
    `yaml-unescaped-backslash` (hint), a backslash in a double-quoted string
    that starts no escape, and `yaml-unquoted-colon` (hint), a `: ` inside an
    unquoted value. Also in the _service_ group (`serviceChecks`):
    `deprecated-factory-setting`, `factory_class` / `factory_method` /
    `factory_service` (YAML) or `factory-class` / `factory-method` /
    `factory-service` (XML) on a service, removed in Symfony 3.0; use `factory`.
  - _Commands_ (`commandChecks`, PHP): `command-invokable-return-type` (hint),
    the `__invoke()` of a class with `#[AsCommand]` (the attribute must resolve
    to the Symfony class) has no `int` or `never` return type;
    `console-input-mode` (warning), two base modes ORed in the mode of
    `InputArgument` (`REQUIRED`, `OPTIONAL`) or `InputOption` (`VALUE_NONE`,
    `VALUE_REQUIRED`, `VALUE_OPTIONAL`), through `new` or `addArgument()` /
    `addOption()`; deprecated in Symfony 8.1. Modifiers, numbers and variables
    are ignored.
  - _Deprecations_ (`deprecationChecks`, PHP), only when `composer.json` names
    `symfony/framework-bundle` 8.1 or newer: `filesystem-mirror-copy-on-windows`,
    the `copy_on_windows` option of `->mirror()` in a file that mentions
    `Symfony\Component\Filesystem` (use `follow_symlinks`);
    `tagged-collection-default-method`, `defaultIndexMethod:` or
    `defaultPriorityMethod:` with a string value on `TaggedIterator`,
    `TaggedLocator`, `AutowireIterator` or `AutowireLocator` (use
    `#[AsTaggedItem]`); `from-callable-option`, `alias`, `parent`,
    `synthetic`, `file`, `arguments`, `properties`, `configurator` or `calls`
    next to `from_callable` in one PHP-array service (YAML is not checked).
  - _Doctrine_ (`doctrineChecks`, PHP attributes, `*.orm.xml`, `*.orm.yml|yaml`;
    written from the Doctrine ORM documentation, not from the Symfony Plugin's
    source). PHP files count only when they import `Doctrine\ORM\Mapping` (or
    a member of it, aliases followed): `orm-empty-table` (hint), `#[Table]`
    with no arguments or `name: ''`; `orm-table-deprecated-property`
    (warning), `indexes:` or `uniqueConstraints:` in `#[Table(...)]`
    (deprecated since ORM 3.2, use `#[Index]` / `#[UniqueConstraint]`);
    `orm-lifecycle-callbacks-missing` (warning), a lifecycle attribute
    (`PrePersist`, `PostPersist`, `PreUpdate`, `PostUpdate`, `PreRemove`,
    `PostRemove`, `PostLoad`, `PreFlush`) in an `#[Entity]` class
    without `#[HasLifecycleCallbacks]` (a `#[MappedSuperclass]` is skipped: a
    subclass may carry it);
    `orm-one-to-many-mapped-by` (warning), `#[OneToMany]` without `mappedBy`
    and without a `#[JoinTable]` on the property; `orm-inverse-side-join-column`
    (hint), `#[JoinColumn]` next to `#[OneToOne(mappedBy: ...)]`;
    `orm-join-column-nullable` (hint), an explicit `nullable: true` (the
    default) on a join column in PHP, XML (`<join-column>`) or YAML
    (`joinColumn` / `joinColumns`).
  - _Container references_ (in the _service_ group, `serviceChecks`):
    `unknown-parameter`, a `%name%` in `config/` YAML or XML that neither the
    config files nor the compiled container define (silent without a compiled
    container, for `%env(…)%`, `kernel.*`, `%%` and any other name shape);
    `service-class-missing`, a `class:` in one of the project's PSR-4
    namespaces without a file (`Tests` namespaces skipped); `deprecated-class-usage`
    (PHP calls group), `new X` / `X::class` of a project class marked
    `@deprecated`, from a file that is not itself deprecated.
  - _Service wiring_ (`serviceChecks`, `src/wiring.rs`): a small class model
    (constructor parameters, parent, interfaces, resolved through `use`) checks
    `service-argument-count` (more positional arguments than the constructor
    has parameters), `service-argument-type` (a `@service` whose project class
    cannot be the typed project class or interface), `service-tag-interface`
    (`kernel.event_subscriber`, `twig.extension`, `form.type`,
    `validator.constraint_validator`, `security.voter`, `kernel.cache_warmer`,
    `doctrine.event_subscriber` on a class that lacks the interface),
    `listener-method-missing` (`kernel.event_listener` with a `method` the class
    lacks) and `yaml-php-const-missing` (`!php/const` of a project class
    without that constant). Silent when a parent or interface is outside the
    project and could matter, for traits, `__call`, `parent`, factories,
    decorators, aliases, named or nested arguments.
  - _Practices_ (`practiceChecks`, PHP, hints unless noted): in controllers
    `controller-container-get`, `controller-superglobal`, `controller-debug-call`,
    `controller-base-class` (the removed `Controller` base class);
    `constructor-container-injection`; `twig-extension-get-name`;
    `twig-legacy-class` (warning, `Twig_Foo` names); `form-type-removed-method`
    and `form-removed-option` (warnings, the Symfony 3.0 form removals).
  - More _Deprecations_ (`deprecationChecks`, each with its own composer
    version gate): `http-kernel-event-argument` (4.3, `GetResponseEvent` family),
    `controller-get-doctrine` (5.4), `command-default-name` (6.1),
    `security-class-moved` and `request-get-content-type` (6.2).
  - More _Doctrine_ checks: `orm-query-iterate`, `orm-entity-manager-merge`,
    `orm-flush-entity` (removed in ORM 3, on conventionally named entity
    managers in files that import one), `orm-column-deprecated-type` (`array`,
    `object`), and `collections-criteria-ordering` (hint, string orderings of
    `Criteria`).
  - More _Twig_ checks: `twig-assets-tag-missing` (`{% javascripts %}` /
    `{% stylesheets %}` literals), `twig-enum-class` (`enum()` / `enum_cases()`
    naming a missing project class or one that is no enum),
    `twig-content-outside-block` (error; text or `{{ }}` outside blocks in a
    template whose first tag is a literal `{% extends %}`; `set`, `import`,
    `use`, `macro` and tags in general stay legal) and `twig-path-replacement`
    (hint with a quick fix, `href="/blog"` / `action="/blog"` equal to the path
    of one YAML or XML route without placeholders).
  - _More practice checks_ (`practiceChecks`, hints unless noted; roles come from
    the parent class, interfaces or class attributes, `src/practice_checks.rs`):
    `constructor-request-injection`, `constructor-session-injection`,
    `constructor-token-injection`, `constructor-user-injection` (request-scoped
    values in the constructor of a controller, command, subscriber, voter, form
    type, Twig extension or attribute-registered service);
    `deprecated-injection` (warning: removed Symfony and Doctrine Persistence
    types such as `UserPasswordEncoderInterface`, `EngineInterface`);
    `kernel-project-dir` (a `KernelInterface` used only for `getProjectDir()`);
    `twig-extension-heavy-constructor` and `command-heavy-constructor`;
    `request-stack-in-action`, `controller-shortcut` (`new RedirectResponse`,
    `JsonResponse`, `BinaryFileResponse`), `controller-missing-exit` (warning,
    `$this->createNotFoundException()` or `render()` as a statement without
    `throw` / `return`); `bundle-register-commands`; `command-return-value`
    (warning, bare `return;`, `null`, booleans or strings from `execute()` /
    `__invoke()` of a `Command`); `request-bag-write` (`$request->attributes = …`
    on a parameter typed `Request`); `violations-find-by-codes` (literal code);
    `form-type-instance` (warning, `->add('f', new TextType())`).
  - More _Deprecations_ with version gates: `route-attribute-namespace` (6.4,
    `Routing\Annotation\Route`, with a quick fix to `Routing\Attribute\Route`),
    `first-class-callable` (6.2, `[$this, 'method']` of an own method),
    `render-form` and `form-create-view` (6.2), `constraint-array-options` (7.4,
    `new NotBlank(['message' => …])` and the attribute form).
  - More _Doctrine_ checks: `orm-lifecycle-method-visibility` (a lifecycle callback
    that is not public), `orm-discriminator-map-duplicate` (PHP attribute and XML);
    `src/query_builder_checks.rs` (files that import a Doctrine query builder or
    call `createQueryBuilder(`, receiver a `createQueryBuilder()` chain or `$qb` /
    `$queryBuilder` / `$builder`): `query-builder-add`, `query-builder-array`,
    `query-builder-set-parameters` (hints), `query-builder-alias` (`from()` without
    alias), `query-builder-sort-direction`, `query-builder-time-parameter`
    (`'CURRENT_DATE'` bound as a parameter) and, with a DBAL import,
    `query-builder-parts` / `query-builder-execute` (deprecated in DBAL 3.1).
  - More _Service wiring_: PHP service files (`$services->set(Foo::class)->args([service(Bar::class)])->tag('x')`)
    get the argument and tag checks, `type="constant"` XML values get the constant
    check, and `autowire-wrong-class` compares `#[Autowire(service: 'id')]` with the
    parameter type.
- Quick fixes (⌥⏎): on a missing translation key, add it to one of the domain's
  YAML files; on an unquoted `@` / `%` YAML value, quote it; on an unescaped backslash in a
  double-quoted YAML string, double it; on a missing route, template, translation key or domain, change it to the one known name that differs by a typo (a unique candidate within one or two edits). Quick fixes follow the check settings.
- Gutter: controller `render()` → template; template → controllers rendering
  it; `{% block %}` → the parent block it overrides; PHP class → its service
  definition; YAML service → its class file.

### Code actions

Refactorings in the code actions menu (⌥⏎), from `src/intentions.rs`. Each
edits the open file only, and is not offered when a needed import would clash
with another class of the same short name.

- _Add #[Route] attribute_: caret in a public, non-static, non-magic method
  without `#[Route]` of a class named `*Controller` or extending
  `AbstractController` / `Controller`, when `composer.json` requires
  `symfony/framework-bundle` 6.2 or newer. Path and name come from the class
  and method names (`/blog/show-post`, `app_blog_show_post`); the caret is not
  moved to the path.
- _Add parameter: Request $request_ / _UserInterface $user_: in a public route
  action (`#[Route]`, `@Route`, or `__invoke` of a class with a `#[Route]`).
  In the `__invoke` of an `#[AsCommand]` class: `InputInterface`,
  `OutputInterface`, `Cursor`, `SymfonyStyle`, `Application`. One action per
  type (no chooser); a type already in the signature, by short or full name, is
  not offered.
- _Use TextType::class_: on the second argument of `->add('field', 'text')`
  when it is a pre-3.0 form type alias; _Use class constants for all form types
  in this file_ when the file has two or more.
- _Override block "name"_: with the caret on `{% extends %}`, one action per
  parent block the file does not define; appends a block calling `parent()`
  at the end of the file.
- _Use Product::class_ (`src/rewrites.rs`): a Doctrine `'App:Product'` alias in
  `getRepository()`, `find()`, `getReference()` or `getClassMetadata()` whose
  `App\Entity\Product` is a project class.
- _Convert to invokable command_: a class with `#[AsCommand]` extending
  `Command`, with `execute(InputInterface $input, OutputInterface $output): int`
  and a `configure()` of only `addArgument()` / `addOption()` calls (modes
  `REQUIRED` / `OPTIONAL`, `VALUE_NONE` / `VALUE_REQUIRED`). `extends` and
  `configure()` go, `execute` becomes `__invoke` with `#[Argument]` / `#[Option]`
  parameters, a lone `parent::__construct();` goes. Not offered for anything
  else (`setName`, defaults, lifecycle methods, `$this` use of inherited
  members). Needs Symfony 7.3.
- _Convert Twig extension methods to attributes_: `getFilters()`,
  `getFunctions()` and `getTests()` holding only `new TwigFilter('x', [$this,
  'method'])` entries become `#[AsTwigFilter('x')]` (and `AsTwigFunction`,
  `AsTwigTest`) on the methods. Not offered with options, other callables or
  other extension methods. Needs Twig 3.21.
- _Add service definition to services.yaml_: caret on a concrete project class
  declaration; appends to the first existing `config/services.yaml`, `.yml` or
  `.xml` (no file is created) named arguments for the constructor's
  parameters without a default (services for class types, empty literals for
  scalars) and, without `autoconfigure: true`, the tags the class implies. Not
  offered for a class already defined or covered by a `resource:` prototype.
  This is now a lazy atomic FileAction with exact resulting-file confirmation,
  not a second legacy CodeAction. It requires the current recognized root and
  full cached index, matching source version/text, bounded source/target and
  unchanged supporting source reads. Fresh duplicate/prototype/target checks run
  again before returning one edit. Suggestions/placeholders do not certify runtime
  wiring; oversized concrete previews refuse rather than truncate. No file is
  written while preparing or cancelling the action.
- In a YAML service file: _Resync arguments with the constructor_ (named
  `$arg` entries only; keeps existing values, adds new parameters, drops
  removed ones) and _Add tags …_ for a service whose class implies tags and has
  none.
- In an XML service file: _Add arguments from the constructor_ (a service with no
  `<argument>`) and _Add tags …_. Argument service ids are the type itself, or
  the only service whose class is the type.
  These YAML/XML argument/tag helpers are now lazy FileActions, with exact
  resulting-source confirmation instead of parallel eager CodeActions. Source
  root/path/language/extension/version/current text and a complete cache are
  required. Generation reads at most32 owned supporting files of32KiB each;
  accepted supporting reads, their project owner and current source are checked
  again after review.
  Tokens are one-use, no file is written during preparation, and oversized exact
  previews refuse. Values/tags remain source-based suggestions, not runtime truth.
- _Move $logger to the constructor_: caret on a typed, uninitialised class-typed
  property without attributes; becomes a promoted parameter of the existing
  constructor (not offered when it assigns the property) or of a new one.
- _Extract translation_: with plain text selected in a Twig template, replaces
  it with `{{ 'blog.index.read_more'|trans }}` and appends the key to the
  domain's YAML translation file (`{% trans_default_domain %}`, else
  `messages`; `.en.` file first).
- _Add form fields for Product_: in a form type whose `configureOptions()` sets
  `data_class`, appends `$builder->add('field', Type::class);` for the entity
  properties the form lacks (`#[ORM\Column]` types or typed properties; ids and
  associations skipped).

## Attribute arguments

Completion inside `#[Route(`, `#[Template(`, `#[IsGranted(`, `#[AsCommand(`,
`#[AsEventListener(` and the Doctrine mapping attributes (`#[ORM\Column(` …,
aliases followed) offers the argument names, and the values of `methods:`,
`schemes:`, `type:`, `fetch:`, `cascade:`, `strategy:` and `onDelete:`. Invoke it
by hand; the IDE does not pop it up by itself there. `IsGranted(attribute:)`
offers the common role names and `AsEventListener(event:)` the core kernel and
console event names. A constraint argument such as `message: 'post.title.blank'`
whose text looks like a key (a dot, no spaces) is a `validators` translation
reference: completion, go to definition and the missing-key problem.

## Twig variables

Completion of variables in Twig also offers what the template defines itself
(`{% set %}`, `{% for %}` with `loop`, macro arguments, `{% with %}`, `{# @var
name Type #}`; blocks, macros, loops and `with` have their own scope), what
other templates pass with `{% include 'x' with {…} %}` / `include()`, the
`twig: globals:` of `config/packages/twig.yaml`, `app`, `_self` and `_context`.

## New file templates

New menu entries for a PHP class with a fixed `App\…` namespace: Command (plain,
invokable, with argument and option, with `#[Argument]` / `#[Option]`),
Controller (attributes, annotations), Form Type, Twig Extension (methods,
attributes), Kernel Test, Web Test, Event Subscriber and Voter. Compiler Pass
creation now uses reviewed FileActions from root Composer or an owned PHP class;
the former native compiler template export is retired.

Service Config (YAML), (XML) and (PHP) now use reviewed Composer-context
FileActions: defaults only, or an explicitly selected valid production PSR4 source
root with resource loading and its Kernel.php exclusion. The three native template
exports are retired after supported replacements; reference assets remain in source.
The other13 generic native templates are unchanged.

## File icons and status item

`services.yaml`, `services.yml`, `routes.yaml`, `routes.yml`, `bundles.php` and
`symfony.lock` get a small "S" icon (drawn for this plugin, not the Symfony
logo). The status bar shows `Symfony 7.1` (the `symfony/framework-bundle`
constraint; just `Symfony` without one) with the route, service and template
counts as tooltip; clicking it reindexes. It disappears when the project is
not a Symfony project.

## Tool window and commands

The _Symfony_ tool window has _Routes_ and _Index_ sections (the index lists templates, services and translations by domain); double-click opens the definition.

The route list is a flat list: a method icon (GET, POST, PUT, DELETE, PATCH, other), the path, and grey text with `[METHOD] Controller` after it. *Filter* narrows the list to one controller (the status line says which), *Clear* restores it, and *Flat/Tree* groups the rows by controller. Row actions: *Copy as curl* and *Create HTTP Request File* (writes `http/<verb>-<path>.http`, needs `project.write`; without it the request is copied). Limits: the grey text follows the path rather than sitting in a column, there is no request panel beside the list, and no gutter icon on the handler; these wait on upstream OpenIDEA requests.

 _Symfony: Warm Up Cache_ and
_Symfony: Clear Cache_ run `php bin/console cache:warmup` or `cache:clear`
(optional `process:php` capability, trusted projects only), then reindex,
because the compiled files hold the routes and services of bundles.
_Symfony: Reindex Project_ rebuilds the index. Saving a file under `config/`,
`templates/` or `translations/`, a PHP file under `src/`, `composer.json`,
`webpack.config.js` or `importmap.php` also rebuilds it.

The _Symfony Profiler_ tool window uses an explicit **Show Profiler** command or
**Refresh index snapshot** button to read `var/cache/dev/profiler/index.csv`.
Rendering performs no profiler file reads, processes or requests. The packaged
pane filters by method/status and shows selected request metadata/provenance;
the native fallback retains its read-only table. URL userinfo is removed, while
query/fragment presence is shown without their contents. IP and profiler token
remain visible only in the project-owned review; rows do not execute URLs or
look up serialized profiles. Selected details explicitly label **Open target: source
index row; no collector/profile page**. Packaged Open/double-click/Enter rechecks
the fresh invocation root, current key membership, exact source-file owner, ordinary
file/byte/parser bounds and unique current record against its cached physical line
and displayed request facts, then opens that index line at column0. Unknown,
foreign, ambiguous, moved or changed displayed records refuse navigation. Native
Table activation remains unsupported; the source index can be opened manually.
The source editor displays the original project-owned CSV, which can contain data
hidden in the summary; no browser or decoded-profile viewer is opened.

This navigation does not fingerprint exact CSV bytes: changes only to removed URL
userinfo/query/fragment contents or equivalent CSV quoting are accepted when the
same physical line and all retained facts still match. No raw hidden values are
cached to prove equality. Membership/line/fact checks are not a storage transaction;
a final source/window change before host navigation remains possible. Existing
Desktop51 acceptance is dated for the preceding690a snapshot workflow; this new
source-index Open requires separate native acceptance.

Profiler source selection falls back to the global SDK setting profilerIndexPath, default
var/cache/dev/profiler/index.csv; empty uses that default. It accepts a normalized
project-relative path (up to4096 bytes/32 components; bounded JSON string), not
absolute/traversal/glob paths. Invalid values hide cached data and refuse Open;
they never silently fall back to a different source. No source is read merely by
rendering. Choose project CSV source stores a versioned root-bound override in
plugin storage; blank input removes that override. Cancelling or invalid input
preserves the previous preference. Selection itself does not read CSV: explicit
refresh is required to adopt a changed source. Malformed, unavailable or wrong-root
stored preferences refuse fallback; storage access is required to establish the
selected source. Find cached CSV source explicitly lists owned existing CSV files
beside the bounded default/configured compiled-container files. It uses file metadata
only, deduplicates shared cache directories and always asks for a choice. It never
runs PHP or reads collector data. Discovery stores the project override; explicit
refresh still reads/adopts CSV. Automatic first-match discovery remains separate.

Snapshots bind both the concrete project root and selected relative source path
in session memory. Changing either hides former rows until explicit refresh;
a failed refresh retains last-good data only for the same pair. Source details
show the selected path. Opaque source-lease row keys renew on source adoption,
so a stale selection cannot retarget a repeated token in another configured file.
Keys remain stable for refreshes of the same pair; this is not an authorization
token, canonical file identity or a storage/window transaction.

Snapshots require a concrete project root and stay in session memory. A failed
refresh keeps only that root's last-good snapshot, prominently marked **STALE**;
missing, denied, unreadable/unknown, malformed and oversized outcomes are distinct.
An unreadable native-host Invalid error does not establish absence. Switching
roots hides old rows, refreshing another root replaces the single cache slot,
and activation or lost root access clears it. Refresh, cached render/details and
Open also require the index file's exact current project owner; a newly opened
nested project that owns the cache path hides the former cached rows and prevents
adopting its records into the parent project's snapshot. A snapshot may change after any
refresh; this is not a live watcher, automatic expiry or runtime certification.

The1 MiB index limit is checked using metadata before reading and actual returned
text afterward (a concurrent growth can still allocate the host-returned text).
Parsing accepts at most10000 physical records and16 KiB per record, then retains
the latest100 valid unambiguous main requests. Strict single-line CSV supports
quoted commas/escaped quotes; multiline/malformed/extra-column data refuses the
refresh rather than adopting a misleading partial parse. Unsupported IDs and
case-insensitive duplicate hex tokens are omitted; no missing-profile inference
is made. UTC timestamps, method/status and literal index facts are not collector
panels or effective application state.

Original comparison: retained `LocalProfilerIndex.java` separates recent-index
selection from bounded hash-owned raw-profile lookup. The original
`SymfonyProfilerRequestDetailRenderer.kt` separates overview/detail and paginates
sanitized collector data. This delivery reproduces only the explicit request
list/selected index-summary workflow, not PHP serialized data decoding, request
headers/bodies, DB/logger/Twig collectors, remote profiler or runtime execution.
The retained7219 gallery emphasizes service/template completion, so it supplies
no profiler-specific visual sign-off. Native desktop acceptance remains separate.

_Symfony: Make..._ lists `make:*` commands (from `bin/console list make`, or a
built-in list), asks for the name and options, and runs `php bin/console make:…` in a
terminal (optional `process:php`). _Symfony: Create Twig Template..._ now explains how to invoke
the reviewed **Create missing Twig template…** native intention on a quoted Twig reference; commands
cannot return atomic FileActions and this legacy command no longer writes through `tee`.
_Symfony: Insert
Form Rows from a Form Type..._ picks a form type of the project and inserts
`form_start`, a `form_row` per literal `->add('name')` and `form_end` at the caret.

A project counts as Symfony when it has `symfony.lock` or requires
`symfony/framework-bundle` or `symfony/symfony` in `composer.json`. Twig files
need the `aritti.twig` plugin (the IDE has no Twig language). Settings: the
public directory, extra compiled container and route files, extra service and
routing files outside `config/` (`extraServiceFiles`, `extraRoutingFiles`), a
switch for the practice hints (`practiceChecks`), and extra Twig
namespaces (`twigNamespaces`, comma-separated `name=directory` pairs, an empty
name is the main namespace; added to `twig: paths:`).

## Not ported

These features rely on PhpStorm's PHP, Twig and YAML PSI and type inference,
which the OpenIDEA plugin API has no equivalent for:

- container `get()` return types, Twig variable types (`{{ post.| }}`),
  form types and options, Doctrine (DQL, repositories, QueryBuilder beyond
  the removed methods above), serializer;
- the remaining inspections and intentions (they need resolved PHP types);
- refactorings, find usages, MCP tools, the profiler's per-request panels;
- general template generation forms beyond the guarded FileActions creation
  subset documented below;
- Doctrine `JOIN ... WITH` to `ON` and QueryBuilder `clearParameter`: no public
  deprecation notice to build a rule from; `@Template` docblock completion
  (legacy annotations); Vite entry completion; run configurations.

Matching is textual: a call is recognised by its method name, not its class,
and templates are looked up in `templates/` and the `twig: paths:` directories of
`config/packages/twig.yaml` (names starting with `@` are never reported missing).

## Reviewed missing-template creation

On an indexed Symfony project's Twig file, place the caret within a plain quoted template name in
`{% extends '…' %}`, `include`, `embed` or `use`, with no trailing expression/options. The native
**Create missing Twig template …** intention offers an empty file or a named body-block scaffold,
then confirms the exact source/version, conventional `templates/` destination, content and line
endings through `confirm_exact`. It infers no parent template or neighboring block names. Cancel
returns no operation; Apply returns one host-managed atomic Create with `overwrite = false` and
opens the new file. The invoking source is unchanged. The command, intention and new workflow
execute no `tee`, PHP, network request or direct write; the unused `process:tee` grant is removed.
Other existing Maker/runtime commands retain their own explicit permissions and behavior.

The supported target is a validated relative ASCII `.twig` name (at most 512 bytes, 255 per segment) beneath the
current project's conventional `templates/` directory. Empty/dot/traversal segments, absolute or
URL paths, backslashes, dynamic strings, namespace/bundle names and arrays are refused. The bounded
source scanner skips comments, complete print strings and raw/verbatim bodies; the full source is
limited to 128 KiB. Provider discovery uses positive cached membership only and never treats cache
absence as proof that a file is missing: resolution checks the actual filesystem and dirty buffers.
Existing files/directories/dirty targets and non-directory parents are refused. Host stat canonicalizes in-root symlink targets, so these checks do not detect every symlink; sandbox root confinement still applies. Missing
parents are delegated to the host Create operation. Source path/text/version/root and target vacancy
are checked again after prompts and confirmation; a final race still fails via non-overwrite.

Known alternate roots, Twig namespace settings, imported/flow/alias loader mappings, PHP/XML loader
configuration or layered `config/**/twig.*` files are outside this subset. Configuration discovery is
capped at 33 paths; supported YAML is limited to 64 KiB and its exact contents are rechecked after
prompts. An explicit `default_path` must resolve literally to this project's `templates/`. This is
conventional source-derived file creation, not runtime Twig loader resolution or a promise to resolve
every application-level missing-template warning. PHP render calls, include functions/options,
macros/imports, namespace mapping, service-class and compiler-pass/Kernel generation remain outside it.

Original comparison: pinned `TemplateCreateByNameLocalQuickFix` discovers creatable template paths,
asks for a target and runs one IDE write command; `TwigCreateContainer` infers neighboring parents and
blocks. This slice keeps the reviewed path/create concept while using only explicit literal/default-root
facts and a static selected scaffold. Original `PhpBundleCompilerPassGenerateAction` requires typed
PHP class/bundle scope before registration; those facts are not fabricated here. Deterministic tests
cover exact creates, unsaved source/dirty destination, Cancel, source/root/config changes, literal
contexts, malformed Unicode prefixes, path/config guards and bounds. No new GUI acceptance is claimed.


### Project Twig namespace path hints

`Symfony: Configure Twig Namespace Paths` opens a native table/form for project-owned editor
hints: namespace, relative directory and Enabled, with Add/Edit/Remove and Save/Cancel. An empty
namespace means the main namespace. The form previews the absolute target and labels every row as
plugin metadata, not effective runtime loader configuration. Saving changes only optional plugin
storage; no YAML/PHP/project files, processes or requests are written or executed. Storage records
carry the full project root, schema version and bounded rows. Save checks its original storage
snapshot; conflicting changes remain unsaved with a correction/reopen message. SDK storage offers
no cross-instance compare-and-swap, so a final simultaneous storage-write race remains possible.

Enabled paths must exist under the invoking root with every component reported as a directory.
Absolute/traversal/glob/parameter paths and empty segments are refused. The host canonical sandbox
rejects links escaping the root; it can report an in-root symlink target as Directory, so this
metadata-only hint editor does not claim to detect every in-root symlink. Disabled rows may preserve
syntactically valid future directories. Limits: 32 rows, 64 ASCII namespace bytes (letters, digits,
underscore), 512 path bytes, 16 path components, 255 bytes per component and 32 KiB stored JSON.
Unknown schema fields, duplicate namespace/directory pairs and mismatched stored roots are refused
rather than silently rewritten. Optional storage denial shows an explicit error. The legacy global
`twigNamespaces` scalar remains readable with the same relative-path checks; it is separate from
this project table. Its changes require explicit Reindex if no relevant filesystem event occurs.

Validated enabled hints augment cached completion/definition navigation, with an explicit
“runtime loader unverified” origin. Hint scans are bounded to 128 files per mapping, 1,024 total and
a 250 ms budget. Discovered conflicting logical names are suppressed instead of choosing a loader
priority. These editor hints neither establish runtime absence nor add automatic Rename/Usages
anchors; missing-template warnings are suppressed while hints or unknown hint configuration could
change resolution. This does not reproduce compiled loader ordering, namespace types, imported
configuration, ignored-file discovery or remote/runtime loader truth. Template generation retains
its separately reviewed conventional-root guards.

Filesystem batches now account for created/modified/removed paths, rebuilding an affected open
project once per batch for conventional source/config roots or any enabled mapped subtree (including
ancestor directory deletion). Missing mapped directories stay watched so later creation can recover.
Unrelated paths do not rebuild the index; closed project caches are pruned, and provider lookup asks
for the document's current exact project owner. Oversized batches (over 1,024 events/paths or a
4,096-byte path) drop cached facts rather than retaining a truncated stale result; reopening a source
or explicit Reindex warms them again. Save/Remove in the path editor refresh the owning index.
Profiler request execution and its HTTP boundary are unchanged and remain a separate parity gap.

Original reference: `TwigNamespaceSetting`, `TwigNamespaceDialog` and
`ConfigAddPathTwigNamespaces` in the pinned decompiled Symfony 2026.2.314 sources establish the
namespace/path/enabled editor and project/config cache ownership. This bounded adaptation exposes
plugin overrides explicitly; it does not claim the original namespace-type/runtime cache contract.
No matching settings-dialog screenshot or new native desktop acceptance is claimed. Deterministic
coverage includes Save/Cancel, oversized input, concurrent storage changes, project switches,
unknown/duplicate storage fields, invalid directories, permission denial, conflicting navigation
names, missing-directory watches, filtered filesystem refresh and oversized-batch invalidation.

Twig path draft fields and Save/Cancel controls carry a fresh edit-session identity. Delayed controls from a cancelled/replaced draft are rejected, even when multiple windows share the plugin actor; storage still uses compare-before-write rather than atomic CAS.


## P27.2 cached hover and reference ownership (partial)

Bounded hover now shows a **cached declaration candidate** for a recognized
literal route, service, template or translation key. It uses the existing
project's positive index facts only, without provider-time file reads, scans or
network access. It selects the deepest known cached project root; a sibling or
nested project cannot borrow another project's facts. Missing names and
ambiguous templates stay quiet. A cached declaration in the current edited file
is suppressed rather than silently presenting an older version of that source.
Partial indexes remain explicitly marked; cache age and effective runtime/type
resolution are unknown. No translated text, parameter values or service secret
values are displayed. A single known route definition can show its declared
path/methods; duplicates do not acquire a guessed unique route.

Hover uses exact current-literal UTF-16 ranges, bounded to 128 KiB and 4096 Twig
code regions/literals. Twig/HTML comments, raw/verbatim contents, HTML text and
quoted expression text cannot create hover references. PHP comment/outer-string
contents, heredoc/nowdoc/backtick bodies, dynamic/interpolated/escaped literals, and unproven XML
namespace-qualified ownership remain quiet. YAML service references are limited
to quoted source shapes under `config/`. These are source-pattern candidates,
not PHP receiver types, callable identity, container activation or runtime loader
resolution. Real PHP references after unrelated literal bodies remain available
through the lexical mask; no whole-document heredoc exclusion remains. XML
hover needs a namespace-aware element model first.

Translation hover resolves the exact literal call domain, a single known template-level Twig
default-domain directive, or the standard `messages` fallback only when a domain
was omitted. Explicit dynamic/missing/duplicate domains, incomplete calls and
lookalike filters stay quiet. Existing completion/definition candidates also no
longer merge other catalogues when the requested domain has no indexed names;
the broader legacy domain parser still needs a complete lexical scope model.

All registered `DocumentIndexer` output is empty. Service/route/event equality,
macro aliases, inferred class/table relations, component/controller transformed
names and synthetic file locations do not establish complete replaceable tokens.
The old parser/tests are retained as internal association evidence only and are
not returned to the host. Existing explicit definition and completion providers
remain; graph-only gutter links are intentionally gated alongside unsafe Rename.
No safe indexed usages/Rename subset is claimed by this slice. A future delivery
must separate read-only relations from complete symbol ownership and test aliases,
imports, namespaces, project boundaries and exact source spans per family.

The original public source at pinned revision `b6c1034f` was inspected:
[RouteDocumentationTarget](https://github.com/Haehnchen/idea-php-symfony2-plugin/blob/b6c1034f6669784d7a58de7535515b7da93db87e/src/main/kotlin/fr/adrienbrault/idea/symfony2plugin/routing/documentation/RouteDocumentationTarget.kt)
revalidates its context and uses a unique navigation target;
[TwigTemplateDocumentationTarget](https://github.com/Haehnchen/idea-php-symfony2-plugin/blob/b6c1034f6669784d7a58de7535515b7da93db87e/src/main/kotlin/fr/adrienbrault/idea/symfony2plugin/templating/documentation/TwigTemplateDocumentationTarget.kt)
revalidates the selected file against template identities before showing usages.
The decompiled `TwigNamespaceDialog` separately confirms explicit path/name/type
configuration, not runtime template identity. Our cached source hint is smaller
than those PSI-backed targets: no original usage counts, expanded runtime
container model or visual documentation parity is asserted.

P27.2 remains partial. Symfony's versioned `DiagnosticsProvider` now owns its
four Twig syntax rules (for-condition, component block/self-import and content
outside blocks), three YAML scalar rules, PHP command return-type/input-mode
checks, source-only PHP practice hints, advisory cached-version deprecations and
source-only Doctrine/QueryBuilder checks. These run on current Twig/YAML/PHP/ORMXML
text up to32KiB, return at most200 valid findings and require an already
recognized cached project. YAML scalar rules remain limited to configuration and
translation files; Doctrine mapping checks use *.orm.xml and *.orm.yml/yaml plus
PHP source. No discovery, saved-file read, network or missing-name lookup
runs in this provider; a partial index does not prevent these source-only facts.
The master and individual group settings apply. Manual open/save/index refresh
publication excludes those syntax, command, practice, PHP deprecation and Doctrine groups
and retains cross-file inspections, which may read project sources. The host owns provider version/close cleanup separately
from manual findings; native lifecycle acceptance for this new slice is pending.

Live PHP deprecations additionally require a complete cache and strict production
`require.symfony/framework-bundle` evidence from bounded valid JSON. A single
numeric, caret, tilde, >= or trailing-wildcard requirement supplies an advisory
lower bound; unions, upper bounds, branches, stability modifiers, unsupported
constraints and require-dev-only metadata stay quiet. Findings explicitly name
the cached constraint and state that installed/runtime version is unknown. File
events or explicit Reindex update this evidence outside provider requests. This
is separate from the legacy loose version hint used by other existing features;
no Composer dependency resolution or runtime version certification is claimed.

Twig's unclosed-tag parser diagnostics remain owned by `aritti.twig`; Symfony
owns the framework syntax rules above, configured roots/namespaces and framework
completion. Twig-local `_self` navigation remains with Twig; Symfony's legacy
macro completion parser and configured external macro definition support still
need a scoped common model. No safe indexed usages/Rename or native
hover/navigation acceptance is claimed.

## Main source-index packaged refresh

Committed main indexes, removal of a formerly recognized Symfony project, closed
root pruning and oversized-event index clears now retain native refresh and send
only model-free `ui-kit-refresh` to the main **symfony** view. The receiving page
fetches its own project/frame model. Empty no-op removals/clears and stable renders
emit no invalidation; no index-progress/render refresh loop is introduced. The
existing exact invoking-root `view_index` lookup is unchanged, without first-root
fallback or another picker. Missing/foreign index yields the existing safe empty
replacement model.

The standalone route toolbar cache is now bound to its rendered root. Selection
captures that root and the exact cached route fact; Copy cURL/Create HTTP and route
activation verify current root, unchanged selected fact and membership in the
current cached index without discovery/CLI/network. Foreign, removed or stale
route rows refuse quietly or as stale instead of using the last other root's
window cache. Controller filters cannot open a foreign cached list. Existing
route base-URL dialogs retain captured-route semantics, not a post-modal active
project guarantee. These actions do not execute an HTTP request.

Profiler state/source is unchanged; its437-test/native51 receipts remain dated
and are not recredited to this main-view source slice. Native namespace editor,
FileActions and source index ownership contracts are preserved. Mock/pinned build
proof does not imply native main-view update acceptance or full original runtime
routing parity. Cached facts can miss unobserved edits and invocation identity is
still a per-call snapshot.

P27.3 project-owned PHP class creation: in an owned PHP class document, the contextual
FileAction offers a plain sibling service class or an empty CompilerPassInterface
implementation, then a class-name input and exact full-file confirmation. It requires
exactly one matching production Composer PSR-4 string mapping among at most64
validated entries, with matching source class/filename/namespace/directory.
Composer JSON is bounded to64KiB and rejects duplicate decoded keys; directory
arrays, fallback namespaces, ambiguous mappings, absolute/traversal/dot paths and
autoload-dev-only mappings are unsupported. The compiler pass uses fully qualified framework names;
it does not infer framework installation, services, tags, constructor arguments or
Kernel registration. The service scaffold is deliberately empty. Source language,
path, version, current dirty text, project, Composer bytes, parent directories and
non-overwrite target are checked before returning the atomic Create operation.
Cached class-name collisions are refused, but this is not a complete PHP classpath
resolver. Multiple source namespaces/classes and ambiguous/array/fallback
mappings remain unsupported. Literal custom production namespace/directory
mappings are supported.
Existing literal missing-Twig creation remains unchanged. The inspected original
CompilerPass action creates/opens a class through its directory/namespace model;
it does not automatically modify Kernel. Kernel wiring is a separate workflow,
not a missing clause inferred from that action. The original PHP Service action
creates a PHP DI configuration via container.php; our plain sibling service class
is an independently scoped convenience. The former service-config template
capability is credited and now replaced by the reviewed workflow below. Broader contextual configuration and directory/type models
remain partial. No matching-wizard screenshot or native acceptance is claimed.

P27.3 compiler-pass creation is also available from the recognized root
`composer.json`: select a production PSR4 string mapping, an existing namespace
suffix/directory, and a class name, then review the exact resulting class. Root,
source version/text, complete Composer bytes, unambiguous namespace/directory,
existing host-resolved project-confined directories, class collision and vacant target checks
apply before one atomic non-overwriting Create/Open. The former native
compiler-pass New template is retired only after this contextual replacement;
its unchanged class/process template body is now used exclusively behind review.
This preserves the empty compiler process capability without inventing Kernel
registration or example service mutations. Unrelated native templates remain.
Pinned Project::stat resolves through the host sandbox before reporting kind.
In-root aliases/symlinks may therefore report Directory: parent checks establish
compatibility and host root confinement, not lstat, universal symlink rejection
or a unique physical target. Atomic non-overwrite/source checks remain separate;
any final host-prepare race or alias semantics are not claimed solved.

P27.3 contextual service-config creation: on the recognized project's root
`composer.json` (JSON source up to64KiB, unique decoded keys), a lazy FileAction
offers YAML/XML/PHP, a plain filename directly under `config/`, and an explicit
mode: defaults only, or loading a selected production PSR4 string mapping. Resource
mode verifies the existing owned source directory and includes autowire/autoconfigure,
the selected namespace/resource path and that root's Kernel.php exclusion. Paths
are encoded for YAML/XML/PHP; no App namespace or source path is guessed.

Exact full-file confirmation precedes one atomic non-overwriting Create/Open.
Source root/path/language/version/text and selected directory ownership are checked
again; existing/dirty targets and unsafe parents refuse. Cancellation writes
nothing. LF/CRLF follows source. No imports, invoking-source edits or Kernel
source registration are added. Whether the filename is loaded and which effective
runtime services result remain unknown in the review.

The three native service-config template exports are retired only after the
contextual formats cover their defaults/resource-load capability;13 unrelated
native templates remain. The original PHP Service action has broader directory
and runtime/type models. Our explicit owned configuration model is bounded, not
an equivalent wizard or matching screenshot. Broader semantic configuration,
multi-file integration and native create/undo/dirty-buffer acceptance stay separate.

P27.4 compiled-cache invalidation now includes the four built-in route/container
inputs the source index reads under `var/cache/dev`, and removal/change of their
ancestor directories. Previously these files were read during indexing but their
subsequent file events did not trigger rebuilding. Unrelated cache files, profiler
CSV and foreign roots remain outside this filter; profiler snapshots still refresh
only explicitly. The same accepted `compiledRoutes`, `compiledContainers`,
`extraServiceFiles` and `extraRoutingFiles` paths now also invalidate on exact
file or ancestor events, including paths outside conventional source folders.
Reader and event filter share the validated project-relative spelling: no absolute,
empty/dot/traversal segments, backslashes, colons or control characters; each
comma-separated field is bounded to32KiB,128 paths and4096 bytes per path. Invalid
entries are ignored. Built indexes capture these four source settings plus
publicDirectory and twigNamespaces. Cached providers, main view/navigation and
source-backed FileActions reject changed or oversized setting snapshots; the
main view offers Reindex. Explicit Reindex or the next event batch rebuilds the
cache. This is lazy admission, with no settings-change listener or broad
cache-directory watcher. Project namespace table changes through its editor
retain their existing immediate reindex path; out-of-band storage changes are
not covered by this global-setting snapshot. Rebuilding uses the existing
bounded project index and packaged-view invalidation path; no provider/render
source fetch, PHP execution or runtime cache mutation is introduced. When a
previously recognized current root has changed source settings, live diagnostics
use an empty partial index: current-document syntax/structural checks continue,
while all rejected cached project facts remain unavailable. Cold nested roots
still cannot borrow a parent cache.

Profiler filesystem events mark retained CSV rows/details STALE when the selected source or an ancestor directory changes; unrelated paths and file-open events do not. Oversized batches conservatively stale a populated snapshot, and closed-project snapshots are discarded. The model-free view invalidation performs no CSV read or data adoption; explicit refresh remains required.

Deprecated YAML/XML factory settings are owned by the current-document diagnostics provider, honoring serviceChecks and the existing service-file scope. Unsaved edits update the warning, including with partial cached indexes, because this rule needs no cross-file/runtime evidence. Manual service diagnostics retain their cross-file checks but exclude this live-owned warning code. Broader original PSI semantics remain separate.

Cached unknown YAML/XML parameter warnings are current-document diagnostics only when the existing index is complete and a compiled container was read. Current unsaved local parameter declarations suppress warnings without altering the cache; deleting a formerly cached definition can remain quiet until cache refresh. No source reader is used by the provider. The same warning code is excluded from manual publication; other service checks remain manual.

Duplicate YAML/XML parameter key warnings use current-document text under serviceChecks and the existing service-config admission. They remain active with partial cache because all evidence is in the source. The exact code is removed from manual publication.

Route-setting deprecations use current-document YAML/XML text under controllerChecks. Unconditional legacy syntax warnings work with a partial source index. Annotation-type warnings require complete matching cached Composer constraint evidence and explicitly state that installed/runtime version is unknown; loose or mismatched version hints cannot enable them. Manual route diagnostics retain cross-file controller checks while excluding the live-owned deprecated-route-setting code.

Legacy XML route requirement keys _method/_scheme now produce source warnings only in an unprefixed routes/route/requirement structure. A bounded metadata-free structural scanner handles quoted >, whitespace around =, comments, CDATA, declarations and UTF8 BOM; structural mismatches, DTDs, duplicate attributes, prefixed roots and bounds violations refuse these new facts. Literal key values only; encoded aliases/full XML validity/namespace resolution remain unsupported. Scanner bounds:128KiB,4096 tokens,64 depth,64 attributes; provider remains32KiB/200 findings.

Built-in PHP form alias warnings use current-document text only with complete matching cached Composer evidence for Symfony>=2.8, matching the original inspection version gate. Warnings state advisory version provenance and unverified receiver type: this remains the existing built-in alias/source-hint subset, rather than original resolved FormBuilderInterface coverage. PHP comments/strings are masked for source hints and call anchors; current unsaved fixes clear warnings. Manual PHP-call diagnostics exclude this code; explicit alias intentions remain separate.

PHP asset/domain diagnostics now use current-document calls with a complete cached index and null reader. Warnings identify cached source provenance and unverified runtime/receiver semantics. PHP masking excludes calls and receiver hints inside comments/strings; existing generated/external/dynamic/unseen-directory and built-in-domain suppressions remain. Manual publication excludes these exact codes, retaining deprecated-class and other cross-file PHP checks. Original getVersion/typed-receiver/alias semantics remain broader.

Shared PHP scalar-literal admission requires exactly one quoted literal with an unescaped final closing quote. Concatenation, ternary/coalescing expressions, adjacent literals and unterminated escapes are refused instead of guessed as asset paths, domains or form aliases. No expression evaluation or escape decoding was added.

Existing PHP route-class source warnings now use current-document text under controllerChecks, including with partial cache. The existing declaration-on-own-line/plain-class/abstract/inheritance bounds are retained; unsaved public __invoke repairs clear warnings. Only the PHP source portion of the manual controller group is retired; YAML/XML controller resolution remains manual. No broader original PSI/type completeness is inferred.

Twig cached route/asset/comparison/domain/legacy-assets-tag/path-hint checks now consume current-document text under twigChecks, with a complete index, null reader and cached-source/runtime-unverified provenance. Comments/verbatim regions are excluded. Named asset packages are refused against the default public index. Manual publication excludes these six codes while retaining enum declaration source reads and separate template-reference diagnostics; existing path-hint quick fixes remain guarded separately.

XML duplicate service IDs now use current-document source in bounded literal container/services/service context, under serviceChecks and with partial cache allowed. Comments/CDATA/nested argument IDs are excluded; existing empty/private-ID skips remain. The generic bounded source_xml scanner also preserves prior route requirement behavior. Unsupported DTD/prefixed/oversized structures retain the existing saved-file fallback; this fallback also honors serviceChecks. Exact supported live ownership suppresses only the corresponding manual duplicate warning. No entity/namespace/full XML validity model was added.

Duplicate routes use live source for bounded literal XML routes/route IDs, YAML actual root mapping keys (including import keys; environment child overrides are separate), and a single PHP class with imports-resolved core Route attributes on its own public methods. PHP name ranges cover the literal suffix only, independent of class prefixes. Multi-document/directive YAML and unresolved/docblock/multiclass/unsupported PHP retain saved legacy fallback; controllerChecks governs both owners. Supported source excludes its manual duplicate warning. No installed classpath, runtime route collection or full original PSI model is inferred.

Service class absence warnings use current YAML definitions and structurally selected XML service class/FQCN-ID fields (explicit class wins, including nested anonymous services) with a complete cached source index, known project PSR4 prefix and null reader. Wording is advisory cached absence with unverified runtime autoloading. Comments are excluded in supported XML; unsupported XML keeps saved fallback. Supported class warning codes are suppressed manually while deprecated-class/constructor checks still read source on saved events. Original parameter/casing/classpath resolution and quick fixes remain broader.

PHP lexical masking excludes heredoc/nowdoc bodies (ASCII labels up to128 bytes, quoted/plain headers, indented closes and CRLF) and backtick literals while preserving byte positions and line breaks. Unfinished/unsupported literal headers consume the remaining source conservatively. Doc-route annotations come only from lexical doc comments; class/namespace discovery ignores literal data. Route-class checks use lexical bodies for attributes/methods. Authored synthetic fixtures passed PHP8.4.21 -n lint and TOKEN_PARSE without fixture execution; no project PHP/collector/network was used. Full PHP grammar/Unicode-label/installed-type semantics remain broader.

Cache admission now marks observable glob errors/cap-sized results, required discovered/configured read failures and expired build/include budgets partial; absent optional default files remain normal. A template-scan observation flag distinguishes a bounded build from default/cold metadata. Live class absence additionally requires strict literal production PSR4 pairs to match the actual source-reader pairs; unproven/default/dev/array/duplicate/traversal mappings retain saved fallback. A non-partial flag reports only these observations: SDK host walk/index truncation and ignored walker errors are not exposed, so full physical/effective discovery is unproved.

Twig base template/key checks are current-document facts under twigChecks: template absence requires an observed bounded scan, unambiguous known paths and a static plain relative literal; concat/interpolation/unknown aliases are quiet. Key absence requires a known static domain and combines normal/ICU catalog facts, with Info severity and locale/runtime caveats. Explicit dynamic domains and unmodelled scoped/default directives never become messages for lookup, navigation, add-key or typo actions. The bounded quote-aware literal lexer excludes comments/verbatim and quoted Twig-looking data, recognizes adjacent tags and retains partial-string metadata completion without diagnostic facts. Syntax/named/path-rewrite checks reject literal data. Static ignore-missing targets can still report source-file absence as in the original; no runtime error is inferred. Full grammar/scoped-domain/alias/locale/native semantics remain separate.

Twig opaque regions and literal tokens now share one bounded lexical pass. Quoted fake comment/verbatim markers cannot hide later actual code; legacy raw and verbatim bodies stay opaque to refs, named checks and path rewrites. Opening/closing tags remain available to structural checks. This is lexical context admission, not a full Twig grammar/version or runtime model.

Cached hover now compares the selected cached root with current host ownership of both the invoking document and cached declaration. Cold nested projects, closed roots and newly foreign declarations stay quiet. These are metadata-only queries, with no file read/discovery; pinned Project.root requires the existing declared project.read grant, so denied metadata access is quiet. Cached age/type/runtime and non-atomic host changes remain unproved.

Definition lookup captures the current invoking logical owner for cache selection and rechecks both invoking and selected declaration owners before returning a cached location. Cold nested caches and newly foreign declaration paths stay quiet. This uses only metadata under the existing project.read capability; cached location age, source membership, effective type/runtime and atomic project leases remain unproved.

Live XML factory-setting diagnostics now use the shared bounded structural scanner
for conventional container/services/service declarations, including nested service
declarations. Attribute-name anchors exclude comments, CDATA and unrelated
structural tags. Unsupported XML stays quiet live and retains the saved-file
legacy fallback. Original PSI service-ancestor semantics are broader; no full
XML validity, entity/namespace resolution or native acceptance is claimed.

Live XML route deprecation checks use structural direct route/import attributes
and route requirement keys. Pattern warnings anchor the attribute name, while
controller bundle notation and cached-version annotation warnings anchor their
literal values. Comments, CDATA and unrelated nested tags stay opaque.
Unsupported XML retains saved legacy fallback; original PSI ancestry and
namespace/runtime semantics remain broader.

Translation extraction shares the strict known Twig default-domain guard used
by reference operations: omitted defaults and one literal prefix directive are
supported; dynamic/scoped/duplicate defaults do not silently select messages.
This remains a conservative source subset, without runtime locale/domain proof.

Translation catalog resolution rejects existing parsed YAML key paths even when
the current value is empty/null or a nested branch. This prevents duplicate
appends missed by the value-bearing translation index; full YAML alias/merge
semantics and an atomic apply lease remain unproved.
