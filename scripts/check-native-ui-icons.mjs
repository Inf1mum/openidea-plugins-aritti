// Verify pinned host-native SVGs, not lookalike glyph names.
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),kit=resolve(root,'crates/ui-kit');
const source=JSON.parse(readFileSync(resolve(kit,'vendor/openidea-icons/SOURCE.json'),'utf8'));
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const output='/tmp/openidea-native-icon-parity';mkdirSync(output,{recursive:true});
const browser=await engines[engine].launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:800,height:600}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.openidea={postMessage:()=>{},onMessage:()=>{}};});
 await page.goto(pathToFileURL(resolve(root,'plugins/ignore/web/resources/hub.html')).href);
 await page.evaluate(()=>{document.body.replaceChildren();document.body.style.display='block';});
 assert.deepEqual(await page.evaluate(()=>uiKitNativeIcons.names),Object.keys(source.icons));
 const result=await page.evaluate(()=>{
  const host=document.createElement('div');document.body.append(host);const failures=[];
  for(const name of uiKitNativeIcons.names){const svg=uiKitNativeIcons.create(name);host.append(svg);if(svg.getAttribute('width')!=='16'||svg.getAttribute('aria-hidden')!=='true'||svg.dataset.uiIconSource!=='openidea-native')failures.push(name);}
  const ids=[...host.querySelectorAll('[id]')].map(n=>n.id);if(new Set(ids).size!==ids.length)failures.push('duplicate SVG IDs');host.remove();return failures;
 });assert.deepEqual(result,[]);
 for(const theme of ['light','dark'])for(const name of ['folder','plus','settings','star','file-ignored','refresh'])for(const size of [16,20]){
  const entry=source.icons[name],large=size>=18&&entry.large;
  const lightPath=large||entry.light,darkPath=large?entry.largeDark:entry.dark;
  const read=path=>readFileSync(resolve(kit,'vendor/openidea-icons/src',path),'utf8');
  const only=(text,color)=>{const values=new Set((text.match(/#[0-9a-f]{3,8}\b/gi)||[]).map(v=>v.toLowerCase()));return values.size>0&&[...values].every(v=>v===color);};
  // OpenIDEA uses the light geometry for monochrome icons in both themes;
  // only multi-color glyphs switch to the dark SVG geometry.
  const mono=only(read(lightPath),'#6c707e')&&(!darkPath||only(read(darkPath),'#ced0d6'));
  const path=mono?lightPath:theme==='dark'?darkPath||lightPath:lightPath;
  const svg=read(path);
  await page.evaluate(({theme,name,size,svg})=>{
   document.documentElement.dataset.theme=theme;document.body.style.background=theme==='dark'?'#181a1b':'#ffffff';
   const host=document.createElement('div');host.id='pair';host.style.cssText=`display:flex;gap:16px;padding:16px;color:${theme==='dark'?'#CED0D6':'#6C707E'};--ui-icon-color:${theme==='dark'?'#CED0D6':'#6C707E'}`;
   const raw=document.createElement('div');raw.dataset.uiRaw='';raw.innerHTML=svg;const original=raw.querySelector('svg');original.id='reference';original.setAttribute('width',size);original.setAttribute('height',size);original.style.display='block';
   // The host converts standard single-grey SVGs to currentColor, including
   // large glyphs without a dark file. Keep colored native palettes untouched.
   const colors=new Set((original.innerHTML.match(/#[0-9a-f]{3,8}\b/gi)||[]).map(c=>c.toLowerCase()));
   if(colors.size===1&&[...colors].every(c=>['#6c707e','#ced0d6'].includes(c)))for(const n of original.querySelectorAll('*'))for(const a of [...n.attributes])n.setAttribute(a.name,a.value.replace(/#[0-9a-f]{3,8}\b/gi,'currentColor'));
   const native=uiKitNativeIcons.create(name,{size});native.id='native';native.style.display='block';host.append(raw,native);document.body.replaceChildren(host);
  },{theme,name,size,svg});
  const reference=await page.locator('#reference').screenshot(),native=await page.locator('#native').screenshot();
  assert.ok(reference.equals(native),`${engine} ${theme} ${name} ${size}px must match the actual host source pixels`);
 }
 await page.evaluate(()=>{
  const host=document.createElement('div');host.id='integration';host.dataset.uiRaw='';host.innerHTML='<button><span data-ui-icon="update"></span></button>';document.body.append(host);uiKitIcons.enhance(host);
  const normal=document.createElement('button');normal.id='native-placeholder';normal.innerHTML='<span data-ui-icon="update"></span>';document.body.append(normal);uiKitIcons.enhance(normal);
 });
 assert.equal(await page.locator('#integration svg').count(),0,'raw vendor content stays isolated');
 assert.equal(await page.locator('#native-placeholder svg').getAttribute('data-ui-native-name'),'refresh','packaged placeholders select the native registry');
 await page.evaluate(()=>{const host=document.createElement('div');host.id='native-shadow';document.body.append(host);const shadow=host.attachShadow({mode:'open'});shadow.append(uiKitNativeIcons.create('folder'));uiKitControls.enhance(shadow);});
 await page.waitForFunction(()=>document.querySelector('#native-shadow').shadowRoot.querySelector('link')?.sheet);
 for(const theme of ['light','dark']){
  await page.evaluate(t=>document.documentElement.dataset.theme=t,theme);
  assert.equal(await page.locator('#native-shadow .ui-native-light').evaluate(n=>getComputedStyle(n).display),theme==='light'?'inline':'none','shadow-root light variant follows live theme');
  assert.equal(await page.locator('#native-shadow .ui-native-dark').evaluate(n=>getComputedStyle(n).display),theme==='dark'?'inline':'none','shadow-root dark variant follows live theme');
 }
 assert.equal(await page.evaluate(()=>{try{uiKitNativeIcons.create('__proto__');return false}catch{return true}}),true);
 await page.screenshot({path:resolve(output,`${engine}-source-parity.png`)});
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: ${Object.keys(source.icons).length} native names, unique IDs, 24 source-pixel comparisons at 16/20px in light/dark, packaged placeholders, raw isolation and safe unknown names`);
}finally{await browser.close();}
