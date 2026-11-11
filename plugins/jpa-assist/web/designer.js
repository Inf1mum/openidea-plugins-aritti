(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const bridge = window.openidea;
  const creator = window.jpaCreator;
  const attr = window.jpaAttribute;
  const rel = window.jpaRelationship;
  const mig = window.jpaMigration;
  const tabs = ['inspect', 'create', 'changes', 'migration'];
  let frame = makeFrame(), sequence = 0, root = null, busy = false, grid = null, model = null;
  let pendingRequest = null;
  function makeFrame() { const bytes = new Uint8Array(16); crypto.getRandomValues(bytes); return [...bytes].map(b => b.toString(16).padStart(2, '0')).join(''); }
  function tab(name, focus = false) {
    for (const key of tabs) {
      const active = key === name, button = $(`tab-${key}`);
      button.setAttribute('aria-selected', String(active)); button.tabIndex = active ? 0 : -1; $(`${key}-panel`).hidden = !active;
      if (active && focus) button.focus();
    }
  }
  function controls() {
    const pending = !!model?.pending;
    const dirty = !!grid?.dirty() || creator.dirty() || attr.dirty() || rel.dirty();
    $('inspect').disabled = busy || pending || dirty;
    $('attribute').disabled = busy || pending || dirty;
    $('bind-creation').disabled = busy || pending || dirty || !root;
    $('reset').disabled = busy || (!pending && !dirty);
    creator.freeze(busy || pending); attr.freeze(busy || pending); rel.freeze(busy || pending);
    $('prepare-relationship').disabled = busy || pending || !rel.valid();
    $('prepare-attribute').disabled = busy || pending || !attr.valid();
    $('prepare-class').disabled = busy || pending || !creator.valid();
    $('prepare').disabled = busy || pending || !grid?.dirty() || !grid?.valid();
    $('apply').disabled = busy || !pending || !model?.canApply;
    $('discard').disabled = busy;
    if (grid) grid.disable(busy || pending);
    mig.freeze(busy); $('mig-plan').disabled = busy || !root;
    const dirtyMessage = 'Preview or reset your edits before inspecting another attribute.';
    if (!busy && !pending && dirty) $('status').textContent = dirtyMessage;
    else if ($('status').textContent === dirtyMessage) $('status').textContent = '';
    for (const id of ['inspect', 'prepare', 'apply', 'prepare-class', 'prepare-attribute', 'prepare-relationship', 'bind-creation', 'mig-plan', 'mig-generate']) $(id).setAttribute('aria-busy', String(busy && pendingRequest?.button === id));
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
  function render(next) {
    if (model && model.root !== next.root) mig.clear();
    model = next; root = next.root;
    creator.configure(next.creation || null); attr.configure(next.empty ? null : next.source);
    rel.configure(next.empty ? null : next.relationship || null, next.source, next.selected);
    $('empty').hidden = !next.empty; $('source').hidden = !!next.empty;
    $('preview').hidden = !next.pending; $('no-changes').hidden = !!next.pending;
    if (!next.empty) {
      $('class-name').textContent = next.className; $('role').textContent = next.role;
      $('source-path').textContent = `${next.source.path} · version ${next.source.version}`;
      $('provenance').textContent = next.provenance;
      $('inspection-note').textContent = next.inspectionNote || '';
      $('attribute').replaceChildren(...next.attributes.map(a => { const option = document.createElement('option'); option.value = a.name; option.textContent = `${a.name}: ${a.type}`; option.selected = a.name === next.selected; return option; }));
      grid = window.uiKitPropertyGrid.render($('properties'), next.properties, controls);
    } else { grid = null; $('properties').replaceChildren(); }
    if (next.pending) {
      $('preview-path').textContent = next.source.path;
      $('before').textContent = next.pending.before; $('after').textContent = next.pending.after;
      tab('changes');
    }
    controls();
  }
  if (bridge) bridge.onMessage(raw => {
    let message; try { message = typeof raw === 'string' ? JSON.parse(raw) : raw; } catch { return; }
    if (message?.channel !== 'jpa-designer') return;
    if (message.invalidate) {
      if (!busy && !grid?.dirty() && !creator.dirty() && !attr.dirty() && !rel.dirty() && !model?.pending) send({kind: 'snapshot'});
      return;
    }
    if (message.frame !== frame || message.sequence !== pendingRequest?.sequence) return;
    const completed = pendingRequest.operation.kind;
    clearTimeout(pendingRequest.timer); pendingRequest = null; busy = false;
    if (message.reset) { frame = makeFrame(); sequence = 0; root = null; model = null; grid = null; creator.configure(null); attr.configure(null); rel.configure(null); mig.clear(); send({kind: 'snapshot'}); return; }
    // A rejected request must not erase unprepared user input. A changed owner
    // or source revision still forces a fresh model rather than rebasing edits.
    const sameSource = model?.source && message.model?.source && model.root === message.model.root
      && model.source.path === message.model.source.path && model.source.version === message.model.source.version;
    if (message.model && !(message.ok === false && sameSource && (grid?.dirty() || creator.dirty() || attr.dirty() || rel.dirty()) && !message.model.pending)) render(message.model);
    else controls();
    $('status').textContent = message.ok === false ? 'The operation was not completed. See the IDE notification.' : '';
    if (completed === 'plan-migration' || completed === 'generate-migration') {
      if (message.ok && message.migration?.kind === 'plan' && completed === 'plan-migration') mig.showPlan(message.migration);
      else if (message.ok && message.migration?.kind === 'sql' && completed === 'generate-migration') mig.showSql(message.migration);
      else if (!message.ok && completed === 'generate-migration') mig.failed();
      mig.freeze(false);
      if (message.ok) $('status').textContent = '';
    }
    if (message.ok && completed === 'apply') { tab('inspect'); $('status').textContent = 'Source updated. Undo is available in the editor.'; }
    if (message.ok && completed === 'bind-creation') tab('create');
    if (message.ok && completed === 'reset') { creator.reset(); attr.reset(); rel.reset(); tab(model?.creation ? 'create' : 'inspect'); controls(); }
  });
  $('inspect').addEventListener('click', () => send({kind: 'inspect'}, 'inspect'));
  $('bind-creation').addEventListener('click', () => send({kind: 'bind-creation'}, 'bind-creation'));
  $('prepare-class').addEventListener('click', () => { if (creator.valid()) send({kind: 'prepare-class', draft: creator.draft()}, 'prepare-class'); });
  mig.onChange(controls);
  $('mig-plan').addEventListener('click', () => send({kind: 'plan-migration'}, 'mig-plan'));
  $('mig-generate').addEventListener('click', () => { if (mig.valid()) send({kind: 'generate-migration', ...mig.draft()}, 'mig-generate'); });
  creator.onChange(controls); attr.onChange(controls); rel.onChange(controls);
  $('prepare-relationship').addEventListener('click', () => { if (rel.valid()) send({kind: 'prepare-relationship', changes: rel.changes()}, 'prepare-relationship'); });
  $('prepare-attribute').addEventListener('click', () => { if (attr.valid()) send({kind: 'prepare-attribute', draft: attr.draft()}, 'prepare-attribute'); });
  $('attribute').addEventListener('change', () => send({kind: 'select', attribute: $('attribute').value}));
  $('prepare').addEventListener('click', () => { if (grid?.valid()) send({kind: 'prepare', changes: grid.changes()}, 'prepare'); });
  $('apply').addEventListener('click', () => { if (model?.pending) send({kind: 'apply', token: model.pending.token}, 'apply'); });
  for (const id of ['reset', 'discard']) $(id).addEventListener('click', () => { if (model?.pending) send({kind: 'reset'}); else if (model) { render(model); creator.reset(); attr.reset(); rel.reset(); controls(); } });
  for (const [index, key] of tabs.entries()) {
    const button = $(`tab-${key}`); button.addEventListener('click', () => tab(key));
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
