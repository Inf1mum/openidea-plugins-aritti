const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 for(const plugin of ['kube-explorer','ignore']) {
  await page.goto(pathToFileURL(resolve(`plugins/${plugin}/web/resources/hub.html`)).href);
  const names=await page.evaluate(()=>Object.keys(window.uiKitPluginIcons));
  assert.equal(names.length,plugin==='kube-explorer'?15:2);
  await page.evaluate(names=>{const request=window.sent.at(-1);window.receive({...request,project:'/fixture',model:{title:'Resource icons',toolbar:[],body:{kind:'tree',items:names.map((icon,i)=>({key:`resource:${i}`,label:icon.split('/').at(-1),icon}))}}});},names);
  for(const theme of ['light','dark']) {
   await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
   await page.waitForFunction(()=>[...document.querySelectorAll('tbody .view-icon img')].every(img=>img.complete&&img.naturalWidth>0));
   const visible=page.locator('tbody .view-icon img:visible');assert.equal(await visible.count(),names.length);
   const sizes=await visible.evaluateAll(images=>images.map(img=>({width:img.getBoundingClientRect().width,height:img.getBoundingClientRect().height})));
   assert.ok(sizes.every(size=>size.width===16&&size.height===16));
  }
 }
 // Spring's GraphQL and Messaging groups use shipped native glyph names.
 await page.goto(pathToFileURL(resolve('plugins/spring-web/web/resources/hub.html')).href);
 const graphSource=readFileSync('plugins/spring-web/modules/graphql/src/lib.rs','utf8');
 const graphIcons=[...graphSource.matchAll(/TreeItem::new\([^\n]+\.icon\("([^"]+)"\)/g)].map(match=>match[1]);
 assert.deepEqual(graphIcons,['web','web','folder','file-graphql']);
 const messagingSource=readFileSync('plugins/spring-web/modules/messaging/src/lib.rs','utf8');
 const brokerIcon=messagingSource.match(/TreeItem::new\(&b, broker.label\(\)\)[^\n]+\.icon\("([^"]+)"\)/)?.[1];
 assert.equal(brokerIcon,'folder');
 const springIcons=[...graphIcons,brokerIcon];
 await page.evaluate(icons=>{const request=window.sent.at(-1);window.receive({...request,project:'/fixture',model:{title:'Spring Endpoints',toolbar:[],body:{kind:'tree',items:icons.map((icon,i)=>({key:`spring:${i}`,label:['Endpoints','POST /graphql','Schema files','schema.graphqls','Kafka'][i],icon}))}}});},springIcons);
 for(const theme of ['light','dark']) {
  await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
  const icons=page.locator('tbody .view-icon[data-ui-icon-source="openidea-native"]');
  assert.deepEqual(await icons.evaluateAll(nodes=>nodes.map(node=>node.dataset.uiNativeName)),springIcons);
  assert.equal(await page.locator('tbody .view-icon[data-ui-icon-source="openidea-native"]:visible').count(),springIcons.length);
 }
 assert.deepEqual(errors,[]);
 console.log(`PASS ${engine}: all Kubernetes, Ignore and Spring resource icons load and remain visible at 16px in light/dark modes`);
} finally {await browser.close();}
