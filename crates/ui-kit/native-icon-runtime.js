/* Native OpenIDEA v0.1.40 icon adapter. Source SVGs and mapping are pinned. */
(() => {
  "use strict";
  const data=__NATIVE_ICON_DATA__, source=Object.freeze(__NATIVE_ICON_SOURCE__);
  const aliases=Object.freeze({update:'refresh',add:'plus',remove:'minus',pencil:'edit',checkmark:'check','more-options':'more','star-empty':'star','star-filled':'star','new-window':'external-link',branches:'branch',compare:'diff',exception:'error',artifacts:'package',redo:'rerun',comment:'review-comment',activity:'github-timeline','arrow-12px-down':'arrow-down','arrow-12px-up':'arrow-up','chevron-12px-down':'chevron-down','chevron-12px-right':'chevron-right'});
  const resolve=name=>Object.hasOwn(data,name)?name:Object.hasOwn(aliases,name)&&Object.hasOwn(data,aliases[name])?aliases[name]:null;
  const templates=new Map();let instance=0;
  function parse(text){const template=document.createElement('template');template.innerHTML=text;return template.content.querySelector('svg');}
  function unique(root,prefix){const ids=new Map();for(const n of root.querySelectorAll('[id]')){const id=n.id;n.id=prefix+id;ids.set(id,n.id);}for(const n of [root,...root.querySelectorAll('*')])for(const a of [...n.attributes]){let value=a.value;for(const [old,id]of ids){value=value.split(`url(#${old})`).join(`url(#${id})`);if(value===`#${old}`)value=`#${id}`;}if(value!==a.value)n.setAttribute(a.name,value);}}
  function prepare(name,large){
    const icon=data[name], light=parse(large&&icon.large?icon.large:icon.light),darkText=large&&icon.large?icon.largeDark:icon.dark;
    const dark=darkText?parse(darkText):null;
    const colors=root=>new Set((root.innerHTML.match(/#[0-9a-f]{3,8}\b/gi)||[]).map(c=>c.toLowerCase()));
    const only=(set,color)=>set.size>0&&[...set].every(c=>c===color);
    const mono=!icon.keepColors&&only(colors(light),'#6c707e')&&(!dark||only(colors(dark),'#ced0d6'));
    if(mono){for(const node of [light,...light.querySelectorAll('*')])for(const a of [...node.attributes])node.setAttribute(a.name,a.value.replace(/#[0-9a-f]{3,8}\b/gi,'currentColor'));light.classList.add('ui-native-mono');}
    else if(dark){const group=(svg,variant)=>{const g=document.createElementNS('http://www.w3.org/2000/svg','g');g.classList.add(`ui-native-${variant}`);if(svg.hasAttribute('fill'))g.setAttribute('fill',svg.getAttribute('fill'));unique(svg,`${variant}-`);g.append(...svg.childNodes);return g;};const l=group(light,'light'),d=group(dark,'dark');light.replaceChildren(l,d);}
    light.querySelectorAll('title,desc').forEach(n=>n.remove());return light;
  }
  function create(name,{size=16,label}={}){
    const key=resolve(name);if(!key)throw new RangeError(`Unknown native icon: ${name}`);
    if(!Number.isFinite(size)||size<1||size>128)throw new RangeError('Icon size must be between 1 and 128 pixels');
    const cache=`${key}:${size>=18}`;if(!templates.has(cache))templates.set(cache,prepare(key,size>=18));
    const svg=templates.get(cache).cloneNode(true);unique(svg,`ui-native-${++instance}-`);svg.classList.add('ui-icon');svg.dataset.uiIconName=name;svg.dataset.uiNativeName=key;svg.dataset.uiIconSource='openidea-native';svg.setAttribute('width',size);svg.setAttribute('height',size);svg.setAttribute('focusable','false');
    if(label){svg.setAttribute('role','img');svg.setAttribute('aria-label',label);}else svg.setAttribute('aria-hidden','true');return svg;
  }
  window.uiKitNativeIcons=Object.freeze({source,names:Object.freeze(Object.keys(data)),resolve,get:name=>resolve(name)?Object.freeze({name:resolve(name),width:16,height:16}):undefined,create});
})();
