// Model hierarchy through the shipped shared tree renderer.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const browser=await engines[engine].launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:1100,height:650}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 await page.goto(pathToFileURL(resolve('plugins/laravel/web/resources/hub.html')).href);await page.waitForFunction(()=>window.sent.length);
 const config=JSON.parse(readFileSync('plugins/laravel/src/ui_config.json','utf8'));
 const items=[{key:'model-source:post',label:'App\\Models\\Post',detail:'posts · app/Models/Post.php',icon:'class',expandable:true,expanded:false},
 {key:'model-group:fields',parent:'model-source:post',label:'Fields (2)',searchText:'App\\Models\\Post',icon:'folder',expandable:true,expanded:true},
 {key:'model-member:title',parent:'model-group:fields',label:'title',detail:'string · database/migrations/create_posts.php',searchText:'App\\Models\\Post',icon:'field'},
 {key:'model-member:date',parent:'model-group:fields',label:'created_at',detail:'timestamp · database/migrations/create_posts.php',searchText:'App\\Models\\Post',icon:'field'},
 {key:'model-group:database',parent:'model-source:post',label:'Database fields (1)',searchText:'App\\Models\\Post',icon:'folder',expandable:true,expanded:true},
 {key:'model-runtime:note',parent:'model-group:database',label:'note',detail:'Captured database schema',searchText:'App\\Models\\Post',icon:'field',activationEnabled:false},
 {key:'model-source:user',label:'App\\Models\\User',detail:'users · app/Models/User.php',icon:'class',expandable:true,expanded:false},
 {key:'model-group:user-fields',parent:'model-source:user',label:'Fields (1)',searchText:'App\\Models\\User',icon:'folder',expandable:true,expanded:true},
 {key:'model-member:email',parent:'model-group:user-fields',label:'email',detail:'string · database/migrations/create_users.php',searchText:'App\\Models\\User',icon:'field'}];
 const model={title:'Models',activeView:'laravel-models',viewTabs:config.group.tabs,toolbar:[],actions:[],contextButtons:[],body:{kind:'tree',searchPlaceholder:'Filter models, fields, relations and scopes…',items}};
 async function answer(){await page.evaluate(model=>{const request=window.sent.findLast(r=>r.channel==='ui-kit'&&r.op!=='notify');window.receive({...request,project:'/fixture',model});},model);}
 async function toggle(key,expanded){items.find(item=>item.key===key).expanded=expanded;await answer();}
 const rows=()=>page.locator('tbody tr');
 await answer();assert.equal(await rows().count(),2,'start with only model roots');assert.equal(await page.locator('thead th').count(),2,'no repeated Model/Kind/File columns');
 await page.getByRole('button',{name:'Expand App\\Models\\Post',exact:true}).click();let event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.kind,'expand');assert.equal(event.key,'model-source:post');await toggle(event.key,true);
 assert.equal(await rows().count(),7);assert.equal(await page.locator('[data-resource-key="model-member:title"]').count(),1);
 await page.locator('[data-resource-key="model-member:title"]').dblclick();event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.kind,'activate');assert.equal(event.key,'model-member:title');await answer();
 await page.getByRole('button',{name:'Collapse Fields (2)',exact:true}).click();event=await page.evaluate(()=>window.sent.at(-1).event);await toggle(event.key,false);assert.equal(await rows().count(),5);
 await page.locator('#search').fill('title');assert.equal(await rows().count(),3,'search reveals a matching field with its ancestors');assert.match(await page.locator('tbody').textContent(),/App\\Models\\Post/);
 await page.locator('#search').fill('App\\Models\\Post');assert.equal(await rows().count(),6,'searching a model includes its members');
 await page.locator('#search').fill('');await toggle('model-group:fields',true);
 const before=await page.evaluate(()=>window.sent.filter(r=>r.event?.kind==='activate').length);await page.locator('[data-resource-key="model-runtime:note"]').dblclick();assert.equal(await page.evaluate(()=>window.sent.filter(r=>r.event?.kind==='activate').length),before,'runtime metadata is not a source destination');await answer();
 await page.locator('[data-resource-key="model-source:post"]').focus();await page.keyboard.press('ArrowLeft');event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.kind,'collapse');await toggle(event.key,false);assert.equal(await rows().count(),2);
 for(const [kind,label,detail] of [['factories','App\\PostMaker','declared model class constant · app/PostMaker.php'],['resources','App\\PostResource','declared model PHPDoc · app/PostResource.php'],['migrations','001.php','literal schema table · database/migrations/001.php'],['accessors','title','string · app/Models/BasePost.php'],['casts','enabled','boolean · app/Models/BasePost.php'],['scopes','active','(bool $enabled = true) → Builder · app/Models/BasePost.php']]){items.push({key:`group:${kind}`,parent:'model-source:post',label:`${kind} (1)`,icon:'folder',expandable:true,expanded:true},{key:`fact:${kind}`,parent:`group:${kind}`,label,detail,icon:'method'});}
 await toggle('model-source:post',true);await toggle('model-source:user',true);
 await page.locator('#search').fill('PostMaker');assert.equal(await rows().count(),3,'source-owned factory search includes ancestors');await page.locator('[data-resource-key="fact:factories"]').dblclick();event=await page.evaluate(()=>window.sent.at(-1).event);assert.equal(event.kind,'activate');assert.equal(event.key,'fact:factories');await answer();await page.locator('#search').fill('');
 for(const width of [320,1100])for(const theme of ['light','dark']){await page.setViewportSize({width,height:650});await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));if(process.env.UI_QA_DIR){mkdirSync(process.env.UI_QA_DIR,{recursive:true});await page.screenshot({path:resolve(process.env.UI_QA_DIR,`${engine}-models-tree-${theme}-${width}.png`)});}}
 assert.deepEqual(errors,[]);console.log(`PASS ${engine}: Models tree groups, expansion, source navigation, ancestor search, keyboard, runtime metadata and dock layouts`);
}finally{await browser.close();}
