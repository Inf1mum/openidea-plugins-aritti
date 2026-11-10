// Five locale definitions of one key must remain searchable, filterable and navigable.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:1000,height:600}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);await page.waitForFunction(()=>window.sent.length);
 const key='enums.affiliate_category.diapers';
 const items=[['de','Windeln'],['en','Diapers'],['es','Pañales'],['fr','Couches'],['nl','Luiers']].map(([locale,value])=>({key:`translation-locale:${JSON.stringify([locale,key])}`,cells:[key,locale,value,`lang/${locale}/enums.php`],icon:"text"}));
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const model={title:'Translations',activeView:'laravel-translations',viewTabs:config.group.tabs.map(tab=>tab.id==='laravel-translations'?{...tab,count:5}:tab),toolbar:[],actions:[],contextButtons:[],body:{kind:'resources',columns:['Key','Locale','Value','File'],columnStyles:['code','locale','text','muted'],showRowIcons:true,searchPlaceholder:'Filter translations…',facetLabels:{1:'All locales'},facets:[1],items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 await answer();assert.equal(await page.locator('tbody .view-icon').count(),5);assert.equal(await page.locator('tbody td.resource-text').first().evaluate(node=>getComputedStyle(node).fontWeight),'400');assert.equal(await page.locator('tbody td.resource-muted').first().textContent(),'lang/de/enums.php');await page.locator('#search').fill('affiliate_category.diapers');assert.equal(await page.locator('tbody tr').count(),5);
 const select=page.locator('#facets select');assert.equal(await select.locator('option').count(),6);
 await select.selectOption('fr',{force:true});assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/Couches/);
 await page.locator('tbody tr').dblclick();let request=await page.evaluate(()=>window.sent.at(-1));assert.equal(request.event.key,items[3].key);assert.equal(request.targetView,'laravel-translations');await answer();
 await page.locator('#facets select').selectOption('',{force:true});assert.equal(await page.locator('tbody tr').count(),5);
 for(const width of [320,1000])for(const theme of ['light','dark']){await page.setViewportSize({width,height:600});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-translations-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: five locale rows, shared-key search, locale filter, precise row navigation and dock layouts`);
}finally{await browser.close();}
