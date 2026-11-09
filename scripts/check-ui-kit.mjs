// Browser acceptance for the shared component catalog and production hub styles.
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url));
const QA=process.env.UI_QA_DIR || '/tmp/openidea-plugin-ui-qa';
mkdirSync(QA,{recursive:true});
const browser=await chromium.launch({headless:true});
try {
  const page=await browser.newPage({viewport:{width:1180,height:900}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/index.html')).href);
  const hub=page.frameLocator('#workflow');
  await hub.locator('tbody tr').first().waitFor();
  // Shared preview uses production renderers; actions remain fixture-only.
  await hub.getByRole('searchbox').fill('not-a-resource');
  await hub.getByRole('button',{name:'Reset filters',exact:true}).click();
  assert.equal(await hub.locator('tbody tr').count(),4);
  await hub.locator('tbody tr').first().click();
  await hub.getByRole('button',{name:'Close details'}).click();
  assert.equal(await hub.locator('#details').isVisible(),false);
  assert.equal(await hub.locator('tbody tr').first().evaluate(n=>n===document.activeElement),true);
  await hub.getByRole('searchbox').fill('orders');
  assert.equal(await hub.locator('tbody tr').count(),3);
  await hub.getByRole('button',{name:'Clear filters',exact:true}).click();
  await page.getByRole('tab',{name:'Empty results'}).focus();
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.getByRole('tab',{name:'Loading',exact:true}).getAttribute('aria-selected'),'true');
  assert.equal(await page.locator('#loading-panel').isVisible(),true);
  await page.keyboard.press('Home');
  assert.equal(await page.locator('#empty-panel').isVisible(),true);
  const focus=await page.getByRole('tab',{name:'Empty results'}).evaluate(n=>getComputedStyle(n).outlineStyle);
  assert.ok(focus==='solid' || await page.getByRole('tab',{name:'Empty results'}).evaluate(n=>getComputedStyle(n).boxShadow!=='none'));
  const invalid=page.locator('input[aria-invalid=true]');
  assert.ok(await invalid.getAttribute('aria-describedby'));
  await page.getByRole('radio',{name:'Workspace',exact:true}).check();
  assert.equal(await page.getByRole('radio',{name:'Current project',exact:true}).isChecked(),false);
  await page.getByRole('switch',{name:'Enable refresh preview'}).check();
  assert.equal(await page.getByRole('switch',{name:'Enable refresh preview'}).isChecked(),true);
  await page.locator('#rows-demo').focus();await page.keyboard.press('ArrowRight');
  assert.equal(await page.locator('#rows-value').textContent(),'60 rows');
  await page.getByRole('button',{name:'Preview dialog',exact:true}).click();
  assert.equal(await page.getByRole('dialog').isVisible(),true);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('dialog').isVisible(),false);
  assert.equal(await page.getByRole('button',{name:'Preview dialog',exact:true}).evaluate(n=>n===document.activeElement),true);
  await page.getByRole('button',{name:'More actions',exact:true}).click();
  assert.equal(await page.locator('#action-popover').evaluate(n=>n.matches(':popover-open')),true);
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#action-popover').evaluate(n=>n.matches(':popover-open')),false);
  for(const theme of ['light','dark']) {
    await page.getByRole('combobox',{name:'Appearance',exact:true}).click();await page.getByRole('option',{name:theme==='light'?'Light':'Dark',exact:true}).click();
    await hub.locator(`html[data-theme="${theme}"]`).waitFor();
    assert.equal(await hub.locator('html').getAttribute('data-theme'),theme);
    // Preserve the user-requested source paints rather than substituting colors.
    assert.equal(await page.locator('.ui-button--primary').first().evaluate(n=>getComputedStyle(n).backgroundColor),'rgb(53, 116, 240)');
    for(const width of [320,375,414,768,1180]) {
      await page.setViewportSize({width,height:900});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`catalog ${theme} ${width} overflow`);
      assert.equal(await hub.locator('html').evaluate(n=>n.scrollWidth<=n.clientWidth),true,`hub ${theme} ${width} overflow`);
    }
    await page.setViewportSize({width:1180,height:900});
    await page.evaluate(()=>window.scrollTo(0,0));
    await hub.locator('body').evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    await page.screenshot({path:resolve(QA,`ui-kit-${theme}.png`),fullPage:true});
  }
  // Documented host tokens must supersede standalone reference colors and fonts.
  const host=await hub.locator('html').evaluate(n=>{
    n.style.setProperty('--openidea-bg','rgb(12, 34, 56)');
    n.style.setProperty('--openidea-font-size','15px');
    return {bg:getComputedStyle(n.ownerDocument.body).backgroundColor,size:getComputedStyle(n.ownerDocument.body).fontSize};
  });
  assert.deepEqual(host,{bg:'rgb(12, 34, 56)',size:'15px'});
  await hub.locator('html').evaluate(n=>{n.style.removeProperty('--openidea-bg');n.style.removeProperty('--openidea-font-size');});
  await page.emulateMedia({reducedMotion:'reduce'});
  assert.equal(await page.locator('.ui-spinner').first().evaluate(n=>getComputedStyle(n).animationName),'none');
  // Standalone production layout must remain usable in very short bottom docks.
  await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/hub.html')).href);
  await page.locator('tbody tr').first().waitFor();
  await page.setViewportSize({width:900,height:240});
  await page.locator('tbody tr').first().click();
  await page.locator('#content[aria-busy=false]').waitFor();
  await page.getByRole('button',{name:'Open',exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:resolve(QA,'ui-kit-short-dock.png')});
  assert.equal(await page.getByRole('button',{name:'Open',exact:true}).isVisible(),true);
  assert.deepEqual(errors,[]);
  console.log('PASS UI kit: catalog, filters, detail dismissal/focus, keyboard tabs, light/dark widths and source paints, host tokens, reduced motion, short docks');
} finally { await browser.close(); }
