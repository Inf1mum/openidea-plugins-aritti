# OpenIDEA plugin UI/UX preferences

The user requested on 2026-11-08 that future improvements to **all plugins in
this repository** adapt the UI/UX ideas from [yzPeedro/laravel-openidea](https://github.com/yzPeedro/laravel-openidea/tree/85e9af5239a83e9161eaab5d43fd146611ba0474).
Laravel is the reference implementation, not the scope of this guidance.
Choose navigation, columns, filters and actions for each plugin's domain;
do not force every plugin into a Laravel-style hub.
This is a source-and-screenshot reference, not an independently tested plugin.
Apply the interaction patterns to our existing functionality; this preference
does not authorize unrelated rewrites or the addition of every reference feature.

## Presentation choices

- Keep related workflows in a coherent framework hub. Use grouped navigation
  with a clear active state and meaningful counts when many sections exist;
  avoid growing a toolbar into a long row of section buttons.
- Prefer real resource columns over concatenated labels. Routes should expose
  method, path, name and handler separately, with source navigation. Use short
  method badges and monospace for paths, class names and other identifiers.
- Put text search and relevant facet filters beside the table. Show active
  filters, a clear/reset action and filtered/total counts. Preserve useful
  filter, sort and page state as users switch sections.
- Use a command list and adjacent argument/option form for command discovery
  workflows such as Artisan. Show what will run and where. Keep short output
  in a contextual pane/drawer with rerun/copy actions; use terminals or supported
  streaming tabs for long-running commands.
- Reuse sorting, pagination, row navigation and loading/empty/error states
  across sections. Distinguish an empty project from no filter matches and
  from runtime access being unavailable.
- Use IDE `--openidea-*` theme/font tokens, restrained framework accents,
  visible keyboard focus and labelled controls. Check narrow dock widths,
  keyboard navigation, light/dark themes and long identifiers.

## Implementation choices

Choose native views for simple lists/forms. Use a packaged web view when
inline filters, multiple panes or richer interactions materially improve the
workflow. These layouts need not wait for new native contribution points.
API 1.3 grouped views and detail panes are also options; check the target API.

The reference declares `web = "web/hub.html"` in `plugin.toml`, uses a vanilla
HTML/CSS/JavaScript frontend, and bridges JSON requests to Rust. Its
`src/panels.rs` handles typed requests, while `src/sections.rs` supplies columns,
rows, source targets and filter facets. Adapt this separation into small shared
frontend modules and reusable Rust data models; avoid reproducing its large
single-file frontend.

Keep indexes and process execution in Rust. Reuse existing plugin logic and
`crates/ui-kit` conventions where appropriate. Scope requests and cached data
to the project; handle obsolete responses and denied optional runtime grants.
Static browsing should remain useful without PHP/Docker execution.

Do not use inline-script `Body::Html` for interactive tools without checking the
desktop CSP. The reference moved to a packaged web view to avoid inherited
`srcdoc` script restrictions. Packaged assets must use relative paths and the
documented bridge. Validate incoming messages and source paths in Rust.

## Application across plugins and verification

Apply these principles whenever improving any plugin UI. Examples include
route tables and framework commands in Laravel/Symfony/Spring, resource lists
and contextual operations in Kubernetes, migration tables and execution output
in Flyway/Liquibase, merge requests and pipeline details in GitLab, and log
filters and event details in Ideolog. Simple plugins may need only a small
native view. Select the implementation order from the current task; there is
no Laravel-first requirement. Share components where interaction patterns
match. Broader runtime features are separate tasks, not prerequisites for a
UI improvement.

Validate manifest/package compatibility and bridge behavior. Render the actual
UI at realistic dock sizes and exercise search, facets, source navigation,
command feedback and failures. Report separately what passed in mocks, in a
browser preview and in the desktop IDE.

The reference is MIT, copyright Pedro Pessoa. If reusing code, preserve its
required notice and check asset licences separately; inspiration alone does not
require copying its code, branding or artwork.
