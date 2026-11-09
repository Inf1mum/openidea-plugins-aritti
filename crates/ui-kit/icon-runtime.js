/* Runtime template for scripts/build-ui-icons.py. The generated registry contains
 * the unchanged upstream SVGs; it needs no network, framework, or build loader. */
(() => {
  "use strict";
  const data = __JETBRAINS_ICON_DATA__;
  const source = Object.freeze(__JETBRAINS_ICON_SOURCE__);
  const templates = new Map(), painted = new WeakMap();let instance = 0;
  const names = Object.freeze(Object.keys(data));
  function create(name, {size, label} = {}) {
    const icon=data[name];if(!Object.hasOwn(data,name))throw new RangeError(`Unknown JetBrains icon: ${name}`);
    if(size!=null&&(!Number.isFinite(size)||size<1||size>128))throw new RangeError('Icon size must be between 1 and 128 pixels');
    if(!templates.has(name)){const template=document.createElement('template');template.innerHTML=icon.svg;templates.set(name,template);}
    const svg=templates.get(name).content.firstElementChild.cloneNode(true),prefix=`ui-jb-${++instance}-`,ids=new Map();
    for(const node of svg.querySelectorAll('[id]')){const old=node.id;node.id=prefix+old;ids.set(old,node.id);}
    for(const node of [svg,...svg.querySelectorAll('*')])for(const attr of [...node.attributes]){
      let value=attr.value;for(const [old,id] of ids){value=value.split(`url(#${old})`).join(`url(#${id})`);if(value===`#${old}`)value=`#${id}`;}if(value!==attr.value)node.setAttribute(attr.name,value);
    }
    svg.classList.add('ui-icon');svg.dataset.uiIconName=name;svg.setAttribute('focusable','false');
    if(size!=null){svg.setAttribute('width',size);svg.setAttribute('height',size);}
    // Upstream title/desc are informational source metadata. Naming comes from
    // the caller or the owning button, so decorative icons stay out of the tree.
    svg.querySelectorAll('title,desc').forEach(n=>n.remove());
    if(label){svg.setAttribute('role','img');svg.setAttribute('aria-label',label);}else svg.setAttribute('aria-hidden','true');
    return svg;
  }
  function enhance(root=document) {
    const hosts=[...(root.matches?.('[data-ui-icon]')?[root]:[]),...root.querySelectorAll?.('[data-ui-icon]')||[]];
    for(const host of hosts){const name=host.dataset.uiIcon,size=host.dataset.uiIconSize?Number(host.dataset.uiIconSize):undefined,label=host.dataset.uiIconLabel,key=JSON.stringify([name,size,label]);
      if(painted.get(host)===key)continue;
      host.replaceChildren(create(name,{size,label}));painted.set(host,key);
    }
  }
  const get=name=>Object.hasOwn(data,name)?Object.freeze({name,width:data[name].width,height:data[name].height}):undefined;
  window.uiKitIcons=Object.freeze({names,source,get,create,enhance});
})();
