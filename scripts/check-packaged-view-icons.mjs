// Exercise shared source assets without mutating any packaged plugin copies.
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {cpSync,mkdtempSync,writeFileSync,rmSync,mkdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const scratch=mkdtempSync(join(tmpdir(),'packaged-view-icons-'));
cpSync(resolve(root,'crates/ui-kit/web'),scratch,{recursive:true});
writeFileSync(join(scratch,'legacy.js'),'window.uiKitLegacy={};');
const browser=await chromium.launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:360,height:500}}),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=s=>listeners.forEach(f=>f(s));});
 await page.goto(pathToFileURL(join(scratch,'hub.html')).href);
 const model={title:'Sources',plugin:'Fixture',toolbar:[
  {key:'refresh',label:'Refresh',icon:'refresh',enabled:true},
  {key:'add',label:'Add',icon:'plus',enabled:false},
  {key:'import',label:'Import',icon:'import',enabled:true},
  {key:'unknown',label:'Keep <name>',icon:'<svg onload=alert(1)>',enabled:true}
 ],body:{kind:'tree',items:[{key:'source',label:'Orders <script>bad()</script>',icon:'folder',detail:'Project source',expandable:false}]}};
 async function reply(){await page.evaluate(model=>{const request=window.sent.at(-1);window.receive(JSON.stringify({channel:'ui-kit',frameId:request.frameId,sequence:request.sequence,project:'/fixture',model}));},model);}
 await reply();
 const toolbar=page.locator('#toolbar');
 assert.equal(await toolbar.getByRole('button',{name:'Refresh',exact:true}).locator('svg').getAttribute('data-ui-icon-name'),'refresh');
 assert.equal(await toolbar.getByRole('button',{name:'Add',exact:true}).count(),0);
 assert.equal(await toolbar.getByRole('button',{name:'Import',exact:true}).locator('svg').getAttribute('data-ui-icon-name'),'import');
 assert.equal(await toolbar.getByRole('button',{name:'Keep <name>',exact:true}).locator('svg').count(),0);
 assert.equal(await page.locator('tbody .view-icon').getAttribute('data-ui-icon-name'),'folder');
 assert.equal(await toolbar.locator('svg:not([aria-hidden="true"])').count(),0);
 assert.equal(await page.locator('#content script').count(),0);
 for (const theme of ['light','dark']) {
  await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
  const refresh=toolbar.getByRole('button',{name:'Refresh',exact:true});
  await refresh.focus();
  assert.equal(await refresh.evaluate(node=>node===document.activeElement),true);
  assert.equal(await refresh.locator('svg').getAttribute('data-ui-icon-name'),'refresh');
  assert.equal(await refresh.getAttribute('aria-label'),null,'visible text remains the accessible action name');
  assert.equal(await toolbar.getByRole('button',{name:'Add',exact:true}).count(),0);
  if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:join(process.env.UI_QA_DIR,`packaged-icons-${theme}.png`)});}
 }
 await toolbar.getByRole('button',{name:'Refresh',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'refresh');
 assert.equal(await toolbar.getByRole('button',{name:'Refresh',exact:true}).isVisible(),true);
 assert.equal(await toolbar.getByRole('button',{name:'Refresh',exact:true}).isDisabled(),true);
 model.toolbar[1].enabled=true;
 await reply();
 assert.equal(await toolbar.getByRole('button',{name:'Add',exact:true}).locator('svg').getAttribute('data-ui-icon-name'),'plus');
 await page.locator('tbody tr').click();
 assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'source');
 await reply();
 const aliases=Object.fromEntries(['branch','external-link','github-pr-open','github-pr-merged','github-pr-closed','review-comment','file-log','revert','star'].map(name=>[name,name]));
 const mapped={...model,toolbar:Object.keys(aliases).map(icon=>({key:icon,label:icon,icon,enabled:true})),body:{kind:'tree',items:[{key:'root',label:'Project',detail:'Source group',icon:'folder-open',expandable:true,expanded:false}]}};
 const accept=async m=>page.evaluate(m=>{const r=window.sent.at(-1);window.receive(JSON.stringify({...r,project:'/fixture',model:m}));},m);
 await accept(mapped);
 for(const [name,target]of Object.entries(aliases))assert.equal(await toolbar.getByRole('button',{name,exact:true}).locator('svg').getAttribute('data-ui-icon-name'),target);
 assert.equal(await page.getByRole('button',{name:'Expand Project',exact:true}).locator('svg').getAttribute('data-ui-icon-name'),'chevron-right');
 await page.getByRole('button',{name:'Expand Project',exact:true}).click();assert.equal(await page.evaluate(()=>window.sent.at(-1).event.kind),'expand');
 await accept({...mapped,body:{kind:'tree',items:[{...mapped.body.items[0],expanded:true}]}});
 assert.equal(await page.getByRole('button',{name:'Collapse Project',exact:true}).locator('svg').getAttribute('data-ui-icon-name'),'chevron-down');
 await page.getByRole('button',{name:'Resource',exact:true}).click();
 assert.equal(await page.locator('th[aria-sort="ascending"] svg').getAttribute('data-ui-icon-name'),'arrow-12px-up');
 await page.getByRole('button',{name:'Resource',exact:true}).click();
 assert.equal(await page.locator('th[aria-sort="descending"] svg').getAttribute('data-ui-icon-name'),'arrow-12px-down');
 assert.equal((await page.locator('.tree-toggle').textContent()).trim(),'','tree affordances use SVG, not font glyphs');
 await accept({...model,toolbar:[],body:{kind:'resources',columns:['Resource'],facets:[],items:Array.from({length:101},(_,i)=>({key:`item:${i}`,cells:[`Item ${i}`]}))}});
 assert.equal(await toolbar.isVisible(),false,'empty toolbar has no reserved space');
 assert.equal(await page.locator('#clear').isVisible(),false,'no clear action without filters');
 assert.equal(await page.getByRole('button',{name:'Previous',exact:true}).count(),0);
 await page.getByRole('button',{name:'Next',exact:true}).click();
 assert.equal(await page.getByRole('button',{name:'Next',exact:true}).count(),0);
 assert.equal(await page.getByRole('button',{name:'Previous',exact:true}).isVisible(),true);
 await page.locator('#search').fill('Item 0');
 assert.equal(await page.locator('#clear').isVisible(),true);
 await page.locator('#clear').click();
 assert.equal(await page.locator('#clear').isVisible(),false);
 assert.deepEqual(errors,[]);
 console.log('PASS packaged icon metadata, native aliases, existing registry names, accessible labels, action availability/busy transitions, pager/filter availability, safe fallback, PR/navigation aliases, SVG tree/sort controls and event identity');
} finally {await browser.close();rmSync(scratch,{recursive:true,force:true});}
