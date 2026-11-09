const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url)),QA='/tmp/openidea-plugin-ui-qa';mkdirSync(QA,{recursive:true});
const source=JSON.parse(readFileSync(resolve(ROOT,'crates/ui-kit/vendor/jetbrains-icons/SOURCE.json'),'utf8'));
const browser=await chromium.launch();
try{
 const page=await browser.newPage({viewport:{width:1100,height:850}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/icons.html')).href);
 assert.deepEqual(await page.evaluate(()=>uiKitIcons.names),Object.keys(source.files).map(n=>n.slice(0,-4)));
 assert.equal(await page.evaluate(()=>uiKitIcons.source.revision),source.revision);
 const failures=await page.evaluate(()=>{
  const root=document.createElement('div');root.style.cssText='position:absolute;left:-10000px;display:flex;flex-wrap:wrap;width:1000px';document.body.append(root);const failures=[];
  for(const name of uiKitIcons.names){const svg=uiKitIcons.create(name),meta=uiKitIcons.get(name);root.append(svg);const r=svg.getBoundingClientRect();if(svg.namespaceURI!=='http://www.w3.org/2000/svg'||r.width!==meta.width||r.height!==meta.height||svg.getAttribute('aria-hidden')!=='true')failures.push(name);}
  const ids=[...root.querySelectorAll('[id]')].map(n=>n.id);if(new Set(ids).size!==ids.length)failures.push('duplicate SVG definition IDs');root.remove();return failures;
 });assert.deepEqual(failures,[],'every source icon renders at its native dimensions');
 assert.equal(await page.evaluate(()=>{try{uiKitIcons.create('__proto__');return false}catch(e){return e instanceof RangeError}}),true);
 await page.getByRole('searchbox',{name:'Find an icon',exact:true}).fill('file-java');assert.ok(await page.locator('.icon-card').count()>0);await page.getByRole('button',{name:'file-java',exact:true}).click();assert.match(await page.locator('#selected-usage').textContent(),/data-ui-icon="file-java"/);
 await page.getByRole('combobox',{name:'Source size',exact:true}).click();await page.getByRole('option',{name:'20 px',exact:true}).click();assert.equal(await page.locator('.icon-card svg').evaluateAll(ns=>ns.every(n=>parseFloat(n.getAttribute('width'))===20)),true);
 await page.getByRole('searchbox',{name:'Find an icon',exact:true}).fill('does-not-exist');assert.equal(await page.locator('#icon-empty').isVisible(),true);
 await page.getByRole('searchbox',{name:'Find an icon',exact:true}).fill('');await page.getByRole('combobox',{name:'Source size',exact:true}).click();await page.getByRole('option',{name:'All sizes',exact:true}).click();await page.getByRole('button',{name:'Next',exact:true}).click();assert.match(await page.locator('#icon-page').textContent(),/Page 2/);await page.getByRole('button',{name:'Previous',exact:true}).click();
 for(const theme of ['light','dark']){
  await page.getByRole('combobox',{name:'Appearance',exact:true}).click();await page.getByRole('option',{name:theme==='light'?'Light':'Dark',exact:true}).click();
  const add=page.getByRole('button',{name:'add',exact:true});await add.hover();assert.equal(await add.locator('path').first().evaluate(n=>getComputedStyle(n).fill),theme==='light'?'rgb(108, 112, 126)':'rgb(206, 208, 214)');await page.screenshot({path:resolve(QA,`jetbrains-icons-${theme}.png`),fullPage:true});
 }
 await page.evaluate(()=>{const host=document.createElement('div');host.id='icon-shadow-fixture';document.body.append(host);const shadow=host.attachShadow({mode:'open'});shadow.innerHTML='<button aria-label="Copy resource"><span data-ui-icon="copy"></span></button>';uiKitControls.enhance(shadow);const meaningful=uiKitIcons.create('file-java',{label:'Java file'});shadow.append(meaningful);});
 assert.equal(await page.getByRole('button',{name:'Copy resource',exact:true}).locator('svg').count(),1);assert.equal(await page.getByRole('img',{name:'Java file',exact:true}).count(),1);
 await page.evaluate(()=>{const n=document.createElement('span');n.id='dynamic-icon';n.dataset.uiIcon='add-20px';document.body.append(n);});await page.waitForFunction(()=>document.querySelector('#dynamic-icon svg')!=null);assert.equal(await page.locator('#dynamic-icon svg').getAttribute('width'),'20');await page.evaluate(()=>{uiKitControls.enhance(document);uiKitControls.enhance(document);});assert.equal(await page.locator('#dynamic-icon svg').count(),1);
 for(const width of [320,375,768]){await page.setViewportSize({width,height:800});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`${width} icon browser overflow`);}
 assert.deepEqual(errors,[]);console.log(`PASS ${Object.keys(source.files).length} JetBrains icons: complete source inventory, native geometry, unique SVG IDs, inherited light/dark color, browser search/size/paging, accessibility, shadow roots, dynamic controls, narrow layout`);
}finally{await browser.close();}
