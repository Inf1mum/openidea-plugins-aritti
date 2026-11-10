// Packaged Details dialog checks; fixture verification, not live-app acceptance.
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {mkdirSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const source='{"id":9007199254740993,"html":"<img src=x onerror=alert(1)>","enabled":true}';
for(const name of ['chromium','webkit']) {
 const browser=await engines[name].launch({headless:true});try {
  const page=await browser.newPage(),errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:message=>window.sent.push(JSON.parse(message)),onMessage:callback=>listeners.push(callback)};window.receive=model=>{const request=window.sent.at(-1);listeners.forEach(callback=>callback(JSON.stringify({...request,project:'/fixture',model})));};});
  await page.goto(pathToFileURL(resolve('plugins/spring-runtime/web/resources/hub.html')).href);
  const details=[{label:'Observation JSON',value:source},{label:'YAML',value:'apiVersion: v1\nkind: Pod\nmetadata:\n  name: example'}, {label:'SQL',value:'CREATE TABLE users (id BIGINT PRIMARY KEY);\nSELECT * FROM users;'}, {label:'XML',value:'<project>\n  <name>Example</name>\n</project>'}, {label:'Diff',value:'--- before\n+++ after\n-old\n+new'}];
  await page.evaluate(details=>window.receive({title:'Runtime',toolbar:[],contextButtons:[],actions:[],body:{kind:'tree',items:[{key:'one',label:'Example resource',detail:'Readable details',details}]}}),details);
  await page.locator('tbody tr').click({button:'right'});await page.getByRole('menuitem',{name:'Show Details…',exact:true}).click();
  const dialog=page.getByRole('dialog');await dialog.waitFor({state:'visible'});
  assert.equal(await dialog.locator('.code-preview').count(),5);assert.ok(await dialog.locator('.code-token').count()>8);
  const json=dialog.locator('.code-preview[data-language=json]');
  const formatted=await json.locator('code').textContent();assert.ok(formatted.includes('\n  "id": 9007199254740993'));assert.ok(formatted.includes('<img src=x onerror=alert(1)>'));assert.equal(await json.locator('img').count(),0);
  await json.getByRole('button',{name:'Format JSON',exact:true}).click();assert.equal(await json.locator('code').textContent(),source);
  assert.equal(await json.locator('pre').evaluate(node=>getComputedStyle(node).whiteSpace),'pre');
  await json.getByRole('button',{name:'Wrap lines',exact:true}).click();assert.equal(await json.locator('pre').evaluate(node=>getComputedStyle(node).whiteSpace),'pre-wrap');
  await json.getByRole('button',{name:'Select all',exact:true}).click();assert.equal(await page.evaluate(()=>getSelection().toString()),source);
  for(const width of [320,768])for(const theme of ['light','dark']) {
   await page.setViewportSize({width,height:480});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
   const box=await dialog.boundingBox();assert.ok(box.x>=0&&box.x+box.width<=width+1&&box.y>=0&&box.y+box.height<=481);
   assert.ok(await dialog.locator('.ui-dialog-body').evaluate(node=>node.scrollHeight>node.clientHeight),'long details scroll inside dialog');
  }
  const preserved=await page.evaluate(()=>{const text='x'.repeat(270000),preview=uiKitCodePreview.create('Code',text);return preview.querySelector('code').textContent===text;});assert.ok(preserved,'large previews remain complete');
  await json.getByRole('button',{name:'Format JSON',exact:true}).click();
  await json.getByRole('button',{name:'Wrap lines',exact:true}).click();
  mkdirSync('/tmp/openidea-readable-code-dialogs',{recursive:true});
  await page.screenshot({path:`/tmp/openidea-readable-code-dialogs/${name}.png`});
  const budgets=await page.evaluate(()=>{
   const source='['+'0,'.repeat(6000)+'0]',preview=uiKitCodePreview.create('JSON',source);
   return {tokens:preview.querySelectorAll('.code-token').length,complete:preview.querySelector('code').textContent===uiKitCodePreview.prettyJson(source),prose:uiKitCodePreview.create('Provenance','Read from the current owned snapshot')===null};
  });assert.deepEqual(budgets,{tokens:0,complete:true,prose:true});
  assert.deepEqual(errors,[]);console.log(`PASS ${name}: readable code dialogs, exact integers/source, safe markup, syntax, wrap/select and bounded layout`);
 }finally{await browser.close();}
}
