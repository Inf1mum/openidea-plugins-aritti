// Shared stable-key UI language behavior through the shipped resource hub.
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
  const page=await browser.newPage({locale:'ja-JP'}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{window.sent=[];const callbacks=[];window.openidea={postMessage:text=>window.sent.push(JSON.parse(text)),onMessage:f=>callbacks.push(f)};window.receive=message=>callbacks.forEach(f=>f(JSON.stringify(message)));});
  await page.goto(pathToFileURL(resolve('crates/ui-kit/web/hub.html')).href);
  await page.waitForFunction(()=>window.sent.length);
  assert.equal(await page.locator('html').getAttribute('lang'),'ja','auto follows the supported browser language');
  assert.equal(await page.evaluate(()=>window.sent[0].clientLocale),'ja','initial correlated request carries the resolved browser locale');
  const catalog=JSON.parse(readFileSync('crates/ui-kit/i18n/catalog.json','utf8'));
  for(const language of ['en','ja','ko','zh-CN']){
    const model={title:'Laravel',uiLanguage:language,activeView:'laravel',viewTabs:[{id:'laravel',label:'Routes',icon:'web'},{id:'laravel-views',label:'Views',icon:'file-html'}],toolbar:[{key:'reindex',label:'Reindex',enabled:true,icon:'refresh'}],actions:[],body:{kind:'resources',columns:['View'],items:[{key:'source',cells:['Routes'],activationEnabled:false}]}};
    await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);
    await page.waitForFunction(language=>document.documentElement.lang===language,language);
    assert.equal(await page.locator('#search').getAttribute('placeholder'),catalog[language]['shared.searchPlaceholder']);
    assert.equal(await page.getByRole('tab',{name:catalog[language]['view.laravel'],exact:true}).count(),1);
    assert.equal(await page.locator('thead button').first().textContent(),catalog[language]['column.view']);
    assert.equal(await page.locator('tbody td').first().textContent(),'Routes','source data must never be translated');
    assert.equal(await page.locator('#toolbar button').getAttribute('title'),catalog[language]['action.reindex']);
  }
  await page.locator('#toolbar button').click();
  assert.equal(await page.evaluate(()=>window.sent.at(-1).clientLocale),'zh-CN');
  await page.evaluate(()=>{const icon=document.createElement('span');icon.id='preserved-icon';const button=document.createElement('button');button.append(icon);const label=document.createElement('span');label.dataset.i18n='shared.copy';label.textContent='Copy';button.append(label);document.body.append(button);window.uiKitI18n.setLocale('ja');});
  assert.equal(await page.locator('#preserved-icon').count(),1);
  assert.equal(await page.evaluate(()=>window.uiKitI18n.t('command.count','',{count:'{status}'})),'{status} 件のコマンド');
  const commandHtml=readFileSync('crates/ui-kit/web/command-workspace.html','utf8');
  await page.evaluate(html=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model:{title:'Artisan',uiLanguage:'ko',activeView:'laravel-artisan',viewTabs:[{id:'laravel',label:'Routes'},{id:'laravel-artisan',label:'Artisan'}],toolbar:[],actions:[],body:{kind:'html',html}}});},commandHtml);
  assert.equal(await page.getByRole('button',{name:catalog.ko['command.load'],exact:true}).count(),1,'stable keys localize shadow-root workspace controls');
  assert.equal((await page.locator('#run').textContent()).trim(),catalog.ko['command.run']);
  assert.equal(await page.evaluate(()=>window.uiKitI18n.t('__proto__','AuthController')),'AuthController');
  assert.deepEqual(errors,[]);
  console.log(`PASS ${engine}: en/ja/ko/zh-CN stable UI keys, explicit language switching, native toolbar/view labels, source-data preservation and nonrecursive arguments`);
}finally{await browser.close();}
