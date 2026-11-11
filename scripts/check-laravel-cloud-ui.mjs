// Captured native Cloud model rendered through the shipped shared hub.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const model=JSON.parse(readFileSync(process.env.LARAVEL_CLOUD_UI_FIXTURE||'/tmp/laravel-cloud-ui-fixture-20261111.json','utf8'));
assert.equal(model.activeView,'laravel-external');assert.equal(model.body.backgroundPoll.intervalMs,5000);
assert.equal(JSON.stringify(model).includes('private-cloud-variable'),false);
for(const row of model.body.items)assert.equal(row.activationEnabled,false);
const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:1100,height:700}}),errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:text=>window.sent.push(JSON.parse(text)),onMessage:callback=>listeners.push(callback)};window.receive=message=>listeners.forEach(callback=>callback(JSON.stringify(message)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);await page.waitForFunction(()=>window.sent.length);
 async function answer(value){await page.evaluate(value=>{const request=window.sent.findLast(request=>request.channel==='ui-kit'&&request.op!=='notify'&&request.op!=='opaque-message');window.receive({...request,project:'/project',model:value});},value);}
 await answer(model);assert.match(await page.locator('tbody').textContent(),/API_KEY/);assert.match(await page.locator('tbody').textContent(),/••••••••/);assert.equal((await page.locator('body').innerText()).includes('private-cloud-variable'),false);
 const variable=model.body.items.find(row=>row.cells[1]==='API_KEY');
 const before=await page.evaluate(()=>window.sent.filter(request=>request.event?.kind==='activate').length);await page.locator(`[data-resource-key="${variable.key}"]`).dblclick();assert.equal(await page.evaluate(()=>window.sent.filter(request=>request.event?.kind==='activate').length),before,'captured Cloud rows cannot navigate as trusted source');await answer(model);
 await page.getByRole('button',{name:'Reveal variable…',exact:true}).click();let event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.key,'cloud-variable-reveal');
 const revealed=structuredClone(model);revealed.body.items.find(row=>row.key===variable.key).cells[2]='explicitly revealed fixture value';await answer(revealed);assert.match(await page.locator('tbody').textContent(),/explicitly revealed fixture value/);
 await page.getByRole('button',{name:'Refresh environment',exact:true}).click();event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.key,'cloud-selected-refresh');await answer(model);
 for(const width of [320,1100])for(const theme of ['light','dark']){await page.setViewportSize({width,height:700});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-cloud-${theme}-${width}.png`)});}}
 await page.waitForFunction(()=>window.sent.some(request=>request.op==='opaque-message'),{timeout:8000});let polls=await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message'));assert.equal(polls.length,1);let payload=JSON.parse(polls[0].message);assert.equal(payload.op,'cloud-tick');assert.equal(payload.scope,model.body.backgroundPoll.message.scope);assert.equal(typeof payload._backgroundPoll,'string');
 await page.waitForTimeout(5200);assert.equal(await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message').length),1,'only one private poll can be outstanding');
 await page.evaluate(({request,nonce})=>window.receive({channel:'ui-kit-view-message',targetView:'laravel-external',frameId:request.frameId,message:JSON.stringify({_backgroundPoll:nonce,pending:true})}),{request:polls[0],nonce:payload._backgroundPoll});
 await page.waitForFunction(()=>window.sent.filter(request=>request.op==='opaque-message').length===2,{timeout:6500});
 const next=structuredClone(model);next.activeView='laravel-about';next.title='About';next.body={kind:'message',text:'Cloud polling is inactive on this tab'};next.toolbar=[];
 await page.getByRole('tab',{name:/About/}).click();await answer(next);const stopped=await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message').length);await page.waitForTimeout(5200);assert.equal(await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message').length),stopped,'navigation stops the Cloud poll producer');
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: captured native Cloud masked/explicit values, read-only rows, actions, 5s single-flight correlated polling, navigation stop and dock layouts`);
}finally{await browser.close();}
