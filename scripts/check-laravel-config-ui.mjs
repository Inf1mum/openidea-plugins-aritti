// Config expressions must remain visible, searchable, filterable and source-navigable.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:1000,height:600}}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);
 await page.waitForFunction(()=>window.sent.length);
 const items=['fr','en','es','de','nl'].map(locale=>({key:`config:affiliate.associate_tags.${locale}`,cells:[`affiliate.associate_tags.${locale}`,`env('AMAZON_TAG_${locale.toUpperCase()}')`,'config/affiliate.php'],icon:'file-config'}));
 items.push({key:'config:app.name',cells:['app.name',"env('APP_NAME', 'Laravel')",'config/app.php'],icon:'file-config'});
 const longValue="env('SANCTUM_STATEFUL_DOMAINS', sprintf('%s%s', 'localhost,localhost:3000,127.0.0.1,127.0.0.1:8000,::1', Sanctum::currentApplicationUrlWithPort()))";
 for(let i=0;i<120;i++)items.push({key:`config:stress.field.${i}`,cells:[`stress.field.${i}`,longValue,`config/long-framework-setting-${i%37}.php`],icon:'file-config'});
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const model={title:'Config',activeView:'laravel-config',viewTabs:config.group.tabs,toolbar:[],actions:[],contextButtons:[],body:{kind:'resources',columns:['Key','Value','File'],columnStyles:['code','code','muted'],showRowIcons:true,searchPlaceholder:'Filter config…',facetLabels:{2:'All files'},facets:[2],items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 await answer();
 assert.deepEqual(await page.locator('thead th').allTextContents(),['Key','Value','File']);
 await page.locator('#search').fill('associate_tags');assert.equal(await page.locator('tbody tr').count(),5);
 assert.match(await page.locator('tbody').textContent(),/env\('AMAZON_TAG_FR'\)/);
 assert.equal(await page.locator('tbody td.resource-muted').first().textContent(),'config/affiliate.php');
 await page.locator('tbody tr').first().dblclick();
 const request=await page.evaluate(()=>window.sent.at(-1));assert.equal(request.event.key,items[0].key);assert.equal(request.targetView,'laravel-config');await answer();
 await page.locator('#search').fill('');await page.locator('#facets select').selectOption('config/app.php',{force:true});
 assert.equal(await page.locator('tbody tr').count(),1);assert.match(await page.locator('tbody').textContent(),/APP_NAME/);
 await page.locator('#facets select').selectOption('',{force:true});await page.locator('#search').fill('AMAZON_TAG_DE');assert.equal(await page.locator('tbody tr').count(),1);
 for(const width of [320,1000])for(const theme of ['light','dark']){await page.setViewportSize({width,height:600});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
 await page.locator('#search').fill('stress.field');
 const bounds=await page.locator('tbody tr').first().evaluate(row=>{const [key,value,file]=row.cells;const label=value.querySelector('.resource-cell-value');return {valueRight:label.getBoundingClientRect().right,fileLeft:file.getBoundingClientRect().left,overflow:getComputedStyle(label).overflow,tableLayout:getComputedStyle(row.closest('table')).tableLayout,title:value.title,rowHeight:row.getBoundingClientRect().height,labelHeight:label.getBoundingClientRect().height};});
 assert.ok(bounds.valueRight<=bounds.fileLeft+1);assert.equal(bounds.overflow,'hidden');assert.equal(bounds.tableLayout,'fixed');assert.equal(bounds.title,longValue);assert.ok(bounds.rowHeight>=bounds.labelHeight+6);
 const trigger=page.locator('#facets .ui-select-trigger');await trigger.click();
 const popup=page.getByRole('listbox');await popup.getByRole('option').last().scrollIntoViewIfNeeded();
 const geometry=await popup.evaluate(node=>{const box=node.getBoundingClientRect();return {left:box.left,right:box.right,width:box.width,viewport:innerWidth,rows:[...node.querySelectorAll('[role=option]')].map(row=>({height:row.getBoundingClientRect().height,whiteSpace:getComputedStyle(row).whiteSpace,title:row.title,text:row.textContent}))};});
 assert.ok(geometry.left>=0&&geometry.right<=geometry.viewport+1);assert.ok(geometry.width>180,'popup should fit long file names instead of the trigger width');
 assert.ok(geometry.rows.every(row=>row.height===24&&row.whiteSpace==='nowrap'&&row.title===row.text));
 if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-config-dropdown-${theme}-${width}.png`)});}
 await page.keyboard.press('Escape');
 await page.locator('#content').evaluate(node=>node.scrollTop=600);assert.ok(await page.locator('thead').isVisible());
 if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-config-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Config source values, long-value cell clipping, 37-file popup geometry, file filter, source navigation and dock layouts`);
}finally{await browser.close();}
