const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
import assert from 'node:assert/strict';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url));
const browser=await chromium.launch({headless:true});
try {
  const page=await browser.newPage({viewport:{width:900,height:650}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    window.sent=[];const listeners=[];
    window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};
    window.receive=value=>listeners.forEach(f=>f(JSON.stringify(value)));
  });
  await page.goto(pathToFileURL(resolve(ROOT,'plugins/gitlab/web/resources/hub.html')).href);
  const model=body=>({title:'Controls',plugin:'Fixture',toolbar:[],actions:[],body});
  async function reply(body,error=null) {await page.evaluate(({body,error})=>{
    const request=window.sent.at(-1);window.receive({channel:'ui-kit',frameId:request.frameId,sequence:request.sequence,project:'/fixture',model:{title:'Controls',plugin:'Fixture',toolbar:[],actions:[],body},error});
  },{body,error});}
  const form={kind:'form',fields:[
    {key:'name',label:'Name',value:{kind:'text',text:'Example'}},
    {key:'choice',label:'Environment',value:{kind:'choice',options:['Development','Production'],text:'Development'}},
    {key:'check',label:'Enabled',value:{kind:'checkbox',checked:false}},
    {key:'comment',label:'Comment',value:{kind:'multiline',text:'Draft'}},
    {key:'output',label:'Read only',value:{kind:'readonly-text',data:{text:'Preview'}}}
  ]};
  await reply(form);
  assert.equal(await page.getByRole('combobox',{name:'Environment',exact:true}).evaluate(n=>n.classList.contains('ui-select-trigger')),true);
  for(const [label,klass] of [['Name','ui-input'],['Enabled','ui-choice'],['Comment','ui-textarea'],['Read only','ui-output']]) {
    assert.equal(await page.getByLabel(label,{exact:true}).evaluate((n,c)=>n.classList.contains(c),klass),true,label);
  }
  await page.getByRole('combobox',{name:'Environment',exact:true}).click();await page.getByRole('option',{name:'Production',exact:true}).click();
  assert.deepEqual(await page.evaluate(()=>window.sent.at(-1).event),{kind:'field',key:'choice',value:'Production'});
  assert.equal(await page.getByRole('progressbar',{name:'Loading resources'}).isVisible(),true);
  await reply(form);
  assert.equal(await page.getByRole('progressbar',{name:'Loading resources'}).isVisible(),false);
  await page.getByLabel('Enabled',{exact:true}).check();
  assert.deepEqual(await page.evaluate(()=>window.sent.at(-1).event),{kind:'field',key:'check',value:'true'});
  await reply(form,'Permission denied');
  assert.equal(await page.locator('#notice,.ui-alert').count(),0,'failure feedback is owned by native notifications');
  // Existing review scripts create controls after the shadow root mounts.
  const review={kind:'html',html:'<body><div class="file" data-new-path="src/a.rs" data-old-path="src/a.rs"><table><tr id="L1" data-new="1"><td class="ln" tabindex="0">1</td><td>code</td></tr></table></div></body>'};
  await reply(review);await page.locator('.ln').click();
  await page.getByLabel('Comment',{exact:true}).waitFor();
  assert.equal(await page.getByLabel('Comment',{exact:true}).evaluate(n=>n.classList.contains('ui-textarea')),true);
  const draft=page.getByRole('button',{name:'Add to Review',exact:true});
  assert.equal(await draft.evaluate(n=>n.classList.contains('ui-button--primary')),true);
  // Wait for the injected shadow stylesheet and compare actual control geometry.
  await page.waitForFunction(()=>{const n=document.querySelector('.legacy').shadowRoot.querySelector('button');return getComputedStyle(n).minHeight==='28px';});
  await page.getByLabel('Comment',{exact:true}).fill('Retained action');await draft.click();
  assert.equal(await page.evaluate(()=>JSON.parse(window.sent.at(-1).event.value).body),'Retained action');
  // A nested control's Space/Enter must not trigger row selection/activation.
  await reply({kind:'tree',items:[{key:'folder',label:'Folder',expandable:true,expanded:false}]});
  const toggle=page.getByRole('button',{name:'Expand Folder'});await toggle.focus();
  const before=await page.evaluate(()=>window.sent.length);await page.keyboard.press('Space');
  assert.equal(await page.evaluate(()=>window.sent.length),before+1);
  assert.equal(await page.evaluate(()=>window.sent.at(-1).event.kind),'expand');
  await reply({kind:'table',columns:['Value'],rows:[['editable']],editable:true});
  await page.locator('[contenteditable]').focus();
  const beforeEnter=await page.evaluate(()=>window.sent.length);await page.keyboard.press('Enter');
  assert.equal(await page.evaluate(()=>window.sent.length),beforeEnter);
  // Standalone runtime page uses the same controls with its original scope driver.
  await page.goto(pathToFileURL(resolve(ROOT,'plugins/spring-runtime/web/resources/runtime-controls.html')).href);
  await page.evaluate(()=>new Promise(resolve=>{const observer=new IntersectionObserver(entries=>{if(entries.some(e=>e.isIntersecting)){observer.disconnect();resolve();}});observer.observe(document.documentElement);}));
  await page.evaluate(()=>{const r=window.sent.at(-1);window.receive({...r,model:{root:'/fixture',url:'http://localhost:8080',auth:'none',enabled:true,active:false,poll:'Stopped',status:'Ready'}});});
  assert.equal(await page.locator('#refresh').evaluate(n=>n.classList.contains('ui-button')),true);
  await page.locator('#refresh').click();
  assert.equal(await page.evaluate(()=>window.sent.at(-1).op),'refresh');
  assert.equal(await page.locator('#refresh').getAttribute('aria-busy'),'true');
  await page.evaluate(()=>{const r=window.sent.at(-1);window.receive({...r,model:{root:'/fixture',active:false,error:'Unavailable'}});});
  assert.equal(await page.locator('#status').evaluate(n=>n.classList.contains('ui-alert--error')),true);
  assert.equal(await page.locator('#refresh').getAttribute('aria-busy'),'false');
  await page.getByText('Polling limits and request scope',{exact:true}).click();
  assert.equal(await page.locator('details').getAttribute('open'),'');
  assert.deepEqual(errors,[]);
  await page.close();
  // Use the actual bundled Swagger renderer with a read-only editor fixture.
  const api=await browser.newPage({viewport:{width:375,height:700}});
  api.on('pageerror',e=>errors.push(e.message));
  await api.addInitScript(()=>{
    window.reloads=0;
    window.openidea={postMessage:()=>{},onMessage:()=>{},document:{onChange:()=>{},get:async()=>{window.reloads++;return {text:'{"openapi":"3.0.0","info":{"title":"Fixture API","version":"1"},"paths":{}}'};}}};
  });
  await api.goto(pathToFileURL(resolve(ROOT,'plugins/openapi/web/index.html')).href);
  await api.locator('#swagger .info').waitFor();
  await api.getByRole('button',{name:'Reload',exact:true}).click();
  assert.equal(await api.evaluate(()=>window.reloads),2);
  await api.locator('html').evaluate(n=>n.dataset.theme='dark');
  assert.equal(await api.locator('.api-toolbar').evaluate(n=>getComputedStyle(n).filter),'none');
  assert.equal(await api.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  assert.deepEqual(errors,[]);
  console.log('PASS all controls: typed forms/events, busy/error states, dynamic shadow review controls, nested keyboard events, runtime driver, OpenAPI toolbar');
} finally {await browser.close();}
