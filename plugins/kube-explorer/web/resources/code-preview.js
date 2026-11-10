// Read-only previews: preserve source bytes and never interpret markup as HTML.
(() => {
  const limit=262144;
  function language(label,text) {
    const value=text.trim();
    if(value.length<=limit && /^[\[{]/.test(value)) {try {JSON.parse(value);return 'json';}catch{}}
    if(/^\s*<\?xml\b|^\s*<[\w:-]+[\s>]/.test(text))return 'xml';
    if(/^(?:diff --git |@@ |--- .+\n\+\+\+ )/m.test(text))return 'diff';
    if(/\b(?:SQL|DDL)\b/i.test(label)||/^\s*(?:SELECT\b|CREATE\s+(?:TABLE|INDEX)|ALTER\s+TABLE|INSERT\s+INTO|UPDATE\b|DELETE\s+FROM)/i.test(text))return 'sql';
    if(/\b(?:YAML|manifest)\b/i.test(label)||/^\s*(?:apiVersion|kind):/m.test(text))return 'yaml';
    return text.includes('\n')||/\b(?:JSON|code|stack trace)\b/i.test(label)?'text':null;
  }
  function prettyJson(text) {
    if(text.length>limit)return text;
    // JSON.parse is validation only: stringifying it would round large integers.
    try {JSON.parse(text);}catch{return text;}
    let result='',depth=0,quoted=false,escaped=false,last='';
    const newline=()=>{result+='\n'+'  '.repeat(depth);};
    for(let i=0;i<text.length;i++) {
      const c=text[i];
      if(quoted){result+=c;if(escaped)escaped=false;else if(c==='\\')escaped=true;else if(c==='"')quoted=false;continue;}
      if(c==='"'){quoted=true;result+=c;last=c;continue;}
      if(/\s/.test(c))continue;
      if(c==='{'||c==='['){result+=c;if(++depth>64)return text;if(!/^[\s]*[}\]]/.test(text.slice(i+1,i+65)))newline();}
      else if(c==='}'||c===']'){depth--;if(last!=='{'&&last!=='[')newline();result+=c;}
      else if(c===','){result+=c;newline();}
      else if(c===':')result+=': ';
      else result+=c;
      last=c;
      if(result.length>limit*2)return text;
    }
    return result;
  }
  function paint(code,text,kind) {
    code.replaceChildren();
    if(text.length>limit){code.textContent=text;return;}
    const pattern=kind==='json'?/"(?:\\.|[^"\\])*"(?=\s*:)|"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b/gi
      :kind==='xml'?/<!--[\s\S]*?-->|<\/?[\w:-]+|\b[\w:-]+(?==)|"[^"\n]*"|'[^'\n]*'/g
      :kind==='sql'?/--[^\n]*|'(?:''|[^'])*'|\b(?:SELECT|FROM|WHERE|CREATE|TABLE|ALTER|DROP|INSERT|INTO|VALUES|UPDATE|SET|DELETE|JOIN|ON|AS|AND|OR|NULL|NOT|PRIMARY|KEY|INDEX|DEFAULT|TRUE|FALSE)\b|\b\d+(?:\.\d+)?\b/gi
      :kind==='yaml'?/#[^\n]*|^[ \t]*[\w./-]+(?=:)|"(?:\\.|[^"\\])*"|'[^'\n]*'|\b(?:true|false|null)\b|\b\d+(?:\.\d+)?\b/gm
      :kind==='diff'?/^[+\-@][^\n]*$/gm:null;
    if(!pattern){code.textContent=text;return;}
    let offset=0,tokens=0;
    for(const match of text.matchAll(pattern)) {
      if(++tokens>5000){code.textContent=text;return;}

      code.append(document.createTextNode(text.slice(offset,match.index)));
      const token=document.createElement('span'),value=match[0];
      token.className='code-token';token.dataset.tone=value.startsWith('#')||value.startsWith('--')||value.startsWith('<!--')?'comment':kind==='diff'?(value[0]==='+'?'added':value[0]==='-'?'removed':'keyword'):/^["']/.test(value)?(kind==='json'&&/^\s*:/.test(text.slice(match.index+value.length))?'key':'string'):/^-?\d/.test(value)?'number':'keyword';
      token.textContent=value;code.append(token);offset=match.index+value.length;
    }
    code.append(document.createTextNode(text.slice(offset)));
  }
  function create(label,value) {
    if(typeof value!=='string')return null;
    const kind=language(label,value);if(!kind)return null;
    const root=document.createElement('div');root.className='code-preview';root.dataset.language=kind;
    const toolbar=document.createElement('div');toolbar.className='code-preview-toolbar';
    const caption=document.createElement('span');caption.textContent=kind==='text'?'Text':kind.toUpperCase();toolbar.append(caption);
    const pre=document.createElement('pre'),code=document.createElement('code');pre.className='ui-code-sample ui-scroll-area';pre.tabIndex=0;pre.setAttribute('aria-label',label);pre.append(code);
    let formatted=kind==='json',wrap=false;
    const render=()=>paint(code,formatted?prettyJson(value):value,kind);
    if(kind==='json') {
      const format=document.createElement('button');format.type='button';format.textContent='Format JSON';format.title='Format JSON';format.setAttribute('aria-pressed','true');format.onclick=()=>{formatted=!formatted;format.setAttribute('aria-pressed',String(formatted));render();};toolbar.append(format);
    }
    const toggle=document.createElement('button');toggle.type='button';toggle.textContent='Wrap lines';toggle.title='Wrap lines';toggle.setAttribute('aria-pressed','false');toggle.onclick=()=>{wrap=!wrap;toggle.setAttribute('aria-pressed',String(wrap));pre.classList.toggle('code-preview-wrap',wrap);};toolbar.append(toggle);
    const select=document.createElement('button');select.type='button';select.textContent='Select all';select.title='Select code';select.onclick=()=>{const range=document.createRange();range.selectNodeContents(code);const selection=window.getSelection();selection.removeAllRanges();selection.addRange(range);pre.focus();};toolbar.append(select);
    render();root.append(toolbar,pre);return root;
  }
  window.uiKitCodePreview=Object.freeze({create,language,prettyJson});
})();
