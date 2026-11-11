// Form-only adapter for the Add attribute section. Rust re-validates every field and renders the source.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const form = $('attribute-form');
  let key = '', dirty = false, change = () => {};
  const text = id => $(id).value;
  const optional = id => text(id).trim() || null;
  const kind = () => text('attr-kind');
  const association = () => ['ManyToOne', 'OneToMany', 'OneToOne', 'ManyToMany'].includes(kind());
  const toMany = () => ['OneToMany', 'ManyToMany'].includes(kind());
  function show(id, visible) {
    $(id).hidden = !visible;
    for (const input of $(id).querySelectorAll('input,select')) input.disabled = !visible;
  }
  function dependencies() {
    const k = kind(), basic = k === 'none' || k === 'ElementCollection';
    $('attr-type-label').textContent = basic ? 'Type' : k === 'Embedded' ? 'Embeddable type' : k === 'transient' ? 'Type' : 'Target entity';
    show('attr-enum-row', k === 'none');
    show('attr-notnull-row', k !== 'transient' && !toMany() && k !== 'Embedded' && k !== 'ElementCollection');
    show('attr-unique-row', k === 'none');
    $('attr-relation-fields').hidden = !association(); $('attr-relation-fields').disabled = !association();
    show('attr-orphan-row', ['OneToOne', 'OneToMany'].includes(k));
    show('attr-order-fields', toMany());
    show('attr-join-fields', k === 'ManyToOne' || k === 'OneToOne');
    show('attr-table-fields', k === 'ManyToMany');
    $('attr-order-value-row').hidden = !text('attr-order'); $('attr-order-value').disabled = !text('attr-order');
    $('attr-join-notnull').disabled = $('attr-join-fields').hidden;
  }
  function draft() {
    const k = kind(), rel = association();
    const order = rel && toMany() ? text('attr-order') : '';
    const joinTable = rel && k === 'ManyToMany' && (optional('attr-table') || optional('attr-table-join') || optional('attr-table-inverse'))
      ? {name: text('attr-table').trim(), joinColumn: text('attr-table-join').trim(), inverseColumn: text('attr-table-inverse').trim()} : null;
    const join = rel && (k === 'ManyToOne' || k === 'OneToOne');
    return {
      name: text('attr-name').trim(), kind: k, type: text('attr-type').trim(),
      enumerated: k === 'none' && $('attr-enum').checked,
      notNull: !$('attr-notnull-row').hidden && $('attr-notnull').checked,
      unique: k === 'none' && $('attr-unique').checked,
      cascade: rel ? [...$('attr-cascade').querySelectorAll('input:checked')].map(i => i.value) : [],
      fetch: rel ? optional('attr-fetch') : null,
      orphanRemoval: rel && !$('attr-orphan-row').hidden && $('attr-orphan').checked,
      orderBy: order === 'by' ? text('attr-order-value').trim() : null,
      orderColumn: order === 'column' ? text('attr-order-value').trim() : null,
      joinColumn: join ? optional('attr-join-column') : null,
      joinColumnNotNull: join && $('attr-join-notnull').checked,
      joinTable,
    };
  }
  function reset() {
    form.reset(); dirty = false; $('attr-validation').textContent = '';
    for (const input of form.querySelectorAll('input,select')) input.removeAttribute('aria-invalid');
    dependencies();
  }
  function configure(source) {
    const next = source ? JSON.stringify([source.path, source.version]) : '';
    if (next === key) return;
    key = next; reset();
  }
  form.addEventListener('submit', event => event.preventDefault());
  function changed() {
    dirty = true; dependencies();
    const invalid = form.querySelector('input:invalid, select:invalid');
    $('attr-validation').textContent = invalid?.validationMessage || '';
    for (const input of form.querySelectorAll('input,select')) input.setAttribute('aria-invalid', String(!input.validity.valid));
    change();
  }
  form.addEventListener('input', changed); form.addEventListener('change', changed);
  dependencies();
  window.jpaAttribute = Object.freeze({
    configure, draft, reset,
    onChange(callback) { change = callback; },
    dirty() { return dirty; },
    valid() { return !!key && form.checkValidity(); },
    freeze(disabled) { $('attribute-configuration').disabled = disabled; if (!disabled) dependencies(); },
  });
})();
