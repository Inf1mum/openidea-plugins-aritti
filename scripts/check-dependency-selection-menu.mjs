const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage();
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/spring-initializr/web/resources/hub.html')).href);
 const model={title:'Dependencies',toolbar:[],actions:[],body:{kind:'tree',items:[{key:'dep:web',label:'Spring Web',activationLabel:'Select',activationIcon:'add'},{key:'sel:security',label:'Spring Security',activationLabel:'Deselect',activationIcon:'remove'}]}};
 const reply=()=>page.evaluate(model=>{const r=window.sent.at(-1);window.receive({...r,project:'/fixture',model});},model);
 await reply();
 for(const [key,label]of [['dep:web','Select'],['sel:security','Deselect']]) {
  await page.locator(`tr[data-resource-key="${key}"]`).click({button:'right'});
  assert.equal(await page.getByRole('menuitem',{name:'Open',exact:true}).count(),0);
  await page.getByRole('menuitem',{name:label,exact:true}).click();
  assert.deepEqual(await page.evaluate(()=>window.sent.at(-1).event),{kind:'activate',key});await reply();
 }
 console.log(`PASS ${engine}: dependency menus say Select/Deselect and preserve the toggle action identity`);
} finally {await browser.close();}
