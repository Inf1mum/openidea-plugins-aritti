// Form-only adapter for existing relationship mappings. Rust parses, validates and edits the source.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const form = $('relationship-form');
  let initial = new Map(), key = '', last = null, change = () => {};
  const find = k => initial.get(k);
  const cascadeInputs = () => [...$('rel-cascade').querySelectorAll('input')];
  function show(rowId, k) {
    const p = find(k), row = $(rowId);
    row.hidden = !p;
    for (const input of row.querySelectorAll('input,select')) input.disabled = !p || !p.editable;
  }
  function configure(properties, source, attribute) {
    const next = JSON.stringify([source?.path, source?.version, attribute, properties]);
    if (next === key) return;
    key = next; last = [properties, source, attribute]; initial = new Map((properties || []).map(p => [p.key, p]));
    form.hidden = !properties; $('rel-note').textContent = ''; $('rel-validation').textContent = '';
    if (!properties) return;
    show('rel-fetch-row', 'fetch'); show('rel-orphan-row', 'orphanRemoval'); show('rel-optional-row', 'optional');
    show('rel-join-column-row', 'joinColumn'); show('rel-join-table-row', 'joinTable'); show('rel-order-by-row', 'orderBy'); show('rel-order-column-row', 'orderColumn');
    const join = find('joinColumn')?.value, table = find('joinTable')?.value;
    $('rel-join-column').value = join?.name || ''; $('rel-join-column-notnull').checked = join?.nullable === false;
    $('rel-join-table').value = table?.name || ''; $('rel-join-table-column').value = table?.joinColumn || ''; $('rel-join-table-inverse').value = table?.inverseColumn || '';
    $('rel-order-by').value = find('orderBy')?.value || ''; $('rel-order-column').value = find('orderColumn')?.value || '';
    const fetch = find('fetch'), orphan = find('orphanRemoval'), optional = find('optional'), cascade = find('cascade');
    if (fetch?.value) $('rel-fetch').value = fetch.value;
    if (orphan) $('rel-orphan').checked = orphan.value === true;
    if (optional) $('rel-optional').checked = optional.value === true;
    for (const input of cascadeInputs()) { input.checked = !!cascade?.value?.includes(input.value); input.disabled = !cascade?.editable; }
    const locked = properties.filter(p => !p.editable).map(p => p.label);
    $('rel-note').textContent = locked.length ? `Read-only (expression not understood): ${locked.join(', ')}.` : '';
    refresh();
  }
  function changes() {
    const out = [];
    const fetch = find('fetch');
    if (fetch?.editable && $('rel-fetch').value !== fetch.value) out.push({property: 'fetch', value: $('rel-fetch').value});
    const orphan = find('orphanRemoval');
    if (orphan?.editable && $('rel-orphan').checked !== orphan.value) out.push({property: 'orphanRemoval', value: $('rel-orphan').checked});
    const optional = find('optional');
    if (optional?.editable && $('rel-optional').checked !== optional.value) out.push({property: 'optional', value: $('rel-optional').checked});
    const cascade = find('cascade');
    if (cascade?.editable) {
      const chosen = cascadeInputs().filter(i => i.checked).map(i => i.value);
      const before = cascade.value || [];
      if (chosen.length !== before.length || chosen.some(v => !before.includes(v))) out.push({property: 'cascade', value: chosen});
    }
    const join = find('joinColumn');
    if (join?.editable) {
      const name = $('rel-join-column').value.trim(), nullable = !$('rel-join-column-notnull').checked;
      if (name !== join.value.name || nullable !== join.value.nullable) out.push({property: 'joinColumn', name, nullable});
    }
    const table = find('joinTable');
    if (table?.editable) {
      const name = $('rel-join-table').value.trim(), joinColumn = $('rel-join-table-column').value.trim(), inverseColumn = $('rel-join-table-inverse').value.trim();
      if (name !== table.value.name || joinColumn !== table.value.joinColumn || inverseColumn !== table.value.inverseColumn) out.push({property: 'joinTable', name, joinColumn, inverseColumn});
    }
    for (const [k, id] of [['orderBy', 'rel-order-by'], ['orderColumn', 'rel-order-column']]) {
      const p = find(k), value = $(id).value.trim();
      if (p?.editable && value !== p.value) out.push({property: k, value});
    }
    return out;
  }
  const column = v => /^[A-Za-z_][A-Za-z0-9_]{0,62}$/.test(v);
  const orderItems = v => { const items = v.split(',').map(i => i.trim()); return items.length <= 4 && items.every(i => /^[A-Za-z_][A-Za-z0-9_]{0,62}(\s+(ASC|DESC))?$/i.test(i)); };
  function problem() {
    if (!allExclusive()) return 'Cascade All already includes every other type.';
    for (const c of changes()) {
      if (c.property === 'joinColumn' && !column(c.name)) return 'Join column needs a name of letters, digits and underscores.';
      if (c.property === 'joinTable' && !(column(c.name) && column(c.joinColumn) && column(c.inverseColumn))) return 'Join table needs a table and two column names of letters, digits and underscores.';
      if (c.property === 'joinTable' && c.joinColumn === c.inverseColumn) return 'The join and inverse join columns must differ.';
      if (c.property === 'orderBy' && !(c.value === '' ? false : orderItems(c.value))) return find('orderBy').value ? 'Removing the ordering is not supported here.' : 'Order by takes up to four property names, each optionally followed by ASC or DESC.';
      if (c.property === 'orderColumn' && !column(c.value)) return find('orderColumn').value ? 'Removing the ordering is not supported here.' : 'Order column needs a name of letters, digits and underscores.';
    }
    if ($('rel-order-by').value.trim() && $('rel-order-column').value.trim()) return 'Choose either Order by or Order column, not both.';
    return '';
  }
  const allExclusive = () => { const chosen = cascadeInputs().filter(i => i.checked).map(i => i.value); return !(chosen.includes('ALL') && chosen.length > 1); };
  function refresh() {
    $('rel-validation').textContent = problem();
  }
  form.addEventListener('submit', event => event.preventDefault());
  function changed() { refresh(); change(); }
  form.addEventListener('input', changed); form.addEventListener('change', changed);
  window.jpaRelationship = Object.freeze({
    configure, changes,
    onChange(callback) { change = callback; },
    dirty() { return !form.hidden && changes().length > 0; },
    valid() { return !form.hidden && changes().length > 0 && !problem(); },
    reset() { const args = last; key = ''; if (args) configure(...args); },
    freeze(disabled) { $('relationship-fields').disabled = disabled; },
  });
})();
