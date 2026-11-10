// Source-backed Views inventory through the shipped shared hub.
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
 const items=[['app','resources/views/app.blade.php','(root)','app'],['pdf.layout','resources/views/pdf/layout.blade.php','pdf','app'],['pdf.partials.line-chart','resources/views/pdf/partials/line-chart.blade.php','pdf.partials','app'],['mail::html.header','resources/views/vendor/mail/html/header.blade.php','vendor.mail.html','mail'],['shop::catalog.item','modules/shop/views/catalog/item.blade.php','catalog','shop']].map(cells=>({key:`view:${cells[0]}`,cells,icon:'file-html'}));
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const model={title:'Views',activeView:'laravel-views',viewTabs:config.group.tabs,toolbar:[],actions:[],contextButtons:[],body:{kind:'resources',columns:['View','File','Folder','Namespace'],columnStyles:['code','code','muted','locale'],columnWeights:[3,4,2,1.5],showRowIcons:true,searchPlaceholder:'Filter views…',facetLabels:{2:'All folders',3:'All namespaces'},facets:[2,3],items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 await answer();assert.deepEqual(await page.locator('thead th').allTextContents(),['View','File','Folder','Namespace']);assert.equal(await page.locator('tbody .view-icon').count(),5);
 await page.locator('#facets select').nth(0).selectOption('pdf',{force:true});assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/pdf.layout/);
 await page.locator('#facets select').nth(0).selectOption('',{force:true});await page.locator('#facets select').nth(1).selectOption('mail',{force:true});assert.equal(await page.locator('tbody tr').count(),1);
 await page.locator('tbody tr').dblclick();const request=await page.evaluate(()=>window.sent.at(-1));assert.equal(request.event.key,'view:mail::html.header');assert.equal(request.targetView,'laravel-views');await answer();
 await page.locator('#facets select').nth(1).selectOption('',{force:true});await page.locator('#search').fill('modules/shop');assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/shop::catalog.item/);
 await page.locator('#search').fill('');await page.getByRole('button',{name:'View',exact:true}).click();await page.getByRole('button',{name:'View',exact:true}).click();assert.match(await page.locator('tbody tr').first().textContent(),/shop::catalog.item/);
 for(const width of [320,1100])for(const theme of ['light','dark']){await page.setViewportSize({width,height:600});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-views-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Views columns, icons, folder/namespace filters, source navigation, search, sorting and dock layouts`);
}finally{await browser.close();}
