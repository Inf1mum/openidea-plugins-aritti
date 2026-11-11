import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const intervalMs=process.argv.includes('--fast')?100:1000;
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage();
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:text=>window.sent.push(JSON.parse(text)),onMessage:f=>listeners.push(f)};window.receive=value=>listeners.forEach(f=>f(JSON.stringify(value)));window.testHidden=false;Object.defineProperty(document,'hidden',{get:()=>window.testHidden});});
 await page.goto(pathToFileURL(resolve('crates/ui-kit/web/hub.html')).href);
 await page.waitForFunction(()=>window.sent.length);
 const initial=await page.evaluate(()=>window.sent.at(-1));
 const model={title:'Fixture',activeView:'models',viewTabs:[{id:'models',label:'Models'},{id:'other',label:'Other'}],toolbar:[],actions:[],body:{kind:'tree',items:[],backgroundPoll:{intervalMs,message:{op:'tick'}}}};
 await page.evaluate(({initial,model})=>window.receive({...initial,project:'/fixture',model}),{initial,model});
 const polls=()=>page.evaluate(()=>window.sent.filter(m=>m.op==='opaque-message'));
 await page.waitForFunction(()=>window.sent.some(m=>m.op==='opaque-message'));
 let requests=await polls();assert.equal(requests.length,1);let token=JSON.parse(requests[0].message)._backgroundPoll;
 await page.waitForTimeout(1250);assert.equal((await polls()).length,1,'single-flight backpressure');
 await page.evaluate(()=>{const clock=performance.now.bind(performance);Object.defineProperty(performance,'now',{value:()=>clock()+60000});});
 await page.waitForFunction(()=>window.sent.filter(m=>m.op==='opaque-message').length===2);
 const timedOutToken=token;token=JSON.parse((await polls())[1].message)._backgroundPoll;assert.notEqual(token,timedOutToken,'missing acknowledgment permits a fresh correlated tick');
 await page.evaluate(({initial,token})=>window.receive({channel:'ui-kit-view-message',targetView:'models',frameId:initial.frameId,message:JSON.stringify({_backgroundPoll:token})}),{initial,token:timedOutToken});

 await page.evaluate(({initial,token})=>window.receive({channel:'ui-kit-view-message',targetView:'models',frameId:'foreign',message:JSON.stringify({_backgroundPoll:token})}),{initial,token});
 await page.waitForTimeout(1100);assert.equal((await polls()).length,2,'foreign-frame ack is ignored');
 await page.evaluate(({initial,token})=>{window.testHidden=true;window.receive({channel:'ui-kit-view-message',targetView:'models',frameId:initial.frameId,message:JSON.stringify({_backgroundPoll:token})});},{initial,token});
 await page.waitForTimeout(1100);assert.equal((await polls()).length,2,'hidden view does no work');
 await page.evaluate(()=>window.testHidden=false);await page.waitForFunction(()=>window.sent.filter(m=>m.op==='opaque-message').length===3);
 await page.locator('[data-view-key="other"]').click();
 await page.waitForTimeout(1200);assert.equal((await polls()).length,3,'navigation cancels old polling before response');
 const next=await page.evaluate(()=>window.sent.at(-1));
 await page.evaluate(({next,model})=>window.receive({...next,project:'/another',model:{...model,activeView:'other',body:{kind:'message',text:'Other'}}}),{next,model});
 await page.waitForTimeout(1200);assert.equal((await polls()).length,3,'non-polling view stays inert');
 await page.evaluate(()=>{window.childReplies=[];window.uiKitRuntimeControls={mount(_root,bridge){bridge.onMessage(raw=>window.childReplies.push(JSON.parse(raw)));return()=>{};}};});
 const htmlModel={...model,activeView:'other',body:{kind:'html',html:'<div data-runtime-controls><input id="draft" value="initial"></div>'}};
 await page.evaluate(({next,htmlModel})=>window.receive({...next,project:'/another',model:htmlModel}),{next,htmlModel});
 await page.locator('#draft').fill('unsaved draft');
 await page.evaluate(()=>{window.draftNode=document.querySelector('.legacy').shadowRoot.querySelector('#draft');});
 for(const enabled of [true,false]){
  await page.evaluate(({next,htmlModel,enabled})=>window.receive({...next,project:'/another',model:{...htmlModel,body:{...htmlModel.body,...(enabled?{backgroundPoll:{intervalMs:100,message:{op:'index-tick'}}}:{})}}}),{next,htmlModel,enabled});
  assert.deepEqual(await page.evaluate(()=>{const node=document.querySelector('.legacy').shadowRoot.querySelector('#draft');return{same:node===window.draftNode,value:node.value};}),{same:true,value:'unsaved draft'},'polling metadata preserves editable DOM and drafts');
 }
 await page.evaluate(initial=>window.receive({channel:'ui-kit-view-message',frameId:initial.frameId,targetView:'other',message:JSON.stringify({_backgroundPoll:'old-poll',pending:false})}),initial);
 assert.equal(await page.evaluate(()=>window.childReplies.length),0,'scheduler replies never consume embedded form responses');
 await page.evaluate(initial=>window.receive({channel:'ui-kit-view-message',frameId:initial.frameId,targetView:'other',message:JSON.stringify({kind:'form-response'})}),initial);
 assert.deepEqual(await page.evaluate(()=>window.childReplies),[{kind:'form-response'}]);
 assert.equal(requests[0].project,'/fixture');assert.equal(requests[0].targetView,'models');
 console.log(`PASS ${engine} (${intervalMs}ms): scoped producer polling, single-flight, acknowledgment timeout, wrong-frame rejection, hidden state, immediate navigation cancellation and draft preservation`);
} finally {await browser.close();}
