// Inspector sections for the existing @Table indexes / unique constraints and @NamedQuery entries of the open entity.
// Rust lists them (model.annotations) and validates and edits the source; this adapter only renders and collects the draft.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const form = $('annotation-form');
  let model = null, key = '', picked = null, change = () => {};
  const entries = () => model ? [...model.indexes, ...model.queries] : [];
  const current = () => entries().find(e => e.id === picked);
  const kindLabel = {index: 'Index', unique: 'Unique constraint', query: 'Named query'};
  const columnList = v => v.split(',').map(c => c.trim()).filter(Boolean);
  function row(entry) {
    const li = document.createElement('li'), button = document.createElement('button');
    button.type = 'button'; button.className = 'ui-option annotation-entry'; button.dataset.id = entry.id;
    button.setAttribute('aria-pressed', String(entry.id === picked));
    const title = document.createElement('span');
    title.textContent = entry.kind === 'query' ? entry.name : `${entry.name || '(unnamed)'}: ${entry.columns.join(', ')}${entry.unique ? ' · unique' : ''}`;
    button.append(title);
    const small = document.createElement('small'); small.className = 'palette-note';
    small.textContent = entry.editable ? kindLabel[entry.kind] : `${kindLabel[entry.kind]} · read-only`; button.append(small);
    if (!entry.editable) { button.setAttribute('aria-disabled', 'true'); button.title = entry.note || ''; }
    li.append(button); return li;
  }
  function fill() {
    const entry = current();
    $('section-annotation-edit').hidden = !entry || !entry.editable;
    $('annotation-note').textContent = entry && !entry.editable ? entry.note : model?.note || '';
    if (!entry || !entry.editable) return;
    $('annotation-title').textContent = `${kindLabel[entry.kind]}${entry.name ? `: ${entry.name}` : ''}`;
    $('ann-name').value = entry.name || ''; $('ann-columns').value = entry.columns.join(', '); $('ann-unique').checked = entry.unique; $('ann-query').value = entry.query || '';
    $('ann-columns-row').hidden = entry.kind === 'query'; $('ann-unique-row').hidden = entry.kind !== 'index'; $('ann-query-row').hidden = entry.kind !== 'query';
    $('annotation-validation').textContent = '';
  }
  function configure(next, source) {
    const signature = JSON.stringify([source?.path, source?.version, next]);
    if (signature === key) return;
    key = signature; model = next || null;
    if (!entries().some(e => e.id === picked)) picked = null;
    $('section-indexes').hidden = !model || (!model.indexes.length && !model.note);
    $('section-queries').hidden = !model || !model.queries.length;
    $('index-list').replaceChildren(...(model?.indexes || []).map(row)); $('query-list').replaceChildren(...(model?.queries || []).map(row));
    $('index-empty').hidden = !model || model.indexes.length > 0;
    fill(); change();
  }
  function draft() {
    const entry = current(); if (!entry) return null;
    const out = {id: entry.id}, name = $('ann-name').value.trim();
    if (name !== (entry.name || '')) out.name = name;
    if (entry.kind !== 'query' && JSON.stringify(columnList($('ann-columns').value)) !== JSON.stringify(entry.columns)) out.columns = columnList($('ann-columns').value);
    if (entry.kind === 'index' && $('ann-unique').checked !== entry.unique) out.unique = $('ann-unique').checked;
    if (entry.kind === 'query' && $('ann-query').value.trim() !== entry.query) out.query = $('ann-query').value.trim();
    return out;
  }
  const changed = () => { const d = draft(); return !!d && Object.keys(d).length > 1; };
  function problem() {
    const entry = current(); if (!entry || !changed()) return '';
    const d = draft(), nameOk = entry.kind === 'query' ? /^[A-Za-z_][A-Za-z0-9_.]{0,127}$/ : /^[A-Za-z_][A-Za-z0-9_]{0,62}$/;
    if ('name' in d && !nameOk.test(d.name)) return entry.kind === 'query' ? 'A query name uses letters, digits, underscores and dots.' : 'A name uses letters, digits and underscores, up to 63 characters.';
    if ('columns' in d && (!d.columns.length || d.columns.length > 8 || !d.columns.every(c => /^[A-Za-z_][A-Za-z0-9_]{0,62}$/.test(c)))) return 'Give 1 to 8 column names separated by commas.';
    if ('query' in d && (!d.query || /["\\\n\r]/.test(d.query))) return 'The query is one line and uses single quotes for string literals.';
    return '';
  }
  $('annotation-lists').addEventListener('click', event => {
    const button = event.target.closest('.annotation-entry'); if (!button || $('annotation-edit-fields').disabled) return;
    if (picked !== button.dataset.id) picked = button.dataset.id;
    for (const b of document.querySelectorAll('.annotation-entry')) b.setAttribute('aria-pressed', String(b.dataset.id === picked));
    fill(); change();
  });
  form.addEventListener('submit', event => event.preventDefault());
  const edited = () => { $('annotation-validation').textContent = problem(); change(); };
  form.addEventListener('input', edited); form.addEventListener('change', edited);
  window.jpaAnnotations = Object.freeze({
    configure, draft,
    onChange(callback) { change = callback; },
    dirty() { return changed(); },
    valid() { return changed() && !problem(); },
    reset() { fill(); change(); },
    freeze(disabled) { $('annotation-edit-fields').disabled = disabled; },
  });
})();
