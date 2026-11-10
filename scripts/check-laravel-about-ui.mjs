// Production About HTML and tab configuration through the shipped hub.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,mkdtempSync,mkdirSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
import {pathToFileURL} from 'node:url';
const scratch=mkdtempSync(join(tmpdir(),'laravel-about-ui-')),htmlPath=join(scratch,'about.html');
execFileSync('cargo',['test','--manifest-path','plugins/laravel/Cargo.toml','--lib','about::','--quiet'],{env:{...process.env,LARAVEL_ABOUT_QA_HTML:htmlPath},stdio:'pipe'});
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:1200,height:650}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);await page.waitForFunction(()=>window.sent.length);
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const expected=['laravel-about','laravel','laravel-models','laravel-views','laravel-components','laravel-middleware','laravel-config','laravel-env','laravel-translations','laravel-generators','laravel-artisan','laravel-runtime','laravel-processes','laravel-external'];
 assert.deepEqual(config.group.tabs.map(tab=>tab.id),expected);
 const registry=JSON.parse(readFileSync('crates/ui-kit/vendor/openidea-icons/SOURCE.json','utf8')).icons;
 assert.equal(new Set(config.group.tabs.map(tab=>tab.icon)).size,14);
 for(const tab of config.group.tabs)assert.ok(registry[tab.icon],`Native icon exists for ${tab.label}`);
 const model={title:'About',activeView:'laravel-about',viewTabs:config.group.tabs,toolbar:[{key:'runtime-about',label:'Refresh App Info',icon:'refresh',enabled:true},{key:'runtime-runner',label:'Local PHP',icon:'terminal',enabled:true}],actions:[],body:{kind:'html',html:readFileSync(htmlPath,'utf8')}};
 await page.evaluate(model=>{const request=window.sent.at(-1);window.receive({...request,project:'/project',model});},model);
 const about=page.locator('.legacy [data-laravel-about]');
 await about.getByRole('heading',{name:'Shop',exact:true}).waitFor();
 assert.match(await about.textContent(),/v12\.3\.0/);assert.match(await about.textContent(),/8\.4/);assert.match(await about.textContent(),/local/);
 assert.equal(await about.locator('script').count(),0);assert.doesNotMatch(await about.textContent(),/APP_KEY|secret-log|private-secret/);
 assert.equal(await page.locator('#view-tabs .view-icon').count(),14);
 await page.getByRole('button',{name:'Refresh App Info',exact:true}).click();assert.equal((await page.evaluate(()=>window.sent.at(-1))).event.key,'runtime-about');await page.evaluate(model=>{const request=window.sent.at(-1);window.receive({...request,project:'/project',model});},model);
 for(const width of [320,1200])for(const theme of ['light','dark']){await page.setViewportSize({width,height:650});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-about-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: About app metadata, runtime facts, escaping, refresh action, reordered tabs, native icons and dock layouts`);
}finally{await browser.close();rmSync(scratch,{recursive:true,force:true});}
