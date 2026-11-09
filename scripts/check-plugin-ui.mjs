const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
import {mkdirSync} from 'node:fs';
const ROOT=fileURLToPath(new URL('../',import.meta.url));
const QA=process.env.UI_QA_DIR || '/tmp/openidea-plugin-ui-qa';
mkdirSync(QA,{recursive:true});
const browser=await chromium.launch({headless:true});
const page=await browser.newPage({viewport:{width:1100,height:800}});
const errors=[];page.on('pageerror', e=>errors.push(e.message));
await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=text=>listeners.slice().forEach(f=>f(text));});
await page.goto(pathToFileURL(resolve(ROOT,'plugins/laravel/web/resources/hub.html')).href);
const model={title:'Routes',plugin:'Laravel',toolbar:[{key:'refresh',label:'Refresh',enabled:true}],actions:[{id:'copy',label:'Copy as curl',kind:'route'}],body:{kind:'resources',columns:['Method','Path','Controller','Handler','Name'],facets:[0,2],items:[{key:'route:0',cells:['GET','/orders','OrderController','index','orders']},{key:'route:1',cells:['POST','/orders','OrderController','store','orders.store']},{key:'route:2',cells:['GET','/<img src=x onerror=alert(1)>','OtherController','show','other']} ]}};
async function reply(m=model,error=null){await page.evaluate(({m,error})=>window.receive(JSON.stringify({channel:'ui-kit',frameId:window.sent.findLast(m=>m.channel==='ui-kit').frameId,sequence:window.sent.findLast(m=>m.channel==='ui-kit').sequence,project:'/fixture',model:m,error})),{m,error});}
await reply();assert.equal(await page.locator('tbody tr').count(),3);
// Native packaged views need explicit invalidation. It carries no model and
// must preserve frame/project correlation and wait for an outstanding action.
const refreshBefore=await page.evaluate(()=>window.sent.length);
await page.evaluate(()=>window.receive(JSON.stringify({channel:'ui-kit-refresh',model:{title:'Untrusted broadcast'}})));
const refreshRequest=await page.evaluate(()=>window.sent.at(-1));
assert.equal(refreshRequest.op,'render');assert.equal(refreshRequest.project,'/fixture');
assert.equal(refreshRequest.frameId,await page.evaluate(()=>window.uiKitFrameId));
assert.equal(await page.locator('#title').textContent(),'Routes');
await page.evaluate(()=>{window.receive(JSON.stringify({channel:'ui-kit-refresh'}));window.receive(JSON.stringify({channel:'ui-kit-refresh'}));});
assert.equal(await page.evaluate(()=>window.sent.length),refreshBefore+1);
await page.evaluate(()=>window.receive(JSON.stringify({channel:'ui-kit',frameId:'f'.repeat(32),sequence:window.sent.at(-1).sequence,project:'/fixture',model:{title:'Foreign'}})));
assert.equal(await page.evaluate(()=>window.sent.length),refreshBefore+1);
await reply();
assert.equal(await page.evaluate(()=>window.sent.length),refreshBefore+2);
assert.equal(await page.evaluate(()=>window.sent.at(-1).sequence),refreshRequest.sequence+1);
await reply();
console.log('PASS model-free invalidation, busy coalescing and correlated refresh');
assert.match(await page.evaluate(()=>window.uiKitFrameId),/^[0-9a-f]{32}$/);
assert.equal(await page.evaluate(()=>Object.getOwnPropertyDescriptor(window,'uiKitFrameId').writable),false);
await page.evaluate(()=>{const request=window.sent.findLast(m=>m.channel==='ui-kit');for(const frameId of ['f'.repeat(32),undefined])window.receive(JSON.stringify({channel:'ui-kit',frameId,sequence:request.sequence,project:'/fixture',error:'Wrong frame',model:{title:'Foreign window',body:{kind:'message',text:'wrong-frame content'}}}));});
assert.equal(await page.locator('tbody tr').count(),3);assert.equal(await page.locator('#notice').textContent(),'');

await page.getByRole('searchbox').fill('store');assert.equal(await page.locator('tbody tr').count(),1);
await page.getByRole('button',{name:'Clear filters',exact:true}).click();
await page.getByRole('combobox',{name:'Method',exact:true}).click();await page.getByRole('option',{name:'POST',exact:true}).click();assert.equal(await page.locator('tbody tr').count(),1);
await page.locator('tbody tr').click();await reply();await page.getByRole('button',{name:'Copy as curl',exact:true}).click();
assert.equal(await page.evaluate(()=>window.sent.at(-1).op),'command');
await reply(model,'Permission denied');assert.equal(await page.getByRole('button',{name:'Copy as curl',exact:true}).isEnabled(),true);
await page.getByRole('button',{name:'Clear filters',exact:true}).click();assert.equal(await page.locator('#content img').count(),0);
await page.screenshot({path:resolve(QA,'routes-wide.png')});
await page.setViewportSize({width:380,height:750});await page.screenshot({path:resolve(QA,'routes-narrow.png')});
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();
const advisory={...model,body:{kind:'resources',columns:['Dependency','Version','Severity'],facets:[2],items:[{key:'dep:1',cells:['example-lib','1.0','High'],details:[{label:'Fixed versions',value:'2.0'},{label:'Advisory',value:'<img src=x onerror=alert(1)>'}]}]}};
await reply(advisory);await page.locator('tbody tr').click();await reply(advisory);
assert.equal(await page.locator('#details h2').textContent(),'example-lib');assert.equal(await page.locator('#details dd').last().textContent(),'<img src=x onerror=alert(1)>');assert.equal(await page.locator('#details img').count(),0);
assert.equal(await page.locator('tbody .method').count(),0);assert.equal(await page.locator('#details dt').filter({hasText:'Fixed versions'}).count(),1);
// A bottom dock can be shorter than its header, toolbar and facets together.
await page.setViewportSize({width:1100,height:260});
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();
await reply(advisory);
assert.ok(await page.locator('main').evaluate(n=>n.scrollHeight>n.clientHeight),'short docks scroll the workflow instead of clipping rows');
await page.locator('tbody tr').scrollIntoViewIfNeeded();await page.locator('tbody tr').click();await reply(advisory);
await page.getByRole('button',{name:'Open',exact:true}).scrollIntoViewIfNeeded();
await page.getByRole('button',{name:'Open',exact:true}).click();
assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'dep:1');
await reply(advisory);await page.screenshot({path:resolve(QA,'advisory-short-dock.png')});
await page.setViewportSize({width:380,height:750});
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();
const form={...model,body:{kind:'form',fields:[{key:'description',label:'Description',value:{kind:'readonly-text',data:{text:'<script>unsafe</script>',multiline:true}}},{key:'comment',label:'Comment',value:{kind:'multiline',text:'draft'}}]}};
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();await reply(form);
assert.equal(await page.locator('output').textContent(),'<script>unsafe</script>');assert.equal(await page.locator('textarea').inputValue(),'draft');assert.equal(await page.locator('#content script').count(),0);
// Short categorical labels stay readable in a narrow settings/resource pane.
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();
await page.setViewportSize({width:320,height:750});
await reply({...model,body:{kind:'table',columns:['Kind','Name or action','Pattern'],rows:[['Format','Example format','^(\\S+ \\S+) (\\w+) (.*)$']],firstRow:0,totalRows:1,editable:false}});
assert.equal(await page.locator('tbody td').first().evaluate(n=>{const r=document.createRange();r.selectNodeContents(n);return r.getClientRects().length;}),1,'Format must remain one word in narrow panes');
await page.screenshot({path:resolve(QA,'patterns-narrow.png')});
// Form validation must not require blur or overwrite text typed after an older reply.
const editing={...model,toolbar:[{key:'refresh',label:'Refresh',enabled:true},{key:'create',label:'Create',enabled:false}],body:{kind:'form',fields:[{key:'summary',label:'Summary',value:{kind:'text',text:''}},{key:'description',label:'Description',value:{kind:'multiline',text:''}}]}};
const edited=text=>({...editing,toolbar:editing.toolbar.map(b=>b.key==='create'?{...b,enabled:!!text}:b),body:{...editing.body,fields:editing.body.fields.map(f=>f.key==='summary'?{...f,value:{...f.value,text}}:f)}});
await page.getByRole('button',{name:'Refresh',exact:true}).first().click();await reply(editing);
const summary=page.getByLabel('Summary',{exact:true});await summary.focus();await summary.fill('First');
await page.waitForFunction(()=>window.sent.at(-1).event?.key==='summary' && window.sent.at(-1).event?.value==='First');await reply(edited('First'));
assert.equal(await summary.evaluate(n=>n===document.activeElement),true);assert.equal(await page.getByRole('button',{name:'Create',exact:true}).isEnabled(),true);
await summary.fill('A');await page.waitForFunction(()=>window.sent.at(-1).event?.value==='A');
await summary.fill('AB');await reply(edited('A'));assert.equal(await summary.inputValue(),'AB');assert.equal(await summary.evaluate(n=>n===document.activeElement),true);
await page.waitForFunction(()=>window.sent.at(-1).event?.value==='AB');await reply(edited('AB'));assert.equal(await summary.evaluate(n=>n.selectionStart),2);
await summary.fill('ab');await page.waitForFunction(()=>window.sent.at(-1).event?.value==='ab');await reply(edited('AB'));assert.equal(await summary.inputValue(),'AB','server normalization applies even when its model body equals the previous accepted body');
await summary.fill('Preserved draft');
await page.evaluate(()=>window.receive(JSON.stringify({channel:'ui-kit-refresh'})));
assert.equal(await page.evaluate(()=>window.sent.at(-1).event?.value),'Preserved draft','invalidation flushes the draft before requesting a new model');
await reply(edited('Preserved draft'));
assert.equal(await page.evaluate(()=>window.sent.at(-1).op),'render');
await reply(edited('Preserved draft'));assert.equal(await summary.inputValue(),'Preserved draft');
await summary.fill('Discard draft');const beforeDiscard=await page.evaluate(()=>window.sent.length);await reply({...editing,body:{kind:'message',text:'Form closed'}});
await page.waitForTimeout(350);assert.equal(await page.evaluate(()=>window.sent.length),beforeDiscard,'removed form must cancel pending field dispatch');
console.log('PASS live form validation, focus/caret, late replies, normalization and removed-form drafts');
assert.deepEqual(errors,[]);console.log('PASS search, facets, selection, row commands, failure recovery, text escaping, readonly forms, wide/narrow rendering');
await page.goto(pathToFileURL(resolve(ROOT,'plugins/gitlab/web/resources/hub.html')).href);
const openSnapshot={op:'open-content-tab',key:'gl-fixture',title:'Read-only MR',content:'snapshot',readOnly:true};
await page.evaluate(payload=>window.receive(JSON.stringify({channel:'gitlab-content-open',frameId:window.uiKitFrameId,payload})),openSnapshot);
assert.deepEqual(await page.evaluate(()=>window.sent.at(-1)),openSnapshot);
const beforeForeignContent=await page.evaluate(()=>window.sent.length);
await page.evaluate(payload=>window.receive(JSON.stringify({channel:'gitlab-content-open',frameId:'f'.repeat(32),payload})),openSnapshot);
assert.equal(await page.evaluate(()=>window.sent.length),beforeForeignContent,'foreign frame must not open snapshot');
const contentReply={op:'content-tab-reply',requestId:'ct-1',content:'local snapshot',readOnly:true};
await page.evaluate(payload=>window.receive(JSON.stringify({channel:'gitlab-content-reply',frameId:'f'.repeat(32),payload})),contentReply);
assert.equal(await page.evaluate(()=>window.sent.length),beforeForeignContent,'foreign content reply must not resolve a local requestId collision');
await page.evaluate(payload=>window.receive(JSON.stringify({channel:'gitlab-content-reply',frameId:window.uiKitFrameId,payload})),contentReply);
assert.deepEqual(await page.evaluate(()=>window.sent.at(-1)),contentReply);

const beforeContentAction=await page.evaluate(()=>window.sent.length);
await page.evaluate(()=>window.receive(JSON.stringify({op:'content-tab-action',key:'gl-fixture',action:'reload'})));
assert.equal(await page.evaluate(()=>window.sent.length),beforeContentAction,'host content callbacks must not dispatch twice');
await page.evaluate(()=>window.receive(JSON.stringify({op:'content-tab-error'})));
assert.match(await page.locator('#notice').textContent(),/could not be opened/);

const review={...model,body:{kind:'html',html:'<body><div id="error" hidden></div><div class="file" data-new-path="src/a.rs" data-old-path="src/a.rs"><table><tr id="L1" data-new="1"><td class="ln" tabindex="0">1</td><td>code</td></tr></table></div></body>'}};
await reply(review);await page.locator('.ln').click();await page.getByLabel('Comment',{exact:true}).fill('retained draft');await page.getByRole('button',{name:'Add to Review',exact:true}).click();
assert.equal(await page.evaluate(()=>JSON.parse(window.sent.at(-1).event.value).body),'retained draft');await reply(review);assert.equal(await page.getByLabel('Comment',{exact:true}).inputValue(),'retained draft');
await page.goto(pathToFileURL(resolve(ROOT,'plugins/kube-explorer/web/resources/hub.html')).href);
await page.evaluate(()=>window.openidea.saveFile=async options=>{window.saved=options;return '/tmp/export.yaml';});
let html=readFileSync(resolve(ROOT,'plugins/kube-explorer/src/page.html'),'utf8').replaceAll('{{highlighted}}','kind: Pod').replaceAll('{{lang}}','yaml').replaceAll('{{name}}','pod').replaceAll('{{readonly}}','').replaceAll('{{text}}','kind: Pod').replaceAll('{{nojs}}','');
await reply({...model,body:{kind:'html',html}});await page.locator('#src').fill('kind: Service');await page.evaluate(()=>window.receive('collect:apply'));
assert.equal(await page.evaluate(()=>window.sent.at(-1).event.value),'apply\nkind: Service');await page.evaluate(()=>window.receive('collect:save'));assert.equal(await page.evaluate(()=>window.saved.data),'kind: Service');
assert.deepEqual(errors,[]);console.log('PASS retained GitLab line-comment drafts; Kubernetes YAML apply and save bridge');
// Real separate browser pages reproduce host fan-out with identical local sequences/request IDs.
const peer=await browser.newPage();
await peer.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=text=>listeners.slice().forEach(f=>f(text));});
await Promise.all([page.goto(pathToFileURL(resolve(ROOT,'plugins/gitlab/web/resources/hub.html')).href),peer.goto(pathToFileURL(resolve(ROOT,'plugins/gitlab/web/resources/hub.html')).href)]);
const frames=await Promise.all([page.evaluate(()=>window.uiKitFrameId),peer.evaluate(()=>window.uiKitFrameId)]);
assert.notEqual(frames[0],frames[1]);
for(let owner=0;owner<2;owner++){
  const target=[page,peer][owner];
  const request=await target.evaluate(()=>window.sent.findLast(m=>m.channel==='ui-kit'));
  const message=JSON.stringify({channel:'ui-kit',frameId:frames[owner],sequence:request.sequence,project:'/fixture',model:{...model,title:`Window ${owner}`,body:{kind:'message',text:`Owned by ${owner}`}}});
  await Promise.all([page,peer].map(p=>p.evaluate(m=>window.receive(m),message)));
  assert.equal(await target.locator('#content').textContent(),`Owned by ${owner}`);
}
assert.equal(await page.locator('#content').textContent(),'Owned by 0');
assert.equal(await peer.locator('#content').textContent(),'Owned by 1');
for(let owner=0;owner<2;owner++){
  const before=await Promise.all([page,peer].map(p=>p.evaluate(()=>window.sent.length)));
  const message=JSON.stringify({channel:'gitlab-content-reply',frameId:frames[owner],payload:{op:'content-tab-reply',requestId:'ct-1',content:`Snapshot ${owner}`,readOnly:true}});
  await Promise.all([page,peer].map(p=>p.evaluate(m=>window.receive(m),message)));
  const after=await Promise.all([page,peer].map(p=>p.evaluate(()=>window.sent.length)));
  assert.equal(after[owner],before[owner]+1);
  assert.equal(after[1-owner],before[1-owner]);
}
await peer.close();console.log('PASS two-page broadcast isolation with matching sequences and content request IDs');
await browser.close();
