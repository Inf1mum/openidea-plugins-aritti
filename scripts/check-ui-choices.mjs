const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url));
const QA=process.env.UI_QA_DIR||'/tmp/openidea-plugin-ui-qa';mkdirSync(QA,{recursive:true});
const browser=await chromium.launch({headless:true});
try{
  const page=await browser.newPage({viewport:{width:1100,height:850},deviceScaleFactor:2});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/choices.html')).href);
  await page.evaluate(()=>document.fonts.ready);
  const choice=(kind,value,state)=>page.getByRole(kind,{name:`${kind} ${value} ${state}`,exact:true});
  for(const theme of ['light','dark']){
    if(theme==='dark'){await page.getByRole('combobox',{name:'Appearance',exact:true}).click();await page.getByRole('option',{name:'Dark',exact:true}).click();}
    // Every source state must paint, including a radio group with no selection.
    for(const kind of ['checkbox','radio'])for(const value of kind==='checkbox'?['unchecked','checked','indeterminate']:['unchecked','checked'])for(const state of ['active','hovered','focused','invalid','disabled']){
      const input=choice(kind,value,state);
      const visual=await input.evaluate(n=>{const mark=n.nextElementSibling,r=n.getBoundingClientRect(),m=mark.getBoundingClientRect(),s=getComputedStyle(mark);return{input:[r.width,r.height],mark:[m.width,m.height],offset:[m.x-r.x,m.y-r.y],image:s.backgroundImage,outline:getComputedStyle(n).outlineStyle};});
      assert.deepEqual(visual.input,[16,16]);assert.deepEqual(visual.mark,[32,32]);assert.deepEqual(visual.offset,[-8,-8]);
      assert.ok(visual.image.includes(`${kind}-${theme}-${value}-${state}.svg`),`${kind} ${value} ${state}: ${visual.image}`);
      assert.equal(visual.outline,'none','source ring must not be doubled by a CSS outline');
    }
    // Source focus rings must have real painted pixels outside the input box.
    for(const kind of ['checkbox','radio']){
      const input=choice(kind,'checked','active');
      await page.keyboard.press('Tab');await input.focus();
      const mark=input.locator('xpath=following-sibling::span');
      assert.match(await mark.evaluate(n=>getComputedStyle(n).backgroundImage),/focused\.svg/);
      // Wait for CSS image decode, then inspect the actual screenshot pixels.
      await mark.evaluate(async n=>{const image=new Image();image.src=getComputedStyle(n).backgroundImage.slice(5,-2);await image.decode();});
      const png=await mark.screenshot();
      const outside=await page.evaluate(async data=>{
        const image=new Image();image.src=`data:image/png;base64,${data}`;await image.decode();
        const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);const pixels=ctx.getImageData(0,0,canvas.width,canvas.height).data;let count=0;
        for(let y=0;y<canvas.height;y++)for(let x=0;x<canvas.width;x++){
          if(x>=16&&x<48&&y>=16&&y<48)continue;
          const i=(y*canvas.width+x)*4;const[r,g,b]=pixels.slice(i,i+3);if(b>180&&r<100&&g>70&&g<170)count++;
        }
        return count;
      },png.toString('base64'));
      assert.ok(outside>20,`${kind}: clipped focus ring (${outside} exterior pixels)`);
      await page.screenshot({path:resolve(QA,`${kind}-focus-${theme}.png`)});
    }
    await page.mouse.move(0,0);await page.locator('h1').click();
    await page.screenshot({path:resolve(QA,`choices-${theme}.png`),fullPage:true});
  }
  const checkbox=page.getByRole('checkbox',{name:'Include inherited resources',exact:true});
  await checkbox.locator('xpath=ancestor::label').click();assert.equal(await checkbox.isChecked(),true);
  await checkbox.focus();await page.keyboard.press('Space');assert.equal(await checkbox.isChecked(),false);
  await checkbox.evaluate(n=>n.indeterminate=true);assert.equal(await checkbox.evaluate(n=>n.indeterminate),true);
  await page.keyboard.press('Space');assert.equal(await checkbox.evaluate(n=>n.indeterminate),false);assert.equal(await checkbox.isChecked(),true);
  const project=page.getByRole('radio',{name:'Project',exact:true}),workspace=page.getByRole('radio',{name:'Workspace',exact:true});
  await project.focus();await page.keyboard.press('ArrowRight');assert.equal(await workspace.isChecked(),true);assert.equal(await project.isChecked(),false);
  const disabled=choice('checkbox','checked','disabled'),disabledLabel=disabled.locator('xpath=ancestor::label');
  await disabledLabel.scrollIntoViewIfNeeded();const disabledBounds=await disabledLabel.boundingBox();
  await page.mouse.click(disabledBounds.x+disabledBounds.width-2,disabledBounds.y+disabledBounds.height/2);assert.equal(await disabled.isChecked(),true);
  await page.evaluate(()=>{for(let i=0;i<3;i++)window.uiKitControls.enhance(document);});
  assert.equal(await page.locator('.ui-choice-control .ui-choice-control').count(),0,'enhancing again must not nest wrappers');
  // Native choices stay visible to assistive technology in forced-colors mode.
  await page.emulateMedia({forcedColors:'active'});
  assert.equal(await checkbox.evaluate(n=>getComputedStyle(n).opacity),'1');
  assert.equal(await checkbox.locator('xpath=following-sibling::span').evaluate(n=>getComputedStyle(n).display),'none');
  assert.deepEqual(errors,[]);
  console.log('PASS checkbox/radio: 50 theme/state paints, unclipped source rings, label/Space toggles, mixed state, native radio arrows, disabled controls, repeated enhancement, forced colors');
}finally{await browser.close();}
