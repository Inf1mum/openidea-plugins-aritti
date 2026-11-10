// Shared embedded composition: kit geometry, keyboard access and compact docks.
const {chromium,webkit}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const output=process.env.UI_QA_DIR||'/tmp/openidea-native-hub-composition';mkdirSync(output,{recursive:true});
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await ({chromium,webkit})[engine].launch({headless:true});
const toolbar=[{key:'section:project',label:'● Project',enabled:true},{key:'section:dependencies',label:'Dependencies (0)',enabled:true},{key:'section:cli',label:'CLI run',enabled:true},{key:'generate',label:'Generate Project',icon:'external-link',enabled:true},{key:'choose-dependencies',label:'Dependencies',icon:'package',enabled:true},{key:'more-actions',label:'More Actions…',icon:'more-horizontal',enabled:true}];
const model={title:'Spring Initializr',plugin:'Spring Initializr Launcher',toolbar,fieldActions:{dependencies:{button:'choose-dependencies',primary:true}},body:{kind:'form',fields:[{key:'build',label:'Build',value:{kind:'choice',text:'Maven',options:['Maven','Gradle']}},{key:'language',label:'Language',value:{kind:'choice',text:'Java',options:['Java','Kotlin']}},{key:'group',label:'Group',value:{kind:'text',text:'com.example'}},{key:'artifact',label:'Artifact',value:{kind:'text',text:'demo'}},{key:'name',label:'Name',value:{kind:'text',text:'demo'}},{key:'dependencies',label:'Dependencies',value:{kind:'text',text:''}}]}};
const measurements=[];
try {
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=s=>listeners.forEach(f=>f(s));});
 await page.goto(pathToFileURL(resolve(root,'plugins/spring-initializr/web/resources/hub.html')).href);
 const reply=async model=>page.evaluate(model=>{const r=window.sent.at(-1);window.receive(JSON.stringify({...r,project:'/fixture',model}));},model);
 await reply(model);
 assert.equal(await page.locator('body > header').count(),0);
 const deps=page.getByRole('button',{name:'Dependencies',exact:true});
 assert.equal(await deps.evaluate(button=>button.classList.contains('ui-button--primary')),true);
 assert.equal(await page.locator('#toolbar').getByRole('button',{name:'Dependencies',exact:true}).count(),0);
 const dependencyInput=page.getByRole('textbox',{name:'Dependencies',exact:true});
 assert.equal(await dependencyInput.locator('..').getByRole('button',{name:'Dependencies',exact:true}).count(),1,'selector sits beside its input');
 await dependencyInput.fill('web,data-jpa');await deps.click();
 assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'choose-dependencies');
 assert.equal(await page.evaluate(()=>window.sent.findLast(message=>message.event?.kind === 'field').event.value),'web,data-jpa','inline navigation flushes the dependency draft');
 assert.equal(await deps.isDisabled(),true,'inline action stays disabled during its request');
 model.body.fields.find(field=>field.key === 'dependencies').value.text='web,data-jpa';await reply(model);
 assert.equal(await deps.isDisabled(),false,'inline action recovers after the response');

 assert.equal(await page.getByRole('button',{name:'More Actions…',exact:true}).getAttribute('title'),'More Actions…');
 await page.getByRole('button',{name:'Project',exact:true}).focus();await page.keyboard.press('ArrowRight');
 assert.equal(await page.getByRole('button',{name:'Dependencies (0)',exact:true}).evaluate(n=>n===document.activeElement),true);
 await page.keyboard.press('Enter');assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'section:dependencies');await reply(model);
 assert.equal(await page.getByRole('button',{name:'Dependencies (0)',exact:true}).evaluate(n=>n===document.activeElement),true,'section focus survives the model roundtrip');
 for(const width of [320,375,414,768])for(const theme of ['light','dark']) {
  await page.setViewportSize({width,height:520});await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);
  const m=await page.evaluate(()=>({rootWidth:document.documentElement.scrollWidth,viewport:innerWidth,toolbar:document.querySelector('#toolbar').getBoundingClientRect().height,nav:document.querySelector('#sections').getBoundingClientRect().height,formTop:document.querySelector('.form').getBoundingClientRect().top,moreRight:[...document.querySelectorAll('#toolbar button')].find(n=>n.title==='More Actions…').getBoundingClientRect().right,moreWidth:[...document.querySelectorAll('#toolbar button')].find(n=>n.title==='More Actions…').getBoundingClientRect().width}));
  assert.ok(m.rootWidth<=width,`no root overflow at ${width}`);assert.ok(m.toolbar<=35,`single kit toolbar row at ${width}: ${m.toolbar}`);assert.ok(m.nav<=43,`single tab strip at ${width}`);assert.ok(m.formTop<80,`form begins below compact controls at ${width}`);assert.equal(m.moreWidth,26);assert.ok(Math.abs(m.moreRight-(width-7))<1,`overflow menu at right edge at ${width}`);
  await page.screenshot({path:resolve(output,`${engine}-initializr-${theme}-${width}.png`)});measurements.push({width,theme,...m});
 }
 const inputBox=await dependencyInput.boundingBox(),depsBox=await deps.boundingBox();assert.ok(Math.abs(inputBox.y-depsBox.y)<2 && depsBox.x>inputBox.x+inputBox.width,'primary selector is inline with its field');
 // Embedded chrome uses the host island surface, never the elevated surface.
 for(const [base,elevated]of [['rgb(24, 26, 27)','rgb(43, 45, 48)'],['rgb(245, 246, 247)','rgb(255, 255, 255)']]) {
  await page.evaluate(({base,elevated})=>{document.documentElement.style.setProperty('--openidea-bg',base);document.documentElement.style.setProperty('--openidea-bg-elevated',elevated);},{base,elevated});
  for(const selector of ['body','#toolbar','#sections'])assert.equal(await page.locator(selector).evaluate(n=>getComputedStyle(n).backgroundColor),base,`${selector} follows live native window background`);
  assert.equal(await page.evaluate(()=>{const n=document.createElement('div');n.style.background='var(--ui-panel)';document.body.append(n);const color=getComputedStyle(n).backgroundColor;n.remove();return color;}),elevated,'elevated surfaces retain their separate token');
 }
 await page.evaluate(()=>{document.documentElement.style.removeProperty('--openidea-bg');document.documentElement.style.removeProperty('--openidea-bg-elevated');});
 await page.getByRole('button',{name:'More Actions…',exact:true}).click();assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'more-actions');
 const rows={title:'Routes',plugin:'Fixture',toolbar:[{key:'refresh',label:'Refresh',icon:'refresh',enabled:true}],body:{kind:'resources',columns:['Method','Path'],facets:[0],items:[{key:'r1',cells:['GET','/orders']},{key:'r2',cells:['POST','/orders']}]}};await reply(rows);
 assert.equal(await page.getByRole('button',{name:'Refresh',exact:true}).count(),1);
 const height=await page.locator('tbody tr').first().evaluate(n=>n.getBoundingClientRect().height);assert.equal(height,24,'kit resource-row density');
 await page.getByRole('searchbox').fill('POST');assert.equal(await page.locator('tbody tr').count(),1);
 await page.getByRole('button',{name:'Clear filters',exact:true}).click();assert.equal(await page.locator('tbody tr').count(),2);
 assert.equal(await page.getByRole('combobox',{name:'Method',exact:true}).count(),1);
 await page.screenshot({path:resolve(output,`${engine}-resources.png`)});
 await page.goto(pathToFileURL(resolve(root,'plugins/openapi/web/resources/hub.html')).href);
 const endpointTree={title:'OpenAPI Endpoints',plugin:'OpenAPI',toolbar:[{key:'refresh',label:'Refresh',icon:'refresh',enabled:true}],body:{kind:'tree',items:[{key:'controller:tag',label:'(no tag)',icon:'folder',expandable:true,expanded:true},...['GET','POST','PUT','DELETE','GET|POST'].map((method,i)=>({key:`route:${i}`,parent:'controller:tag',label:i===4?'/mixed':`/user/${i}`,method,detail:'(no tag)'}))]}};
 await reply(endpointTree);
 assert.equal(await page.locator('tbody .ui-http-method').count(),6,'tree routes render individual method badges');
 assert.equal(await page.locator('tbody').textContent().then(t=>t.includes('[GET]')),false,'method is not bracketed plain detail text');
 await page.screenshot({path:resolve(output,`${engine}-openapi-method-badges.png`)});
 await page.evaluate(()=>document.documentElement.dataset.theme='dark');
 await page.screenshot({path:resolve(output,`${engine}-openapi-method-badges-dark.png`)});
 await page.locator('tbody tr').nth(1).click();await reply(endpointTree);
 assert.equal(await page.locator('#details').evaluate(n=>n.open),false,'selection never opens a side panel');
 await page.locator('tbody tr').nth(1).click({button:'right'});await page.getByRole('menuitem',{name:'Show Details…',exact:true}).click();
 assert.equal(await page.locator('#details .ui-http-method').textContent(),'GET','selected endpoint retains method badge');
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await page.getByRole('searchbox').fill('DELETE');assert.equal(await page.locator('tbody tr').count(),2,'method search retains the group and matching endpoint');
 await page.locator('tr[data-resource-key="route:3"]').dblclick();assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'route:3','badge rendering preserves activation identity');
 await page.goto(pathToFileURL(resolve(root,'plugins/ignore/web/resources/hub.html')).href);
 await reply({...rows,title:'Ignore Templates',plugin:'.ignore',body:{kind:'resources',columns:['Template','Source'],facets:[1],items:[{key:'rust',cells:['Rust','Built-in']},{key:'project',cells:['Project','User']}]},toolbar:[{key:'new',label:'Create User Template',icon:'plus',enabled:true},{key:'delete',label:'Delete User Template',icon:'trash',enabled:false},{key:'star',label:'Star',icon:'star',enabled:false},{key:'preferences',label:'Preferences',icon:'settings',enabled:true}]});
 await page.evaluate(()=>{document.documentElement.dataset.theme='dark';document.documentElement.style.setProperty('--openidea-bg','rgb(24, 26, 27)');document.documentElement.style.setProperty('--openidea-bg-elevated','rgb(33, 35, 36)');});
 assert.equal(await page.locator('#toolbar').evaluate(n=>getComputedStyle(n).backgroundColor),'rgb(24, 26, 27)','actual Ignore module uses native base surface');
 assert.equal(await page.getByRole('button',{name:'Refresh',exact:true}).count(),0,'Ignore only shows declared actions, without an injected refresh');
 const gear=page.getByRole('button',{name:'Preferences',exact:true});
 assert.equal(await gear.getAttribute('title'),'Preferences','gear provides a tooltip');
 assert.equal(await gear.locator('svg').getAttribute('data-ui-icon-name'),'settings');
 const gearBox=await gear.boundingBox();assert.equal(gearBox.width,26);assert.ok(Math.abs(gearBox.x+gearBox.width-(await page.evaluate(()=>innerWidth))-(-7))<1,'gear sits at the toolbar right edge');

 await page.screenshot({path:resolve(output,`${engine}-ignore-native-background.png`)});
 await gear.click();assert.equal(await page.evaluate(()=>window.sent.at(-1).event.key),'preferences','gear dispatches the existing preference action');

 // Status stays pinned while short and overflowing forms share the remaining space.
 for (const height of [180, 360, 420, 520]) for (const count of [1, 30]) {
  await page.setViewportSize({width:600,height});
  await reply({title:'Setup',toolbar:[{key:'save',label:'Save',icon:'check',enabled:true}],status:'Preferences apply after Save.',body:{kind:'form',fields:Array.from({length:count},(_,i)=>({key:`field-${i}`,label:`Field ${i}`,value:{kind:'text',text:''}}))}});
  const geometry=await page.evaluate(()=>{
   const content=document.querySelector('#content'),status=document.querySelector('#status');
   return {bottom:status.getBoundingClientRect().bottom,statusHeight:status.getBoundingClientRect().height,contentBottom:content.getBoundingClientRect().bottom,statusTop:status.getBoundingClientRect().top,rootHeight:document.documentElement.scrollHeight,scrollable:content.scrollHeight>content.clientHeight};
  });
  assert.ok(Math.abs(geometry.bottom-height)<1,`status pinned at ${height}px with ${count} fields`);
  assert.ok(geometry.statusHeight<40,'status remains a compact line');
  assert.ok(Math.abs(geometry.contentBottom-geometry.statusTop)<1,'content fills space above status');
  assert.ok(geometry.rootHeight<=height,'dock does not overflow');
  if(count===30) assert.ok(geometry.scrollable,'long forms scroll inside the content area');
 }
 assert.deepEqual(errors,[]);writeFileSync(resolve(output,`${engine}-receipt.json`),JSON.stringify(measurements,null,2)+'\n');
 console.log(`PASS ${engine}: kit toolbar/tab/form geometry, 8 dock layouts, section keyboard navigation, compact action dispatch, row density and accessible filters`);
}finally{await browser.close();}
