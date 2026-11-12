// JPA Palette: category list, search and item list. Attribute kinds open the Add attribute form; every other
// item names the existing editor code action, because a web view cannot start editor actions.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const ACTION = {
    identifier: 'Add JPA identifier with preview…',
    embeddedId: 'Add embedded identifier with preview…',
    index: 'Add index or unique constraint with preview…',
    namedQuery: 'Add named query with preview…',
    equals: 'Generate proxy-safe equals and hashCode with preview…',
    toString: 'Generate lazy-safe toString with preview…',
  };
  const attribute = (label, kind, enumerated = false) => ({label, attribute: {kind, enumerated}});
  const action = (label, title, where) => ({label, action: title, where});
  const callbacks = ['PrePersist', 'PostPersist', 'PreUpdate', 'PostUpdate', 'PreRemove', 'PostRemove', 'PostLoad']
    .map(event => action(`@${event}`, `Add @${event} callback`, 'on the entity class declaration'));
  const categories = [
    {id: 'attributes', label: 'Attributes', items: [
      attribute('Association', 'ManyToOne'),
      attribute('Basic Type', 'none'),
      attribute('Element Collection', 'ElementCollection'),
      attribute('Embedded', 'Embedded'),
      action('Embedded Id', ACTION.embeddedId, 'on the entity class declaration'),
      attribute('Enum', 'none', true),
      action('Id', ACTION.identifier, 'on the entity class declaration'),
      attribute('Transient', 'transient'),
      {label: 'Version', unavailable: 'No action adds a version field to an existing class. Tick Version field on the Create tab when creating a class.'},
      {label: 'From DB', unavailable: 'Generating attributes from a database table needs a live database connection, which this plugin does not provide yet.'},
    ]},
    {id: 'callbacks', label: 'Lifecycle Callbacks', items: callbacks},
    {id: 'indexes', label: 'Indexes', items: [action('Index or Unique Constraint', ACTION.index, 'on the entity class declaration')]},
    {id: 'queries', label: 'Named Query', items: [action('Named Query', ACTION.namedQuery, 'on the entity class declaration')]},
    {id: 'utilities', label: 'Utilities', items: [
      action('equals() and hashCode()', ACTION.equals, 'on the entity class declaration'),
      action('toString()', ACTION.toString, 'on the entity class declaration'),
    ]},
  ];
  let category = 'attributes', query = '', block = 'Inspect an entity first.', selected = null, choose = () => {};
  const catList = $('palette-categories'), itemList = $('palette-items'), search = $('palette-search');
  const entries = () => {
    const q = query.trim().toLowerCase();
    if (!q) return categories.find(c => c.id === category).items.map(item => ({item, from: null}));
    return categories.flatMap(c => c.items.filter(i => i.label.toLowerCase().includes(q) || c.label.toLowerCase().includes(q)).map(item => ({item, from: c})));
  };
  const itemKey = item => `${item.label}|${item.action || item.attribute?.kind || 'x'}`;
  const note = item => item.unavailable || (item.attribute && block) || '';
  const disabled = item => !!item.unavailable || !!(item.attribute && block);
  function explain(item) {
    if (item.unavailable) return item.unavailable;
    if (item.attribute) return block ? `${block} Attribute items start the Add attribute form.` : 'Fill in the Add attribute form below, then preview the attribute.';
    return `Run the editor action "${item.action}" ${item.where}. The Designer cannot start editor actions from this panel.`;
  }
  function renderCategories() {
    catList.replaceChildren(...categories.map(c => {
      const option = document.createElement('button');
      option.type = 'button'; option.className = 'ui-option'; option.setAttribute('role', 'option'); option.dataset.id = c.id; option.textContent = c.label;
      const active = c.id === category;
      option.setAttribute('aria-selected', String(active)); option.tabIndex = active ? 0 : -1;
      return option;
    }));
    catList.dataset.searching = String(!!query.trim());
  }
  function renderItems(focusKey) {
    const list = entries();
    itemList.replaceChildren(...list.map(({item, from}) => {
      const li = document.createElement('li'), button = document.createElement('button');
      button.type = 'button'; button.className = 'ui-option palette-item'; button.dataset.key = itemKey(item);
      const label = document.createElement('span'); label.textContent = item.label; button.append(label);
      const small = document.createElement('small'); small.className = 'palette-note';
      small.textContent = [from?.label, note(item)].filter(Boolean).join(' · '); if (small.textContent) button.append(small);
      if (disabled(item)) { button.setAttribute('aria-disabled', 'true'); }
      button.setAttribute('aria-pressed', String(selected === itemKey(item)));
      li.append(button); return li;
    }));
    if (!list.length) { const li = document.createElement('li'); li.className = 'ui-menu-empty'; li.textContent = 'No palette items match.'; itemList.append(li); }
    $('palette-count').textContent = query.trim() ? `${list.length} of ${categories.reduce((n, c) => n + c.items.length, 0)} palette items` : '';
    if (focusKey) itemList.querySelector(`[data-key="${CSS.escape(focusKey)}"]`)?.focus();
  }
  function render() { renderCategories(); renderItems(document.activeElement?.closest?.('#palette-items') ? document.activeElement.dataset.key : null); }
  function select(id, focus) {
    category = id; selected = null; $('palette-detail').textContent = ''; render();
    if (focus) catList.querySelector(`[data-id="${id}"]`)?.focus();
  }
  catList.addEventListener('click', event => { const o = event.target.closest('[role=option]'); if (o) select(o.dataset.id, false); });
  catList.addEventListener('keydown', event => {
    const index = categories.findIndex(c => c.id === category);
    const target = {ArrowDown: index + 1, ArrowUp: index - 1, Home: 0, End: categories.length - 1}[event.key];
    if (target === undefined) return;
    event.preventDefault(); select(categories[(target + categories.length) % categories.length].id, true);
  });
  itemList.addEventListener('click', event => {
    const button = event.target.closest('.palette-item'); if (!button) return;
    const item = entries().map(e => e.item).find(i => itemKey(i) === button.dataset.key); if (!item) return;
    selected = itemKey(item); $('palette-detail').textContent = explain(item);
    for (const b of itemList.querySelectorAll('.palette-item')) b.setAttribute('aria-pressed', String(b === button));
    if (item.attribute && !block) choose(item.attribute);
  });
  search.addEventListener('input', () => { query = search.value; selected = null; $('palette-detail').textContent = ''; render(); });
  search.addEventListener('keydown', event => { if (event.key === 'Escape' && search.value) { event.preventDefault(); search.value = ''; query = ''; render(); } });
  render();
  window.jpaPalette = Object.freeze({
    onChoose(callback) { choose = callback; },
    configure(reason) { if (reason === block) return; block = reason; if (selected) $('palette-detail').textContent = ''; render(); },
  });
})();
