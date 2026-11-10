const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const output='/tmp/openidea-resource-badges';mkdirSync(output,{recursive:true});
const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:1000,height:500}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve(root,'plugins/dependency-audit/web/resources/hub.html')).href);
 const severities=['CRITICAL','HIGH','MODERATE','MEDIUM','LOW','Unknown','HIGH + unknown'];
 const model={title:'Vulnerable Dependencies',plugin:'Dependency Audit',toolbar:[],body:{kind:'resources',columns:['Type','Dependency','Ecosystem','Installed','Fixed','Severity','Advisory'],facets:[0,5],items:severities.map((severity,i)=>({key:`advisory:${i}`,cells:[i?'Advisory':'Dependency','sample-lib','npm','1.0','1.1',severity,'Fixture advisory']}))}};
 const reply=async()=>page.evaluate(model=>{const r=window.sent.at(-1);window.receive({...r,project:'/fixture',model});},model);await reply();
 assert.equal(await page.locator('tbody .ui-type-badge').count(),7);assert.equal(await page.locator('tbody .ui-severity-badge').count(),7);
 const badges=page.locator('tbody .ui-severity-badge');
 for(const [i,tone]of ['danger','danger','warning','warning','info',null,'danger'].entries()){
  assert.equal(await badges.nth(i).textContent(),severities[i]);
  if(tone)assert.equal(await badges.nth(i).evaluate((n,t)=>n.classList.contains(`ui-badge--${t}`),tone),true);
 }
 assert.equal(await badges.nth(5).evaluate(n=>n.className.includes('ui-badge--')),false,'unknown severity stays neutral');
 assert.equal(await page.locator('tbody tr').first().evaluate(n=>n.getBoundingClientRect().height),24,'badges retain compact row density');
 assert.equal(await page.locator('tbody tr').first().locator('td').nth(3).getAttribute('class'),'mono','version columns keep their original styling');
 for(const theme of ['light','dark']){
  await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);
  const paints=await page.evaluate(()=>{const samples=[...document.querySelectorAll('tbody .ui-severity-badge')];return {foreground:getComputedStyle(document.body).color,colors:samples.map(n=>({text:getComputedStyle(n).color,bg:getComputedStyle(n).backgroundColor}))};});
  for(const i of [0,1,2,3,4,6])assert.equal(paints.colors[i].text,paints.foreground,'severity uses normal foreground with colored fill');
  assert.notEqual(paints.colors[0].bg,paints.colors[2].bg,'danger and warning have different background colors');
  assert.notEqual(paints.colors[2].bg,paints.colors[4].bg,'warning and info have different background colors');
  await page.screenshot({path:resolve(output,`${engine}-audit-${theme}.png`)});
 }
 await page.getByRole('combobox',{name:'Severity',exact:true}).click();await page.getByRole('option',{name:'MODERATE',exact:true}).click();assert.equal(await page.locator('tbody tr').count(),1);assert.equal(await page.locator('tbody .ui-severity-badge').textContent(),'MODERATE');
 await page.locator('tbody tr').click({button:'right'});await page.getByRole('menuitem',{name:'Show Details…',exact:true}).click();assert.equal(await page.getByRole('dialog').locator('.ui-severity-badge').textContent(),'MODERATE');await page.getByRole('button',{name:'Close',exact:true}).click();
 await page.getByRole('button',{name:'Clear filters',exact:true}).click();await page.getByRole('searchbox').fill('HIGH + unknown');assert.equal(await page.locator('tbody tr').count(),1);assert.equal(await page.locator('tbody .ui-severity-badge').textContent(),'HIGH + unknown');
 await page.setViewportSize({width:320,height:400});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Type/Severity badges, severity tones, unknown qualifiers, 24px rows, original cell values, filtering/search, on-demand detail badges and narrow layout`);
}finally{await browser.close();}
