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

## JetBrains icons

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
