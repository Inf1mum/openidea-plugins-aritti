// Error feedback belongs to the native bridge, never persistent panel chrome.
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage();
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve(root,'plugins/laravel/web/resources/hub.html')).href);
 const model={title:'Laravel',plugin:'Laravel',toolbar:[{key:'refresh',label:'Refresh',enabled:true,icon:'refresh'}],body:{kind:'message',text:'Project resources'}};
 const first=await page.evaluate(()=>window.sent.at(-1));await page.evaluate(({r,model})=>window.receive({...r,project:'/fixture',model}),{r:first,model});
 await page.getByRole('button',{name:'Refresh',exact:true}).click();const action=await page.evaluate(()=>window.sent.at(-1));
 const beforeDiagnostics=await page.evaluate(()=>window.sent.length);
 await page.evaluate(()=>{
   for(const message of ['ResizeObserver loop completed with undelivered notifications.','ResizeObserver loop limit exceeded']) {
     window.dispatchEvent(new ErrorEvent('error',{message,filename:'tauri://localhost',lineno:0,colno:0}));
     window.dispatchEvent(new ErrorEvent('error',{message,error:message}));
   }
 });
 assert.equal(await page.evaluate(()=>window.sent.length),beforeDiagnostics,'browser resize diagnostics do not become plugin notifications');
 await page.evaluate(()=>window.dispatchEvent(new ErrorEvent('error',{message:'ResizeObserver loop completed with undelivered notifications.',error:new Error('ResizeObserver loop completed with undelivered notifications.')})));
 assert.equal(await page.evaluate(()=>window.sent.at(-1).op),'notify','real exception with identical wording is still reported');
 await page.evaluate(()=>window.sent.pop());
 await page.evaluate(()=>window.dispatchEvent(new ErrorEvent('error',{message:'Panel rendering failed'})));
 const report=await page.evaluate(()=>window.sent.at(-1));assert.equal(report.op,'notify');assert.equal(report.sequence,0);assert.equal(report.message,'Panel rendering failed');assert.equal(report.warning,false);
 await page.evaluate(r=>window.receive({...r,notified:true}),report);
 assert.equal(await page.locator('#content').getAttribute('aria-busy'),'true','notification ack cannot complete an outstanding action');
 await page.evaluate(r=>window.receive({...r,error:'artisan optimize:clear failed'}),action);
 assert.equal(await page.locator('#content').getAttribute('aria-busy'),'false');
 assert.equal(await page.getByRole('button',{name:'Refresh',exact:true}).isEnabled(),true);
 assert.equal(await page.locator('#notice,.ui-alert').count(),0,'command errors never add a persistent banner');
 assert.equal(await page.locator('#content').textContent(),'Project resources','failure retains current content');
 assert.equal(await page.evaluate(()=>window.sent.filter(r=>r.op==='notify').length),1,'backend error is already notified; do not report it twice');
 console.log(`PASS ${engine} panel failures: no inline banner, native notification reports, no double notification, retained content, action recovery and isolated notification acknowledgements, exact browser resize diagnostics filtered, genuine exceptions preserved`);
}finally{await browser.close();}
