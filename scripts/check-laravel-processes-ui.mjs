// Shipped managed-process controller and shared opaque bridge; no process starts.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:1000,height:700}}),errors=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:value=>window.sent.push(JSON.parse(value)),onMessage:callback=>listeners.push(callback)};window.receive=value=>listeners.forEach(callback=>callback(JSON.stringify(value)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);
 await page.waitForFunction(()=>window.sent.length);
 const initial=await page.evaluate(()=>window.sent.at(-1));
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const source=readFileSync('plugins/laravel/src/managed_processes.rs','utf8');
 const html=source.match(/fn html\(\)->String \{r#"([\s\S]*?)"#\.into\(\)\}/)?.[1];assert.ok(html,'use the shipped Rust HTML');
 const model={title:'Processes',activeView:'laravel-processes',viewTabs:config.group.tabs,toolbar:[],actions:[],body:{kind:'html',html}};
 await page.evaluate(({initial,model})=>window.receive({...initial,project:'/fixture',model}),{initial,model});
 const root=page.locator('.legacy');let consumed=0;
 async function next(op){await page.waitForFunction(({consumed,op})=>window.sent.filter(r=>r.op==='opaque-message').slice(consumed).some(r=>JSON.parse(r.message).op===op),{consumed,op});const all=await page.evaluate(()=>window.sent.filter(r=>r.op==='opaque-message').map(r=>JSON.parse(r.message)));const at=all.findIndex((r,i)=>i>=consumed&&r.op===op);assert.ok(at>=consumed);consumed=at+1;return all[at];}
 async function reply(value){await page.evaluate(({value,frameId})=>window.receive({channel:'ui-kit-view-message',targetView:'laravel-processes',frameId,message:JSON.stringify(value)}),{value,frameId:initial.frameId});}
 await next('state');await reply({root:'/fixture',jobs:[],sessionOnly:true});
 assert.equal(await root.locator('#process-start').isDisabled(),true);
 for(const [id,icon] of [['process-preview','eye'],['process-start','run'],['process-refresh','refresh']]){const svg=root.locator(`#${id} svg`);assert.equal(await svg.count(),1,`${id} has its shared native icon`);assert.equal(await svg.getAttribute('data-ui-icon-name'),icon);assert.equal(await svg.getAttribute('data-ui-icon-source'),'openidea-native');assert.equal(await svg.getAttribute('aria-hidden'),'true');assert.equal(await svg.getAttribute('width'),'16');}
 assert.equal(await root.locator('.process-actions').evaluate(node=>getComputedStyle(node).gap),await root.locator('.process-actions').evaluate(node=>getComputedStyle(node).getPropertyValue('--ui-space-2').trim()));
 await root.locator('#process-preview').click();let preview=await next('preview');assert.deepEqual(preview.options,{host:'127.0.0.1',port:8000});assert.equal(preview.runner,'php');
 // Editing while a preview is in flight must not enable a stale start.
 await root.locator('#process-host').fill('localhost');await reply({preview:{token:'old',program:'php',argv:['artisan','serve','--host=127.0.0.1','--port=8000']},jobs:[]});
 assert.equal(await root.locator('#process-start').isDisabled(),true);
 await root.locator('#process-preview').click();await next('preview');await reply({preview:{token:'current',program:'php',argv:['artisan','serve','--host=localhost','--port=8000']},jobs:[]});
 assert.equal(await root.locator('#process-start').isEnabled(),true);
 await root.locator('#process-start').click();const run=await next('run');assert.equal(run.token,'current');assert.ok(run.id.length>=16);
 const job={id:run.id,command:'serve',runner:'php',running:true,status:null,stdout:'<img src=x onerror=alert(1)>',stderr:'',stopped:false};
 await reply({jobs:[job]});assert.equal(await root.locator('#process-jobs img').count(),0);assert.match(await root.locator('#process-jobs pre').textContent(),/<img/);
 const stopIcon=root.getByRole('button',{name:'Stop serve',exact:true}).locator('svg');assert.equal(await stopIcon.count(),1,'dynamic stop action has icon');assert.equal(await stopIcon.getAttribute('data-ui-native-name'),'stop');assert.equal(await stopIcon.getAttribute('aria-hidden'),'true');
 await root.getByRole('button',{name:'Stop serve',exact:true}).click();assert.equal((await next('stop')).id,run.id);job.running=false;job.status=-1;job.stopped=true;await reply({jobs:[job]});
 assert.match(await root.locator('#process-jobs strong').textContent(),/Stopped/);assert.equal(await root.getByRole('button',{name:'Stop serve',exact:true}).count(),0);
 await root.locator('#process-command').selectOption('queue:work');assert.equal(await root.locator('#process-host').isVisible(),false);assert.equal(await root.locator('#process-connection').isVisible(),true);
 await root.locator('#process-connection').fill('redis');await root.locator('#process-queue').fill('high,default');await root.locator('#process-preview').click();preview=await next('preview');assert.deepEqual(preview.options,{connection:'redis',queue:'high,default'});await reply({error:'Permission denied'});assert.equal(await root.locator('#process-error').textContent(),'Permission denied');assert.equal(await root.locator('#process-start').isDisabled(),true);
 for(const width of [320,1000])for(const theme of ['light','dark']){await page.setViewportSize({width,height:700});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-processes-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: managed process preview, stale token, start/stop, escaping, queue options, permission error, themes and dock layouts`);
}finally{await browser.close();}
