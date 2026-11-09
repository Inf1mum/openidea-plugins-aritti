"use strict";
document.getElementById('inventory-theme').addEventListener('change',event=>document.documentElement.dataset.theme=event.target.value);
document.getElementById('show-notification').addEventListener('click',()=>window.uiKitWidgets.notify({title:'Index refreshed',message:'Sample resources are ready to browse.',tone:'success'}));
document.getElementById('inventory-search').addEventListener('input',event=>{
  let count=0;for(const item of document.querySelectorAll('#search-results li')){item.hidden=!item.textContent.toLowerCase().includes(event.target.value.toLowerCase());if(!item.hidden)count++;}
  document.getElementById('search-empty').hidden=count>0;
});
document.querySelectorAll('[data-action]').forEach(button=>button.addEventListener('click',()=>{
  const action=button.dataset.action;
  if(action==='dismiss-banner')button.closest('.ui-banner').hidden=true;
  else if(action==='dismiss-panel')button.closest('.ui-window').hidden=true;
  else if(action==='restore-panel')document.querySelector('[aria-label="Sample tool window"]').hidden=false;
  else document.getElementById(action.startsWith('toolbar-')?'toolbar-result':'menu-result').textContent=`Preview: ${button.textContent.trim()}`;
}));
document.querySelector('[data-ui-tree]').addEventListener('ui-select',event=>document.getElementById('tree-result').textContent=`Selected: ${event.detail.key}`);
