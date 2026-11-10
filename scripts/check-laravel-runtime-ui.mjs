// Runtime snapshot table through the shipped shared hub.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:1100,height:600}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);await page.waitForFunction(()=>window.sent.length);
 const items=[['Application','environment.php_version','8.4'],['Events','App\\Events\\Created','App\\Listeners\\Notify'],['Schedule','inspire','* * * * * · tomorrow · UTC'],['Failed Jobs','uuid-1','Job · redis / default · today'],['Events','<img src=x onerror=alert(1)>','Untrusted application label']].map((cells,index)=>({key:`runtime:fixture:${index}`,cells,activationEnabled:false}));
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const model={title:'Runtime',activeView:'laravel-runtime',viewTabs:config.group.tabs,toolbar:[{key:'runtime-about',label:'Application',icon:'info',enabled:true},{key:'runtime-events',label:'Events',icon:'text',enabled:true},{key:'runtime-schedule',label:'Schedule',icon:'calendar',enabled:true},{key:'runtime-queues',label:'Failed Jobs',icon:'warning',enabled:true},{key:'runtime-queue-status',label:'Queue Status',icon:'services',enabled:true},{key:'runtime-workers',label:'Workers',icon:'services',enabled:true},{key:'runtime-logs',label:'Logs',icon:'text',enabled:true},{key:'runtime-runner',label:'Local PHP',icon:'terminal',enabled:true}],actions:[],contextButtons:config.context_buttons['laravel-runtime'],body:{kind:'resources',columns:['Section','Resource','Details'],columnStyles:['text','code','muted'],columnWeights:[1,3,4],searchPlaceholder:'Filter runtime information…',facetLabels:{0:'All sections'},facets:[0],items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 await answer();
 for(const [key,label] of [['runtime-about','Application'],['runtime-events','Events'],['runtime-schedule','Schedule'],['runtime-queues','Failed Jobs']]){await page.getByRole('button',{name:label,exact:true}).click();const request=await page.evaluate(()=>window.sent.at(-1));assert.equal(request.event.key,key);await answer();}
 assert.deepEqual(await page.locator('thead th').allTextContents(),['Section','Resource','Details']);assert.equal(await page.locator('tbody tr').count(),5);assert.equal(await page.locator('tbody img').count(),0);
 await page.locator('#facets select').selectOption('Events',{force:true});assert.equal(await page.locator('tbody tr').count(),2);
 const before=await page.evaluate(()=>window.sent.filter(request=>request.event?.kind==='activate').length);await page.locator('tbody tr').first().dblclick();assert.equal(await page.evaluate(()=>window.sent.filter(request=>request.event?.kind==='activate').length),before,'runtime strings cannot become source destinations');await answer();
 await page.locator('#facets select').selectOption('',{force:true});await page.locator('#search').fill('redis');assert.equal(await page.locator('tbody tr').count(),1);
 await page.locator('#search').fill('');await page.getByRole('button',{name:'Resource',exact:true}).click();assert.equal(await page.locator('tbody tr').count(),5);
 for(const width of [320,1100])for(const theme of ['light','dark']){await page.setViewportSize({width,height:600});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-runtime-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Runtime columns, section filter, output escaping, non-activatable rows, search, sorting and dock layouts`);
}finally{await browser.close();}
