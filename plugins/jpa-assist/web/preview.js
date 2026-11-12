// Changelog preview controls (G111, G123): grouping, per-change-type routing what-if, danger colouring. Rust owns the
// routing rules (change_routing.rs); this file only asks for a plan with other routes and shows the result.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const root = $('mig-routing');
  if (!root) return;
  const locations = {primary: 'Primary changelog', secondary: 'Secondary changelog', ignored: 'Ignored'};
  let plan = null, request = () => {}, groupBy = 'table';
  const el = (tag, text, className) => { const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (className) node.className = className; return node; };
  const button = (label, onClick, className = 'ui-button ui-button--quiet') => { const b = el('button', label, className); b.type = 'button'; b.addEventListener('click', onClick); return b; };
  const withRoute = (type, location) => [...(plan?.routes || []).filter(r => r.changeType !== type), ...(location === 'primary' ? [] : [{changeType: type, location}])];
  const setRoute = (type, location) => request(withRoute(type, location));
  const sameRoutes = (a, b) => JSON.stringify([...a].sort((x, y) => x.changeType.localeCompare(y.changeType))) === JSON.stringify([...b].sort((x, y) => x.changeType.localeCompare(y.changeType)));
  function decorate(row, item) {
    if (item.level) row.classList.add(`migration-row--${item.level.toLowerCase()}`);
    if (!item.type) return;
    const head = row.querySelector('.migration-row-head');
    if (item.location && item.location !== 'primary') head.append(el('span', locations[item.location], 'ui-badge ui-badge--info migration-location'));
    const menu = el('details', undefined, 'migration-remove'), summary = el('summary', 'Remove from Changelog');
    const list = el('div', undefined, 'migration-remove-list');
    if (item.location === 'ignored') list.append(button('Restore from Ignored', () => setRoute(item.type, 'primary')));
    else list.append(button('Remove and Ignore', () => setRoute(item.type, 'ignored')));
    if (item.location === 'secondary') list.append(button('Move to primary changelog', () => setRoute(item.type, 'primary')));
    else if (item.location === 'primary') list.append(button('Move to secondary changelog', () => setRoute(item.type, 'secondary')));
    list.append(el('p', `Applies to every ${item.type} change: the setting routes a change type, not a single change.`, 'ui-help'));
    menu.append(summary, list); row.append(menu);
  }
  function summaryBar() {
    const counts = {primary: 0, secondary: 0, ignored: 0};
    for (const c of plan.changes) counts[c.location || 'primary']++;
    const bar = el('div', undefined, 'preview-bar');
    const group = el('label', 'Group by', 'ui-field preview-group'), select = el('select', undefined, 'ui-select'); select.id = 'mig-group-by';
    for (const [value, label] of [['table', 'Table'], ['type', 'Change type'], ['location', 'Changelog']]) { const o = el('option', label); o.value = value; select.append(o); }
    select.value = groupBy; select.addEventListener('change', () => { groupBy = select.value; window.jpaMigration?.rerender(); });
    group.append(select);
    bar.append(group, el('p', `${counts.primary} primary, ${counts.secondary} secondary, ${counts.ignored} ignored`, 'ui-help preview-counts'));
    return bar;
  }
  function typeTable() {
    const details = el('details', undefined, 'preview-types'); details.id = 'mig-types';
    details.append(el('summary', 'Statement types'));
    details.append(el('p', 'Where each change type goes in the Liquibase changelog (what-if: nothing is saved). The danger level and the changeSet attributes are edited in Settings.', 'ui-help'));
    const rows = el('ul', undefined, 'migration-list');
    for (const t of plan.statementTypes || []) {
      const row = el('li', undefined, 'preview-type'), name = el('strong', t.type), select = el('select', undefined, 'ui-select');
      select.setAttribute('aria-label', `Changelog for ${t.type}`); select.dataset.type = t.type;
      for (const [value, label] of Object.entries(locations)) { const o = el('option', label); o.value = value; select.append(o); }
      select.value = t.location; select.addEventListener('change', () => setRoute(t.type, select.value));
      row.append(name, el('span', `${t.count} in this plan`, 'ui-help'), el('span', t.levelOverride ? `level ${t.levelOverride}` : 'default level', 'ui-help'), select);
      rows.append(row);
    }
    details.append(rows);
    const options = el('div', undefined, 'preview-create-table');
    options.append(el('h3', 'CREATE TABLE settings', 'ui-group-header'));
    const dl = el('ul', undefined, 'migration-list');
    for (const o of plan.createTableOptions || []) dl.append(el('li', `${o.label}: ${o.value} (${o.setting})`));
    options.append(dl, el('p', 'Other statement types have no settings of their own; their Liquibase attributes follow liquibaseChangeSetDefaults.', 'ui-help'));
    details.append(options);
    return details;
  }
  function keepNote() {
    if (!plan.previewingRoutes || sameRoutes(plan.routes, plan.settingRoutes)) return null;
    const note = el('div', undefined, 'ui-alert ui-alert--info preview-keep');
    note.append(el('p', 'Previewing other routes. Nothing is saved: copy this into the schemaChangeLocations setting to keep it.'));
    const json = el('pre', JSON.stringify(plan.routes, null, 1), 'ui-code'); json.id = 'mig-routes-json';
    const copy = button('Copy routes', async () => { try { await navigator.clipboard.writeText(json.textContent); copy.textContent = 'Copied'; } catch { copy.textContent = 'Copy is unavailable'; } });
    note.append(json, el('div', undefined, 'designer-actions'));
    note.lastChild.append(copy, button('Back to the setting', () => request(null)));
    return note;
  }
  function render() {
    if (!plan || plan.state !== 'ready' || !plan.statementTypes) { root.replaceChildren(); return; }
    root.replaceChildren(...[summaryBar(), keepNote(), typeTable()].filter(Boolean));
  }
  const groupOf = c => groupBy === 'type' ? c.type || c.kind : groupBy === 'location' ? locations[c.location || 'primary'] : c.table;
  window.jpaPreview = {
    plan(next) { plan = next; render(); },
    clear() { plan = null; groupBy = 'table'; render(); },
    group: c => groupOf(c),
    decorate,
    onRoutes(callback) { request = routes => callback(routes); },
    busy(flag) { for (const node of root.querySelectorAll('select, button')) node.disabled = flag; for (const node of document.querySelectorAll('.migration-remove button')) node.disabled = flag; },
  };
})();
