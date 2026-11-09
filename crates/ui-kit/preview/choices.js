"use strict";
document.getElementById('choices-theme').addEventListener('change',event=>document.documentElement.dataset.theme=event.target.value);
const rows=[['checkbox','unchecked','Checkbox'],['checkbox','checked','Checkbox · checked'],['checkbox','indeterminate','Checkbox · mixed'],['radio','unchecked','Radio'],['radio','checked','Radio · selected']];
for(const [kind,value,title] of rows){
  const row=document.createElement('tr');const heading=document.createElement('th');heading.scope='row';heading.textContent=title;row.append(heading);
  for(const state of ['active','hovered','focused','invalid','disabled']){
    const cell=document.createElement('td'),label=document.createElement('label'),input=document.createElement('input');label.className='ui-check';input.type=kind;input.name=`${kind}-${value}-${state}`;input.setAttribute('aria-label',`${kind} ${value} ${state}`);
    input.checked=value==='checked';input.indeterminate=value==='indeterminate';input.disabled=state==='disabled';
    if(state==='invalid')input.setAttribute('aria-invalid','true');
    if(['hovered','focused'].includes(state))input.dataset.uiState=state;
    label.append(input,document.createTextNode('Text'));cell.append(label);row.append(cell);
  }
  document.getElementById('choice-matrix').append(row);
}
window.uiKitControls.enhance(document.getElementById('choice-matrix'));
