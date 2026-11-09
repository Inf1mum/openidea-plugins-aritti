// Source-backed component audit. Unlike the interaction suite, failures here
// compare rendered CSS/geometry with measured FigCraft node properties.
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const ROOT=fileURLToPath(new URL('../',import.meta.url)),OUT=resolve(ROOT,'crates/ui-kit/audit');
mkdirSync(resolve(OUT,'screenshots'),{recursive:true});
const inventory=JSON.parse(readFileSync(resolve(ROOT,'crates/ui-kit/components.json'),'utf8')).components;
const sets=JSON.parse(readFileSync(resolve(ROOT,'crates/ui-kit/reference/int-ui-kit.json'),'utf8')).sets;
const mapping={Alert:'alert',Banner:'banner',Button:'button',Checkbox:'checkbox','Combo box':'combo','Context menu':'popup',Dialog:'dialog',Dropdown:'dropdown','Group header':'group','Input field':'input',Link:'link',Loader:'loader',Notification:'notification',Popup:'popup','Progress bar':'progress','Radio button':'radio','Segmented control':'segmented',Search:'search',Scrollbar:'scrollbar','Shortcut and code sample':'shortcut',Tab:'tab',Table:'table',Toggle:'toggle',Toolbar:'toolbar',Tooltip:'tooltip','Tooltip: Editor':'editor-tooltip','Tooltip: Got It':'got-it',Tree:'tree','Windows: Main window':'main-window','Windows: Tool window':'tool-window'};
const report={reference:'Int UI Kit (Community).fig',checkedAt:new Date().toISOString(),scope:'30 component categories, light/dark measurements, state checks and screenshots. Content-sized compositions are not pixel-identity claims.',rows:[]};
const rgb=hex=>{const h=hex.replace('#','');return `rgb(${[0,2,4].map(i=>parseInt(h.slice(i,i+2),16)).join(', ')})`;};
function variant(key,theme,extra=()=>true){return sets[key].find(v=>new RegExp(`^Theme\\s*=\\s*${theme==='light'?'Light':'Dark'}(?:,|$)`).test(v.name)&&extra(v));}
function content(v){const root=v.nodes.find(n=>n.id===v.id&&n.fills?.some(f=>f.visible!==false&&f.color));return root||v.nodes.find(n=>n.path.endsWith('/Content')&&n.fills?.some(f=>f.visible!==false&&f.color))||v.nodes.find(n=>n.path.split('/').length===2)||v.nodes[0];}
function paint(n,key){return n?.[key]?.find(p=>p.visible!==false&&p.color)?.color;}
function check(row,property,expected,actual,source){if(expected==null)return;const pass=typeof expected==='number'?Math.abs(expected-actual)<.51:expected===actual;row.checks.push({property,expected,actual,pass,source});}
async function measure(target){return target.evaluate(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return{width:r.width,height:r.height,bg:s.backgroundColor,border:s.borderTopColor,borderWidth:parseFloat(s.borderTopWidth),radius:parseFloat(s.borderTopLeftRadius),font:s.fontFamily,size:parseFloat(s.fontSize),line:parseFloat(s.lineHeight),weight:s.fontWeight,color:s.color,image:s.backgroundImage,shadow:s.boxShadow,minHeight:parseFloat(s.minHeight)};});}
function sourcePaint(row,expected,actual){if(paint(expected,'fills'))check(row,'background',rgb(paint(expected,'fills')),actual.bg,expected.id);if(paint(expected,'strokes'))check(row,'border color',rgb(paint(expected,'strokes')),actual.border,expected.id);if(expected.cornerRadius!=null&&typeof expected.cornerRadius==='number')check(row,'corner radius',expected.cornerRadius,actual.radius,expected.id);}
const browser=await chromium.launch({headless:true});
try{
 const page=await browser.newPage({viewport:{width:1200,height:900},deviceScaleFactor:2});
 const errors=[];page.on('pageerror',error=>errors.push(error.message));
 for(const theme of ['light','dark']){
  await page.goto(pathToFileURL(resolve(ROOT,'crates/ui-kit/preview/inventory.html')).href);
  if(theme==='dark'){await page.getByRole('combobox',{name:'Appearance',exact:true}).click();await page.getByRole('option',{name:'Dark',exact:true}).click();}
  await page.evaluate(()=>document.fonts.ready);
  await page.emulateMedia({reducedMotion:'reduce'});
  for(const component of inventory){
   const key=mapping[component.name],v=variant(key,theme,n=>key==='link'?n.name.includes('Type=Default, State=Default'):key==='banner'?n.name.includes('Type=Info, Inline=False'):key==='editor-tooltip'?n.name.includes('Type=Error, Arrow=Up'):key==='got-it'?n.name.includes('Arrow=Up'):key==='button'?n.name.includes('Primary, State=Active'):key==='toggle'?n.name.endsWith('State=Off'):true),section=page.locator(`[data-component=${JSON.stringify(component.name)}]`);
   const row={name:component.name,theme,sourcePageId:component.sourcePageId,sourceId:v.id,sourceVariant:v.name,sourceVariantCount:sets[key].length,sourceImage:`./source/${key}-${theme}.png`,checks:[],notes:[]};
   let target;
   if(key==='alert'){await section.getByRole('button',{name:'Open alert',exact:true}).click();target=page.locator('#source-alert');sourcePaint(row,content(v),await measure(target));check(row,'width',v.width,(await measure(target)).width,v.id);check(row,'minimum height',v.height,(await measure(target)).minHeight,v.id);}
   else if(key==='banner'){target=section.locator('.ui-banner');const m=await measure(target);sourcePaint(row,content(v),m);check(row,'height',v.height,m.height,v.id);}
   else if(key==='button'){target=section.locator('.ui-button--primary');const n=content(variant(key,theme,x=>x.name.includes('Primary')&&x.name.endsWith('State=Active'))),m=await measure(target);sourcePaint(row,n,m);check(row,'height',n.height,m.height,n.id);check(row,'type size',13,m.size,n.id);check(row,'line height',16,m.line,n.id);}
   else if(['checkbox','radio'].includes(key)){target=section.locator('.ui-choice-control').first();const m=await measure(target);check(row,'logical glyph width',16,m.width,v.id);check(row,'logical glyph height',16,m.height,v.id);check(row,'native control retained',true,await target.locator(`input[type=${key}]`).count()===1,v.id);row.notes.push('All checked, unchecked, mixed, hover, focus, error and disabled paints are exercised by check-ui-choices.mjs.');}
   else if(['combo','dropdown','input','search'].includes(key)){target=section.locator(key==='combo'?'input[role=combobox]':key==='dropdown'?'button[role=combobox]':key==='search'?'input[type=search]':'input.ui-input').first();const n=content(v),m=await measure(target);sourcePaint(row,n,m);check(row,'height',n.height,m.height,n.id);check(row,'type size',13,m.size,n.id);check(row,'line height',16,m.line,n.id);if(key==='search')check(row,'source search glyph',true,m.image.includes(`search-${theme}.svg`),62184);}
   else if(['popup'].includes(key)){
    const context=component.name==='Context menu';await section.getByRole('button',{name:context?'Resource actions':'Show resource information',exact:true}).click();target=page.locator(context?'#resource-menu':'#resource-popup');sourcePaint(row,content(v),await measure(target));if(context)check(row,'menu row height',24,(await measure(target.getByRole('menuitem').first())).height,63555);row.notes.push('Popup content and width depend on the resources supplied by the plugin.');
   }
   else if(key==='dialog'){await section.getByRole('button',{name:'Open preferences',exact:true}).click();target=page.locator('#inventory-dialog');sourcePaint(row,content(v),await measure(target));check(row,'header height',33,(await measure(target.locator('.ui-dialog-header'))).height,71344);check(row,'footer height',57,(await measure(target.locator('.ui-dialog-footer'))).height,71351);row.notes.push('Reference frame is 883×495; live dialog sizes its body to its content and viewport. Native window buttons belong to the host.');}
   else if(key==='group'){target=section.locator('.ui-group-header');const m=await measure(target);check(row,'height',v.height,m.height,v.id);check(row,'type size',13,m.size,v.id);check(row,'line height',16,m.line,v.id);}
   else if(key==='link'){target=section.locator('a');const n=v.nodes.find(n=>n.type==='TEXT'),m=await measure(target);check(row,'link text color',rgb(paint(n,'fills')),m.color,n.id);check(row,'type size',n.fontSize,m.size,n.id);check(row,'line height',Number(n.lineHeight),m.line,n.id);}
   else if(key==='loader'){target=section.locator('.ui-spinner');const m=await measure(target);check(row,'width',v.width,m.width,v.id);check(row,'height',v.height,m.height,v.id);check(row,'source drawing',true,m.image.includes(`loader-${theme}.svg`),v.id);}
   else if(key==='notification'){await section.getByRole('button',{name:'Show notification',exact:true}).click();target=page.locator('.ui-notification').last();const m=await measure(target);check(row,'width',v.width,m.width,v.id);check(row,'minimum height',v.height,m.minHeight,v.id);check(row,'background',rgb(paint(content(v),'fills')),m.bg,v.id);check(row,'radius',v.nodes[0].cornerRadius,m.radius,v.id);}
   else if(key==='progress'){target=section.locator('progress');const n=v.nodes.find(n=>n.path.endsWith('/_Progress')),m=await measure(target);check(row,'track height',n.height,m.height,n.id);check(row,'track paint',rgb(paint(n,'fills')),m.bg,n.id);check(row,'radius',n.cornerRadius,m.radius,n.id);}
   else if(key==='segmented'){target=section.locator('.ui-segmented');const m=await measure(target);check(row,'height',v.height,m.height,v.id);check(row,'radius',v.nodes[0].cornerRadius,m.radius,v.id);}
   else if(key==='scrollbar'){target=section.locator('.ui-scroll-area');check(row,'source thumb color',rgb(theme==='light'?'#D9D9D9':'#48494B'),await target.evaluate(n=>getComputedStyle(n).getPropertyValue('--ui-scroll-thumb').trim()).then(rgb),64002);row.notes.push('Scrollbar visibility and overlay behavior are controlled by the browser/OS.');}
   else if(key==='shortcut'){target=section.locator('kbd');const m=await measure(target);sourcePaint(row,content(v),m);check(row,'height',v.height,m.height,v.id);check(row,'type size',13,m.size,v.id);check(row,'line height',16,m.line,v.id);row.notes.push('Inline and multiline code samples use packaged JetBrains Mono; Skia/browser text rasterization is not claimed identical.');}
   else if(key==='tab'){target=section.locator('[role=tablist]');const selected=target.locator('[aria-selected=true]'),m=await measure(selected);check(row,'tab height',v.height,m.height,v.id);check(row,'underline',3,await selected.evaluate(n=>parseFloat(getComputedStyle(n).borderBottomWidth)),62904);}
   else if(key==='table'){target=section.locator('table');check(row,'header height',24,(await measure(target.locator('th').first())).height,71502);check(row,'row height',24,(await measure(target.locator('td').first())).height,71511);row.notes.push('Columns and rows are plugin data; table frame width is responsive.');}
   else if(key==='toggle'){target=section.locator('input[role=switch]');const m=await measure(target);check(row,'width',v.width,m.width,v.id);check(row,'height',v.height,m.height,v.id);check(row,'source drawing',true,m.image.includes(`toggle-${theme}-off.svg`),v.id);}
   else if(key==='toolbar'){target=section.locator('[role=toolbar]');const m=await measure(target);check(row,'height',v.height,m.height,v.id);check(row,'tool button height',26,(await measure(target.locator('button').first())).height,59969);}
   else if(['tooltip','editor-tooltip','got-it'].includes(key)){
    const trigger=section.locator('button').first();await trigger.focus();target=page.locator(key==='tooltip'?'#refresh-tip':key==='editor-tooltip'?'#editor-tip':'#got-it-tip');await target.waitFor({state:'visible'});const m=await measure(target),n=key==='editor-tooltip'?v.nodes.find(n=>n.path.endsWith('/Tooltip/Content')):key==='got-it'?v.nodes.find(n=>n.path.endsWith('/Body')):v.nodes[0];
    check(row,'background',rgb(paint(n,'fills')),m.bg,n.id);check(row,'radius',n.cornerRadius,m.radius,n.id);if(key==='editor-tooltip')check(row,'border color',rgb(paint(n,'strokes')),m.border,n.id);
    if(key==='got-it')check(row,'body width',n.width,m.width,n.id);if(key!=='tooltip')check(row,'source pointer drawing',true,await target.evaluate(n=>getComputedStyle(n,'::before').backgroundImage!=='none'),n.id);
    row.notes.push('Tooltip text is application content; source paints, corner radii and pointer drawings are checked.');
   }
   else if(key==='tree'){target=section.locator('[role=tree]');const rowNode=target.locator('.ui-tree-row').first();check(row,'row height',v.height,(await measure(rowNode)).height,v.id);check(row,'chevron width',16,(await measure(rowNode.locator('.ui-tree-chevron'))).width,59287);check(row,'source icon',true,await rowNode.locator('.ui-tree-icon').evaluate(n=>getComputedStyle(n).backgroundImage!=='none'),59294);check(row,'source indentation step',22,await target.locator('[role=group]').first().evaluate(n=>parseFloat(getComputedStyle(n).paddingLeft)),v.id);}
   else if(key==='main-window'){target=section.locator('.ui-main-window');const header=await measure(target.locator('.ui-window-header'));check(row,'main toolbar height',40,header.height,68475);check(row,'main toolbar paint',rgb(theme==='light'?'#27282E':'#2B2D30'),header.bg,68475);check(row,'status bar height',29,(await measure(target.locator('.ui-status-bar'))).height,69281);check(row,'navigation inset',12,await target.locator('nav').evaluate(n=>parseFloat(getComputedStyle(n).paddingLeft)),67352);check(row,'navigation row height',24,(await measure(target.locator('nav>.ui-link').first())).height,59284);row.notes.push('Responsive composition, not a replica of the entire 1536×960 IDE. OS window controls and editor internals are host-owned.');}
   else if(key==='tool-window'){target=section.locator('.ui-tool-window');const m=await measure(target);check(row,'reference width',v.width,m.width,v.id);check(row,'minimum height',v.height,m.minHeight,v.id);check(row,'header height',41,(await measure(target.locator('.ui-window-header'))).height,67352);check(row,'background',rgb(paint(v.nodes[0],'fills')),m.bg,v.id);row.notes.push('The body grows with the plugin content; the 360×191 source frame is the starting size.');}
   if(!target)throw new Error(`Missing audit target: ${component.name}`);
   row.liveImage=`./screenshots/${key}-${component.sourcePageId}-${theme}.png`;
   await page.mouse.move(1199,0);
   await target.screenshot({path:resolve(OUT,row.liveImage)});
   row.status=row.checks.every(c=>c.pass)?(row.notes.some(n=>/composition|live dialog sizes/.test(n))?'adapted composition':'measured properties match'):'mismatch';
   report.rows.push(row);
   await page.evaluate(()=>{for(const dialog of document.querySelectorAll('dialog[open]'))dialog.close();for(const popover of document.querySelectorAll(':popover-open'))popover.hidePopover();document.activeElement?.blur();});
  }
 }
 report.errors=errors;
 report.summary={categories:inventory.length,themes:2,specimens:report.rows.length,checks:report.rows.flatMap(r=>r.checks).length,failed:report.rows.flatMap(r=>r.checks).filter(c=>!c.pass).length,adapted:report.rows.filter(r=>r.status==='adapted composition').length};
 writeFileSync(resolve(OUT,'results.json'),JSON.stringify(report,null,2)+'\n');
 writeFileSync(resolve(OUT,'results.js'),'window.uiKitAudit='+JSON.stringify(report)+';\n');
 for(const row of report.rows)for(const c of row.checks)if(!c.pass)console.log(`MISMATCH ${row.name} ${row.theme}: ${c.property}; expected ${c.expected}, got ${c.actual}`);
 console.log(JSON.stringify(report.summary));if(report.summary.failed||errors.length)process.exitCode=1;
}finally{await browser.close();}
