// Exercise the real Runtime driver inside the grouped shared-kit hub.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const html=readFileSync('plugins/spring-runtime/src/controls.html','utf8');
const tabs=[{id:'runtime',label:'Overview'},{id:'runtime-snapshot',label:'Observations'},{id:'runtime-graph',label:'Bean Graph'},{id:'runtime-controls',label:'Refresh Controls'}];
for(const name of ['chromium','webkit']) {
 const browser=await engines[name].launch({headless:true});
 try {
  const page=await browser.newPage({viewport:{width:620,height:480}}),errors=[];page.setDefaultTimeout(8000);page.on('pageerror',error=>{errors.push(error.message);console.error(error.message);});
  await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:raw=>window.sent.push(JSON.parse(raw)),onMessage:callback=>listeners.push(callback)};window.receive=value=>listeners.forEach(callback=>callback(JSON.stringify(value)));});
  await page.goto(pathToFileURL(resolve('plugins/spring-runtime/web/resources/hub.html')).href);
  const model=activeView=>({title:activeView==='runtime-controls'?'Refresh Controls':'Overview',plugin:'Spring Runtime',activeView,viewTabs:tabs,toolbar:[],actions:[],body:activeView==='runtime-controls'?{kind:'html',html}:{kind:'message',text:'Cached overview'}});
  const render=async(activeView)=>page.evaluate(model=>{const request=window.sent.filter(request=>request.channel==='ui-kit'&&request.op!=='opaque-message').at(-1);window.receive({...request,project:'/fixture',model});},model(activeView));
  const privateRequest=async(op)=>page.evaluate(op=>{const request=window.sent.filter(request=>request.op==='opaque-message').map(request=>({outer:request,inner:JSON.parse(request.message)})).filter(request=>request.inner.op===op).at(-1);return request;},op);
  const reply=async(request,active)=>page.evaluate(({request,active})=>window.receive({channel:'ui-kit-view-message',targetView:'runtime-controls',frameId:request.outer.frameId,message:JSON.stringify({...request.inner,model:{root:'/fixture',url:'http://localhost:8080/actuator',auth:'none',active,enabled:true,poll:active?'Instance lease':'Stopped',status:'Ready',bounds:'Fixture request bounds'}})}),{request,active});
  await render('runtime-controls');
  await page.waitForFunction(()=>window.sent.some(request=>request.op==='opaque-message'&&JSON.parse(request.message).op==='hello'));
  await page.locator('.legacy').evaluate(element=>new Promise(resolve=>{const observer=new IntersectionObserver(entries=>{if(entries.some(entry=>entry.isIntersecting)){observer.disconnect();resolve();}});observer.observe(element);}));
  await reply(await privateRequest('hello'),false);
  const start=page.getByRole('button',{name:'Start bounded polling',exact:true});await start.waitFor({state:'visible'});await start.click();
  const pendingStart=await privateRequest('start');assert.ok(pendingStart);
  await page.getByRole('tab',{name:'Overview',exact:true}).click();
  const ordering=await page.evaluate(()=>window.sent.map(request=>request.op==='opaque-message'?JSON.parse(request.message).op:request.op));
  assert.ok(ordering.lastIndexOf('stop')>ordering.lastIndexOf('start'),'leaving with Start outstanding sends Stop');
  assert.ok(ordering.lastIndexOf('stop')<ordering.lastIndexOf('select-view'),'Stop queues before logical navigation');
  await render('runtime');
  const count=await page.evaluate(()=>window.sent.length);await reply(pendingStart,true);assert.equal(await page.evaluate(()=>window.sent.length),count,'retired Start reply cannot rearm the departed driver');
  await page.getByRole('tab',{name:'Refresh Controls',exact:true}).click();await render('runtime-controls');
  await page.waitForFunction(()=>window.sent.filter(request=>request.op==='opaque-message'&&JSON.parse(request.message).op==='hello').length>=2);
  await page.locator('.legacy').evaluate(element=>new Promise(resolve=>{const observer=new IntersectionObserver(entries=>{if(entries.some(entry=>entry.isIntersecting)){observer.disconnect();resolve();}});observer.observe(element);}));
  const hello=await privateRequest('hello');assert.notEqual(hello.inner.frame,pendingStart.inner.frame);await reply(hello,false);
  assert.equal(await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message'&&JSON.parse(request.message).op==='start').length),1,'returning to controls never starts a new lease');
  await start.click();const activeStart=await privateRequest('start');await reply(activeStart,true);
  await page.getByRole('button',{name:'Stop future polls',exact:true}).waitFor({state:'visible'});
  const beforeHide=await page.evaluate(()=>window.sent.filter(request=>request.op==='opaque-message'&&JSON.parse(request.message).op==='stop').length);
  await page.locator('.legacy').evaluate(element=>element.style.display='none');
  await page.waitForFunction(before=>window.sent.filter(request=>request.op==='opaque-message'&&JSON.parse(request.message).op==='stop').length>before,beforeHide);
  assert.deepEqual(errors,[]);console.log(`PASS ${name}: grouped Runtime private controls, pending-Start disposal, fresh frame on return and hidden-pane Stop`);
 } finally {await browser.close();}
}
