const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(pathToFileURL(resolve('crates/ui-kit/web/hub.html')).href);
 await page.evaluate(()=>{
  const fixture=document.createElement('section');fixture.id='tooltip-fixture';fixture.innerHTML=`
   <button id="labelled" aria-label="Expand templates"><svg aria-hidden="true"></svg></button>
   <button id="text"><svg aria-hidden="true"></svg>Rescan</button>
   <span id="settings-label">Preferences</span><button id="referenced" aria-labelledby="settings-label"><svg></svg></button>
   <button id="explicit" title="Refresh project data" aria-label="Refresh"><svg></svg></button>
   <button id="plain">Plain text action</button>
   <div data-ui-raw><button id="raw" aria-label="Vendor control"><svg></svg></button></div>`;
  document.body.append(fixture);
  const host=document.createElement('div');fixture.append(host);const shadow=host.attachShadow({mode:'open'});
  shadow.innerHTML='<button aria-label="Close preview"><svg></svg></button>';window.tooltipShadow=shadow;window.uiKitControls.enhance(shadow);
 });
 await page.waitForFunction(()=>document.querySelector('#labelled').title==='Expand templates');
 assert.equal(await page.locator('#text').getAttribute('title'),'Rescan');
 assert.equal(await page.locator('#referenced').getAttribute('title'),'Preferences');
 assert.equal(await page.locator('#explicit').getAttribute('title'),'Refresh project data');
 assert.equal(await page.locator('#plain').getAttribute('title'),null);
 assert.equal(await page.locator('#raw').getAttribute('title'),null);
 assert.equal(await page.evaluate(()=>window.tooltipShadow.querySelector('button').title),'Close preview');
 await page.locator('#labelled').evaluate(button=>button.setAttribute('aria-label','Collapse templates'));
 await page.waitForFunction(()=>document.querySelector('#labelled').title==='Collapse templates');
 await page.locator('#text').evaluate(button=>button.lastChild.textContent='Rescan dependencies');
 await page.waitForFunction(()=>document.querySelector('#text').title==='Rescan dependencies');
 await page.locator('#explicit').evaluate(button=>button.setAttribute('aria-label','Reload'));
 assert.equal(await page.locator('#explicit').getAttribute('title'),'Refresh project data');
 assert.deepEqual(errors,[]);
 console.log(`PASS ${engine}: icon button tooltips, dynamic labels, referenced labels, shadow roots, explicit tooltip preservation and vendor isolation`);
} finally {await browser.close();}
