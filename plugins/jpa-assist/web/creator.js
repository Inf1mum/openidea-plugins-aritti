// Form-only adapter. Rust owns source identity, semantic validation and edits.
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const form = $('create-form');
  let context = null, key = '', dirty = false, change = () => {};
  const text = id => $(id).value;
  const number = id => Number(text(id));
  const inherit = () => text('create-role') === 'entity' ? text('create-inherit') : 'none';
  const hasIdentifier = () => inherit() !== 'subclass' && (text('create-role') === 'entity' || (text('create-role') === 'mapped-superclass' && $('create-declare-id').checked));
  function generationOptions() {
    const previous = text('create-id-strategy');
    const type = text('create-id-type');
    const options = [['assigned', 'Assigned']];
    if (['long', 'integer'].includes(type)) options.push(['identity', 'Identity'], ['sequence', 'Sequence'], ['table', 'Table'], ['auto', 'Auto']);
    else if (text('create-namespace') === 'jakarta' && ['uuid', 'string'].includes(type)) options.push(['uuid', 'UUID'], ['auto', 'Auto']);
    $('create-id-strategy').replaceChildren(...options.map(([value, label]) => { const option = document.createElement('option'); option.value = value; option.textContent = label; return option; }));
    $('create-id-strategy').value = options.some(([value]) => value === previous) ? previous : 'assigned';
  }
  function dependencies() {
    const role = text('create-role'), id = hasIdentifier(), strategy = text('create-id-strategy');
    $('create-table-row').hidden = role !== 'entity'; $('create-table').disabled = role !== 'entity';
    $('create-declare-id-row').hidden = role !== 'mapped-superclass';
    $('create-id-fields').hidden = !id; $('create-id-fields').disabled = !id;
    $('create-sequence-fields').hidden = strategy !== 'sequence'; $('create-sequence-fields').disabled = strategy !== 'sequence';
    $('create-generator-table-fields').hidden = strategy !== 'table'; $('create-generator-table-fields').disabled = strategy !== 'table';
    const sub = inherit() === 'subclass', root = inherit() === 'root', tpc = text('create-inherit-strategy') === 'table-per-class';
    $('create-version-row').hidden = role === 'embeddable' || sub; $('create-version').disabled = role === 'embeddable' || sub;
    $('create-inherit-row').hidden = role !== 'entity'; $('create-inherit').disabled = role !== 'entity';
    $('create-table').required = !sub;
    $('create-inherit-root-fields').hidden = !root; $('create-inherit-root-fields').disabled = !root;
    $('create-inherit-sub-fields').hidden = !sub; $('create-inherit-sub-fields').disabled = !sub;
    $('create-discriminator-fields').hidden = tpc; for (const input of $('create-discriminator-fields').querySelectorAll('input,select')) input.disabled = tpc;
    const text_ = text('create-discriminator-type') !== 'string';
    $('create-discriminator-length-row').hidden = tpc || text_; $('create-discriminator-length').disabled = tpc || text_;
  }
  function configure(next, force = false) {
    if (!next) { context = null; key = ''; dirty = false; form.hidden = true; $('create-target').textContent = ''; return; }
    const nextKey = JSON.stringify([next.source.root, next.source.path, next.source.version]);
    if (key === nextKey && !force) return;
    context = next; key = nextKey; dirty = false; form.reset(); form.hidden = false;
    for (const input of form.querySelectorAll('input,select')) input.removeAttribute('aria-invalid');
    $('create-name').value = next.name; $('create-package').value = next.package;
    $('create-target').textContent = `${next.source.path} · ${next.source.language}`;
    $('create-table').value = next.defaultTableName;
    $('create-sequence-generator').value = `${next.defaultTableName}_ids`;
    $('create-sequence-name').value = `${next.defaultTableName}_seq`;
    $('create-table-generator').value = `${next.defaultTableName}_ids`;
    $('create-table-key-value').value = next.defaultTableName;
    $('create-validation').textContent = '';
    generationOptions(); dependencies();
  }
  function draft() {
    const role = text('create-role');
    const strategy = text('create-id-strategy');
    const identifier = !hasIdentifier() ? null : {
      name: text('create-id-name'), column: text('create-id-column'), idType: text('create-id-type'), strategy,
      sequence: strategy !== 'sequence' ? null : {generator: text('create-sequence-generator'), sequenceName: text('create-sequence-name'), initialValue: number('create-sequence-initial'), allocationSize: number('create-sequence-allocation')},
      table: strategy !== 'table' ? null : {generator: text('create-table-generator'), tableName: text('create-generator-table'), pkColumnName: text('create-table-key-column'), valueColumnName: text('create-table-value-column'), pkColumnValue: text('create-table-key-value'), initialValue: number('create-table-initial'), allocationSize: number('create-table-allocation')},
    };
    const optional = id => text(id).trim() || null;
    const kind = inherit();
    const tpc = text('create-inherit-strategy') === 'table-per-class';
    const inheritance = kind === 'root' ? {
      kind: 'root', strategy: text('create-inherit-strategy'),
      discriminator: tpc ? null : {name: text('create-discriminator-name'), discriminatorType: text('create-discriminator-type'), length: text('create-discriminator-type') === 'string' ? number('create-discriminator-length') : 0},
      discriminatorValue: tpc ? null : optional('create-root-value'),
    } : kind === 'subclass' ? {
      kind: 'subclass', superclass: text('create-superclass').trim(), discriminatorValue: optional('create-sub-value'), primaryKeyJoinColumn: optional('create-pk-join'),
    } : null;
    return {name: text('create-name'), role, namespace: text('create-namespace'), tableName: role === 'entity' ? text('create-table').trim() : '', identifier, versioned: role !== 'embeddable' && kind !== 'subclass' && $('create-version').checked, inheritance};
  }
  form.addEventListener('submit', event => event.preventDefault());
  function changed(event) {
    if (event.target.id === 'create-role') $('create-declare-id').checked = text('create-role') === 'entity';
    if (['create-role', 'create-id-type', 'create-namespace'].includes(event.target.id)) generationOptions();
    dirty = true; dependencies();
    const invalid = form.querySelector('input:invalid, select:invalid');
    $('create-validation').textContent = invalid?.validationMessage || '';
    for (const input of form.querySelectorAll('input,select')) input.setAttribute('aria-invalid', String(!input.validity.valid));
    change();
  }
  form.addEventListener('input', changed); form.addEventListener('change', changed);
  window.jpaCreator = Object.freeze({
    configure, draft,
    onChange(callback) { change = callback; },
    dirty() { return dirty; },
    valid() { return !!context && form.checkValidity(); },
    reset() { if (context) configure(context, true); },
    freeze(disabled) { $('create-configuration').disabled = !context || disabled; dependencies(); },
  });
})();
