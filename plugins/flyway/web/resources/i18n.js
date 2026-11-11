// Stable-key UI translation. Application data and unmarked text are never translated.
(() => {
  const catalog=window.uiKitTranslations||{en:{}};
  const normalize=value=>{const tag=String(value||"").replaceAll("_","-").toLowerCase();return tag.startsWith("ja")?"ja":tag.startsWith("ko")?"ko":tag==="zh-cn"||tag==="zh-sg"||tag==="zh-hans"||tag.startsWith("zh-hans-")?"zh-CN":"en";};
  const detected=()=>normalize((navigator.languages||[navigator.language]).find(value=>/^(en|ja|ko|zh-(cn|sg|hans))\b/i.test(value))||navigator.language);
  let locale=detected();
  function t(key,fallback,args={}){const local=catalog[locale],english=catalog.en;const template=(local&&Object.hasOwn(local,key)?local[key]:undefined)??(english&&Object.hasOwn(english,key)?english[key]:undefined)??fallback??key;return String(template).replace(/\{([A-Za-z][A-Za-z0-9_]*)\}/g,(token,key)=>Object.hasOwn(args,key)?String(args[key]):token);}
  function apply(root=document){
    const nodes=[...(root.querySelectorAll?.('[data-i18n],[data-i18n-title],[data-i18n-placeholder],[data-i18n-aria-label]')||[])];
    if(root.matches?.('[data-i18n],[data-i18n-title],[data-i18n-placeholder],[data-i18n-aria-label]'))nodes.unshift(root);
    for(const node of nodes){
      if(node.hasAttribute('data-i18n') && !node.querySelector('input,textarea,select,[data-ui-raw]')){const value=t(node.dataset.i18n,node.dataset.i18nFallback??node.textContent);if(node.textContent!==value)node.textContent=value;}
      for(const attribute of ['title','placeholder','aria-label']){const key=node.getAttribute(`data-i18n-${attribute}`);if(key){const value=t(key,node.getAttribute(attribute));if(node.getAttribute(attribute)!==value)node.setAttribute(attribute,value);}}
    }
  }
  function setLocale(value){locale=!value||value==='auto'?detected():normalize(value);document.documentElement.lang=locale;apply(document);window.dispatchEvent(new CustomEvent('ui-kit-language',{detail:locale}));return locale;}
  function observe(root){apply(root);const observer=new MutationObserver(()=>apply(root));observer.observe(root,{childList:true,subtree:true});const change=()=>apply(root);window.addEventListener('ui-kit-language',change);return()=>{observer.disconnect();window.removeEventListener('ui-kit-language',change);};}
  window.uiKitI18n=Object.freeze({t,format:t,normalize,get locale(){return locale;},setLocale,apply,observe});
  setLocale('auto');
})();
