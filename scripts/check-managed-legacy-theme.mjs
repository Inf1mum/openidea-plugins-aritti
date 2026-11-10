// Render shared source assets with real retained legacy CSS/templates. No plugin copies mutated.
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {readFileSync,cpSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),scratch=mkdtempSync(join(tmpdir(),'managed-legacy-theme-'));
const source=readFileSync(resolve(root,'plugins/gitlab/src/review.rs'),'utf8');
const defaults=JSON.parse(source.match(/const THEME_DEFAULTS: &str = ("(?:\\.|[^"\\])*");/)[1]);
const controls=source.match(/pub const CONTROLS: &str = r#"([\s\S]*?)"#;/)[1];
const style=source.match(/const STYLE: &str = r#"([\s\S]*?)"#;/)[1];
for(const name of ['gitlab','kube-explorer']) {
 const target=join(scratch,name);cpSync(resolve(root,'crates/ui-kit/web'),target,{recursive:true});
 cpSync(resolve(root,`plugins/${name}/web/resources/legacy.js`),join(target,'legacy.js'));
}
const browser=await engines[process.env.BROWSER_ENGINE||'chromium'].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:800,height:600}}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 const standalone=await browser.newPage();
 await standalone.setContent(`<style>${defaults}${controls}</style><textarea>Standalone fallback</textarea>`);
 assert.equal(await standalone.locator('html').evaluate(n=>getComputedStyle(n).getPropertyValue('--openidea-bg').trim()),'Canvas');
 assert.equal(await standalone.locator('textarea').getAttribute('class'),null);
 await standalone.close();
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=s=>listeners.forEach(f=>f(s));});
 await page.goto(pathToFileURL(join(scratch,'gitlab/hub.html')).href);
 async function reply(html) {await page.evaluate(html=>{const request=window.sent.at(-1);window.receive(JSON.stringify({channel:'ui-kit',frameId:request.frameId,sequence:request.sequence,project:'/fixture',model:{title:'Theme fixture',plugin:'Fixture',toolbar:[],body:{kind:'html',html}}}));},html);}
 const review=`<style>${defaults}${controls}${style}</style><body><div class="file" data-new-path="src/a.rs" data-old-path="src/a.rs"><table><tr id="L1" data-new="1"><td class="ln" tabindex="0">1</td><td>code</td></tr></table></div></body>`;
 await reply(review);await page.locator('.ln').click();
 const comment=page.getByLabel('Comment',{exact:true}),submit=page.getByRole('button',{name:'Add to Review',exact:true});
 await submit.waitFor();
 for(const [theme,bg,input,fg,font,mono] of [['light','#e7edf4','#f2f7fa','#123456','Courier New','Arial'],['dark','#16232f','#203344','#dceeff','Arial','Courier New']]) {
  await page.evaluate(({theme,bg,input,fg,font,mono})=>{const node=document.documentElement;node.dataset.theme=theme;for(const [key,value]of Object.entries({'--openidea-bg':bg,'--openidea-bg-input':input,'--openidea-fg':fg,'--openidea-font-ui':`"${font}"`,'--openidea-font-mono':`"${mono}"`,'--openidea-font-size':'15px'}))node.style.setProperty(key,value);},{theme,bg,input,fg,font,mono});
  await page.waitForFunction(({fg,font})=>{const shadow=document.querySelector('.legacy').shadowRoot;const input=shadow.querySelector('textarea');const style=getComputedStyle(input);return style.fontFamily.startsWith(font)||style.fontFamily.startsWith(`"${font}"`);},{fg,font});
  const paints=await comment.evaluate(n=>{const s=getComputedStyle(n),host=n.getRootNode().host,h=getComputedStyle(host);return {font:s.fontFamily,bg:s.backgroundColor,fg:s.color,hostBg:h.backgroundColor,hostFg:h.color,classes:[...n.classList]};});
  assert.match(paints.font,new RegExp(font));assert(paints.classes.includes('ui-textarea'));
  const expected=await page.evaluate(({bg,input,fg})=>{const n=document.createElement('div');document.body.append(n);const rgb=value=>{n.style.color=value;return getComputedStyle(n).color;};const result={bg:rgb(bg),input:rgb(input),fg:rgb(fg)};n.remove();return result;},{bg,input,fg});
  assert.equal(paints.hostBg,expected.bg);assert.equal(paints.hostFg,expected.fg);assert.equal(paints.bg,expected.input);assert.equal(paints.fg,expected.fg);
  assert.equal(await submit.evaluate(n=>getComputedStyle(n).fontSize),'15px');
  await submit.hover();assert.equal(await submit.evaluate(n=>getComputedStyle(n).filter),'none');
 }
 await comment.fill('Theme-safe draft');await submit.click();assert.equal(await page.evaluate(()=>JSON.parse(window.sent.at(-1).event.value).body),'Theme-safe draft');
 // Ordinary managed form controls are kit controls, while unknown vendor markup is inert.
 await reply(`<style>${defaults}${controls}</style><body><form><label>Name<input id="name"></label><input type="button" value="Run"><select aria-label="Mode"><option>One</option></select><button disabled>Disabled</button></form><div data-ui-raw id="vendor"><div role="tablist"><button role="tab" id="vendor-tab">Vendor</button></div><div data-ui-tree><div role="treeitem" id="vendor-tree">Vendor tree</div></div><select id="vendor-select"><option>Keep native</option></select><span data-ui-icon="update" id="vendor-icon">Keep text</span><span data-ui-icon="not-a-kit-icon">Unknown</span></div><span data-ui-icon="not-a-kit-icon" id="unknown">Unknown outside</span></body>`);
 await page.waitForFunction(()=>document.querySelector('.legacy').shadowRoot.querySelector('#name').classList.contains('ui-input'));
 const raw=await page.locator('#vendor').innerHTML();
 await page.evaluate(()=>{const r=document.querySelector('.legacy').shadowRoot;window.uiKitControls.enhance(r);window.uiKitWidgets.enhance(r);window.uiKitIcons.enhance(r);});
 assert.equal(await page.locator('#vendor').innerHTML(),raw);assert.equal(await page.locator('#vendor-icon').innerText(),'Keep text');
 assert.equal(await page.locator('#vendor-tab').getAttribute('class'),null);assert.equal(await page.locator('#unknown').innerText(),'Unknown outside');
 assert.equal(await page.locator('#vendor [role=tablist]').getAttribute('class'),null);
 assert.equal(await page.locator('#vendor-tab').getAttribute('tabindex'),null);
 assert.equal(await page.locator('#vendor-tree').innerHTML(),'Vendor tree');
 assert.equal(await page.locator('#vendor-select').getAttribute('class'),null);
 assert.equal(await page.locator('#vendor .ui-select-shell').count(),0);
 const nestedRaw=await page.evaluate(()=>{
  const vendor=document.querySelector('.legacy').shadowRoot.querySelector('#vendor');
  const host=document.createElement('div');vendor.append(host);const shadow=host.attachShadow({mode:'open'});
  shadow.innerHTML='<div role="tablist"><button role="tab">Vendor nested tab</button></div><select><option>Native option</option></select><a href="https://example.invalid">Vendor link</a><span data-ui-icon="update">Untouched icon</span>';
  const before=shadow.innerHTML;window.uiKitControls.enhance(shadow);window.uiKitWidgets.enhance(shadow);window.uiKitIcons.enhance(shadow);
  return {before,after:shadow.innerHTML};
 });
 assert.equal(nestedRaw.after,nestedRaw.before,'raw isolation crosses nested shadow host boundaries');
 assert.equal(await page.locator('input[type=button]').evaluate(n=>n.classList.contains('ui-button')),true);
 assert.equal(await page.getByRole('combobox',{name:'Mode',exact:true}).count(),1);
 // Production log CSS (including its standalone Canvas fallback) inherits managed colors/fonts.
 const logs=readFileSync(resolve(root,'plugins/kube-explorer/src/logs.rs'),'utf8');
 const encoded=logs.match(/"(<style>:root\{\{--bg:Canvas[\s\S]*?)",\s*body\.join/)[1].replace(/\\\r?\n\s*/g,'');
 const logHtml=JSON.parse(`"${encoded}"`).replaceAll('{white_space}','pre-wrap').replaceAll('{{','{').replaceAll('}}','}').replace('{}','api | INFO ready');
 await reply(logHtml);assert.match(await page.locator('pre').evaluate(n=>getComputedStyle(n).fontFamily),/Courier New/);
 const doc=`<style>${defaults}body{color:var(--openidea-fg);background:var(--openidea-bg)}pre{font-family:var(--openidea-font-mono)}</style><article><h2>Documentation</h2><pre class="ui-code">@Bean OrderService</pre></article>`;
 await reply(doc);assert.match(await page.locator('pre').evaluate(n=>getComputedStyle(n).fontFamily),/Courier New/);
 // Raw production syntax overlays retain the same pre/textarea sizing and typography.
 await page.goto(pathToFileURL(join(scratch,'kube-explorer/hub.html')).href);
 const html=readFileSync(resolve(root,'plugins/kube-explorer/src/page.html'),'utf8').replaceAll('{{highlighted}}','kind: Pod').replaceAll('{{lang}}','yaml').replaceAll('{{name}}','pod').replaceAll('{{callback}}','qa-theme').replaceAll('{{readonly}}','').replaceAll('{{text}}','kind: Pod').replaceAll('{{nojs}}','');
 await reply(html);
 const geometry=await page.locator('#src').evaluate(n=>{const pre=n.getRootNode().querySelector('#hl'),a=getComputedStyle(n),b=getComputedStyle(pre);return {raw:n.hasAttribute('data-ui-raw'),classes:[...n.classList],src:[a.fontFamily,a.fontSize,a.lineHeight,a.padding,a.tabSize],pre:[b.fontFamily,b.fontSize,b.lineHeight,b.padding,b.tabSize],position:a.position,color:a.color,background:a.backgroundColor};});
 assert.equal(geometry.raw,true);assert(!geometry.classes.includes('ui-textarea'));assert.deepEqual(geometry.src,geometry.pre);assert.equal(geometry.position,'absolute');assert.equal(geometry.color,'rgba(0, 0, 0, 0)');assert.equal(geometry.background,'rgba(0, 0, 0, 0)');
 assert.deepEqual(errors,[]);
 console.log('PASS managed legacy theme/font inheritance and live updates; real review/log templates, kit controls, raw vendor isolation and syntax-overlay geometry');
} finally {await browser.close();rmSync(scratch,{recursive:true,force:true});}
