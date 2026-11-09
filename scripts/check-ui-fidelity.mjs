const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync,readFileSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url));
const QA=process.env.UI_QA_DIR||'/tmp/openidea-plugin-ui-qa';mkdirSync(QA,{recursive:true});
const reference=JSON.parse(readFileSync(resolve(ROOT,'crates/ui-kit/reference/exports.json'),'utf8'));
const browser=await chromium.launch({headless:true});
try{
  const page=await browser.newPage({viewport:{width:1200,height:1000},deviceScaleFactor:2});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/parity.html')).href);
  await page.evaluate(()=>document.fonts.ready);
  for(const theme of ['light','dark']){
    if(theme==='dark'){await page.getByRole('combobox',{name:'Appearance',exact:true}).click();await page.getByRole('option',{name:'Dark',exact:true}).click();}
    await page.evaluate(()=>document.fonts.ready);
    assert.equal(await page.evaluate(()=>document.fonts.check('500 13px "Int UI Inter"')),true);
    const button=page.locator('[data-live=button] button');
    const measured=await button.evaluate(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return {w:r.width,h:r.height,font:s.fontFamily,size:s.fontSize,line:s.lineHeight,weight:s.fontWeight,radius:s.borderRadius,border:s.borderColor};});
    assert.equal(measured.w,72);assert.equal(measured.h,28);assert.equal(measured.size,'13px');assert.equal(measured.line,'16px');assert.equal(measured.weight,'500');assert.equal(measured.radius,'4px');assert.match(measured.font,/Int UI Inter/);
    const input=await page.locator('[data-live=input] input').evaluate(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return{w:r.width,h:r.height,bg:s.backgroundColor,border:s.borderColor};});
    assert.equal(input.w,224);assert.equal(input.h,28);
    assert.equal(input.bg,theme==='light'?'rgb(255, 255, 255)':'rgb(43, 45, 48)');
    assert.equal(input.border,theme==='light'?'rgb(201, 204, 214)':'rgb(78, 81, 87)');
    const toggle=await page.locator('[data-live=toggle] input').boundingBox();assert.equal(toggle.width,53);assert.equal(toggle.height,20);
    const combo=page.getByRole('combobox',{name:'Parity combobox',exact:true});
    await page.mouse.move(0,0);
    const arrow=await combo.evaluate(n=>getComputedStyle(n).backgroundImage);
    assert.ok(arrow.includes(`chevron-${theme}.svg`),'combobox source arrow');
    await combo.hover();
    assert.equal(await combo.evaluate(n=>getComputedStyle(n).backgroundImage),arrow,'combobox arrow survives hover');
    await combo.focus();
    assert.equal(await combo.evaluate(n=>getComputedStyle(n).backgroundImage),arrow,'combobox arrow survives focused hover');
    await combo.evaluate(n=>{n.parentElement.querySelector('select').disabled=true;});
    await page.waitForFunction(()=>document.querySelector('[data-live=combo] input').disabled);
    assert.equal(await combo.evaluate(n=>getComputedStyle(n).backgroundImage),arrow,'combobox arrow survives disabled state');
    await combo.evaluate(n=>{n.parentElement.querySelector('select').disabled=false;});
    await page.waitForFunction(()=>!document.querySelector('[data-live=combo] input').disabled);
    await combo.blur();await page.mouse.move(0,0);
    for(const live of await page.locator('[data-live]').all()){
      const name=await live.getAttribute('data-live'),source=reference.find(r=>r.name===name&&r.theme===theme),bounds=await live.boundingBox();
      assert.equal(Math.round(bounds.width),source.width,`${name} source width`);assert.equal(Math.round(bounds.height),source.height,`${name} source height`);
      await live.evaluate(n=>{const r=n.getBoundingClientRect();n.style.transform=`translate(${Math.round(r.x)-r.x}px,${Math.round(r.y)-r.y}px)`;});
      await live.screenshot({path:resolve(QA,`int-live-${name}-${theme}.png`)});
    }
    await page.evaluate(()=>scrollTo(0,0));await page.screenshot({path:resolve(QA,`int-parity-${theme}.png`),fullPage:true});
  }
  assert.deepEqual(errors,[]);
  console.log('PASS FigCraft source metrics: Inter 500 13/16, original paints, 72×28 buttons, 224×28 fields, 53×20 toggles, reference-sized controls; source/live screenshots captured');
}finally{await browser.close();}
