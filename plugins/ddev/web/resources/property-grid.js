/* Generic form component. It emits typed changes, never source edits or commands. */
(() => {
  'use strict';
  let next = 0;
  function render(container, properties, onChange = () => {}) {
    const controls = new Map();
    const prefix = `ui-property-${++next}`;
    container.replaceChildren();
    for (const property of properties) {
      const row = document.createElement('div'); row.className = 'ui-property-row';
      const id = `${prefix}-${property.key}`;
      const label = document.createElement('label'); label.className = 'ui-property-label'; label.htmlFor = id; label.textContent = property.label;
      const editor = document.createElement('div'); editor.className = 'ui-property-editor';
      const input = document.createElement('input'); input.id = id; input.disabled = !property.editable;
      const kind = typeof property.value;
      if (kind === 'boolean') {
        input.type = 'checkbox'; input.checked = property.value;
        const choice = document.createElement('span'); choice.className = 'ui-choice'; choice.append(input); editor.append(choice);
      } else {
        input.className = 'ui-input'; input.value = String(property.value ?? '');
        input.type = kind === 'number' ? 'number' : 'text';
        if (kind === 'number') { input.step = '1'; input.min = property.key === 'length' ? '1' : '0'; input.max = '2147483647'; input.required = true; }
        else { input.maxLength = 128; input.required = false; input.pattern = '[A-Za-z_][A-Za-z0-9_$]*'; input.placeholder = property.key === 'name' ? 'Default property name' : ''; }
        editor.append(input);
      }
      const note = document.createElement('small'); note.className = 'ui-property-origin'; note.id = `${id}-help`;
      note.textContent = property.detail || ({explicit: 'Declared in source', 'annotation-default': 'JPA annotation default', unresolved: 'Unresolved source expression'}[property.origin] || 'Source value');
      input.setAttribute('aria-describedby', note.id); editor.append(note);
      const read = () => kind === 'boolean' ? input.checked : kind === 'number' ? Number(input.value) : input.value;
      controls.set(property.key, {input, property, read});
      input.addEventListener('input', () => { input.setAttribute('aria-invalid', String(!input.validity.valid)); onChange(); });
      row.append(label, editor); container.append(row);
    }
    return {
      changes() {
        const changes = [];
        for (const [key, {input, property, read}] of controls) {
          if (!property.editable) continue;
          if (!input.checkValidity()) throw new Error('Correct the marked property values.');
          const value = read();
          if (value !== property.value) changes.push({property: key, value});
        }
        return changes;
      },
      valid() { return [...controls.values()].every(({input}) => input.disabled || input.validity.valid); },
      dirty() { return [...controls.values()].some(({property, read}) => property.editable && read() !== property.value); },
      disable(disabled) { for (const {input, property} of controls.values()) input.disabled = disabled || !property.editable; },
    };
  }
  window.uiKitPropertyGrid = Object.freeze({render});
})();
