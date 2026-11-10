const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const output='/tmp/openidea-row-context-menu';mkdirSync(output,{recursive:true});
const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:720,height:420}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve(root,'plugins/openapi/web/resources/hub.html')).href);
 const model={title:'OpenAPI Endpoints',plugin:'OpenAPI',toolbar:[],actions:[{id:'unavailable',label:'Unavailable action',kind:'route',enabled:false},{id:'curl',label:'Copy as curl',kind:'route'},{id:'specific',label:'First route only',kind:'route',keys:['route:1']},{id:'other',label:'Other kind action',kind:'other'}],body:{kind:'resources',columns:['Method','Path'],facets:[],items:[{key:'route:1',cells:['GET','/users'],details:[{label:'Description',value:'<img src=x onerror=alert(1)>'}]},{key:'route:2',cells:['DELETE','/users/{id}']}]}};
 const reply=async()=>page.evaluate(model=>{const r=window.sent.findLast(r=>r.op!=='notify');window.receive({...r,project:'/fixture',model});},model);
 await reply();
 const policy=await page.evaluate(()=>{
  const results=[];let propagated=false;
  document.body.addEventListener('contextmenu',()=>propagated=true,{once:true});
  for(const target of [document.body,document.querySelector('#search')]){
   const event=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,composed:true});target.dispatchEvent(event);results.push(event.defaultPrevented);
  }
  const raw=document.createElement('div');raw.dataset.uiRaw='';document.body.append(raw);const shadow=raw.attachShadow({mode:'open'});const input=document.createElement('input');shadow.append(input);
  const rawEvent=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,composed:true});input.dispatchEvent(rawEvent);results.push(rawEvent.defaultPrevented);raw.remove();
  for(const key of [{key:'ContextMenu'},{key:'F10',shiftKey:true}]){const event=new KeyboardEvent('keydown',{...key,bubbles:true,cancelable:true});document.querySelector('#search').dispatchEvent(event);results.push(event.defaultPrevented);}
  const copy=new KeyboardEvent('keydown',{key:'c',metaKey:true,bubbles:true,cancelable:true});document.querySelector('#search').dispatchEvent(copy);
  document.body.classList.remove('ui-embedded-panel');const standalone=new MouseEvent('contextmenu',{bubbles:true,cancelable:true});document.body.dispatchEvent(standalone);document.body.classList.add('ui-embedded-panel');
  return {results,propagated,copyPrevented:copy.defaultPrevented,standalonePrevented:standalone.defaultPrevented};
 });
 assert.deepEqual(policy.results,[true,true,true,true,true],'browser menus are suppressed on background, fields, raw shadow content and keyboard gestures');
 assert.equal(policy.propagated,true,'custom handlers still receive the event');assert.equal(policy.copyPrevented,false,'clipboard shortcuts remain available');assert.equal(policy.standalonePrevented,false,'policy is scoped to embedded plugin panels');
 const rows=page.locator('tbody tr');
 await rows.nth(0).click();await reply();assert.equal(await page.locator('aside').count(),0);assert.equal(await page.getByRole('dialog').count(),0);
 await rows.nth(1).click({button:'right'});const menu=page.getByRole('menu');await menu.waitFor();
 assert.equal(await rows.nth(1).getAttribute('aria-selected'),'true','right click selects its own row');
 assert.equal(await menu.getByRole('menuitem',{name:'Other kind action',exact:true}).count(),0);
 assert.equal(await menu.getByRole('menuitem',{name:'Unavailable action',exact:true}).count(),0);
 assert.equal(await menu.getByRole('menuitem',{name:'First route only',exact:true}).count(),0);
 await page.screenshot({path:resolve(output,`${engine}-endpoint-menu.png`)});
 await menu.getByRole('menuitem',{name:'Copy as curl',exact:true}).click();assert.deepEqual(await page.evaluate(()=>{const r=window.sent.at(-1);return{op:r.op,id:r.id,key:r.key};}),{op:'command',id:'curl',key:'route:2'});await reply();
 await rows.nth(0).focus();await page.keyboard.press('Shift+F10');await menu.waitFor();await page.keyboard.press('Escape');await menu.waitFor({state:'hidden'});await page.waitForFunction(()=>document.activeElement?.matches('tr[aria-selected=true]'));
 assert.equal(await rows.nth(0).evaluate(n=>n===document.activeElement),true,'Escape returns keyboard focus to the row');
 await rows.nth(0).click({button:'right'});assert.equal(await menu.getByRole('menuitem',{name:'First route only',exact:true}).count(),1);await menu.getByRole('menuitem',{name:'Show Details…',exact:true}).click();
 const dialog=page.getByRole('dialog');await dialog.waitFor();assert.equal(await dialog.locator('dd').last().textContent(),'<img src=x onerror=alert(1)>');assert.equal(await dialog.locator('img').count(),0);
 await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});await page.waitForFunction(()=>document.activeElement?.matches('tr[aria-selected=true]'));assert.equal(await rows.nth(0).evaluate(n=>n===document.activeElement),true,'closing details restores row focus');
 await page.setViewportSize({width:320,height:260});await rows.nth(1).click({button:'right'});await menu.waitFor();
 const box=await menu.boundingBox();assert.ok(box.x>=0&&box.y>=0&&box.x+box.width<=320&&box.y+box.height<=260,'context menu stays inside a narrow dock');await page.keyboard.press('Escape');
 await rows.nth(1).press('Enter');assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'route:2');assert.equal(await page.evaluate(()=>window.sent.at(-1).event.kind),'activate');
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: no selection sidebar, right-click row actions, correct command key, filtered menu actions, Shift+F10/Escape, details on demand, safe text, restored focus and narrow-dock placement`);
}finally{await browser.close();}
