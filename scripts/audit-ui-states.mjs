const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url)),OUT=resolve(ROOT,'crates/ui-kit/audit');mkdirSync(OUT,{recursive:true});
const sets=JSON.parse(readFileSync(resolve(ROOT,'crates/ui-kit/reference/int-ui-kit.json'),'utf8')).sets;
const rows=[],rgb=h=>`rgb(${[1,3,5].map(i=>parseInt(h.slice(i,i+2),16)).join(', ')})`;
const paint=(n,key)=>n?.[key]?.find(p=>p.color&&p.visible!==false)?.color;
const checks=(row,property,expected,actual)=>{if(expected!=null)row.checks.push({property,expected,actual,pass:typeof expected==='number'?Math.abs(expected-actual)<.51:expected===actual});};
const content=v=>v.nodes.find(n=>n.id===v.id&&paint(n,'fills'))||v.nodes.find(n=>n.path.endsWith('/Content')&&paint(n,'fills'))||v.nodes[0];
const browser=await chromium.launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:1200,height:900}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/inventory.html')).href);await page.evaluate(()=>document.fonts.ready);
 for(const key of ['button','input','dropdown','combo','checkbox','radio','link','banner','toggle'])for(const v of sets[key]){
  const theme=/Theme\s*=\s*Dark/.test(v.name)?'dark':'light',disabled=/State=Disabled/.test(v.name),focused=/State=Focused|Focus=On/.test(v.name),hovered=/State=Hovered/.test(v.name),pressed=/State=Pressed/.test(v.name),invalid=/State=Invalid/.test(v.name);
  const value=key==='radio'?(/Selected=On/.test(v.name)?'checked':'unchecked'):v.name.match(/Type=(Checked|Unchecked|Indeterminate)/)?.[1].toLowerCase();
  const row={component:key,sourceId:v.id,variant:v.name,checks:[]};
  await page.mouse.move(1199,0);await page.evaluate(()=>document.activeElement?.blur());
  await page.evaluate(({key,theme,disabled,invalid,value,name})=>{
   document.documentElement.dataset.theme=theme;document.getElementById('audit-state-fixture')?.remove();
   const root=document.createElement('div');root.id='audit-state-fixture';root.style.cssText='position:fixed;top:16px;left:16px;z-index:10000;padding:12px;background:var(--ui-bg);font:500 13px/16px var(--ui-font)';document.body.append(root);let n;
   if(['dropdown','combo'].includes(key)){n=document.createElement('select');n.innerHTML='<option>Text</option>';if(key==='combo')n.dataset.uiSearchable='';}
   else if(key==='button'){n=document.createElement('button');n.type='button';n.className=name.includes('Button type=Primary')?'ui-button ui-button--primary':'ui-button';n.textContent='Button';}
   else if(key==='link'){n=document.createElement('a');n.href='#audit';n.textContent='Text';n.className='ui-link'+(name.includes('Type=External')?' ui-link--external':name.includes('Type=Dropdown')?' ui-link--dropdown':'');if(disabled)n.setAttribute('aria-disabled','true');}
   else if(key==='banner'){n=document.createElement('div');const tone=name.match(/Type=(\w+)/)[1].toLowerCase();n.className=`ui-banner ui-banner--${tone}${name.includes('Inline=True')?' ui-banner--inline':''}`;n.style.width=name.includes('Inline=True')?'400px':'600px';n.textContent='Text';}
   else{n=document.createElement('input');n.type=['checkbox','radio','toggle'].includes(key)?(key==='toggle'?'checkbox':key):'text';n.value='Text';if(key==='toggle'){n.setAttribute('role','switch');n.checked=name.endsWith('State=On');}else{n.checked=value==='checked';n.indeterminate=value==='indeterminate';}}
   n.id='audit-state-native';n.setAttribute('aria-label','Audit state control');if(disabled)n.disabled=true;if(invalid)n.setAttribute('aria-invalid','true');root.append(n);window.uiKitControls.enhance(root);
   const active=root.querySelector('[role=combobox]')||n;active.dataset.auditActive='';
  },{key,theme,disabled,invalid,value,name:v.name});
  const target=page.locator('[data-audit-active]');
  if(hovered||pressed){await target.hover();if(pressed)await page.mouse.down();}
  if(focused){await page.keyboard.press('Tab');await target.focus();}
  const actual=await target.evaluate(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return{height:r.height,width:r.width,bg:s.backgroundColor,border:s.borderTopColor,radius:parseFloat(s.borderTopLeftRadius),fontSize:parseFloat(s.fontSize),line:parseFloat(s.lineHeight),color:s.color,image:s.backgroundImage};});
  if(['button','input','dropdown','combo','banner'].includes(key)){
   const n=content(v);checks(row,'background',paint(n,'fills')?rgb(paint(n,'fills')):null,actual.bg);checks(row,'border',paint(n,'strokes')?rgb(paint(n,'strokes')):null,actual.border);checks(row,'height',n.height,actual.height);checks(row,'radius',typeof n.cornerRadius==='number'?n.cornerRadius:0,actual.radius);
   const text=v.nodes.find(n=>n.type==='TEXT'&&paint(n,'fills'));if(text){checks(row,'type size',text.fontSize,actual.fontSize);checks(row,'text paint',rgb(paint(text,'fills')),actual.color);}
   // Reproduce the focus+hover combination that used to override the source ring.
   if(focused&&!disabled){await target.hover();const border=await target.evaluate(n=>getComputedStyle(n).borderTopColor);checks(row,'focus while hovered',paint(n,'strokes')?rgb(paint(n,'strokes')):actual.border,border);}
  }else if(['checkbox','radio'].includes(key)){
   if(focused)await page.keyboard.press('Tab').then(()=>target.focus());
   const image=await target.evaluate(n=>getComputedStyle(n.nextElementSibling).backgroundImage);
   const state=disabled?'disabled':invalid?'invalid':focused?'focused':hovered?'hovered':'active';checks(row,'source paint asset',true,image.includes(`${key}-${theme}-${value}-${state}.svg`));checks(row,'logical width',16,actual.width);checks(row,'logical height',16,actual.height);
  }else if(key==='toggle'){
   checks(row,'width',v.width,actual.width);checks(row,'height',v.height,actual.height);checks(row,'source paint asset',true,actual.image.includes(`toggle-${theme}-${v.name.endsWith('State=On')?'on':'off'}.svg`));
  }else if(key==='link'){
   const n=v.nodes.find(n=>n.type==='TEXT');checks(row,'text paint',rgb(paint(n,'fills')),actual.color);checks(row,'type size',n.fontSize,actual.fontSize);
   if(disabled)checks(row,'disabled navigation blocked',true,await target.evaluate(n=>!n.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}))));
   if(/Type=(External|Dropdown)/.test(v.name))checks(row,'source suffix glyph',true,await target.evaluate(n=>getComputedStyle(n,'::after').backgroundImage!=='none'));
  }
  if(pressed)await page.mouse.up();row.pass=row.checks.every(c=>c.pass);rows.push(row);
 }
 await page.evaluate(()=>document.getElementById('audit-state-fixture')?.remove());
 for(const key of ['editor-tooltip','got-it'])for(const v of sets[key]){
  const theme=v.name.includes('Theme=Dark')?'dark':'light',tone=v.name.match(/Type=(\w+)/)?.[1].toLowerCase(),arrow=v.name.match(/Arrow=(\w+)/)[1].toLowerCase();
  const trigger=page.locator(key==='editor-tooltip'?'[data-ui-tooltip=editor-tip]':'[data-ui-tooltip=got-it-tip]'),target=page.locator(key==='editor-tooltip'?'#editor-tip':'#got-it-tip');
  const row={component:key,sourceId:v.id,variant:v.name,checks:[]};
  await page.evaluate(({theme,key,tone,arrow})=>{document.documentElement.dataset.theme=theme;const n=document.getElementById(key==='editor-tooltip'?'editor-tip':'got-it-tip');n.hidePopover();if(tone)n.dataset.tone=tone;n.dataset.uiPlacement={up:'bottom',down:'top',left:'right',right:'left'}[arrow];document.activeElement?.blur();},{theme,key,tone,arrow});
  await trigger.focus();await target.waitFor({state:'visible'});const actual=await measureTip(target),n=v.nodes.find(n=>n.path.endsWith(key==='editor-tooltip'?'/Tooltip/Content':'/Body'));
  checks(row,'background',rgb(paint(n,'fills')),actual.bg);checks(row,'radius',n.cornerRadius,actual.radius);if(paint(n,'strokes'))checks(row,'border',rgb(paint(n,'strokes')),actual.border);checks(row,'source pointer asset',true,actual.arrow.includes('.svg'));checks(row,'inside viewport',true,actual.inBounds);
  row.pass=row.checks.every(c=>c.pass);rows.push(row);
 }
 const summary={sourceVariants:rows.length,checks:rows.flatMap(r=>r.checks).length,failed:rows.flatMap(r=>r.checks).filter(c=>!c.pass).length};
 writeFileSync(resolve(OUT,'states.json'),JSON.stringify({summary,rows,errors},null,2)+'\n');writeFileSync(resolve(OUT,'states.js'),'window.uiKitStateAudit='+JSON.stringify({summary,rows,errors})+';\n');
 for(const row of rows)for(const c of row.checks)if(!c.pass)console.log(`MISMATCH ${row.component} ${row.variant}: ${c.property}; expected ${c.expected}, got ${c.actual}`);
 console.log(JSON.stringify(summary));if(summary.failed||errors.length)process.exitCode=1;
}finally{await browser.close();}
async function measureTip(target){return target.evaluate(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return{bg:s.backgroundColor,border:s.borderTopColor,radius:parseFloat(s.borderTopLeftRadius),arrow:getComputedStyle(n,'::before').backgroundImage,inBounds:r.x>=0&&r.y>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1};});}
