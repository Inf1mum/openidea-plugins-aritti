# Laravel

Laravel support for OpenIDEA in PHP and Blade files. Partial port of the
[Laravel Plugin](https://github.com/Haehnchen/idea-php-laravel-plugin) for
IntelliJ (`de.espend.idea.laravel` 0.15.4, revision `3a07f4a5`, MIT, by Daniel
Espendiller). The port uses only the free code of the public repository; the
features that plugin doesn't have follow [Laravel's documentation](https://laravel.com/docs).

Blade highlighting, folding and the structure view come from the `aritti.blade`
plugin (the IDE has no Blade grammar). Without it, Blade files open as PHP, and
everything below still works in them.

## Completion and go to definition (⌘B)

Typing `@` (or `@fo`) in a Blade file completes Laravel's and Livewire's own
directives (`@if`, `@foreach`, `@section`, `@livewire`, `@vite`, the `@end…`
closers) and the custom ones the project registers with `Blade::directive('name'`
in `app/**`. The common ones insert a snippet with their arguments and closing
directive (`@if(…)…@endif`, `@foreach`, `@section`, `@include`, `@vite`); the
others and the custom ones insert their name only. `@@if`, `user@example.com`
and `@` inside `{{ }}` are left alone. Directives registered by vendor packages aren't known.

| Names           | Indexed from                                                                                    | Referenced in                                                                                                                                 |
| --------------- | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Routes          | `->name()`, `['as' => …]`, `Route::resource` (`only`, `except`, `names`) in `routes/`, with the name prefixes of the groups around them | `route()`, `to_route()`, `redirect()->route()`, `Redirect::route()`, `URL::signedRoute()`, `Route::has()`, `routeIs()`                       |
| Views           | `resources/views/**` (`.blade.php`, `.php`), `vendor/<package>/` overrides as `package::name`, extra directories from the settings and from `ide-blade.json` (see below) | `view()`, `View::make/exists/first`, `Route::view`, `->view()`, `->markdown()`, `new Content(view:)`; Blade `@extends`, `@include*`, `@each`, `@component` |
| Config          | `return [...]` of `config/**/*.php`, nested keys joined with `.`                                | `config()`, `Config::get/has/set/string/…`                                                                                                    |
| Translations    | `lang/<locale>/<file>.php` (`file.key`), `lang/<locale>.json`, `lang/vendor/<package>/…`, main locale first | `__()`, `trans()`, `trans_choice()`, `Lang::get/has/choice`, Blade `@lang`                                                                  |
| Env keys        | `.env.example`, `.env` (names only, values are never read)                                      | `env()`, `Env::get()`                                                                                                                         |
| Middleware      | `$middleware->alias([...])` in `bootstrap/app.php`, `$routeMiddleware` / `$middlewareAliases` in `app/Http/Kernel.php`, the framework's own | `->middleware()`, `->withoutMiddleware()`, `'middleware' => […]`; the part after `:` (`throttle:60,1`) is ignored                             |
| Controllers     | `app/Http/Controllers/**`, public instance methods                                              | `Route::get('/x', 'UserController@index')`, `['uses' => …]`, `[UserController::class, '…']`, `Route::resource('x', '…')`, `action()`         |
| Assets          | files under `public/`                                                                           | `asset()`, `secure_asset()`, `URL::asset()`                                                                                                   |
| Blade components | `resources/views/components/**` (`card/index` is `card`), `app/View/Components/**` as kebab-case (`Forms/InputField` is `forms.input-field`), classes win | `<x-forms.input-field>` and `</x-…>` tags |
| Livewire components | `app/Livewire/**`, `app/Http/Livewire/**` as kebab-case                                | `<livewire:counter>`, `@livewire('counter')`                                                                                                   |
| Vite entries    | `resources/**` files with a `css`, `scss`, `sass`, `less`, `js`, `jsx`, `ts`, `tsx` or `vue` extension | Blade `@vite(...)`, `Vite::asset()`                                                                                                            |
| Abilities       | `Gate::define('name')` in `app/Providers/**`, public methods of `app/Policies/**` (except `before`, `after`) | `->can()`, `->cannot()`, `->authorize()`, `Gate::allows/denies/check/authorize/inspect/any/none`; Blade `@can`, `@cannot`, `@canany`, `@elsecan*` |
| Validation rules | Laravel's rule names (fixed list; custom rule objects aren't strings)                         | strings and lists in `->validate()`, `->validateWithBag()`, `Validator::make()` and a form request's `rules()`; completes the rule under the cursor, not after its `:` |
| Columns         | `$table->string('name')` and the other Blueprint column methods in `Schema::create` / `Schema::table` of `database/migrations` (`id`, `timestamps`, `softDeletes`, `morphs`, `foreignIdFor`…) | `Post::where('…')`, `orderBy`, `select`, `pluck`, `whereIn`… on a static call or a chain that starts with the model (`Post::query()->with('x')->where('…')`), `DB::table('posts')->…`, and `$query->where('…')` or `$fillable` / `$guarded` / `$hidden` / `$visible` / `$casts` / `$dates` in the model file itself. The table is the model's `$table`, else the snake-case plural of its class name |
| Sections, stacks | the layouts a Blade view extends (its `@extends` chain)                                         | Blade `@section`, `@yield`, `@hasSection`, `@push`, `@prepend`, `@stack`                                                                      |

## Problems, quick fix and gutter

- Problems: a view that doesn't exist, a config key missing from an array (or
  config file) the project defines, a translation key missing from a lang file
  the project has. Keys built at run time or with a wildcard aren't checked.
  Route names are only checked when _Settings › Plugins › Laravel_ asks for it,
  because packages add routes the plugin can't see. Cheap inventory checks use
  current unsaved text through DiagnosticsProvider; source-reading Eloquent,
  function, Livewire-method and controller checks still run on open/save.
- Eloquent problems (setting `checkEloquent`, PHP files): `Post::create($request->all())`
  or `(new Post)->fill($request->all())` when `Post` in `app/Models` or `app/` has
  `$guarded = []` and no `$fillable`; a literal key of `Post::create([...])` that
  a non-empty `$fillable` leaves out (mass assignment discards it); a model using
  `Prunable` or `MassPrunable` while none of `routes/console.php`,
  `app/Console/Kernel.php` and `bootstrap/app.php` mentions `model:prune` (quiet
  when none can be read); and `orderBy('created_at', 'desc')` /
  `orderBy('created_at')` (also any literal column ending in `_at` or `_date`,
  or `date`), a hint with a quick fix to `latest()` / `oldest()` (with the
  column when it is not `created_at`).
  Models with `getFillable()`, `#[Fillable]` or `unguard` stay quiet, as do
  classes without a file in the project and dynamic class names.
- Function problems (setting `checkFunctions`, PHP files): `env-outside-config`,
  a warning on `env()` calls outside `config/`, `bootstrap/`, `vendor/` and
  `tests/`, because `env()` returns null once the configuration is cached.
- Quick fix (⌥⏎) on a missing `file.key` translation key: add `'key' => 'key'`
  to the main locale's lang file.
- Intentions (⌥⏎): `'required|email'` in a validation call or `rules()` becomes
  `['required', 'email']` (not offered when a rule is `regex:`) and back: a list of
  plain string rules becomes `'required|email'` (not offered when an element is
  a `Rule` object, closure or contains `|`, or a regex rule contains `,`);
  `'UserController@index'` becomes `[UserController::class, 'index']` when the
  file is in `App\Http\Controllers` or already imports the class (no import is
  added); in a `.blade.php` file `{{ $x }}` and `{!! $x !!}` toggle.
- More intentions (⌥⏎), each editing the open file only:
  - Blade `class="btn btn-primary"` becomes `@class(['btn', 'btn-primary'])`, and
    `class="btn {{ $size }}"` becomes `@class(['btn', $size])` (not offered with
    `{!! !!}`, `@if`/`@php`, a single quote, an empty value or an existing `@class`).
  - "Convert all validation rules in this array to arrays": every `'a|b'` of the
    array that can be converted (at least two; `regex:` and non-string items stay).
  - "Convert all controller strings in this file": every `'Name@action'` and
    `Route::resource('x', 'Name')` string whose class is in the controllers
    namespace or imported (at least two). A single `Route::resource` string
    converts to `Name::class`.
  - `'exists:users,id'`, `'unique:users,email'` (at most two arguments),
    `'in:a,b'` and `'not_in:a,b'` in a rule list become `Rule::exists(...)`,
    `Rule::unique(...)`, `Rule::in([...])`, `Rule::notIn([...])` with the
    `Illuminate\Validation\Rule` import (not offered when another `Rule` is imported).
  - `Route::`, `Auth::`, `Str::` and the other core facades get their `use` line
    when the file has a namespace and does not import the name (skipped in rare
    code where a word pair reads as a class declaration, such as `$class = Auth::user()`).
- Setting `controllersNamespace` (default `App\Http\Controllers`): the namespace
  that `'Name@action'` strings resolve in.
- Gutter: `view()` → view file; controller method → its route; view → views
  extending or including it and the controllers rendering it; `@section` →
  parent section and the views filling it; `@push` ↔ `@stack`.

## Tool window and commands

The _Laravel_ tool window has two sections, switched by buttons. _Routes_ (the default) is the route list described below; _Index_ lists views, config keys, translations, middleware and env keys; double-click opens the definition.

The route list is a flat list: a method icon (GET, POST, PUT, DELETE, PATCH, other), the path, and grey text with `[METHOD] Controller` after it. *Filter* narrows the list to one controller (the status line says which), *Clear* restores it, and *Flat/Tree* groups the rows by controller. Row actions: *Copy as curl* and *Create HTTP Request File* (writes `http/<verb>-<path>.http`, needs `process:tee`; without it the request is copied). Limits: the grey text follows the path rather than sitting in a column, there is no request panel beside the list, and no gutter icon on the handler; these wait on upstream OpenIDEA requests.

 Commands, all under the _Laravel_ category: Reindex Project, Clear
All Caches (`optimize:clear`), Clear Application / Config / Route Cache,
Clear Compiled Views, Run Migrations, Link Storage, Seed Database, Fresh
Migration and Show Route List. Fresh Migration drops all tables, so it asks
for a second run within 30 seconds. They run `php artisan` (optional
`process:php` capability, trusted projects only) and all but Show Route List
reindex afterwards. Run configurations (`php artisan serve`, `test`,
`queue:work`, `schedule:work`) are offered for any project with an `artisan`
file. Saving a file under `routes/`,
`config/`, `lang/`, `resources/views/`, `app/Http/Controllers/` or `app/Mail/`,
`.env`, `.env.example`, `bootstrap/app.php`, `app/Http/Kernel.php` or
`composer.json` also rebuilds the index.

A project counts as Laravel when it has an `artisan` file or requires
`laravel/framework`. Settings: problems on/off, route problems, one switch each
for unknown Blade components, Livewire components (tag and `@livewire`),
Inertia pages and `Model::factory()` without a factory, extra view
directories, main locale, public directory.

Problems for names that don't exist (warnings): `<x-name>` with no file under
`resources/views/components` or class under `app/View/Components`;
`<livewire:name>` / `@livewire('name')` with no class under `app/Livewire` and
no `livewire.name` view; `Inertia::render('Page')`, `inertia('Page')` and
`Route::inertia()` naming a page missing from `resources/js/Pages` or
`resources/js/pages` (`vue`, `jsx`, `tsx`, `svelte`); `Model::factory()` of a
model of the project without `database/factories/<Model>Factory.php`. Names in
a package namespace (`<x-package::alert>`), names built at run time, and
projects where a provider registers component paths or namespaces
(`anonymousComponentPath`, `componentNamespace`) stay quiet. Component
checks are also off when `composer.json` lists a package that ships
un-namespaced components: any `*/blade-*` package (icon packs such as
`blade-ui-kit/blade-heroicons`), `laravel/jetstream`, `filament/*` or
`livewire/flux`. Livewire checks are off when a provider calls
`Livewire::component()`, `componentNamespace()`, `addNamespace()` or
`addLocation()`. `Blade::component(Alert::class)` registers `alert`; a
`Blade::component()` call with no literal alias or class makes the component
check quiet. None of the five checks reports while the index is partial (the
3 s reading budget ran out).

## Components, containers and columns

- Attribute completion inside `<x-name ` and `<livewire:name `: `@props([...])`
  of an anonymous component, the constructor parameters of a class component,
  or the public properties and `mount()` parameters of a Livewire class
  (kebab-case, `:` prefix kept). `<x-slot:` completes the variables the
  enclosing component's view prints.
- `app('…')`, `resolve('…')`, `App::make/get/bound('…')` and the class of
  `@inject('x', '…')` complete and open: the framework's aliases, literal
  `bind/singleton/scoped/instance/alias('x'` calls in `app/`, and the classes
  of `app/` (PSR-4 by namespace line). In `config/app.php` (`'providers'`) and
  `bootstrap/providers.php` the classes of `app/Providers` complete as
  `Name::class`, and a class name opens its file. Names registered by vendor
  packages or built at run time are unknown.
- Column completion follows relations and scopes: `Post::find(1)->comments()->where('`,
  `Post::active()->comments()->orderBy('` and `$this->comments()->where('` in
  a model complete the columns of `Comment`; so does `$query->where('` after
  `$query = Post::query()...;` earlier in the file. A relation is a model method whose
  body calls `$this->hasMany(Comment::class)` (or `hasOne`, `belongsTo`,
  `belongsToMany`, `morphMany`, `morphOne`, `morphToMany`, `hasManyThrough`,
  `hasOneThrough`); a scope is `scopeActive` or a `#[Scope]` method. A call that is
  neither stays silent.

## Livewire, routes and unused views

- Problem `laravel.unknown-livewire-method` (setting `reportUnknownLivewireMethods`):
  `wire:click`, `wire:submit`, `wire:keydown` and the other event attributes
  naming a method that the component class of a `livewire/` view does not
  declare. Quiet for classes with a trait, a parent other than `Component`,
  `__call`, `#[Computed]`, `#[On]` or Volt, and for expressions (`$set`,
  `form.save`, `a = 1`).
- Problems `laravel.unknown-route-controller` and `laravel.unknown-route-method`
  (setting `reportUnknownRouteControllers`): a controller imported from the
  controllers namespace that has no file, and a method the controller doesn't
  declare. For `Route::resource` / `apiResource` one problem
  `laravel.missing-resource-methods` lists every missing resource method
  (setting `reportMissingResourceMethods`); `only` and `except` are honoured. Quiet for invokables, vendor controllers,
  controllers with traits, `__call` or another parent than `Controller`,
  and files that use `Route::controller(...)` groups.
- _Laravel: Find Unused Views_ lists the views no code names (`view()`,
  `@include`, `@extends`, `Route::view`, mail and `markdown()` calls) and opens
  the one you pick. Components, `errors.*`, `livewire.*`, `vendor.*`,
  `mail.*`/`emails.*` and package views are never listed; the entries are marked
  _maybe used_ when the project builds view names at run time (`view($name)`), and
  names with a literal prefix (`'reports.' . $type`) count as used.
- Document indexer: gutter links between a model and its factory, policy, observer,
  seeder and migrations (a marker exists only when the counterpart is at its
  conventional path), between an event and the listeners whose `handle()` takes it,
  and `Model::factory()` goes to the factory. Routes, views and controllers keep
  the provider's markers above. Unverified in the IDE.

## Class and string intentions

- `scopeActive()` in a model becomes `#[Scope] protected function active()` with
  the import (not offered when the scope is called by name); "Use #[Scope] on all
  scope methods" converts every eligible one with a single import.
- On a typed parameter of a method: "Move $x to a constructor property" adds a
  `private` promoted property (creating the constructor) and rewrites `$x` to
  `$this->x`; not offered when the variable is reassigned, captured by `compact`
  or when the class extends something other than `Controller`.
- On a core facade call (`Cache::`, `Log::`, `Mail::`, ...): "Inject ... instead of
  the facade" replaces every use of that facade in the class with an injected
  property of the documented underlying class, and removes the facade import
  (not offered when a use is in a static method or is not a call).
- A selected plain string in Blade or a quoted PHP string: "Extract to translation
  key" replaces it with `{{ __('messages.key') }}` / `__('messages.key')` and adds the
  entry to the existing main-locale lang file (`messages` first). Nothing is created.
- In a form request's `rules()` (`StorePostRequest` maps to `Post`): "Add validation
  rules for the columns of Post" appends the columns of the migrations that the
  array lacks (`'title' => ['required', 'string', 'max:255']`); in a model, "Add the
  missing columns to $fillable". Both need a multi-line or empty array.
- Removing the `@property`/`@method`/`@mixin` lines that IDE helper writes above a model
  class: "Remove IDE helper PHPDoc from model" at the class or its docblock, or
  _Laravel: Remove IDE Helper PHPDoc from Models_ for every model (or the models
  selected in the Project view; needs the optional `process:tee` capability).

## Generators

Commands under the _Laravel_ category ask for names and options in dialogs,
then run `php artisan` in a terminal (optional `process:php` capability):
_Run Artisan Command..._ (cached explicitly discovered runtime commands, or
clearly labeled bundled static choices without executing discovery), _Generate with artisan make..._ (30 `make:*` generators with
their flags, for example `--resource`, `--api`, `--requests`, `--inline`; the
Livewire one only when Livewire is installed), _New Eloquent Model..._ (with
`-m -f -s -c --policy --all`), _Generate for Model..._ (policy, observer,
factory, resource or controller with `--model`; right-click a model file in the
Project view to skip the model pick; with `process:tee` allowed the factory and
the JSON resource are written with the model's migration columns in them,
otherwise artisan makes the empty stub) and _Create Missing View, Component or
Controller_ (editor context menu, from the name under the caret; also a Job,
Mail, Event, Notification, Listener, Policy, Request, Resource... class named by
`new X` or `X::class` that no file defines). Nova, Filament and Livewire form
generators appear when `composer.json` requires `laravel/nova`, `filament/filament`
or `livewire/livewire`. Names are validated before they reach the command line.
The remaining legacy generators use Artisan or `tee`; API 1.3 FileActions now
provide the undoable literal missing-view subset described below. _Laravel: Show Laravel Tool Window_ brings the tool
window forward; right-click a group in it for _Reindex_.

## New file templates

New › Laravel Controller, Resource Controller, Job, Event, Listener, Mailable,
Middleware, Form Request, Policy, Observer, Enum, Seeder, Factory, Migration
(anonymous class), Pest Feature Test and Pest Unit Test, Action, Cast, Global
Scope, Notification, Validation Rule, Trait, JSON Resource, Service Provider,
View Composer, Exception, Livewire Component, Broadcast Channel, Console Command,
Http Exception, PHPUnit Feature and Unit Test, Resource Collection, plain Class,
Eloquent Model, Dusk Test, Nova Resource, Livewire Form, Fractal Transformer,
Livewire Volt Component, Inertia Vue / React / Svelte Page and Blade View, Layout,
Anonymous Component and View Extending a Layout, written from Laravel's
documentation and default stubs. The host gives a template only the file name
and the caret position, so each uses the default namespace of its type
(`App\Http\Controllers`, `App\Jobs`, ...) and has no prompts for a model or
table; edit the namespace by hand when the file lives in a subfolder. A
migration file name needs its timestamp prefix typed by hand (see
`LIMITATIONS.md`).

## JavaScript, TypeScript and Vue

In `javascript`, `typescript`, `vue`, `javascriptreact` and `typescriptreact`
files the plugin completes and opens (Go to) Ziggy route names in
`route('...')` and `route().current('...')`, including `:href="route('...')"`
in a Vue template. It works from the in-memory route index only and returns
at once when the file has no `route(` / `.current(`. Page names are not
completed in JavaScript: no Inertia client call takes one (the page name is
resolved by `resolvePageComponent` and glob imports).

Declaring a provider for these languages makes the host load the plugin for
them, so activation is limited to `on-file:resources/js/**`. The language ids
`vue`, `javascriptreact` and `typescriptreact` are declared but unverified in
the IDE; files are also recognised by extension. Scanning is line-bounded for quotes and backticks; block comments carry across
lines, so a `/*` in the text of a multi-line template literal hides later route
calls in the file, and a stray quote hides route calls to the end of its line.
Comments and strings are tracked with a simple scan (no regex literals, no Vue text awareness; a stray quote can pair with a later one on the same line).

## Not ported

These rely on PhpStorm's PHP PSI and type inference, which the OpenIDEA plugin
API has no equivalent for (see `LIMITATIONS.md`):

- container bindings made outside `app/` or with a non-literal name, facade
  resolution, and custom Blade directives registered outside `app/`;
- attributes added by accessors, and `where('column')` on a query whose model
  is lost (`$user->posts()->where(...)`, a variable of unknown type); dropped
  and renamed columns aren't tracked;
- the _Extract partial view_ refactoring.

The artisan `make:*` generators are ported as commands (see _Generators_ below).

View namespaces and extra view roots come from `ide-blade.json` at the project
root or `.idea/ide-blade.json` (both are read when present):
`{"namespaces": [{"namespace": "admin", "path": "modules/admin/views"}]}`
indexes those views as `admin::users.index`; a blank or missing `namespace`
adds a plain view root, an entry without `path` is ignored, and malformed JSON
is ignored. Limit: only these two paths are read, not an `ide-blade.json`
elsewhere in the project, and saving either file reindexes.

Vite entries, components and Livewire components are indexed when the project
is, so a new file under `resources/js` or `resources/css` shows up after a
reindex or the next save of a watched file.

Route group handling is textual: a group whose body is a file
(`->group(base_path('routes/admin.php'))`) doesn't add its prefix to the
routes in that file, and `Route::resource` guesses the singular of a
parameter by dropping a trailing `s`.

### Related-file navigation and Rename

Model/factory/policy/observer/seeder, migration/table, controller/route and
listener/event relationships are resolved by the plugin's project-owned navigation
model. Candidate target files are re-read through the project API so unsaved
changes update ranges or remove stale links. File-change events refresh cached
relationship keys, including events/listeners/observers and legacy app models.

These relationships are not published as host Rename/Find Usages anchors: names
such as `Post`, `PostFactory` and inferred `posts` are not interchangeable literal
spellings. Native language-server refactoring remains available where supported.
Framework-wide exact/scoped Rename is still implementation work. Navigation is
bounded, project-scoped and may return multiple matching targets rather than
inventing a module/type resolution result.


## Explicit Artisan runtime metadata (P11.2 PHP subset)

**Laravel: Refresh Artisan Command Metadata (Executes Project PHP)** is an
explicit Tools/command-palette action. Before running anything it shows a complete
review naming the verified project/working directory, the literal `php` runner
(resolved by the host PATH), and exact argv:
`["artisan","list","--format=json","--no-interaction","--no-ansi"]`.
The review states that Laravel bootstrap executes project-controlled PHP and may
have side effects; this is not static file parsing. Dismissing the review runs
nothing. `process:php`, `project.read` and optional `storage` must be granted;
storage is checked before execution and the host enforces the PHP grant at spawn.
No anonymous project, Docker, remote interpreter or alternative runner is used.

Discovery uses a kept host process with closed stdin, a cancellable task and at
most sixteen waits within a four-second polling budget (up to 250 ms per wait).
Cancellation, elapsed/poll budget, nonzero exit, malformed JSON, oversize output,
stale project/source identity and persistence failure do not replace the prior
saved snapshot. The process is killed on collection failure and its handle is
released. This bounds discovery collection, not side effects already performed
by executed project code. Output is rejected beyond 512 KiB combined stdout and
stderr. The supported Symfony Artisan list JSON has 1–512 commands with unique,
bounded names and argument/option definition objects or arrays (up to 128 each).
Only normalized names, descriptions and declared boolean definition flags are
cached; defaults, aliases and arbitrary extra runtime fields are not treated as
validated input forms.

The owner-checked cache identifies the exact project root, logical runner `php`,
argv and fingerprints of `artisan`, `composer.json` and `composer.lock`. Both
reader and writer enforce a 768 KiB envelope bound and a 512 KiB normalized
metadata bound before replacement. Another owner's or unreadable existing cache
is retained and refused. Project/source identities are rechecked after review
and after execution. Changed fingerprints make a saved snapshot unavailable to
the picker; failed refreshes still leave its stored evidence intact. The logical
PHP runner is not a binary digest: PATH/interpreter changes and edits to command
classes or environment require explicit refresh. No full source freshness or
interpreter attestation is claimed.

**Run Artisan Command...** reads that scoped cache and never runs `artisan list`
on opening. Descriptions identify an **executed runtime snapshot**, capture time
and argument/option counts, or **bundled static choices** when no usable cache
exists. Browsing/rendering/providers/typing do not trigger metadata execution.
The existing final selected-command terminal workflow remains separate. A full
argument/option execution form, execution history/output lifecycle and native
interpreter selection remain outside this PHP slice. The separately owned
opt-in local Docker metadata adapter below is now available.

Original/reference comparison: the inspected Laravel Idea
`PhpInterpreterArtisanCommandRunner` resolves a project interpreter/process
environment and console; `ArtisanProcessEnvironment` owns interpreter/Artisan
path discovery. The user's `yzPeedro/laravel-openidea` reference `src/artisan.rs`
models argument/option/source facts from PHP source. This delivery distinguishes
that static-source approach from explicitly executed JSON metadata rather than
claiming runtime facts from bundled command names. Existing grouped resources,
endpoints, UI, providers and index-safety ownership are preserved.


## Blade directive completion ranking (P01.3 owner gap)

Laravel remains the directive-completion owner; the Blade plugin keeps syntax,
structure, diagnostics and hover and gains no second completion provider.
Directive items now carry stable `sort_text`: exact prefix matches first,
case-folded prefix then substring candidates, followed by other candidates.
Within those tiers, known unmatched block context is preferred, then the existing
custom-before-core tie behavior and deterministic names. Duplicate labels retain
the cached custom declaration's detail and name-only insertion. A custom override
is not assumed to retain a core directive's opening/closing semantics.

The source-local scan follows the established Blade block/inline conventions:
nearest unmatched known opening blocks before the caret promote their closers;
unmatched closers after the caret promote possible opening directives. Section
closers retain **endsection → show → stop** order (then append/overwrite).
Completing an opening directive before an already present unmatched closer uses
the snippet's opening line, retaining argument placeholders without inserting a
second closer. Normal core snippets remain unchanged. Inline two-argument
sections, inline PHP and forelse's bare empty separator do not create extra
blocks.

Blade/HTML comments, escaped at-signs, echoes (including multiline and quoted
close markers), PHP blocks/strings, directive arguments and verbatim content are
excluded from directive ranking/completion. Suppressed or malformed contexts stay
quiet; custom block pairs and injected-language AST semantics are not inferred.
Ranking is limited to 512 KiB, 8192 directive tokens and 128 typed prefix bytes.
It uses current text and the already cached owning-project directive names;
there is no provider discovery, source walk, PHP execution or network request.
Core completion also works before a project cache is warm or without project
access. Native cross-provider weighting and full parser recovery are not claimed.

Comparison receipt: inspected original `BladeCompletionWeigher` (directive-item
priority) and `BladeDirectiveCompletionProvider` (prefix pass, missing opener/
closer priorities and predefined section order). Viewed cached Marketplace
`/private/tmp/parity-plan-20261108/screenshots/7569-0.png`: `@se` has `section`
first with substring candidates below. The new screenshot-context regression
checks that ordering; separate tests cover unmatched nesting, existing closers,
custom ownership, Unicode replacement ranges and suppressed contexts. No GUI
acceptance screenshot was produced for our plugin in this slice.


## Atomic literal missing-view creation (P11.3 subset)

From a supported literal view reference, the editor intention **Create missing
literal Blade view** previews one new file under the existing conventional
`resources/views` directory. It returns a host FileAction with a single
`Create(overwrite: false)` operation and opens that file after application. The
host applies it as one undoable workspace action. This path runs no Artisan,
PHP, process or network request and changes no source, imports or variables.
The legacy **Create Missing** command now guides missing-view users to this
intention; component/controller/class legacy generators remain separate.

Supported source contexts are a whole plain first argument of PHP `view(...)` /
`\view(...)`, and non-overridden core Blade `@include`, `@includeIf` or
`@extends`. Names are at most 128 ASCII bytes and 16 nonempty dot-separated
alphanumeric/underscore/hyphen segments. Namespaces, package/vendor roots,
escapes, interpolation, concatenation, arrays, alternate argument positions,
known function import/shadow ambiguity, heredocs and suppressed Blade contexts
are excluded. Custom core directive overrides are not assigned core semantics.
An owning complete cached Laravel index is required; providers do not discover
files or run project code to decide availability.

Resolution validates the exact project/file owner, editor version, supplied text
and current unsaved source buffer. It requires a plain existing conventional
view directory; custom `viewPaths` and either ide-blade metadata file are refused.
A missing `config/view.php` uses the convention; an existing one must be a plain
returned array with one literal `paths` entry containing only
`resource_path('views')` (other ordinary config fields are allowed). Dynamic,
namespaced, computed/spread, multiple or executable configuration shapes require
a separate root/namespace workflow. This static subset does not reproduce
runtime service-provider view registration or complete module resolution.

Before and after exact confirmation, the action checks configured scope,
conventional/legacy `.blade.php` and `.php` collisions, open target buffers and
parent path kinds. Source/config changes refuse resolution; a final race is
still protected by the host's no-overwrite workspace operation. The preview names
the project, source/version, literal, exact destination and complete new contents:

```html
<div>
</div>
```

Sources are limited to 128 KiB, view configuration to 64 KiB and lazy action data
to 8 KiB. Non-cancelled resolution errors explain why creation did not proceed.
This missing-view action invents no extraction scope, view data, form-request
type, PHP namespace or class import. The separate reviewed extraction and
conventional FormRequest paths below implement restricted source-safe subsets;
custom/namespaced roots and broad validation/controller promotion remain work.

Original comparison: inspected cached Laravel Idea
`LaravelUnknownViewInspection.CreateViewFix`; it uses module generation settings
and selects an explicit namespace view root before template creation. This
subset supports only the validated conventional unnamespaced root and an exact
simple skeleton, rather than pretending to reproduce its namespace/module
chooser. Tests cover source/owner/version, unsaved/Unicode positions, cancellation,
name/context exclusions, current collisions and conservative configuration.
Native GUI application/undo acceptance remains a separate host check.

### Mounted Routes / Index cache invalidation

Observed reindexing and explicit Reindex now send a model-free `ui-kit-refresh`
to the already mounted Laravel page. Losing Laravel detection notifies when the
old index is actually removed; repeated absence does not notify again. Each
frame requests its own cached model. Routes/Index rendering still does no scan,
Artisan/CLI or HTTP work and never invalidates itself.

The displayed cache uses the exact invoking IDE project root, verified against
open-project metadata. Missing/denied/foreign identity returns an empty model
without discarding another root's cache. Existing explicit host project selection
is preserved: a nested app cache is not an implicit fallback for its parent.
There is no new application picker or multi-app discovery in this change.
A direct cached Routes open event must still identify a route in that root's
current index. This does not add per-frame generation tokens or change row IDs.
Native endpoints,
source/provider/index-prefix/namespace rules and source-version behavior remain
unchanged. These are source/mock/package checks, not new desktop acceptance or
continuous watcher completeness guarantees.

### Reviewed Blade extraction and FormRequest creation (P11.3 extension)

**Extract reviewed HTML/simple-variable Blade view…** accepts a complete selected
HTML element up to4KiB in a bounded plain HTML/simple-echo Blade document. It
previews exact child contents and the complete replacement, then returns one
non-overwriting FileAction Create plus one source Edit atomically. Selected
`{{ $name }}` reads are explicitly passed as `['name' => $name]`, preserving
original child bytes/CRLF and UTF-16 selection. Ordinary HTML nesting/quotes,
fragment boundaries, unique variable names and reserved framework variables are
validated. PHP, directives/raw/comment/component/script/style contexts, computed
expressions, assignments, ambiguous/partial elements and custom include directives
stay quiet; no full PHP/Blade variable or rendering/composer model is invented.
The existing conventional view-root/configuration/collision checks are reused.

**Create reviewed mapped FormRequest …** applies to an absolute
`\Prefix\Http\Requests\NameRequest` spelling in a lexically named PHP function
parameter list. Resolution validates one exact literal production Composer PSR-4
prefix/source mapping with decoded duplicate-key checks; overlapping/fallback/dev
namespace mappings, opaque files/classmap/PSR-0 loading and unsupported roots are
refused. It creates one non-overwriting PHP file with the mapped explicit
`Prefix\Http\Requests` namespace and sole known framework import. The original
`App\` → `app/` convention remains supported; custom source-root bounds are below. The
basename cannot collide with `FormRequest`; exact/case-equivalent files, dirty
target buffers, ambiguous directory scans and foreign/nested targets are refused.
Source imports/variables/controller code stay unchanged. The exact preview shows
`authorize(): bool { return false; }` and empty `rules()`; both require manual
policy/rule authoring. This is missing-type scaffolding, not promotion or extraction
of inline validation/authorization. Framework/PHP/runtime autoloader state and
custom generation presets are not resolved. No Artisan/PHP/process is executed.

Both paths require a complete cached owning Laravel index and recheck full source,
document version/path/language, innermost owner and current unsaved project-visible
text around prompts. Configuration/metadata and vacancy are checked again after
exact confirmation. Host no-overwrite guards remain distinct from an unproven
final editor-version CAS. The existing missing-view action now also refuses targets
owned by separately opened nested projects. No direct source or target write occurs
while preparing these actions. Atomic host apply, Undo and new UI/native acceptance
remain separate from source/MockHost/package proof.

Original Laravel Idea ExtractViewHandler and PromoteRequestToFormRequestIntention
contracts were inspected locally: they use module generation settings and semantic
PHP/variable models. This implementation uses the declared conventional/plain-source
subsets, not a transplantation of those models or messages. Public Laravel12 Blade
include-data and FormRequest stub contracts were checked. Full namespace/module,
complex variable/refactoring and inline validation promotion parity remain partial;
no matching new native/original screenshot acceptance is claimed.

### Cached Laravel reference hover (P11.4)

PHP/Blade hover now describes positive closed-literal named-route, view,
configuration and translation matches already held in the owning project's
source index. It shows a relative source location and bounded route/key/locale
details where available, plus a Laravel documentation link. The literal range
uses the current document's UTF-16 positions. This request performs no source
reads, discovery, PHP/Artisan calls, network requests or diagnostic publication.
Relevant saved source changes and Reindex refresh the cached facts; unsaved
reference text is used immediately.

The hover identifies cached source provenance explicitly. A partial inventory can
show a recorded positive match but never proves absence. Unsaved target edits,
duplicate registrations, effective runtime configuration/locale, custom loaders
and overrides are not resolved. No hover is offered for same-file cached targets,
dynamic/escaped/unclosed expressions, unknown keys, foreign/nested-project cache
fallbacks or non-normalized paths. Documents over128KiB and names over512bytes are
refused; each displayed field is capped at512characters and Markdown escaped.
This adds no Rename anchors. The cheap live-inventory diagnostic migration below
is separate from the frozen hover301 receipt. Native hover/dirty-buffer/project-switch
acceptance and full original semantic parity remain unverified.


### Current-text cached inventory diagnostics (P11.4)

The DiagnosticsProvider checks unsaved PHP/Blade text against the already built
complete owning-project inventory. The cheap existing view/config/translation/route,
component/Livewire/Inertia and factory checks moved out of manual publication,
so each finding has one producer. The provider does no source reads, discovery,
network or process work. Cold/partial/foreign/nested cache fallbacks and denied
project identity return no findings; sources over32KiB are refused and output is
capped at200 diagnostics. Existing per-rule settings and the diagnostics switch
apply. Every finding states that it reflects cached source inventory, with unsaved
target changes and runtime registrations unresolved. Saved definition changes
refresh that inventory; source changes alone do not force a new diagnostic request
for an unchanged open reference document. Its next edit/open/provider request uses
the refreshed facts. Host-owned version cancellation, debounce and close cleanup
are supplied by API1.3, not established by MockHost tests.

Source-reading Eloquent, function, Livewire-method and route-controller families
remain manual open/save checks. Empty manual publication cannot clear provider
findings, and the provider cannot clear manual findings; mixed-producer regression
tests cover both directions. No complete unsaved cross-file semantic, runtime
registration, usages/Rename or native acceptance claim follows from this delivery.

### Framework source target ranges and anchor audit

Route, configuration and translation navigation now computes UTF-16 target
widths from the actual recorded source token. Effective keys can be composed:
`admin.name` may target only the literal `name` below a route group, configuration
and PHP translations may target a nested leaf, and JSON keys may use escaped raw
spellings. Route markers, route-table navigation and endpoint locations reuse the
same source width. A source token spanning lines uses a point destination rather
than constructing an invalid same-line range.

These target locations remain navigation relationships. Views name derived paths
and namespaces rather than declarations in file contents. Group/resource routes,
file/nested config prefixes, locale duplicates and encoded JSON keys do not form
a proven interchangeable rename graph. The registered DocumentIndexer emits no
host definition/reference/marker edges. Exact framework usages/Rename remains
unfinished pending complete duplicate, namespace/root/locale, escape and source
ownership handling; this audit does not promote navigation ranges into Rename.


## Opt-in local Docker Artisan metadata (P11.2)

Configure **Docker Artisan metadata** settings for a local `unix:///absolute/socket`
endpoint, the full64hex running container ID and an absolute container project
directory. **Refresh Docker Artisan Metadata (Executes Container PHP)** shows
the exact project/CLI directory, endpoint, target, workdir, inspect and exec argv
before any Docker process starts. Even `artisan list` executes project/container
PHP bootstrap code and can have side effects. Cancellation at this review runs
nothing. `project.read`, `storage` and optional `process:docker` are required.
The host PHP runner, its grant and its separate cache remain unchanged.

Every Docker invocation supplies an explicit `--host unix:///...`; no ambient
Docker context or DOCKER_HOST chooses the endpoint. The compatible Docker CLI
uses `inspect --type container --format` with fixed minimal identity/mount fields,
then `exec --workdir DIR FULL_ID php artisan list --format=json --no-interaction
--no-ansi`, then repeats inspection. No shell, TTY, interactive stdin, container
creation/start or user-selected application command executes. Before exec, fresh
inspection must match the full ID, running/unpaused/not-restarting state, image
and start identity, and exactly one bind mapping this exact project root to the
container directory. Overlapping/shadowing mounts are refused. Project source,
settings and previous-cache identity are checked around the runtime steps; changed
post-exec proof refuses adoption. Docker environment/secret fields are not requested.

Each CLI process closes stdin and has the shared four-second/16wait collection
budget and512KiB combined-output bound. On failure/cancel the Docker client is
killed; stopping that client does not guarantee stopping container PHP that has
already started. No side-effect rollback or native remote-interpreter integration
is claimed. The proof is the daemon's reported lexical bind mapping, not filesystem,
PHP-binary, socket/daemon or full application-source attestation. Named volumes,
remote TCP/SSH contexts, bind-path aliases, shadowed dependencies and Windows
named pipes are outside this supported adapter.

Accepted metadata is saved under its own bounded768KiB root-owned envelope, with
explicit endpoint/container/workdir/argv, image/start/mount proof, capture time and
the same `artisan`/Composer fingerprints as PHP discovery. Bounded strict JSON
rejects duplicate decoded object keys for metadata and both cache readers. Failed,
stale, cancelled, malformed or concurrent-refresh candidates retain the previously
saved snapshot. An observed intervening cache change refuses replacement; storage
has no atomic compare-and-set, so this is not a final CAS claim. Defaults, aliases,
complete effective runtime freshness and binary identity remain unverified.

**Browse Cached Docker Artisan Metadata (Read-only)** displays captured command
names/descriptions and their declared arguments/options/boolean flags, with the
execution source and capture time labelled. It never runs Docker/PHP or executes
a selected command. It refuses caches for changed project/source/endpoint/container
owners. Docker command facts never feed **Run Artisan Command...**, which retains
its existing PHP snapshot/bundled-static behavior. Full native UI/daemon/project
switch/provider lifecycle acceptance has not been performed.


### Mapped FormRequest namespace/source roots (P11.3 subset)

The absolute parameter `\Acme\Web\Http\Requests\StorePostRequest` can now create
`src/Web/Http/Requests/StorePostRequest.php` when production Composer metadata
declares the single literal mapping `Acme\Web\` → `src/Web/`. The existing
`App\` namespace can likewise map to another explicit source root such as `domain/`.
The whole mapped source-root directory chain must already exist and belong to the
same innermost open project. The exact preview names the source type, production
mapping, target and complete generated contents before returning one atomic
non-overwriting Create. No controller imports, variables or source text change.
Authorization stays false and rules stay empty for manual authoring. No PHP,
Composer, Artisan or Docker process executes.

Namespace prefixes are bounded ASCII identifiers (up to8segments/256bytes), with
reserved/contextual words refused conservatively; source roots are bounded literal
relative directories (up to8segments/256bytes). Dot/absolute/hidden/known ignored
roots, source-root arrays, fallback/overlapping/case-variant/dev mappings and
opaque autoloading are refused. Source files, Composer metadata, root ownership
and vacancy are revalidated around the exact review. A bounded source-root-wide
PHP path scan rejects case-equivalent target paths, including differing directory
case; a4097-result scan refuses the operation as incomplete. It is not a complete
ignored-source, arbitrary-class-declaration or runtime-autoloader proof.

Candidate recognition now requires the innermost lexical delimiter to be a named
function parameter list, rather than arbitrary call-like parentheses or a nested
default-value array. Aliases/relative types, nullable/union/by-reference/variadic
parameters, arrow functions, attributes, dynamic expressions and complex semantic
promotion remain outside this subset. Composer and PSR-4 define broader mappings
than this deliberate single-root adapter. Full inline-validation/controller
promotion, custom generation presets and complex Blade extraction remain partial;
native Apply/Undo and final source/storage/version CAS are not claimed.

### Literal helper namespace ownership

Literal references now recognize only unqualified helper names or one explicit
global prefix, such as `view(...)` and `\view(...)`. `Vendor\view(...)`,
`\Vendor\config(...)` and `namespace\route(...)` refer to explicit PHP namespaces
and no longer receive global Laravel helper hover, inventory warnings or other
shared literal-reference features merely because the final name matches.
Registered hover/live regressions cover this distinction;327 tests retain existing
global helpers and facade/directive behavior. This does not resolve imports,
locally declared shadowing helpers, namespace fallback/runtime registrations or
arbitrary facade aliases. No PHP/Artisan/network/native execution is added.


### Current project ownership of view configuration inputs

Missing-view creation and atomic local view extraction now refuse when any
conventional view configuration input (`config/view.php`, `ide-blade.json` or
`.idea/ide-blade.json`) belongs to a separately open nested project. The same
owner checks run on each validation pass before probing/reading those paths,
including missing configuration; absence in a foreign scope cannot establish
this application's default view root. Existing current-source/version, target
collision, preview and non-overwrite transaction checks remain.329 tests retain
normal conventional configuration behavior. No physical symlink isolation,
post-resolve transaction/CAS, PHP execution or new native acceptance is claimed.

### Bounded query-variable hints stay near their binding

Column completion no longer borrows a model assignment from another method or
skips nearer parameter, arrow-parameter, destructuring, foreach or unset uses of
the same variable. Its existing4KiB assignment heuristic requires the nearest
non-dereference exact variable occurrence to be a simple assignment and refuses
any intervening brace boundary. Adjacent same-function assignments, similarly
prefixed unrelated variables and consecutive `$query->...` calls retain hints.
Branch/capture/argument-flow and complete PHP variable/type resolution remain
outside this bounded heuristic; no broader Eloquent semantic parity is claimed.

### Explicit facade class namespace ownership

Shared literal references no longer treat an explicitly foreign qualified class
such as `\Vendor\View::make(...)` as Laravel merely by its final class name.
Conventional short/global aliases remain supported, along with exact explicitly
global supported `\Illuminate\Support\Facades\...` paths, `\Illuminate\Support\Env`
and `\Inertia\Inertia`. Relative qualified names are withheld without namespace
or import resolution.334 tests retain existing supported behavior and verify
registered hover/live refusal for foreign View, Config, Route and Lang classes.
Short aliases are still conventional source hints; imported/local alias ownership,
loaded classes, runtime registrations and full PHP type resolution are unverified.

### Cached provider inventories follow the document's current owner

PHP, Blade and script cache lookups now select only the cache for the document's
currently open innermost project. A cold nested project cannot inherit an outer
app's inventory; unknown/denied ownership withholds cached results. Cached roots
are preserved rather than treating query errors as confirmed closure. Warm outer
and nested documents each keep their own inventory even when the invoking window
uses the other project.337 tests cover registered completion/definition paths.
This selects cache ownership, not target freshness, a reopened-project generation,
request-time source reads, complete cache cleanup or native lifecycle acceptance.

### Opaque PHP and Blade data do not become literal inventory facts

The bounded Laravel Blade adapter now skips raw PHP blocks, Blade comments and
stored `@php`/`@verbatim` blocks when collecting literal regions and HTML component
tags. PHP close tags inside quoted strings or block comments remain PHP data;
close tags in line comments retain PHP's actual boundary behavior. Heredoc/complex
interpolation or ambiguous raw block delimiters stay opaque rather than exposing
an invented template continuation. At most128KiB/4096 spans are admitted; unknown
or oversized input withholds these source facts. No PHP or template is executed.

Hover/literal lookup enters the enclosing Blade expression rather than an inner
`@include(...)`-looking string. This also prevents false saved Used-by markers and
live missing-view/component findings from raw PHP data. Real successors after a
PHP close keep original byte/UTF16 locations; ordinary inline `@php(...)` helper
references remain supported.345 tests retain all337 prior tests and verify these
registered paths. This is a limited source adapter, not full Blade/PHP compilation,
interpolation, target property inference or native parity. Blade's independent
plugin implementation/receipt remains separate; no shared SDK change was made.

### Current Blade layout member targets

Section/stack completion and layout-member definition/gutter lookup now parse the
current bounded target text instead of projecting saved byte offsets onto a dirty
editor buffer. Shortened or changed UTF8 targets withhold removed members without
panicking; moved declarations use current positions. Existing source-width logic
selects UTF16 token widths and uses a point for multiline spellings. Each target
must remain a regular file owned by the same current innermost project; size is
checked before reading and current text is admitted up to128KiB after retrieval.

349 tests cover registered stale short/mid-UTF8 buffers, moved unsaved declarations,
Unicode ranges and newly nested-owned targets. Parent/descendant path selection
still uses the bounded saved inventory; runtime inheritance/loaders and atomic
source/read/apply snapshots remain unresolved. No PHP/Artisan or native execution
is added, and no general stale-target or project-generation guarantee is implied.

### PHP argument comments remain trivia in Blade literal adapters

Delimiter balancing, comma splitting, array arrows and plain literal admission now
ignore PHP block/line/hash comment trivia while retaining original source offsets.
Blade expression walks skip PHP comments too, so punctuation/directive-looking
comment data cannot create a new outer include region. Valid literals surrounded
by comments contribute their real associations and Unicode target ranges; computed
concatenations remain outside plain-literal admission. PHP8 attributes retain their
existing delimiter treatment, and comment markers inside quoted strings stay data.
355 tests cover registered hover/live/Used-by/definition paths plus primitive controls.
This remains a bounded source adapter; specialized component tags, heredoc/mixed
PHP grammar, full scopes and runtime template compilation are separate.

### UTF16 ranges for dynamic source keys

Gate ability, container-binding alias and migration-column definitions now derive
navigation width from the recorded source token in UTF16 units. They no longer
use UTF8 byte length for non-ASCII names. Multiline/invalid spans keep the existing
point fallback.357 tests verify all three registered definition families with
CRLF/non-BMP source and preserve all prior355 tests. This adjusts source ranges;
registration, model/type/runtime ownership and generic target freshness are unchanged.

### Current ownership of navigation destinations

Literal PHP/script definition destinations now remain inside the current document
project's innermost scope. Checked related-source navigation refuses a foreign
current target before reading it, and all returned gutter targets are filtered by
the same current owner rule. Newly opened nested route/view/config/translation or
factory scopes cannot receive stale outer navigation links; unaffected own links
remain.360 tests cover registered literal, related and gutter paths.

The guard withholds destinations and preserves caches; unknown owner queries do
not imply confirmed closure. Generic existence/range freshness, physical symlinks,
atomic project switching and specialized completion/source reads remain separate.
No PHP/Artisan execution, host Rename edges or native lifetime acceptance is added.

### Component property reads retain current source ownership

Anonymous component props/slots, class constructor props and Livewire properties
now check the target's current innermost project owner before reading cached class
or view paths. A newly separate source project no longer supplies those members
to the outer app. Owned current editor property updates remain available without
replacing its root cache.362 tests cover these registered cases. This is an owner
read guard; class alias/type/runtime inference, comprehensive source bounds and
native/component rendering acceptance remain separate.

Missing FormRequest scaffolds also recognize a short named-method parameter type through one exact unaliased `use App\Http\Requests\NameRequest;` (or the supported explicit Composer prefix) in a single semicolon namespace and plain class header. Grouped/aliased/function/constant imports, duplicate short names, multiple/braced namespaces, inherited class headers and relative qualified types are withheld. This creates the mapped missing class only; controller imports and source remain unchanged. Full validation-to-FormRequest promotion is still unsupported.

A bounded **Promote literal validation to reviewed FormRequest…** refactoring is available on an explicit `\Illuminate\Http\Request` parameter. The complete controller must have one literal semicolon namespace ending `Http\Controllers`, one plain class, and one public nonmagic named method with that sole parameter, `: array`, and either `return $request->validate([...]);` or exactly `$validated = $request->validate([...]); return $validated;`. The assignment local must be an ordinary distinct variable with no intervening statements. Rules require nonempty literal field keys and single-quoted rule strings/string lists; expressions, escaped literals, comments, imports, inheritance, additional statements and other PHP scopes are withheld. The source file must match its strict production Composer mapping. After entering `NameRequest`, a complete preview shows the new request and replacement controller. One nonoverwrite Create plus one source Edit transfers the exact rule array, uses an absolute generated parameter type and replaces the call with `validated()` to retain an array result, preserving a supported local assignment and return unchanged. `authorize()` returns true to add no request-specific gate; review authorization and the earlier FormRequest validation timing before applying. Source/version/owner, Composer and target collisions are rechecked around confirmation. No PHP/Artisan/process is run. This supported literal promotion does not provide general PHP binding resolution, controller-wide rule collection, external expression movement, custom generation settings or a host-level final commit/CAS guarantee.

### Find Literal Source Matches (read-only)

Place the caret inside a plain closed route, view, config or translation literal and run **Laravel: Find Literal Source Matches… (Read-only)**. The explicit command scans conventional app, routes, resources/views, config, lang, resources/lang and src PHP/Blade sources plus the current document. It shows matching literal spellings of the same kind with relative file/line/UTF-16 column labels. Choosing a result rechecks its current project owner and exact source text before opening it. No files are edited and no PHP, Artisan, process or network request runs. Providers do not start this scan.

These are source candidates, not semantic usages or Rename. Custom source roots, effective registrations/overrides and complete unsaved-buffer coverage remain unresolved. Comments, opaque Blade/PHP regions, dynamic/escaped strings, hidden/known ignored paths and sources outside the current innermost project are withheld. The active editor's current text is included; other files use the host's project read result. Unreadable/oversize/unresolved-owner sources mark results incomplete. Discovery over2048paths,8MiB total source or512matches aborts instead of reporting a complete result; individual sources are bounded128KiB. A source or query changed while the chooser is open must be rescanned. No definition/reference anchors are added to the host Rename graph.
