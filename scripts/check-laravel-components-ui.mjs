// Source-backed Components inventory through the shipped shared hub.
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
 const items=[['component:alert','<x-alert>','Blade class','app/View/Components/Alert.php','class'],['component:card','<x-card>','Blade anonymous','resources/views/components/card.blade.php','file-html'],['component:package-alert','<x-package-alert>','Blade alias','app/Providers/ComponentServiceProvider.php','file-html'],['livewire:counter','<livewire:counter>','Livewire','app/Livewire/Counter.php','class']].map(([key,tag,type,file,icon])=>({key,cells:[tag,type,file],icon}));
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const model={title:'Components',activeView:'laravel-components',viewTabs:config.group.tabs,toolbar:[],actions:[],contextButtons:[],body:{kind:'resources',columns:['Component','Type','File'],columnStyles:['code','text','muted'],columnWeights:[3,1.5,5],showRowIcons:true,searchPlaceholder:'Filter components…',facetLabels:{1:'All types'},facets:[1],items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 await answer();assert.deepEqual(await page.locator('thead th').allTextContents(),['Component','Type','File']);assert.equal(await page.locator('tbody .view-icon').count(),4);
 await page.locator('#facets select').selectOption('Livewire',{force:true});assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/<livewire:counter>/);
 await page.locator('tbody tr').dblclick();const request=await page.evaluate(()=>window.sent.at(-1));assert.equal(request.event.key,'livewire:counter');assert.equal(request.targetView,'laravel-components');await answer();
 await page.locator('#facets select').selectOption('Blade anonymous',{force:true});assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/<x-card>/);
 await page.locator('#facets select').selectOption('',{force:true});await page.locator('#search').fill('ComponentServiceProvider');assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/<x-package-alert>/);
 await page.locator('#search').fill('');await page.getByRole('button',{name:'Component',exact:true}).click();await page.getByRole('button',{name:'Component',exact:true}).click();assert.match(await page.locator('tbody tr').first().textContent(),/<x-package-alert>/);
 for(const width of [320,1100])for(const theme of ['light','dark']){await page.setViewportSize({width,height:600});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-components-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Components columns, icons, type filters, source navigation, search, sorting and dock layouts`);
}finally{await browser.close();}
