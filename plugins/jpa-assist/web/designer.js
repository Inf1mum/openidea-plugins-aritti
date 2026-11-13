(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const bridge = window.openidea;
  const creator = window.jpaCreator;
  const attr = window.jpaAttribute;
  const rel = window.jpaRelationship;
  const ann = window.jpaAnnotations;
  const mig = window.jpaMigration;
  const rev = window.jpaReverse, preview = window.jpaPreview;
  const tabs = ['inspect', 'create', 'changes', 'migration'];
  let frame = makeFrame(), sequence = 0, root = null, busy = false, grid = null, model = null;
  let pendingRequest = null;
  function makeFrame() { const bytes = new Uint8Array(16); crypto.getRandomValues(bytes); return [...bytes].map(b => b.toString(16).padStart(2, '0')).join(''); }
  function tab(name, focus = false) {
    for (const key of tabs) {
      const active = key === name, button = $(`tab-${key}`);
      if (active) $('view-current').textContent = button.textContent;
      button.setAttribute('aria-selected', String(active)); button.tabIndex = active ? 0 : -1; $(`${key}-panel`).hidden = !active;
      if (active && focus) button.focus();
    }
    if (!focus) $('view-switch').open = false;
  }
  function controls() {
    const pending = !!model?.pending;
    const dirty = !!grid?.dirty() || creator.dirty() || attr.dirty() || rel.dirty() || ann.dirty();
    $('inspect').disabled = busy || pending || dirty;
    $('tool-add').disabled = busy || pending || !root || !model || !!model.empty;
    $('attribute').disabled = busy || pending || dirty;
    $('bind-creation').disabled = busy || pending || dirty || !root;
    $('reset').disabled = busy || (!pending && !dirty);
    creator.freeze(busy || pending); attr.freeze(busy || pending); rel.freeze(busy || pending); ann.freeze(busy || pending);
    $('prepare-annotation').disabled = busy || pending || !ann.valid();
    $('prepare-relationship').disabled = busy || pending || !rel.valid();
    $('prepare-attribute').disabled = busy || pending || !attr.valid();
    $('prepare-class').disabled = busy || pending || !creator.valid();
    $('prepare').disabled = busy || pending || !grid?.dirty() || !grid?.valid();
    $('apply').disabled = busy || !pending || !model?.canApply;
    $('discard').disabled = busy;
    if (grid) grid.disable(busy || pending);
    $('action-prepare-column').disabled = $('prepare').disabled;
    $('action-prepare-relationship').disabled = $('prepare-relationship').disabled;
    $('action-discard').disabled = $('reset').disabled;
    $('action-add-attribute').disabled = busy || pending || !root || !model || !!model.empty;
    window.jpaPalette.configure(busy || pending ? 'Finish the current request or preview first.' : '');
    mig.freeze(busy); rev.freeze(busy); $('mig-plan').disabled = busy || !root;
    const dirtyMessage = 'Preview or reset your edits before inspecting another attribute.';
    if (!busy && !pending && dirty) $('status').textContent = dirtyMessage;
    else if ($('status').textContent === dirtyMessage) $('status').textContent = '';
    for (const id of ['inspect', 'prepare', 'apply', 'prepare-class', 'prepare-attribute', 'prepare-relationship', 'prepare-annotation', 'bind-creation', 'mig-plan', 'mig-generate']) $(id).setAttribute('aria-busy', String(busy && pendingRequest?.button === id));
  }
  function send(operation, button = '') {
    if (!bridge || busy) return;
    busy = true;
    const seq = ++sequence;
    const timer = setTimeout(() => {
      if (pendingRequest?.sequence !== seq) return;
      busy = false; pendingRequest = null;
      // Keep the draft. A timeout never implies an edit was not applied.
      $('status').textContent = 'The request has not returned. Inspect the source before retrying.'; controls();
    }, 15000);
    pendingRequest = {sequence: seq, timer, button, operation}; controls();
    const context = model?.source ? {path: model.source.path, version: model.source.version, attribute: model.selected} : null;
    bridge.postMessage(JSON.stringify({channel: 'jpa-designer', protocol: 1, frame, sequence: seq, root, context, operation}));
  }
  const readOnly = (key, label, value, detail) => ({key, label, value, origin: 'explicit', editable: false, detail});
  const cardinality = {ManyToOne: 'Many to one', OneToMany: 'One to many', OneToOne: 'One to one', ManyToMany: 'Many to many'};
  function inspectorRows(next) {
    const picked = next.attributes.find(a => a.name === next.selected);
    const fetch = (next.relationship || []).find(p => p.key === 'fetch');
    const grids = window.uiKitPropertyGrid;
    grids.render($('general-properties'), [
      readOnly('name', 'Name', picked?.name ?? '', 'Read-only here'),
      readOnly('type', 'Type', picked?.type ?? '', 'Declared type, read-only here'),
      readOnly('transient', 'Transient', false, 'Transient attributes are not listed'),
      readOnly('mutable', 'Mutable', '', 'Not read from source'),
      readOnly('fetchType', 'Fetch type', fetch?.value ?? '', fetch ? 'Edit it under Association' : 'Applies to associations'),
      readOnly('fetchMode', 'Fetch mode', '', 'Not read from source'),
    ]);
    const association = next.association;
    $('section-association').hidden = !association;
    if (!association) { $('association-properties').replaceChildren(); return; }
    const type = picked?.type || '', collection = type.includes('<') ? type.slice(0, type.indexOf('<')) : '';
    grids.render($('association-properties'), [
      readOnly('cardinality', 'Cardinality', cardinality[association.cardinality] || association.cardinality, 'Declared relationship annotation'),
      readOnly('collectionType', 'Collection type', collection, collection ? 'From the declared type' : 'Not a collection'),
      readOnly('owningSide', 'Owning side', association.owningSide, association.owningSide ? 'No mappedBy declared' : 'mappedBy declared'),
      readOnly('mappedBy', 'Mapped by', association.mappedBy ?? '', association.mappedByResolved ? (association.mappedBy ? 'Declared in source' : 'Not declared') : 'Expression not understood'),
    ]);
  }
  function tooltips() {
    for (const row of document.querySelectorAll('.ui-property-row')) {
      const note = row.querySelector('.ui-property-origin');
      if (note) row.title = note.textContent;
    }
  }
  function filterInspector() {
    const q = $('inspector-search').value.trim().toLowerCase();
    let shown = 0, total = 0;
    for (const section of document.querySelectorAll('.inspector-section')) {
      const rows = [...section.querySelectorAll('.ui-property-row, #relationship-fields > :is(.ui-field, .ui-check, fieldset)')].filter(row => !row.hidden);
      let matches = 0;
      for (const row of rows) { const match = !q || row.textContent.toLowerCase().includes(q); row.classList.toggle('is-filtered', !match); if (match) matches++; }
      const keep = !q || matches > 0 || section.querySelector('summary').textContent.toLowerCase().includes(q);
      section.classList.toggle('is-filtered', !keep);
      if (q && keep) section.open = true;
      if (!section.hidden) { total++; if (keep) shown++; }
    }
    $('inspector-no-match').hidden = !q || shown > 0 || total === 0;
  }
  function openAttributeForm() {
    $('add-attribute').open = true; $('attr-name').focus(); $('add-attribute').scrollIntoView({block: 'nearest'});
  }
  function render(next) {
    if (model && model.root !== next.root) { mig.clear(); rev.clear(); }
    model = next; root = next.root;
    creator.configure(next.creation || null); attr.configure(next.empty ? null : next.source);
    rel.configure(next.empty ? null : next.relationship || null, next.source, next.selected);
    ann.configure(next.empty ? null : next.annotations || null, next.source);
    $('empty').hidden = !next.empty; $('source').hidden = !!next.empty;
    $('preview').hidden = !next.pending; $('no-changes').hidden = !!next.pending;
    if (!next.empty) {
      $('class-name').textContent = next.className; $('role').textContent = next.role;
      $('source-path').textContent = `${next.source.path} · version ${next.source.version}`;
      $('source-caption').title = $('source-path').textContent;
      $('attribute-type').textContent = next.attributes.find(a => a.name === next.selected)?.type ?? '';
      $('provenance').textContent = next.provenance;
      $('inspection-note').textContent = next.inspectionNote || '';
      $('attribute').replaceChildren(...next.attributes.map(a => { const option = document.createElement('option'); option.value = a.name; option.textContent = a.name; option.selected = a.name === next.selected; return option; }));
      grid = window.uiKitPropertyGrid.render($('properties'), next.properties, controls);
      inspectorRows(next);
    } else { grid = null; $('properties').replaceChildren(); $('general-properties').replaceChildren(); $('association-properties').replaceChildren(); $('section-association').hidden = true; }
    if (next.pending) {
      $('preview-path').textContent = next.source.path;
      $('before').textContent = next.pending.before; $('after').textContent = next.pending.after;
      tab('changes');
    }
    controls(); tooltips(); filterInspector();
  }
  if (bridge) bridge.onMessage(raw => {
    let message; try { message = typeof raw === 'string' ? JSON.parse(raw) : raw; } catch { return; }
    if (message?.channel !== 'jpa-designer') return;
    if (message.invalidate) {
      if (!busy && !grid?.dirty() && !creator.dirty() && !attr.dirty() && !rel.dirty() && !ann.dirty() && !model?.pending) send({kind: 'snapshot'});
      return;
    }
    if (message.frame !== frame || message.sequence !== pendingRequest?.sequence) return;
    if (message.layout) for (const [key, shown] of Object.entries({palette: message.layout.palette, inspector: message.layout.inspector, toolbar: message.layout.toolbar})) document.body.classList.toggle(`no-${key}`, shown === false);
    if (message.palette) window.jpaPalette.setModel(message.palette);
    if (message.layout) document.body.classList.toggle('minimal', message.layout.minimal === true);
    const completed = pendingRequest.operation.kind;
    clearTimeout(pendingRequest.timer); pendingRequest = null; busy = false;
    if (message.reset) { frame = makeFrame(); sequence = 0; root = null; model = null; grid = null; creator.configure(null); attr.configure(null); rel.configure(null); ann.configure(null); mig.clear(); rev.clear(); send({kind: 'snapshot'}); return; }
    // A rejected request must not erase unprepared user input. A changed owner
    // or source revision still forces a fresh model rather than rebasing edits.
    const sameSource = model?.source && message.model?.source && model.root === message.model.root
      && model.source.path === message.model.source.path && model.source.version === message.model.source.version;
    if (message.model && !(message.ok === false && sameSource && (grid?.dirty() || creator.dirty() || attr.dirty() || rel.dirty() || ann.dirty()) && !message.model.pending)) render(message.model);
    else controls();
    $('status').textContent = message.ok === false ? 'The operation was not completed. See the IDE notification.' : '';
    if (completed === 'plan-migration' || completed === 'generate-migration') {
      if (message.ok && message.migration?.kind === 'plan' && completed === 'plan-migration') mig.showPlan(message.migration);
      else if (message.ok && message.migration?.kind === 'sql' && completed === 'generate-migration') mig.showSql(message.migration);
      else if (!message.ok && completed === 'generate-migration') mig.failed();
      mig.freeze(false);
      if (message.ok) $('status').textContent = '';
    }
    if (completed.startsWith('reverse-')) { if (message.ok && message.reverse) rev.show(message.reverse); controls(); }
    if (message.ok && completed === 'apply') { tab('inspect'); $('status').textContent = 'Source updated. Undo is available in the editor.'; }
    if (message.ok && completed === 'bind-creation') tab('create');
    if (message.ok && completed === 'reset') { creator.reset(); attr.reset(); rel.reset(); ann.reset(); tab(model?.creation ? 'create' : 'inspect'); controls(); }
  });
  window.jpaPalette.onChoose(choice => { attr.preset(choice); openAttributeForm(); });
  $('inspector-search').addEventListener('input', filterInspector);
  $('inspector-search').addEventListener('keydown', event => { if (event.key === 'Escape' && event.target.value) { event.preventDefault(); event.target.value = ''; filterInspector(); } });
  for (const [id, run] of [['action-prepare-column', () => $('prepare').click()], ['action-prepare-relationship', () => $('prepare-relationship').click()], ['action-add-attribute', openAttributeForm], ['action-discard', () => $('reset').click()]]) {
    $(id).addEventListener('click', () => { $('inspector-actions').open = false; run(); });
  }
  document.addEventListener('click', event => { if (!event.target.closest('#inspector-actions')) $('inspector-actions').open = false; });
  $('inspector-actions').addEventListener('keydown', event => { if (event.key === 'Escape' && event.currentTarget.open) { event.currentTarget.open = false; event.currentTarget.querySelector('summary').focus(); } });
  $('tool-add').addEventListener('click', openAttributeForm);
  $('view-switch').addEventListener('keydown', event => { if (event.key === 'Escape' && event.currentTarget.open) { event.currentTarget.open = false; $('view-current').focus(); } });
  document.addEventListener('click', event => { if (!event.target.closest('#view-switch')) $('view-switch').open = false; });
  $('view-switch').addEventListener('focusout', event => { if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) event.currentTarget.open = false; });
  $('inspect').addEventListener('click', () => send({kind: 'inspect'}, 'inspect'));
  $('bind-creation').addEventListener('click', () => send({kind: 'bind-creation'}, 'bind-creation'));
  $('prepare-class').addEventListener('click', () => { if (creator.valid()) send({kind: 'prepare-class', draft: creator.draft()}, 'prepare-class'); });
  mig.onChange(controls);
  rev.onRequest(operation => send(operation));
  preview.onRoutes(routes => send(routes ? {kind: 'plan-migration', routes} : {kind: 'plan-migration'}, 'mig-plan'));
  $('mig-plan').addEventListener('click', () => send({kind: 'plan-migration'}, 'mig-plan'));
  $('mig-generate').addEventListener('click', () => { if (mig.valid()) send({kind: 'generate-migration', ...mig.draft()}, 'mig-generate'); });
  creator.onChange(controls); attr.onChange(controls); rel.onChange(controls); ann.onChange(controls);
  $('prepare-annotation').addEventListener('click', () => { if (ann.valid()) send({kind: 'prepare-annotation', edit: ann.draft()}, 'prepare-annotation'); });
  $('prepare-relationship').addEventListener('click', () => { if (rel.valid()) send({kind: 'prepare-relationship', changes: rel.changes()}, 'prepare-relationship'); });
  $('prepare-attribute').addEventListener('click', () => { if (attr.valid()) send({kind: 'prepare-attribute', draft: attr.draft()}, 'prepare-attribute'); });
  $('attribute').addEventListener('change', () => send({kind: 'select', attribute: $('attribute').value}));
  $('prepare').addEventListener('click', () => { if (grid?.valid()) send({kind: 'prepare', changes: grid.changes()}, 'prepare'); });
  $('apply').addEventListener('click', () => { if (model?.pending) send({kind: 'apply', token: model.pending.token}, 'apply'); });
  for (const id of ['reset', 'discard']) $(id).addEventListener('click', () => { if (model?.pending) send({kind: 'reset'}); else if (model) { render(model); creator.reset(); attr.reset(); rel.reset(); ann.reset(); controls(); } });
  for (const [index, key] of tabs.entries()) {
    const button = $(`tab-${key}`); button.addEventListener('click', event => tab(key, event.detail === 0));
    button.addEventListener('keydown', event => {
      let target;
      if (event.key === 'ArrowRight') target = tabs[(index + 1) % tabs.length];
      else if (event.key === 'ArrowLeft') target = tabs[(index + tabs.length - 1) % tabs.length];
      else if (event.key === 'Home') target = 'inspect'; else if (event.key === 'End') target = 'migration';
      if (target) { event.preventDefault(); tab(target, true); }
    });
  }
  if (bridge) send({kind: 'snapshot'});
  else { $('inspect').disabled = true; $('status').textContent = 'Open this panel inside the IDE to inspect project source.'; }
})();
