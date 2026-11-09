"use strict";
const parityControls=[
  ['button','Button',72,40,'<button type="button">Button</button>'],
  ['input','Input field',224,40,'<input value="Text" aria-label="Parity input">'],
  ['dropdown','Custom dropdown',224,40,'<select aria-label="Parity dropdown"><option>Text</option><option>Another option</option></select>'],
  ['combo','Custom combobox',224,40,'<select data-ui-searchable aria-label="Parity combobox"><option>Text</option><option>Another option</option></select>'],
  ['checkbox','Checkbox',51,32,'<label class="ui-check"><input type="checkbox">Text</label>'],
  ['radio','Radio button',51,32,'<label class="ui-check"><input type="radio" name="parity-radio">Text</label>'],
  ['toggle','Toggle · ON',53,20,'<input type="checkbox" role="switch" aria-label="Parity toggle" checked>'],
  ['segmented','Segmented control',378,28,'<div data-ui-segmented aria-label="Parity language"><button aria-checked="true">Java</button><button>Kotlin</button><button>Groovy</button><button>JavaScript</button><button>Python</button></div>'],
  ['tab','Tab · selected',87,41,'<div role="tablist" aria-label="Parity tabs"><div class="ui-tab"><button role="tab" aria-selected="true"><span class="ui-tab-icon"></span>Text</button><button class="ui-tab-close" aria-label="Close parity tab"></button></div></div>'],
  ['shortcut','Shortcut',36,20,'<kbd class="ui-shortcut">Text</kbd>'],
  ['code','Code sample',110,20,'<code class="ui-code-sample">public static</code>']
];
function renderParity() {
  const theme=document.getElementById('parity-theme').value;document.documentElement.dataset.theme=theme;
  const rows=document.getElementById('parity-rows');rows.replaceChildren();
  for(const [name,label,width,height,html] of parityControls){
    const row=document.createElement('div');row.className='parity-row';row.dataset.control=name;
    const title=document.createElement('span');title.textContent=label;
    const source=document.createElement('div');source.className='sample-slot';source.style.height=`${height}px`;
    const img=document.createElement('img');img.src=`../reference/${name}-${theme}.png`;img.width=width;img.height=height;img.alt=`Int ${label} ${theme}`;source.append(img);
    const live=document.createElement('div');live.className='sample-slot';live.dataset.live=name;live.style.width=`${width}px`;live.style.height=`${height}px`;live.style.minHeight=`${height}px`;live.innerHTML=html;
    row.append(title,source,live);rows.append(row);
  }
  window.uiKitControls.enhance(rows);
}
document.getElementById('parity-theme').addEventListener('change',renderParity);renderParity();
