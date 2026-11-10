// Actual standalone pages; editor/runtime transports are deterministic fixtures.
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const output=process.env.UI_QA_DIR||'/tmp/openidea-standalone-kit-panels';mkdirSync(output,{recursive:true});
const browser=await chromium.launch({headless:true});const errors=[],captures=[];
try {
 const runtime=await browser.newPage();runtime.on('pageerror',e=>errors.push(e.message));
 await runtime.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=s=>listeners.forEach(f=>f(s));});
 await runtime.goto(pathToFileURL(resolve(root,'plugins/spring-runtime/web/resources/runtime-controls.html')).href);
 await runtime.waitForFunction(()=>window.sent?.length>0);
 await runtime.evaluate(()=>new Promise(resolve=>{const observer=new IntersectionObserver(entries=>{if(entries.some(e=>e.isIntersecting)){observer.disconnect();resolve();}});observer.observe(document.documentElement);}));
 await runtime.evaluate(()=>{const r=window.sent.at(-1);window.receive({...r,model:{root:'/fixture',url:'http://localhost:8080/actuator',auth:'none',enabled:true,active:false,poll:'Stopped',status:'Ready'}});});
 assert.equal(await runtime.locator('link[href="./hub.css"]').count(),0,'standalone composition does not import the hub shell');
 assert.equal(await runtime.locator('.runtime-actions.ui-toolbar').count(),1);
 assert.equal(await runtime.locator('body > header').count(),0,'native tab supplies the title');
 assert.equal(await runtime.locator('#refresh svg').getAttribute('data-ui-icon-name'),'update');
 assert.equal(await runtime.locator('#start svg').getAttribute('data-ui-icon-name'),'play');
 assert.equal(await runtime.locator('#stop').isDisabled(),true);
 await runtime.locator('#refresh').click();assert.equal(await runtime.evaluate(()=>window.sent.at(-1).op),'refresh');
 assert.equal(await runtime.locator('#refresh').getAttribute('aria-busy'),'true');
 await runtime.evaluate(()=>{const r=window.sent.at(-1);window.receive({...r,model:{root:'/fixture',url:'http://localhost:8080/actuator',auth:'none',enabled:true,active:false,poll:'Stopped',status:'Ready'}});});

 const api=await browser.newPage();api.on('pageerror',e=>errors.push(e.message));
 await api.addInitScript(()=>{window.reloads=0;window.redocRuns=0;window.openidea={postMessage:()=>{},onMessage:()=>{},document:{onChange:()=>{},get:async()=>{window.reloads++;return{text:'{"openapi":"3.0.0","info":{"title":"Fixture API","version":"1"},"paths":{"/ping":{"get":{"summary":"Check status","responses":{"200":{"description":"OK"}}}}}}'};}}};});
 // Only Redoc's vendor renderer is stubbed: the actual page, switch driver and
 // generated container are exercised without workers or external requests.
 await api.route('**/vendor/redoc.standalone.js',route=>route.fulfill({contentType:'text/javascript',body:'window.Redoc={init:(_url,_options,root)=>{window.redocRuns++;root.innerHTML="<button>Vendor control</button><select><option>Vendor option</option></select>";}};'}));
 await api.goto(pathToFileURL(resolve(root,'plugins/openapi/web/index.html')).href);
 await api.locator('#swagger .info').waitFor();
 assert.equal(await api.locator('.api-toolbar.ui-toolbar').count(),1);
 assert.equal(await api.locator('.api-toolbar strong').count(),0,'editor already supplies the title');
 assert.equal(await api.locator('#swagger').getAttribute('data-ui-raw'),'');
 assert.equal(await api.locator('#swagger .ui-button,#swagger .ui-select-shell').count(),0,'kit does not decorate vendor controls');
 await api.locator('#reload svg').click();assert.equal(await api.evaluate(()=>window.reloads),2,'icon clicks preserve reload action');
 await api.locator('#switch svg').click();await api.waitForFunction(()=>window.redocRuns===1);
 assert.equal(await api.locator('#redoc').getAttribute('data-ui-raw'),'','replacement vendor root preserves raw scope');
 assert.equal(await api.locator('#redoc .ui-button,#redoc .ui-select-shell').count(),0);
 assert.equal(await api.locator('#switch svg').count(),1,'switching preserves its icon');
 assert.equal(await api.locator('#switch').getAttribute('aria-label'),'Switch API renderer to Swagger UI');
 await api.locator('#switch svg').click();assert.equal(await api.locator('#switch [data-renderer-label]').textContent(),'Redoc');
 for(const [name,page,toolbar] of [['runtime',runtime,'.runtime-actions'],['openapi',api,'.api-toolbar']])for(const theme of ['light','dark'])for(const [width,height]of [[280,400],[600,240]]){
  await page.setViewportSize({width,height});await page.locator('html').evaluate((n,t)=>n.dataset.theme=t,theme);
  await page.evaluate(()=>document.fonts.ready);
  const metrics=await page.evaluate(selector=>({width:innerWidth,docWidth:document.documentElement.scrollWidth,buttons:[...document.querySelectorAll(`${selector} button`)].map(n=>({name:n.getAttribute('aria-label')||n.textContent.trim(),left:n.getBoundingClientRect().left,right:n.getBoundingClientRect().right}))}),toolbar);
  assert(metrics.docWidth<=width,`${name} ${theme} ${width}: no page horizontal overflow`);
  assert(metrics.buttons.every(b=>b.left>=0&&b.right<=width),`${name} toolbar buttons stay visible`);
  if(name==='openapi')assert.equal(await page.locator(toolbar).evaluate(n=>getComputedStyle(n).filter),'none');
  const screenshot=resolve(output,`${name}-${theme}-${width}x${height}.png`);await page.screenshot({path:screenshot});captures.push({name,theme,width,height,screenshot,metrics});
 }
 assert.deepEqual(errors,[]);writeFileSync(resolve(output,'receipt.json'),JSON.stringify({scope:'Actual standalone shell and shared kit; mock editor/runtime transport, bundled Swagger, stubbed Redoc vendor content. No live HTTP/polling/container operations.',captures,pageErrors:errors},null,2));
 console.log('PASS standalone shared-kit toolbar/icons, raw vendor boundaries, SVG click actions, runtime scope driver and 8 light/dark compact layouts');
} finally {await browser.close();}
