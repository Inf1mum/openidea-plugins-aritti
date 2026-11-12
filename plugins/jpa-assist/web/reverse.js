// "From database / SQL" panel (G087, G084, G098): a read-only inventory of one SQL file against the entities of the
// project. Rust parses and compares; the entity generation itself is a code action the page cannot start.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const host = $('reverse-root'), bridge = () => !!window.openidea;
  if (!host) return;
  const el = (tag, text, className) => { const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (className) node.className = className; return node; };
  const groups = [['mapped', 'Mapped Relations'], ['table', 'Tables'], ['view', 'Views']];
  let send = () => {}, sources = null, inventory = null, busy = false;
  const state = {source: '', dialect: '', schema: '', table: '', filter: ''};
  host.innerHTML = '';
  const details = el('details', undefined, 'reverse-details'); details.id = 'rev-details';
  const title = el('summary', 'From database / SQL', 'designer-heading');
  const intro = el('p', 'Compare a SQL file with the entities of the project. Nothing is written and no database is contacted.', 'ui-help');
  const sourceSelect = el('select', undefined, 'ui-select'); sourceSelect.id = 'rev-source';
  const refresh = el('button', undefined, 'ui-button ui-button--icon'); refresh.type = 'button'; refresh.id = 'rev-refresh';
  refresh.setAttribute('aria-label', 'Refresh data source'); refresh.title = 'Refresh data source: read the file and the entities again';
  const icon = el('span'); icon.dataset.uiIcon = 'refresh'; icon.setAttribute('aria-hidden', 'true'); refresh.append(icon, el('span', 'Refresh', 'ui-visually-hidden'));
  const sourceRow = el('div', undefined, 'reverse-source');
  const sourceLabel = el('label', 'Source', 'ui-field'); sourceLabel.append(sourceSelect);
  sourceRow.append(sourceLabel, refresh);
  const dialectSelect = el('select', undefined, 'ui-select'); dialectSelect.id = 'rev-dialect';
  for (const [value, label] of [['', 'Detect from the file'], ['postgresql', 'PostgreSQL'], ['mysql', 'MySQL']]) { const o = el('option', label); o.value = value; dialectSelect.append(o); }
  const schemaSelect = el('select', undefined, 'ui-select'); schemaSelect.id = 'rev-schema';
  const dialectLabel = el('label', 'Database', 'ui-field'), schemaLabel = el('label', 'Schema', 'ui-field');
  dialectLabel.append(dialectSelect); schemaLabel.append(schemaSelect);
  const options = el('div', undefined, 'reverse-options'); options.append(dialectLabel, schemaLabel);
  const note = el('p', '', 'ui-help'); note.id = 'rev-note'; note.setAttribute('role', 'status'); note.setAttribute('aria-live', 'polite');
  const filter = el('input', undefined, 'ui-input'); filter.type = 'search'; filter.id = 'rev-filter'; filter.placeholder = 'Filter tables and views'; filter.setAttribute('aria-label', 'Filter tables and views'); filter.autocomplete = 'off';
  const left = el('div', undefined, 'reverse-list'); left.id = 'rev-relations'; left.setAttribute('aria-label', 'Relations');
  const right = el('div', undefined, 'reverse-detail'); right.id = 'rev-detail'; right.setAttribute('aria-live', 'polite');
  const body = el('div', undefined, 'reverse-body'); body.append(left, right);
  const types = el('details', undefined, 'reverse-types'); types.id = 'rev-types';
  const next = el('div', undefined, 'ui-alert ui-alert--info reverse-next'); next.id = 'rev-next';
  details.append(title, intro, sourceRow, options, note, filter, body, types, next);
  host.append(details);
  details.addEventListener('toggle', () => { if (details.open && bridge()) window.jpaReverse.activate(); });

  const sqlFile = () => sources?.sqlFiles.find(f => f.path === state.source);
  const connection = () => sources?.connections.find(c => `conn:${c.name}` === state.source);
  function ask(table) {
    if (!sqlFile()) return;
    state.table = table ?? state.table;
    send({kind: 'reverse-inventory', path: state.source, dialect: state.dialect || null, schema: state.schema || null, table: state.table || null});
  }
  function renderSources() {
    const sqlGroup = el('optgroup'); sqlGroup.label = 'SQL files';
    for (const f of sources.sqlFiles) { const o = el('option', f.label); o.value = f.path; sqlGroup.append(o); }
    const connGroup = el('optgroup'); connGroup.label = 'Saved connections';
    for (const c of sources.connections) { const o = el('option', `${c.name} (${c.dbms})`); o.value = `conn:${c.name}`; connGroup.append(o); }
    const placeholder = el('option', sources.sqlFiles.length || sources.connections.length ? 'Choose a source' : 'No SQL file or connection found'); placeholder.value = '';
    sourceSelect.replaceChildren(placeholder, ...(sources.sqlFiles.length ? [sqlGroup] : []), ...(sources.connections.length ? [connGroup] : []));
    if (![...sourceSelect.options].some(o => o.value === state.source)) { state.source = ''; state.table = ''; inventory = null; }
    sourceSelect.value = state.source;
  }
  function row(item) {
    const b = el('button', undefined, 'ui-option reverse-relation'); b.type = 'button'; b.dataset.name = item.name;
    b.setAttribute('aria-pressed', String(item.name === state.table));
    b.append(el('span', item.name));
    if (item.group === 'mapped' && item.unmapped) b.append(el('small', `${item.unmapped} unmapped column${item.unmapped === 1 ? '' : 's'}`, 'palette-note reverse-unmapped'));
    b.addEventListener('click', () => ask(item.name));
    return b;
  }
  function renderLeft() {
    const q = state.filter.trim().toLowerCase();
    const items = (inventory?.relations || []).filter(r => !q || r.name.toLowerCase().includes(q));
    const nodes = [];
    for (const [key, label] of groups) {
      const part = items.filter(r => r.group === key);
      if (!part.length) continue;
      const section = el('section', undefined, 'reverse-group'); section.dataset.group = key;
      const list = el('div', undefined, 'reverse-items'); list.append(...part.map(row));
      section.append(el('h3', `${label} (${part.length})`, 'ui-group-header'), list);
      nodes.push(section);
    }
    if (!nodes.length && inventory) nodes.push(el('p', q ? 'No tables or views match.' : 'The file defines no table or view.', 'ui-help'));
    left.replaceChildren(...nodes);
  }
  function columnRow(c) {
    const li = el('li', undefined, 'reverse-column');
    li.append(el('strong', c.name + (c.key ? ' (key)' : '')), el('span', `${c.sql} → ${c.java}${c.nullable === false ? ', not null' : ''}`, 'ui-help'));
    return li;
  }
  function referenceRow(r) {
    const li = el('li', undefined, 'reverse-column');
    const target = `${r.refTable}${r.refColumns ? ` (${r.refColumns.join(', ')})` : ''}`;
    li.append(el('strong', `${r.columns.join(', ')} → ${target}`), el('span', [r.refMapped ? 'target is mapped' : 'target is not mapped', r.columnPresent ? '' : 'no matching column in this table'].filter(Boolean).join(', '), 'ui-help'));
    return li;
  }
  function section(label, className, items, render, empty) {
    const s = el('section', undefined, 'reverse-group'); s.dataset.group = className;
    const list = el('ul', undefined, 'migration-list'); list.append(...items.map(render));
    s.append(el('h3', `${label} (${items.length})`, 'ui-group-header'), items.length ? list : el('p', empty, 'ui-help'));
    return s;
  }
  function renderRight() {
    const d = inventory?.selected;
    if (!d) { right.replaceChildren(...(inventory?.relations?.length ? [el('p', 'Select a table or view to list its columns and references.', 'ui-help')] : [])); return; }
    const mapped = d.columns.filter(c => c.mapped), rest = d.columns.filter(c => !c.mapped);
    right.replaceChildren(el('h3', d.name + (d.view ? ' (view)' : ''), 'reverse-title'),
      section('Mapped Columns', 'mapped-columns', mapped, columnRow, d.mappedEntity ? 'No column is mapped yet.' : 'No entity maps this relation.'),
      section('Columns', 'columns', rest, columnRow, 'Every column is mapped.'),
      section('References', 'references', d.references, referenceRow, 'No foreign key.'));
    if (d.columnsTruncated) right.append(el('p', 'The column list is truncated.', 'ui-help'));
  }
  function renderOptions() {
    const inv = inventory;
    schemaSelect.replaceChildren(...[{value: '', label: 'All schemas'}, ...(inv?.schemas || [])].map(s => { const o = el('option', s.label); o.value = s.value; return o; }));
    schemaSelect.value = inv?.schema ?? '';
    schemaSelect.disabled = !inv || inv.schemas.length < 2;
    dialectSelect.disabled = !sqlFile();
    note.textContent = !inv ? '' : [inv.dialectDetected ? `Detected ${inv.dialect === 'mysql' ? 'MySQL' : 'PostgreSQL'}.` : inv.dialect ? `Reading as ${inv.dialect === 'mysql' ? 'MySQL' : 'PostgreSQL'}.` : '',
      inv.skipped ? `${inv.skipped} statement${inv.skipped === 1 ? '' : 's'} not understood.` : '', inv.modelScanned ? '' : 'The entity sources could not be scanned, so nothing is marked mapped.', inv.truncated ? 'The list is truncated.' : ''].filter(Boolean).join(' ');
    const label = inv?.dialect === 'mysql' ? 'MySQL' : 'PostgreSQL';
    types.replaceChildren(el('summary', inv ? `Default type mappings (${label})` : 'Default type mappings'));
    types.hidden = !inv;
    if (inv) {
      const list = el('ul', undefined, 'migration-list');
      list.append(...(inv.typeMappings.length ? inv.typeMappings : []).map(m => el('li', `${m.sql} → ${m.java} (${m.count})`)));
      types.append(el('p', 'The Java type each column type of this file becomes. Types without a default are set in the sqlTypeMappings setting, per database.', 'ui-help'), list);
    }
  }
  function renderNext() {
    const c = connection();
    next.replaceChildren();
    if (!state.source) { next.hidden = true; return; }
    next.hidden = false;
    const actions = inventory || sqlFile();
    const generate = inventory?.generateAction || sources?.generateAction, attrs = inventory?.attributesAction || sources?.attributesAction;
    next.append(el('p', c
      ? `${c.name} (${c.dbms}, ${c.target}) is read only by the editor actions, which ask you to confirm the connection and its password. This panel does not connect.`
      : 'The page cannot start an editor action. Open this SQL file in the editor and run:'));
    const list = el('ul', undefined, 'migration-list');
    list.append(el('li', `"${generate}" on the SQL file: choose the tables to map.`), el('li', `"${attrs}" on an entity: add the columns it does not map yet.`));
    const copy = el('button', 'Copy action name', 'ui-button'); copy.type = 'button';
    copy.addEventListener('click', async () => { try { await navigator.clipboard.writeText(generate); copy.textContent = 'Copied'; } catch { copy.textContent = 'Copy is unavailable'; } });
    next.append(list); if (actions) next.append(copy);
  }
  function render() { renderOptions(); renderLeft(); renderRight(); renderNext(); freeze(busy); }
  function freeze(flag) {
    busy = flag;
    for (const node of [sourceSelect, dialectSelect, schemaSelect, refresh, filter]) node.disabled = flag || (node === schemaSelect && (!inventory || inventory.schemas.length < 2)) || (node === dialectSelect && !sqlFile());
    for (const node of left.querySelectorAll('button')) node.disabled = flag;
  }
  sourceSelect.addEventListener('change', () => { state.source = sourceSelect.value; state.table = ''; state.schema = ''; inventory = null; render(); ask(); });
  dialectSelect.addEventListener('change', () => { state.dialect = dialectSelect.value; state.table = ''; ask(); });
  schemaSelect.addEventListener('change', () => { state.schema = schemaSelect.value; state.table = ''; ask(); });
  filter.addEventListener('input', () => { state.filter = filter.value; renderLeft(); });
  refresh.addEventListener('click', () => send({kind: 'reverse-sources'}));
  window.jpaReverse = {
    onRequest(callback) { send = callback; },
    activate() { if (!sources) send({kind: 'reverse-sources'}); },
    clear() { sources = null; inventory = null; state.source = ''; state.table = ''; state.schema = ''; sourceSelect.replaceChildren(); render(); },
    freeze,
    show(reverse) {
      if (reverse?.kind === 'reverse-sources') { sources = reverse; renderSources(); render(); if (sqlFile()) ask(); return; }
      if (reverse?.kind === 'reverse') { inventory = reverse; state.schema = reverse.schema ?? ''; state.table = reverse.selected?.name ?? state.table; render(); }
    },
  };
  render();
})();
