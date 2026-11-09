/* Int-style widgets. Native form values/events remain the integration contract. */
(() => {
  "use strict";
  let nextId = 0;
  const enhanced = new WeakSet(), selects = new WeakMap(), openWidgets = new Set(), links = new WeakSet(), menuOrigins = new WeakMap(), openMenus = new Set();
  const node = (tag, className, text) => {const n=document.createElement(tag);if(className)n.className=className;if(text!=null)n.textContent=text;return n;};
  const announce = (target, type, detail) => target.dispatchEvent(new CustomEvent(`ui-${type}`,{bubbles:true,detail}));
  const members = (root,selector) => [...(root.matches?.(selector)?[root]:[]),...root.querySelectorAll?.(selector)||[]];
  function position(popup, anchor, point) {
    const rect=anchor.getBoundingClientRect(), margin=8;
    const tip=popup.classList.contains('ui-tooltip'), menu=popup.classList.contains('ui-menu'), arrow=tip&&popup.dataset.uiTip;
    popup.style.maxWidth='';popup.style.width=tip||menu?'max-content':`${Math.min(Math.max(rect.width,180),innerWidth-margin*2)}px`;
    popup.style.maxHeight=`${Math.min(260,innerHeight-margin*2)}px`;
    const naturalWidth=popup.getBoundingClientRect().width;
    popup.style.maxWidth=`${Math.min(naturalWidth,innerWidth-margin*2)}px`;
    const width=popup.getBoundingClientRect().width;
    const height=popup.getBoundingClientRect().height;
    const gap=arrow?8:4, below=point?.y??rect.bottom+gap;
    let side=popup.dataset.uiPlacement||'bottom';if(side==='bottom'&&below+height>innerHeight-margin)side='top';
    if(side==='top'&&rect.top-height-gap<margin&&below+height<=innerHeight-margin)side='bottom';
    let left=point?.x??(tip?rect.left+(rect.width-width)/2:rect.left),top=side==='top'?(point?.y??rect.top)-height-gap:below;
    if(side==='left'||side==='right'){left=side==='left'?rect.left-width-gap:rect.right+gap;top=rect.top+(rect.height-height)/2;}
    left=Math.max(margin,Math.min(left,innerWidth-width-margin));top=Math.max(margin,Math.min(top,innerHeight-height-margin));
    popup.dataset.side=side;popup.style.setProperty('--ui-tip-offset',`${Math.max(16,Math.min(width-16,rect.left+rect.width/2-left))}px`);
    popup.style.left=`${left}px`;popup.style.top=`${top}px`;
  }
  function customSelect(select) {
    if(selects.has(select)||select.multiple||select.size>1||select.closest('[data-ui-raw]'))return;
    const id=`ui-select-${++nextId}`, searchable=select.hasAttribute('data-ui-searchable');
    const labelCopy=select.labels?.[0]?.cloneNode(true);labelCopy?.querySelectorAll('select,input,textarea,button,output').forEach(n=>n.remove());
    const labelledBy=select.getAttribute('aria-labelledby')?.split(/\s+/).map(id=>select.getRootNode().getElementById(id)?.textContent||'').join(' ');
    const label=select.getAttribute('aria-label')||labelledBy||labelCopy?.textContent.trim()||select.name||'Select option';
    const shell=node('span','ui-select-shell'), trigger=node(searchable?'input':'button',searchable?'ui-input ui-combobox-input':'ui-button ui-select-trigger');
    if(searchable){trigger.type='text';trigger.autocomplete='off';trigger.spellcheck=false;}
    else trigger.type='button';
    trigger.setAttribute('role','combobox');trigger.setAttribute('aria-label',label);trigger.setAttribute('aria-haspopup','listbox');trigger.setAttribute('aria-controls',`${id}-list`);trigger.setAttribute('aria-expanded','false');
    if(searchable)trigger.setAttribute('aria-autocomplete','list');
    if(select.dataset.fieldKey)trigger.dataset.fieldKey=select.dataset.fieldKey;
    const popup=node('div','ui-popup ui-select-popup');popup.id=`${id}-list`;popup.setAttribute('popover','manual');popup.setAttribute('role','listbox');popup.setAttribute('aria-label',label);
    select.classList.add('ui-select-native');select.setAttribute('aria-hidden','true');select.tabIndex=-1;
    const controller={close,position:()=>position(popup,trigger),connected:()=>select.isConnected};selects.set(select,controller);
    select.before(shell);shell.append(select,trigger,popup);
    let active=-1, filtered=[], opened=false, typeahead='', typeTimer;
    function sync() {
      trigger.disabled=select.disabled;trigger.setAttribute('aria-required',String(select.required));
      for(const attr of ['aria-invalid','aria-describedby']) {if(select.hasAttribute(attr))trigger.setAttribute(attr,select.getAttribute(attr));else trigger.removeAttribute(attr);}
      if(!opened){const text=select.selectedOptions[0]?.label||'';if(searchable)trigger.value=text;else trigger.textContent=text;}
      if(select.disabled&&opened)close();
    }
    function highlight(index) {
      if(!filtered.length){active=-1;trigger.removeAttribute('aria-activedescendant');return;}
      active=(index+filtered.length)%filtered.length;
      popup.querySelectorAll('[role=option]').forEach(n=>n.classList.toggle('ui-option--active',n.dataset.index===String(active)));
      const current=popup.querySelector(`[data-index="${active}"]`);if(current){trigger.setAttribute('aria-activedescendant',current.id);current.scrollIntoView({block:'nearest'});}
    }
    function render(query='') {
      popup.replaceChildren();const shown=[...select.options].filter(o=>!o.hidden&&o.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()));filtered=shown.filter(o=>!o.disabled&&!o.parentElement.disabled);
      let group;
      shown.forEach(option=>{
        const index=filtered.indexOf(option);
        const parent=option.parentElement.localName==='optgroup'?option.parentElement.label:'';
        if(parent&&parent!==group){popup.append(node('div','ui-group-header',parent));}group=parent;
        const item=node('div','ui-option',option.label);item.id=`${id}-option-${option.index}`;if(index>=0)item.dataset.index=index;item.setAttribute('role','option');item.setAttribute('aria-selected',String(option.selected));item.setAttribute('aria-disabled',String(index<0));
        item.addEventListener('pointerdown',event=>event.preventDefault());if(index>=0)item.addEventListener('click',()=>choose(index));popup.append(item);
      });
      if(!shown.length)popup.append(node('div','ui-menu-empty','No matching options'));
      highlight(Math.max(0,filtered.findIndex(o=>o.selected)));
    }
    function open(selectText=true) {
      if(select.disabled||opened)return;
      for(const other of [...openWidgets])other.close();
      opened=true;trigger.setAttribute('aria-expanded','true');popup.showPopover();render();position(popup,trigger);openWidgets.add(controller);
      if(searchable&&selectText){trigger.select();}
    }
    function close() {
      if(!opened)return;opened=false;popup.hidePopover();trigger.setAttribute('aria-expanded','false');trigger.removeAttribute('aria-activedescendant');openWidgets.delete(controller);sync();
    }
    function choose(index) {
      const option=filtered[index];if(!option)return;
      const changed=select.selectedIndex!==option.index;select.selectedIndex=option.index;close();trigger.focus();sync();
      if(changed){select.dispatchEvent(new Event('input',{bubbles:true}));select.dispatchEvent(new Event('change',{bubbles:true}));}
    }
    trigger.addEventListener('click',event=>{event.preventDefault();opened&&!searchable?close():open();});
    popup.addEventListener('click',event=>event.preventDefault());
    trigger.addEventListener('input',()=>{if(searchable){open(false);render(trigger.value);position(popup,trigger);}});
    trigger.addEventListener('keydown',event=>{
      const key=event.key;
      if(key==='Tab'){close();return;}
      if(key==='Escape'&&opened){event.preventDefault();event.stopPropagation();close();return;}
      if(['ArrowDown','ArrowUp','Home','End'].includes(key)){
        if(!opened&&searchable&&['Home','End'].includes(key))return;
        event.preventDefault();const wasOpen=opened;open();
        highlight(key==='Home'?0:key==='End'?filtered.length-1:wasOpen?active+(key==='ArrowDown'?1:-1):Math.max(0,filtered.findIndex(o=>o.selected)));return;
      }
      if(key==='Enter'||(key===' '&&!searchable)){event.preventDefault();if(opened)choose(active);else open();return;}
      if(!searchable&&key.length===1&&!event.ctrlKey&&!event.metaKey){event.preventDefault();open();clearTimeout(typeTimer);typeahead+=key.toLocaleLowerCase();highlight(Math.max(0,filtered.findIndex(o=>o.label.toLocaleLowerCase().startsWith(typeahead))));typeTimer=setTimeout(()=>typeahead='',600);}
    });
    trigger.addEventListener('blur',()=>queueMicrotask(()=>{if(!shell.contains(select.getRootNode().activeElement))close();}));
    select.addEventListener('change',sync);
    select.form?.addEventListener('reset',()=>setTimeout(sync));
    new MutationObserver(()=>{sync();if(opened){render(searchable?trigger.value:'');position(popup,trigger);}}).observe(select,{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','selected','label','value','required','aria-invalid','aria-describedby']});
    controller.contains=event=>event.composedPath().includes(shell);sync();
  }
  function tabs(root, segmented=false) {
    if(enhanced.has(root))return;enhanced.add(root);root.classList.add(segmented?'ui-segmented':'ui-tabs');
    const items=()=>[...root.querySelectorAll(segmented?'button':'[role=tab]')].filter(n=>!n.disabled);
    if(segmented){root.setAttribute('role','radiogroup');for(const button of items()){button.setAttribute('role','radio');button.setAttribute('aria-checked',button.getAttribute('aria-checked')||'false');}}
    const activate=button=>{
      for(const item of items()) {
        const active=item===button;item.tabIndex=active?0:-1;item.setAttribute(segmented?'aria-checked':'aria-selected',String(active));
        if(!segmented){const panel=root.getRootNode().getElementById(item.getAttribute('aria-controls'));if(panel)panel.hidden=!active;}
      }
      announce(root,'change',{value:button.dataset.value||button.textContent.trim()});
    };
    const initial=items().find(n=>n.getAttribute(segmented?'aria-checked':'aria-selected')==='true')||items()[0];if(initial)activate(initial);
    root.addEventListener('click',event=>{
      const button=event.target.closest('button');
      if(!segmented&&button?.classList.contains('ui-tab-close')){
        const wrapper=button.closest('.ui-tab'),tab=wrapper?.querySelector('[role=tab]');if(!tab)return;
        const all=items(),index=all.indexOf(tab),selected=tab.getAttribute('aria-selected')==='true';
        const close=new CustomEvent('ui-close',{bubbles:true,cancelable:true,detail:{tab:tab.id,panel:tab.getAttribute('aria-controls')}});
        if(!tab.dispatchEvent(close))return;
        const panel=root.getRootNode().getElementById(tab.getAttribute('aria-controls'));wrapper.remove();panel?.remove();
        const next=items()[Math.min(index,items().length-1)];if(next){if(selected)activate(next);next.focus();}else{root.tabIndex=-1;root.focus();}return;
      }
      if(items().includes(button))activate(button);
    });
    root.addEventListener('keydown',event=>{
      const all=items(), index=all.indexOf(event.target);if(index<0)return;
      if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key))return;
      event.preventDefault();const next=event.key==='Home'?all[0]:event.key==='End'?all.at(-1):all[(index+(['ArrowLeft','ArrowUp'].includes(event.key)?-1:1)+all.length)%all.length];next.focus();next.click();
    });
  }
  function menu(popup) {
    if(enhanced.has(popup))return;
    enhanced.add(popup);
    popup.classList.add('ui-popup','ui-menu');
    popup.setAttribute('role','menu');
    // WebKit auto-popovers light-dismiss on the release of the right-button
    // gesture that opened them. Menus own dismissal instead; native invoker
    // buttons still toggle manual popovers normally.
    popup.setAttribute('popover','manual');
    const items=()=>[...popup.querySelectorAll('button,a[href]')].filter(n=>!n.disabled);
    for(const item of items())item.setAttribute('role','menuitem');
    const close=()=>{
      const origin=menuOrigins.get(popup);
      if(origin)origin.restoreFocus=true;
      popup.hidePopover();
    };
    popup.addEventListener('toggle',()=>{
      const origin=menuOrigins.get(popup);
      if(popup.matches(':popover-open')){
        for(const other of openMenus)if(other!==popup)other.hidePopover();
        openMenus.add(popup);
        const trigger=origin?.trigger||popup.getRootNode().querySelector(`[popovertarget="${CSS.escape(popup.id)}"]`);
        if(trigger){
          if(!origin)menuOrigins.set(popup,{trigger});
          position(popup,trigger,origin?.point);
        }
        items()[0]?.focus();
      }else{
        openMenus.delete(popup);
        menuOrigins.delete(popup);
        if(origin?.restoreFocus&&origin.trigger.isConnected&&origin.trigger.getClientRects().length){
          origin.trigger.focus({preventScroll:true});
        }
      }
    });
    popup.addEventListener('keydown',event=>{
      if(event.key==='Escape'){
        event.preventDefault();
        event.stopPropagation();
        close();
        return;
      }
      if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){event.preventDefault();const all=items(),index=all.indexOf(event.target);(event.key==='Home'?all[0]:event.key==='End'?all.at(-1):all[(index+(event.key==='ArrowDown'?1:-1)+all.length)%all.length])?.focus();}
    });
    popup.addEventListener('click',event=>{
      if(event.target.closest('[role=menuitem]'))close();
    });
  }
  function toolbar(root) {
    if(enhanced.has(root))return;enhanced.add(root);root.classList.add('ui-toolbar');
    const items=()=>[...root.querySelectorAll('button,input,select,[tabindex="0"]')].filter(n=>!n.disabled&&n.getAttribute('aria-hidden')!=='true');
    root.addEventListener('focusin',event=>{for(const item of items())item.tabIndex=item===event.target?0:-1;});
    root.addEventListener('keydown',event=>{
      if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)||event.target.getAttribute('role')==='combobox')return;
      const all=items(),index=all.indexOf(event.target);if(index<0)return;event.preventDefault();
      (event.key==='Home'?all[0]:event.key==='End'?all.at(-1):all[(index+(event.key==='ArrowRight'?1:-1)+all.length)%all.length])?.focus();
    });
  }
  function tree(root) {
    if(enhanced.has(root))return;enhanced.add(root);root.setAttribute('role','tree');root.classList.add('ui-tree');
    const items=()=>[...root.querySelectorAll('[role=treeitem]')].filter(n=>!n.closest('[hidden]'));
    const expand=(item,value)=>{item.setAttribute('aria-expanded',String(value));item.querySelector(':scope > .ui-tree-row > .ui-tree-chevron')?.setAttribute('aria-expanded',String(value));const group=[...item.children].find(n=>n.getAttribute('role')==='group');if(group)group.hidden=!value;announce(item,value?'expand':'collapse',{key:item.dataset.key});};
    const select=item=>{for(const n of items()){n.tabIndex=n===item?0:-1;n.setAttribute('aria-selected',String(n===item));}announce(item,'select',{key:item.dataset.key});};
    for(const [i,item] of items().entries()) {
      const row=node('span','ui-tree-row'),label=node('span','ui-tree-label');
      const own=[...item.childNodes].filter(n=>n.nodeType!==1||n.getAttribute('role')!=='group');
      for(const child of own)label.append(child);label.id=`ui-tree-label-${++nextId}`;
      if(!item.hasAttribute('aria-label')&&!item.hasAttribute('aria-labelledby'))item.setAttribute('aria-labelledby',label.id);
      if(item.hasAttribute('aria-expanded')){
        const chevron=node('button','ui-tree-chevron');chevron.type='button';chevron.tabIndex=-1;chevron.setAttribute('aria-label',`Toggle ${label.textContent.trim()}`);
        chevron.addEventListener('click',event=>{event.stopPropagation();expand(item,item.getAttribute('aria-expanded')!=='true');item.focus();});row.append(chevron);
      }else row.append(node('span','ui-tree-chevron-gap'));
      const icon=node('span','ui-tree-icon');icon.setAttribute('aria-hidden','true');row.append(icon,label);item.prepend(row);
      item.tabIndex=i===0?0:-1;if(item.hasAttribute('aria-expanded'))expand(item,item.getAttribute('aria-expanded')==='true');
    }
    root.addEventListener('click',event=>{const item=event.target.closest('[role=treeitem]');if(item&&!event.target.closest('button,a,input')){select(item);item.focus();}});
    root.addEventListener('keydown',event=>{
      const item=event.target.closest('[role=treeitem]');if(!item||event.target!==item)return;
      const all=items(),index=all.indexOf(item),key=event.key;let next;
      if(key==='ArrowDown')next=all[Math.min(index+1,all.length-1)];else if(key==='ArrowUp')next=all[Math.max(0,index-1)];else if(key==='Home')next=all[0];else if(key==='End')next=all.at(-1);
      else if(key==='ArrowRight'){if(item.getAttribute('aria-expanded')==='false')expand(item,true);else next=item.querySelector('[role=treeitem]');}
      else if(key==='ArrowLeft'){if(item.getAttribute('aria-expanded')==='true')expand(item,false);else next=item.parentElement.closest('[role=treeitem]');}
      else if(key===' '){select(item);}else if(key==='Enter'){announce(item,'activate',{key:item.dataset.key});}else return;
      event.preventDefault();if(next){select(next);next.focus();}
    });
  }
  function tooltip(trigger) {
    if(enhanced.has(trigger))return;enhanced.add(trigger);
    const content=trigger.getRootNode().getElementById(trigger.dataset.uiTooltip);if(!content)return;
    content.classList.add('ui-popup','ui-tooltip');content.setAttribute('popover','manual');content.setAttribute('role',trigger.dataset.uiTip?'dialog':'tooltip');trigger.setAttribute('aria-describedby',content.id);
    if(trigger.dataset.uiTip)content.setAttribute('aria-label',trigger.textContent.trim());
    if(trigger.dataset.uiTip)content.dataset.uiTip=trigger.dataset.uiTip;
    let dismissed=false,suppressFocus=false,timer;
    const close=()=>content.hidePopover();
    const open=()=>{if(!dismissed&&!suppressFocus){clearTimeout(timer);content.showPopover();position(content,trigger);}};
    trigger.addEventListener('mouseenter',open);trigger.addEventListener('focus',open);trigger.addEventListener('mouseleave',()=>timer=setTimeout(close,150));trigger.addEventListener('blur',()=>timer=setTimeout(()=>{if(!content.contains(content.getRootNode().activeElement))close();},150));
    trigger.addEventListener('keydown',event=>{if(event.key==='Escape')close();});content.addEventListener('mouseenter',()=>clearTimeout(timer));content.addEventListener('mouseleave',close);
    content.addEventListener('keydown',event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();suppressFocus=true;trigger.focus();queueMicrotask(()=>suppressFocus=false);}});
    content.querySelector('[data-ui-dismiss]')?.addEventListener('click',()=>{dismissed=true;close();trigger.focus();});
  }
  function notify({title='Notification',message='',tone='info',root=document}={}) {
    let region=root.querySelector('.ui-notifications');if(!region){region=node('div','ui-notifications');(root.body||root).append(region);}
    const toast=node('section',`ui-notification ui-alert ui-alert--${tone}`);toast.setAttribute('role',tone==='error'?'alert':'status');
    const close=node('button','ui-button ui-button--quiet','×');close.type='button';close.setAttribute('aria-label','Dismiss notification');close.addEventListener('click',()=>toast.remove());
    toast.append(node('strong',null,title),node('p',null,message),close);region.append(toast);return toast;
  }
  function enhance(root=document) {
    for(const link of members(root,'a[href]'))if(!links.has(link)){
      links.add(link);link.addEventListener('click',event=>{if(link.getAttribute('aria-disabled')==='true'){event.preventDefault();event.stopImmediatePropagation();}},true);
      if(link.getAttribute('aria-disabled')==='true')link.tabIndex=-1;
    }
    for(const select of members(root,'select'))customSelect(select);
    for(const tablist of members(root,'[role=tablist]'))tabs(tablist);
    for(const group of members(root,'[data-ui-segmented]'))tabs(group,true);
    for(const popup of members(root,'[data-ui-menu]'))menu(popup);
    for(const bar of members(root,'[role=toolbar]'))toolbar(bar);
    for(const view of members(root,'[data-ui-tree]'))tree(view);
    for(const trigger of members(root,'[data-ui-tooltip]'))tooltip(trigger);
    for(const popup of members(root,'[popover].ui-popup:not(.ui-select-popup):not(.ui-menu):not(.ui-tooltip)')) {
      if(enhanced.has(popup))continue;enhanced.add(popup);
      popup.addEventListener('toggle',()=>{if(popup.matches(':popover-open')){const trigger=popup.getRootNode().querySelector(`[popovertarget="${CSS.escape(popup.id)}"]`);if(trigger)position(popup,trigger);}});
    }
    for(const trigger of members(root,'[data-ui-dialog],[data-ui-close],[data-ui-context]')) {
      if(enhanced.has(trigger))continue;enhanced.add(trigger);
      if(trigger.hasAttribute('data-ui-context')){
        const open=point=>{const popup=trigger.getRootNode().getElementById(trigger.dataset.uiContext);if(popup){menuOrigins.set(popup,{trigger,point});popup.showPopover();position(popup,trigger,point);popup.querySelector('[role=menuitem]')?.focus();}};
        trigger.addEventListener('contextmenu',event=>{event.preventDefault();open(event.clientX||event.clientY?{x:event.clientX,y:event.clientY}:undefined);});
        trigger.addEventListener('keydown',event=>{if(event.key==='ContextMenu'||(event.shiftKey&&event.key==='F10')){event.preventDefault();open();}});
      }
      else trigger.addEventListener('click',()=>{if(trigger.dataset.uiDialog)trigger.getRootNode().getElementById(trigger.dataset.uiDialog)?.showModal();else trigger.closest('dialog')?.close();});
    }
  }
  function prune(){for(const widget of [...openWidgets])if(!widget.connected())widget.close();for(const popup of openMenus)if(!popup.isConnected)openMenus.delete(popup);}
  document.addEventListener('pointerdown',event=>{for(const widget of [...openWidgets])if(!widget.contains(event))widget.close();},true);
  document.addEventListener('pointerdown',event=>{
    if(event.button!==0)return;
    const path=event.composedPath();
    for(const popup of openMenus)if(!path.includes(popup)&&!path.some(n=>n.getAttribute?.('popovertarget')===popup.id))popup.hidePopover();
  },true);
  window.addEventListener('resize',()=>{prune();for(const widget of openWidgets)widget.position();});
  document.addEventListener('scroll',()=>{prune();for(const widget of openWidgets)widget.position();},true);
  window.uiKitWidgets=Object.freeze({enhance,notify,prune});
})();
