# OpenIDEA Plugin UI Kit

Shared Rust view adapters and a dependency-free web UI kit for the plugin collection.
The web foundations implement the locally supplied **Int UI Kit (Community)** reference:
13 px UI text, 28 px desktop controls, 4 px corners, compact resource tables, and
light/dark states. Reference measurements and exports are in `reference/`.
Checkboxes, radios, toggles, chevrons, tab icons and loaders use SVG drawings
exported from the supplied file. Inter faces match the bundled FigCraft renderer;
monospace uses JetBrains Mono. Font licences are preserved in `web/fonts/`.
The original `.fig` stays outside the repo.

## Preview

Open [`preview/index.html`](preview/index.html) in a browser, or serve the repository:

```sh
python3 -m http.server 8080
# http://localhost:8080/crates/ui-kit/preview/
```

The catalog includes an interactive resource hub, buttons, fields, badges, alerts,
tabs, progress, and empty states. Data and actions in the preview are fixtures.
The production JavaScript and CSS power its resource hub.

Use [`preview/parity.html`](preview/parity.html) to compare direct FigCraft exports
with live controls, and [`preview/inventory.html`](preview/inventory.html) for all
30 component categories. `components.json` maps them to their reference page IDs.
The [component audit](audit/index.html) includes selected source/live screenshots,
measured property checks, and 178 source variant state checks. Dialog and main-window
examples are explicitly identified as responsive compositions.
The [tool-window preview](preview/tool-windows.html) demonstrates label-only,
separator, and tabbed headers, plus proposed window and resource context menus.
Docking items represent host commands; the preview never changes IDE placement.

The kit restores original colors (including primary `#3574F0`) and the reference
28 px buttons/fields, 16 px checkbox/radio glyphs, and 53 × 20 px ON/OFF toggles.
Source and browser renderers can differ in text antialiasing; the comparison page
shows those differences instead of claiming pixel identity.

Open [`design/int-ui-controls.figcraft`](design/int-ui-controls.figcraft) in FigCraft
for the original component library: all 32 source sections copied directly from
the supplied document, including its variants and compositions.
CSS does not automatically synchronize with the design file.

## Native IDE icons

Embedded panels now prefer the **same IntelliJ New UI (expui) and OpenIDEA glyphs
as the released v0.1.40 host**, rather than using the JetBrains web-product set as
lookalikes. The pinned registry contains 484 host icon names and 893 unchanged SVG
files including theme/size variants. `vendor/openidea-icons/SOURCE.json` records
the exact host revision and SHA-256 hashes; original SVG headers and licensing
are retained. The procedural host quote icon is excluded from this SVG registry.

Load `native-icons.js` before `icons.js` and the shared adapters. Native names are
resolved first in custom panel actions/trees and embedded placeholders; the web
registry remains a fallback. Monochrome normalization, colored light/dark variants,
16/20 px source selection and unique SVG IDs follow the host renderer. Theme
variant switches inherit into managed shadow roots and update live.

```sh
python3 scripts/build-native-ui-icons.py --check
python3 scripts/build-ui-icons.py --check
python3 scripts/sync-plugin-ui.py --check
PLAYWRIGHT_MODULE=/absolute/path/to/@playwright/test/index.mjs node scripts/check-native-ui-icons.mjs
# Repeat with --webkit for the desktop engine.
```

## JetBrains web icon fallback

The complete [JetBrains/icons](https://github.com/JetBrains/icons) collection is
vendored at revision `29dcfa6cc3196e5c0a8b218f2ac5e6ed7661ca51`: 770 original SVG
files, including size variants. Browse them in the [icon browser](preview/icons.html).
This repository supplies JetBrains web application icons; the existing Int UI
Kit control drawings and IntelliJ header icons remain available alongside them.

Load `icons.js` before `controls.js`. The shared plugin pages already do this.

```html
<button><span data-ui-icon="copy"></span>Copy path</button>
<button class="ui-icon-button" aria-label="Add tab">
  <span data-ui-icon="add"></span>
</button>
```

```js
// A decorative SVG; its owning button provides the accessible name.
button.append(uiKitIcons.create('copy', { size: 16 }));
// Standalone meaningful image:
container.append(uiKitIcons.create('file-java', { label: 'Java file' }));
uiKitIcons.names;                 // all 770 exact source identifiers
uiKitIcons.get('add-20px');       // original width and height
uiKitControls.enhance(shadowRoot); // also renders data-ui-icon placeholders
```

Monochrome SVGs inherit `currentColor`; original multicolor fills are preserved.
SVGs default to their source dimensions. Optional `data-ui-icon-size="20"` changes
the rendered dimensions; `data-ui-icon-label` names a meaningful image. Unknown
names fail explicitly. Dynamic placeholders are enhanced by the shared observer.
The registry works offline, including `file:` previews and shadow roots, without
an npm package, external SVG requests, or loading every icon into the DOM.

Original sources, checksums, copyright, and Apache 2.0 license are preserved in
`vendor/jetbrains-icons/`; packaged copies include the license and provenance.
To update, replace the vendored sources from an explicit upstream revision,
update `SOURCE.json` and its SHA-256 checksums, then regenerate and synchronize:

```sh
python3 scripts/build-ui-icons.py
python3 scripts/build-ui-icons.py --check
python3 scripts/sync-plugin-ui.py
PLAYWRIGHT_MODULE=/absolute/path/to/@playwright/test/index.mjs node scripts/check-ui-icons.mjs
```
## Use in packaged plugin views

All custom plugin panels must use this package and follow the native IDE spirit.
The host's theme and configured UI/monospace fonts take priority; bundled design
fonts and reference colors remain standalone fallbacks. Reuse shared controls,
spacing, focus/disabled states and icons rather than private plugin skins.

The sync command validates local kit dependencies for handwritten manifest pages
and materializes assets even for standalone-only custom editors. Raw `html`/`body`
shell opt-outs are rejected. Embedded legacy HTML is managed by the shared shadow
adapter: its fallback styles stay below kit styles and inherit live kit/host
colors and fonts. Raw syntax geometry is preserved. Vendor renderers may own
only their marked inner subtree; the surrounding toolbar remains kit-owned.


Existing adopters load `web/resources/hub.html`; synchronization includes
`tokens.css` and `components.css` through `hub.css`, so they receive the shared
foundations without changes to Rust handlers or manifests.

```sh
python3 scripts/sync-plugin-ui.py
python3 scripts/sync-plugin-ui.py --check
PLAYWRIGHT_MODULE=/absolute/path/to/@playwright/test/index.mjs node scripts/check-ui.mjs
# Also verify WebKit right-button release and regenerate the comparison report:
PLAYWRIGHT_MODULE=/absolute/path/to/@playwright/test/index.mjs node scripts/check-ui.mjs --webkit --audit
```

Edit files in `web/`, then synchronize. Never edit generated `web/resources/`
copies or `src/ui_config.json`. The preview directory is deliberately outside
`web/` and is not copied into plugins. Rebuild/reload affected plugins to use the
assets in the IDE. Native lists, Services trees, and native forms retain host styling.
Legacy shadow-root editors receive shared controls and widgets, including dynamically
created GitLab review controls. Kubernetes syntax overlays keep their matched text geometry.
The OpenAPI toolbar and Spring Runtime polling page also load the shared kit.

For a custom packaged page, copy `tokens.css` and `components.css` beside it and
load them in that order. Use semantic HTML with the opt-in classes:

```html
<link rel="stylesheet" href="./tokens.css">
<link rel="stylesheet" href="./components.css">
<button type="button" class="ui-button ui-button--primary">Create resource</button>
<label class="ui-field">
  Resource name
  <input class="ui-input" aria-describedby="name-help">
  <span class="ui-help" id="name-help">Use the name shown in your project.</span>
</label>
```

Load `widgets.js` and then `controls.js` at the end of a custom page to enhance
native controls. Single-select fields become custom keyboard-operated dropdowns
while retaining their backing select, form value and native change events. Add
`data-ui-searchable` to a select for a filtered combobox. Disabled options, optgroups,
Escape, typeahead, arrows, Home/End, outside dismissal and viewport positioning are
handled by the widget. Multiple selects remain native listboxes.

The adapter observes new controls and supports shadow roots with
`uiKitControls.enhance(shadowRoot)`. `data-ui-raw` protects specialized editors.
Use `[role=tablist]` with `aria-controls` panels, `[data-ui-segmented]`,
`[data-ui-menu][popover]`, `[data-ui-tree]`, `[data-ui-tooltip="content-id"]`,
and `[data-ui-dialog="dialog-id"]` for shared interaction behavior. Notifications
use `uiKitWidgets.notify({title,message,tone})`. These helpers emit DOM events;
project actions continue through existing Rust handlers.

Checkboxes and radios keep their native input and label behavior. A separate
32 px paint layer surrounds the 16 px layout slot so the reference radio edges,
focus rings and error rings remain visible. No second CSS focus ring is added.
Set `input.indeterminate` for a mixed checkbox; radio groups use native `name`
grouping and arrow-key behavior. See [`preview/choices.html`](preview/choices.html)
for all states. Forced-colors mode restores native control rendering.

| Foundation | Contract |
| --- | --- |
| Colors | `--ui-bg`, `--ui-panel`, `--ui-fg`, `--ui-muted`, `--ui-border`, `--ui-accent`, `--ui-on-accent`, `--ui-focus`, status colors |
| Type | `--ui-font`, `--ui-mono`, `--ui-text`, `--ui-text-small` |
| Geometry | `--ui-space-1/2/3/4/6`, `--ui-radius`, `--ui-control-height` |
| Buttons | `.ui-button`, `--primary`, `--quiet`, `--danger`; native `disabled`, `aria-busy`, optional `data-state="error|success"` |
| Fields | `.ui-input`, `.ui-select`, `.ui-textarea`, `.ui-field`, `.ui-help`; `aria-invalid` and `aria-describedby` for errors |
| Feedback | `.ui-alert` + `--error/--warning/--success`; `.ui-badge` + `--info/--success/--warning/--danger` |
| Content | `.ui-empty`, `.ui-tabs`, `.ui-progress`, `.ui-code`, `.ui-code-sample`, `.ui-shortcut`, `.ui-group-header` |
| Selection | `.ui-choice`, `.ui-switch`, custom `.ui-select-shell`, `.ui-segmented` |
| Compositions | `.ui-toolbar`, `.ui-window`, `.ui-dialog-header/body/footer`, `.ui-alert-dialog`, `.ui-tree` |
| Overlays | `.ui-popup`, `.ui-menu`, `.ui-tooltip`, `.ui-notification` |

Shared surface and semantic colors resolve through documented `--openidea-*`
host tokens. Component state paints and drawings preserve the Int reference.
Inter is bundled to avoid system-font substitution. `data-theme="light|dark"` selects standalone
fallbacks, and system appearance is used if no attribute exists. Host changes apply
without reloading. The implementation preserves reference paints rather than
silently changing them to another palette.

Controls include hover, focus-visible, pressed, disabled, loading, error and success
treatments. CSS only supplies appearance: consumers must set semantics, messages,
disabled state, and behavior. Use real buttons, checkboxes and selects. Tabs need
arrow-key navigation and associated panels (see the catalog). Loading buttons should
be disabled, keep their label, and use `aria-busy="true"`. Error and success must
include text, not color alone. No action becomes enabled solely through a CSS class.
The kit respects reduced motion and increases control height to 44 px for coarse pointers.

## Authority and scope

Keep resource indexing, source navigation, capability checks and process execution
in Rust. `src/web.rs` remains the project/frame-correlated bridge; styling does not
change its command allowlist. Use the native SDK for simple native lists/forms.
This kit does not change the upstream IDE renderer or add new host APIs.

Browser checks cover theme overrides, responsive docks and interaction states.
They do not establish acceptance in a running desktop IDE.

Embedded tool-window pages rely on the native window/tab title. Do not repeat plugin branding or a view heading inside the content; place refresh and other actions in one shared toolbar. Keep descriptive guidance in contextual help or disclosures.

The resource hub composes the actual `.ui-toolbar`, `.ui-tabs`, `.ui-field`, `.ui-table` and icon-button components. Section navigation occupies one horizontal strip; utility actions retain accessible names and tooltips in 26 px icon buttons. Forms use inline labels in wide docks and stack at narrow widths. Filters use accessible hidden captions, and resource rows keep the kit’s 24 px density. Run `scripts/check-hub-native-composition.mjs` (also with `--webkit`) for dock geometry and keyboard/interaction checks.

Packaged panels resolve host-native names first in `web/hub.js`, so `branch`, `external-link`, pull-request state icons, folder/file icons and toolbar controls use the actual host drawings. Older names are translated only for registry compatibility or a web fallback. Preserve accessible action labels and use a text fallback when no faithful icon is available.

Errors and warnings from packaged view actions belong to native IDE notifications (`host::notify`), never a persistent banner in panel content. The shared Rust bridge reports errors once, classifies stale/denied/invalid actions as warnings, and keeps cancellation silent. Client rendering/snapshot-tab failures use the bounded notification-only protocol; its sequence-zero acknowledgements cannot complete pending actions. Keep informational empty states and useful field guidance in the content.

The global panel error reporter excludes only browser-generated ResizeObserver delivery diagnostics (the two standard messages, no exception object or source position). It does not call preventDefault or replace ResizeObserver, so developer logging remains available. A real exception with identical text is still reported. This does not alter the host IDE’s own observer scheduling.

Use `.ui-embedded-panel` on custom tool-window/editor page bodies. Its `--ui-toolbar-bg` follows `--ui-bg` (the host’s native island/background surface), so action bars and section strips do not appear as elevated bands. `.ui-tool-window` uses the same surface for preview headers/toolbars. General controls, menus and dialogs retain their own surface tokens.

HTTP methods use `.ui-badge.ui-http-method[data-method]` in resource tables, grouped route-tree details and selected endpoint facts. Producers send structured method data; the renderer never extracts methods from arbitrary detail text. Composite methods get individual badges. The method text remains present for accessibility, filtering and activation identity.

Resource selection does not open a permanent inspector. Row actions live in the kit context menu (right-click or Shift+F10); double-click/Enter opens the resource. Additional facts are available through Show Details… in an on-demand kit dialog. Escape restores focus to the row. Keep actions contextual rather than repeating them in an automatic sidebar.

Resource Type/Kind and Severity columns use shared badges, including their on-demand detail facts. Critical/High use red fills, Moderate/Medium amber fills, Low blue fills, and unknown values stay neutral. Severity labels keep the normal theme foreground. Original labels, including aggregate `+ unknown` qualifiers, are preserved for display, search, sorting and filtering.

Embedded custom pages suppress the browser’s default context menu on backgrounds, controls and raw vendor subtrees. The shared adapter cancels the default only: propagation stays intact for kit context menus and Shift+F10/ContextMenu-key handlers. Clipboard shortcuts are unaffected. The rule is scoped to `.ui-embedded-panel` and does not disable browser menus in standalone kit examples.

Toolbar Preferences/Settings actions use the bundled settings glyph, an icon-only control and tooltip, aligned with `.ui-toolbar-end`. Use them for on-demand settings instead of adding permanent tabs or extra tool-window entries.

Tree models render by parent hierarchy, regardless of producer row order. Sorting applies within siblings; search retains ancestors. Trees use continuous scrolling rather than row pagination, aligned disclosure/icon gutters and 24 px rows. Single-column trees omit an empty Details column/header. Context menus are created only when invoked, avoiding thousands of unused popup nodes in expanded trees.

The shared shell never injects a generic Refresh action. Toolbars contain only producer-declared actions, empty toolbars disappear, and normal view invalidation refreshes the model automatically. Empty states do not add a redundant refresh button. Explicit data-fetch/rescan/catalogue actions remain owned by their producers.

Unavailable toolbar actions are omitted at the native view boundary with `actions::available`; the shared web shell also filters unavailable actions and context-menu entries. Request-busy controls stay visible and disabled until the response arrives.

Icon buttons receive native hover tooltips from their accessible action labels through the shared controls adapter. The adapter supports dynamic controls and enhanced shadow roots, updates generated tooltips when labels change, and preserves explicit titles and custom tooltip widgets.

Plugin resource icons (`ext:fileIcon/<plugin>/<id>`) resolve through a generated local asset registry from each manifest’s `contributes.fileIcons`. Synchronization copies the declared SVGs into `web/resources/plugin-icons`; the hub renders them as 16px images and preserves `iconDark` variants. Registry assets are restricted to packaged relative SVG paths.

Context actions may declare an exact `keys` allowlist. Both the menu renderer and command bridge enforce it; an empty list exposes no rows. Plugins with dynamic menus can use `web::handle_with_actions` to derive actions from the final rendered view after a command changes scope or contents.

Use `ui-scroll-area` on panel content and custom overflow containers. Kit dialogs, popups, tabs, textareas, code samples, notifications and embedded page scrollbars share the same themed scrollbar paint; WebKit uses the kit’s 10px thumb styling and transparent track/corner. Hidden tool-window tab scrollbars retain their own policy.

Resource context menus use declared row commands when available. Otherwise, right-click/Shift+F10 selects the row through the plugin and mirrors its enabled toolbar actions (excluding section and settings/back navigation). The `item-button` operation validates the clicked row, reselects it, and checks action availability again; scoped callback IDs may rotate only through an unambiguous matching label/icon. Project-bound guards still apply. Pending menus respect dismissal and frame/project/sequence correlation.

Route rows use the generic native `web` endpoint icon in both flat lists and grouped trees. HTTP methods remain available as method labels/badges rather than method-letter icons.

Explicit context commands can set `replaces_open` in their source menu declaration to replace the generic Open entry for matching rows. Use this only when both actions have the same destination; opening a dependency declaration and opening its advisory remain distinct.

Declare inline form buttons in a plugin’s `src/ui_form_actions.json`, keyed by field ID with `button` (an existing toolbar action ID) and optional `primary`. The hub moves the action beside that input, uses the kit’s primary style when requested, preserves accessible labels and typed drafts, and wraps within narrow panels. Button dispatch retains the existing server-side toolbar eligibility checks.

Tree/resource rows can provide `activationLabel` and `activationIcon` to name their existing activation behavior accurately. Spring Initializr dependency rows use Select/Deselect instead of Open, derived from the current selection.

Declare per-view row-toolbar actions in `src/ui_context_buttons.json`. The generated model’s `contextButtons` list limits the generic row menu; an empty list keeps view-wide commands in the toolbar. A trailing-colon entry permits a scoped action prefix (for example, `pipeline-action:`); producer and bridge checks still authorize execution. Explicit item commands may override their in-context `label` and `icon` through `src/ui_actions.json`.

Custom panels may declare `src/ui_toolbar_icon_buttons.json` as a map of view IDs
to toolbar button IDs. The synchronizer emits `toolbar_icon_buttons`; the bridge
exposes the current view's `iconButtons` list. These existing buttons render as
compact icon buttons with their original labels retained for accessibility and
tooltips. Missing icons keep visible labels. The policy never changes dispatch or
authorization.
