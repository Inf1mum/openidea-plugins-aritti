// Migration page: renders the read-only ChangePlan and collects options. Rust plans, emits and bounds everything.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const labels = {safe: 'Safe', rewrite: 'Rewrite', 'needs-backfill': 'NeedsBackfill', 'data-loss': 'DataLoss', ambiguous: 'Ambiguous'};
  const tones = {safe: 'success', rewrite: 'warning', 'needs-backfill': 'warning', 'data-loss': 'danger', ambiguous: 'info'};
  const levelTones = {SAFE: 'success', CAUTION: 'warning', DANGER: 'danger'};
  const renameKey = c => JSON.stringify([c.table, c.from, c.to]);
  let backfillSeq = 0;
  const backfillKey = c => JSON.stringify([c.backfill.table, c.backfill.column]);
  const ranges = {tinyint: [-128n, 127n], smallint: [-32768n, 32767n], integer: [-2147483648n, 2147483647n], bigint: [-(2n ** 63n), 2n ** 63n - 1n]};
  const textTypes = new Set(['char', 'varchar', 'text']), decimalTypes = new Set(['real', 'double', 'decimal']);
  let plan = null, renames = new Set(), backfills = new Map(), change = () => {}, sqlStale = false;
  // Same rules as persistence-core BackfillValue::parse and ::check; Rust validates again on the server.
  function checkBackfill(input, spec) {
    if (!input.trim()) return '';
    const type = spec.logical;
    if (ranges[type]) {
      if (!/^[+-]?\d+$/.test(input.trim())) return 'Enter a whole number.';
      const [low, high] = ranges[type], n = BigInt(input.trim());
      return n < low || n > high ? `Outside the ${type} range.` : '';
    }
    if (decimalTypes.has(type)) {
      const m = /^-?(\d+)(?:\.(\d+))?$/.exec(input.trim());
      if (!m || m[1].length + (m[2] || '').length > 65) return 'Enter a plain decimal number (digits, optional minus, optional fraction).';
      if (type === 'decimal' && spec.precision != null) {
        const scale = spec.scale || 0;
        if ((m[2] || '').length > scale || m[1].replace(/^0+/, '').length > Math.max(spec.precision - scale, 0)) return `Does not fit decimal(${spec.precision},${scale}).`;
      }
      return '';
    }
    if (type === 'boolean') return ['true', 'false'].includes(input.trim().toLowerCase()) ? '' : 'Enter true or false.';
    if (textTypes.has(type)) {
      if (/[\p{Cc}\\\uFFFE\uFFFF]/u.test(input)) return 'Control characters and backslashes are not accepted.';
      return type !== 'text' && spec.length != null && [...input].length > spec.length ? `Longer than ${spec.length} characters.` : '';
    }
    return 'A backfill value is not supported for this column type.';
  }
  const includesBackfill = () => !!document.querySelector('#mig-include input[value="needs-backfill"]')?.checked;
  const backfillError = () => (plan?.changes || []).some(c => c.backfill && checkBackfill(backfills.get(backfillKey(c)) || '', c.backfill));
  function syncBackfill() { for (const node of document.querySelectorAll('.migration-backfill')) node.hidden = !includesBackfill(); }
  function el(tag, text, className) { const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (className) node.className = className; return node; }
  function clearSql() { $('mig-sql-wrap').hidden = true; $('mig-sql').textContent = ''; sqlStale = false; }
  function clear() { plan = null; renames = new Set(); backfills = new Map(); window.jpaPreview?.clear(); render(); clearSql(); }
  function renderRow(item) {
    const row = el('li', undefined, 'migration-row');
    const head = el('div', undefined, 'migration-row-head');
    head.append(el('span', item.level || labels[item.danger] || item.danger, `ui-badge ui-badge--${levelTones[item.level] || tones[item.danger] || 'info'}`), el('strong', `${item.kind} ${item.detail}`.trim()));
    row.append(head, el('p', item.reason, 'ui-help'));
    if (item.kind === 'rename-candidate') {
      const label = el('label', undefined, 'ui-check'), box = document.createElement('input');
      box.type = 'checkbox'; box.checked = renames.has(renameKey(item)); box.dataset.rename = renameKey(item);
      box.addEventListener('change', () => { if (box.checked) renames.add(box.dataset.rename); else renames.delete(box.dataset.rename); invalidateSql(); change(); });
      label.append(box, document.createTextNode(`Treat as rename (${item.from} to ${item.to})`));
      row.append(label);
    }
    if (item.backfill) {
      const wrap = el('div', undefined, 'migration-backfill'), label = el('label', `Backfill value for ${item.backfill.column} (${item.backfill.logical}); empty keeps the statement withheld`, 'ui-field');
      const input = document.createElement('input'), error = el('p', '', 'ui-help migration-backfill-error');
      input.className = 'ui-input'; input.autocomplete = 'off'; input.dataset.backfill = backfillKey(item); input.value = backfills.get(backfillKey(item)) || '';
      error.setAttribute('role', 'status'); error.setAttribute('aria-live', 'polite');
      error.id = `mig-bf-${backfillSeq++}`; input.setAttribute('aria-describedby', error.id);
      const sync = () => { const why = checkBackfill(input.value, item.backfill); error.textContent = why; input.setAttribute('aria-invalid', why ? 'true' : 'false'); };
      input.addEventListener('input', () => { if (input.value.trim()) backfills.set(input.dataset.backfill, input.value); else backfills.delete(input.dataset.backfill); sync(); invalidateSql(); change(); });
      label.append(input); wrap.append(label, error); wrap.hidden = !includesBackfill(); row.append(wrap); sync();
    }
    window.jpaPreview?.decorate(row, item);
    return row;
  }
  function render() {
    const state = plan?.state;
    $('mig-empty').hidden = !!plan; $('mig-nobaseline').hidden = state !== 'no-baseline'; $('mig-result').hidden = state !== 'ready';
    if (state !== 'ready') return;
    const counts = {}; for (const c of plan.changes) counts[c.danger] = (counts[c.danger] || 0) + 1;
    $('mig-summary').textContent = plan.changes.length || plan.notes.length
      ? `${plan.changes.length} changes across ${new Set(plan.changes.map(c => c.table)).size} tables, ${plan.notes.length} notes.${plan.truncated ? ' The list is truncated.' : ''}`
      : 'The model matches the baseline; there is nothing to migrate.';
    const warnings = [...plan.incomplete]; if (plan.pendingBaseline) warnings.push('A refreshed baseline (db/jpa-schema-baseline.next.json) is waiting to replace the baseline; the file action refuses until it is moved.');
    $('mig-incomplete').replaceChildren(...warnings.map(w => el('li', w))); $('mig-incomplete').hidden = !warnings.length;
    const groupOf = window.jpaPreview?.group || (c => c.table), groups = new Map(); for (const c of plan.changes) { const g = groupOf(c); if (!groups.has(g)) groups.set(g, []); groups.get(g).push(c); }
    $('mig-groups').replaceChildren(...[...groups].map(([table, items]) => {
      const section = el('section', undefined, 'migration-group');
      const list = el('ul', undefined, 'migration-list'); list.append(...items.map(renderRow));
      section.append(el('h3', table, 'ui-group-header'), list);
      return section;
    }));
    $('mig-notes').hidden = !plan.notes.length;
    $('mig-notes-list').replaceChildren(...plan.notes.map(n => el('li', `${n.table} ${n.subject}: ${n.reason}`)));
    for (const key of ['needs-backfill', 'data-loss']) { const box = document.querySelector(`#mig-include input[value="${key}"]`); box.closest("label").lastChild.textContent = `${labels[key]} (${counts[key] || 0})`; }
    $('mig-rename-note').hidden = !plan.changes.some(c => c.kind === 'rename-candidate');
    syncBackfill();
  }
  function invalidateSql() { if (!$('mig-sql-wrap').hidden || $('mig-sql').textContent) { sqlStale = true; $('mig-sql-state').textContent = 'Options changed. Preview again to refresh the SQL.'; $('mig-sql').classList.add('migration-stale'); $('mig-copy').disabled = true; } }
  function showPlan(next) {
    plan = next; window.jpaPreview?.plan(next);
    const known = new Set((plan.changes || []).filter(c => c.kind === 'rename-candidate').map(renameKey));
    renames = new Set([...renames].filter(k => known.has(k)));
    const targets = new Set((plan.changes || []).filter(c => c.backfill).map(backfillKey));
    backfills = new Map([...backfills].filter(([k]) => targets.has(k)));
    render();
    if ($('mig-sql').textContent) invalidateSql();
  }
  function showSql(result) {
    sqlStale = false; $('mig-sql-wrap').hidden = false; $('mig-sql').classList.remove('migration-stale'); $('mig-copy').disabled = false;
    $('mig-sql').textContent = result.sql;
    $('mig-sql-state').textContent = `${result.emitted} statements shown, ${result.policySkipped} withheld by policy, ${result.unsupported} unsupported. Read-only: nothing was created.`;
    $('mig-pointer').textContent = `The Designer cannot create files. To write V<next>${result.fileSuffix}, open an entity and run "${result.fileAction}" from the editor actions.`;
  }
  function draft() {
    const include = [...document.querySelectorAll('#mig-include input:checked')].map(i => i.value);
    const decisions = [...renames].map(k => { const [table, from, to] = JSON.parse(k); return {table, from, to}; });
    const values = includesBackfill() ? (plan?.changes || []).filter(c => c.backfill && (backfills.get(backfillKey(c)) || '').trim()).map(c => ({table: c.backfill.table, column: c.backfill.column, value: backfills.get(backfillKey(c))})) : [];
    return {dialect: $('mig-dialect').value, includeDangers: include, renameDecisions: decisions, backfills: values, description: $('mig-description').value};
  }
  const valid = () => !!plan && plan.state === 'ready' && plan.changes.length + plan.notes.length > 0 && /[A-Za-z0-9]/.test($('mig-description').value) && $('mig-description').value.length <= 80 && !(includesBackfill() && backfillError());
  function freeze(busy) { for (const id of ['mig-plan', 'mig-dialect', 'mig-description']) $(id).disabled = busy; for (const box of document.querySelectorAll('#migration-panel input[type=checkbox], .migration-backfill input')) box.disabled = busy; $('mig-generate').disabled = busy || !valid(); window.jpaPreview?.busy(busy); if (sqlStale || !$('mig-sql').textContent) $('mig-copy').disabled = true; else $('mig-copy').disabled = busy; }
  $('mig-copy').addEventListener('click', async () => {
    try { await navigator.clipboard.writeText($('mig-sql').textContent); $('mig-sql-state').textContent = 'SQL copied.'; }
    catch { $('mig-sql-state').textContent = 'Copy is unavailable here. Select the SQL text and copy it manually.'; }
  });
  for (const id of ['mig-dialect', 'mig-description']) $(id).addEventListener('input', () => { invalidateSql(); change(); });
  for (const box of document.querySelectorAll('#mig-include input')) box.addEventListener('change', () => { syncBackfill(); invalidateSql(); change(); });
  window.jpaMigration = {rerender: render, showPlan, showSql, clear, draft, valid, freeze, onChange: callback => { change = callback; }, failed: clearSql};
})();
