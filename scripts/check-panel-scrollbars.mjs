const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:360,height:260}});
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/kube-explorer/web/resources/hub.html')).href);
 await page.evaluate(()=>{const r=window.sent.at(-1);window.receive({...r,project:'/fixture',model:{title:'Resources',toolbar:[],body:{kind:'tree',items:Array.from({length:80},(_,i)=>({key:`item:${i}`,label:`Resource ${i}`}))}}});});
 for(const theme of ['light','dark']) {
  await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
  const metrics=await page.evaluate(()=>{
   const content=document.querySelector('#content'), reference=document.createElement('div');reference.className='ui-scroll-area';document.body.append(reference);
   const sample=node=>({width:getComputedStyle(node,'::-webkit-scrollbar').width,thumb:getComputedStyle(node,'::-webkit-scrollbar-thumb').backgroundColor,track:getComputedStyle(node,'::-webkit-scrollbar-track').backgroundColor});
   const result={scrolls:content.scrollHeight>content.clientHeight,content:sample(content),reference:sample(reference)};reference.remove();return result;
  });
  assert.equal(metrics.scrolls,true);assert.deepEqual(metrics.content,metrics.reference);
  assert.equal(metrics.content.width,'10px');assert.equal(metrics.content.track,'rgba(0, 0, 0, 0)');assert.notEqual(metrics.content.thumb,'rgba(0, 0, 0, 0)');
  await page.locator('#content').evaluate(node=>node.scrollTop=node.scrollHeight);
  assert.ok(await page.locator('#content').evaluate(node=>node.scrollTop>0));
 }
 console.log(`PASS ${engine}: panel scrollbars match the UI kit at 10px with themed thumbs, transparent tracks and functional scrolling in light/dark modes`);
} finally {await browser.close();}
