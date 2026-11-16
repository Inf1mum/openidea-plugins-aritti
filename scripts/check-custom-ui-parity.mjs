// Exercise every shipped custom hub with representative models and its real UI policy.
// Fixtures validate packaged presentation; they are not live-app acceptance.
const engines=await import(process.env.PLAYWRIGHT_MODULE||'@playwright/test');
import assert from 'node:assert/strict';
import {readdirSync,readFileSync,existsSync,mkdirSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const engine=process.argv.includes('--webkit')?'webkit':'chromium';
const output=process.env.UI_QA_DIR||'/tmp/openidea-custom-ui-review-20261110/browser';mkdirSync(output,{recursive:true});
const button=(key,label,icon)=>({key,label,icon,enabled:true});
const tree=(label='Source',icon='class')=>({kind:'tree',items:[{key:'group:source',label:'Sources',icon:'folder',expandable:true,expanded:true},{key:'source:one',parent:'group:source',label,icon}]});
const form={kind:'form',fields:[{key:'groupId',label:'Group',value:{kind:'text',text:'com.example'}},{key:'dependencies',label:'Dependencies',value:{kind:'text',text:'web'}}]};
const route={kind:'resources',columns:['Method','Path','Controller'],facets:[0],items:[{key:'route:1',cells:['GET','/users','UserController']},{key:'route:2',cells:['POST','/users','UserController']}]};
const graph={kind:'html',html:'<style>svg{width:100%;height:200px}.n rect{fill:var(--ui-bg);stroke:var(--ui-border)}text{fill:var(--ui-fg);font:13px var(--ui-font)}</style><svg viewBox="0 0 280 200"><g class="n" data-id="source:one" tabindex="0"><rect x="20" y="30" width="220" height="40"/><text x="30" y="55">Source node</text></g></svg>'};
const fixtures={
 'gitlab/merge-requests':{body:tree('!42 Review change','github-pr-open'),toolbar:[button('filter','Filter…','filter')]},
 'gitlab/merge-request':{body:{kind:'resources',columns:['Job','Status'],facets:[1],items:[{key:'j:42',cells:['Build','Failed']}]},toolbar:[button('pipeline-action:retry','Retry Job','rerun'),button('pipeline-action:browser','Open on GitLab','external-link')]},
 'gitlab/gitlab-projects':{body:tree('Project','folder')},
 'spring-integration/structure':{body:graph,toolbar:[button('graph-zoom-out','Zoom Out','zoom-out'),button('graph-zoom-in','Zoom In','zoom-in')]},
 'spring-beans/beans':{body:tree('userService','class')},
 'spring-runtime/runtime-graph':{body:graph,toolbar:[button('graph-observations','Inspect Snapshot List','list')]},
 'ignore/templates':{body:{kind:'tree',items:[{key:'g:user',label:'User templates',icon:'folder',expandable:true,expanded:true},{key:'u:mine',parent:'g:user',label:'.gitignore',icon:'file-ignored',activationLabel:'Add to Ignore File',activationIcon:'add',activationEnabled:true}]},toolbar:[button('new','Create User Template','plus'),button('delete','Delete User Template','trash'),button('star','Unstar','star'),button('preferences','Preferences','settings')]},
 'spring-initializr/initializr':{body:form,toolbar:[button('choose-dependencies','Dependencies','package'),button('generate','Generate Project','export')]},
 'openapi/endpoints':{body:route},
 'openapi/compare':{body:{kind:'resources',columns:['Type','Method','Path'],facets:[0],items:[{key:'change:1',cells:['Added','GET','/users']}] }},
 'dependency-audit/vulnerabilities':{body:{kind:'resources',columns:['Type','Dependency','Severity'],facets:[0,2],items:[{key:'dep:example',cells:['Dependency','example-lib','HIGH']},{key:'advisory:example|GHSA-example',cells:['Advisory','example-lib','LOW']}]},toolbar:[button('review-ignores','Review Scoped Ignores','list')]},
 'spring-web/endpoints':{body:route},
 'laravel/laravel':{body:route,toolbar:[button('optimize-clear','Clear Caches','build-project')]},
 'symfony/symfony':{body:route,toolbar:[button('cache-warmup','Warm Up Cache','build-project')]},
 'symfony/symfony-profiler':{body:{kind:'resources',columns:['Method','Status','URL'],facets:[0,1],items:[{key:'profile:1',cells:['GET','200','/users']}]},toolbar:[button('choose-profiler-source','Choose CSV Source…','folder-open')]},
 'kube-explorer/resource-details':{body:{kind:'tree',items:[{key:'node:pods',label:'Pods',icon:'ext:fileIcon/aritti.kube-explorer/pod',expandable:true,expanded:true},{key:'res:pods/web',parent:'node:pods',label:'web',icon:'ext:fileIcon/aritti.kube-explorer/pod'}]},actions:[{id:'resource-describe',label:'Describe',icon:'info',keys:['res:pods/web'],replacesOpen:true}]}
};
Object.assign(fixtures, {
 'ddev/setup':{body:{kind:'form',fields:[
  {key:'project',label:'Project',value:{kind:'readonly-text',data:{text:'/fixture/project'}}},
  {key:'config-status',label:'Configuration',value:{kind:'readonly-text',data:{text:'Not configured. Configure Project creates .ddev/config.yaml.'}}},
  ...[['watchDdev','Refresh status every minute'],['checkForUpdates','Check for updates automatically'],['autoConfigureDataSource','Maintain database connection configuration'],['autoConfigureRunTarget','Offer DDEV for PHP, Node.js, Composer and project runs']].map(([key,label])=>({key,label,value:{kind:'checkbox',checked:true}})),
  {key:'containerRoot',label:'Project path inside the web container',value:{kind:'text',text:'/var/www/html'}},
  {key:'phpVersion',label:'PHP version override (optional)',value:{kind:'text',text:''}},
  {key:'execution-help',label:'Run and debug',value:{kind:'readonly-text',data:{text:'Choose DDEV in Run/Debug settings. Enable Xdebug in DDEV for PHP debugging.'}}},
  {key:'diagnostics',label:'Environment',value:{kind:'readonly-text',data:{text:'Use Check Environment to read installed DDEV and Docker versions.'}}},
  {key:'binary-help',label:'DDEV executable',value:{kind:'readonly-text',data:{text:'Choose and approve the executable in plugin permissions.'}}},
  {key:'steps',label:'Setup',value:{kind:'readonly-text',data:{text:'Install DDEV, configure the project and start it from Services.'}}}
 ]},toolbar:[button('diagnose','Check Environment','refresh'),button('config','Configure Project','settings'),button('show-services','Show Services','open-in-tool-window'),button('save-settings','Save Preferences','check'),button('reset-settings','Reset Changes','undo'),button('open-docs','DDEV Documentation','help')]},
 'openapi/try-response':{body:{kind:'form',fields:[{key:'review',label:'Request / Response',value:{kind:'readonly-text',data:{text:'GET http://127.0.0.1:8080/users\nNot sent'}}}]},toolbar:[button('send:fixture','Send Reviewed Request','run')]},
 'symfony/twig-paths':{body:{kind:'form',fields:[{key:'paths',label:'Namespace paths',value:{kind:'multiline',text:'App: templates/'}}]},toolbar:[button('save','Save','check')]},
 'kube-explorer/resource-watch':{body:{kind:'form',fields:[{key:'status',label:'Status',value:{kind:'label',data:'No resource watch prepared'}}]}},
 'flyway/migrations':{body:{kind:'resources',columns:['Version','Description','State'],facets:[2],items:[{key:'migration:1',cells:['1','Create users','Success']}]}},
 'flyway/command-output':{body:{kind:'html',html:'<pre>Successfully applied one migration.</pre>'}},
 'flyway/comparison-review':{body:{kind:'html',html:'<pre>Reviewed source comparison</pre>'}},
 'liquibase/changesets':{body:{kind:'resources',columns:['ID','Author','File'],facets:[1],items:[{key:'change:1',cells:['create-users','example','changelog.xml']}]}},
 'liquibase/offline-diff':{body:{kind:'resources',columns:['Type','Object','Change'],facets:[0],items:[{key:'observation:100',cells:['Table','users','Added']}]},toolbar:[button('offline-previous','Previous Observations','arrow-left'),button('offline-next','Next Observations','arrow-right')]},
 'ideolog/patterns':{body:{kind:'form',fields:[{key:'pattern',label:'Pattern',value:{kind:'text',text:'timestamp level message'}}]}},
 'ideolog/log-snapshot':{body:{kind:'resources',columns:['Time','Severity','Message'],facets:[1],items:[{key:'event:100',cells:['12:00:00','INFO','Started application']}]}},
 'spring-runtime/runtime':{body:tree('Spring application','spring')},
 'spring-runtime/runtime-snapshot':{body:{kind:'resources',columns:['Observation','Value'],facets:[],items:[{key:'snapshot:1',cells:['Health','UP'],details:[{label:'Provenance',value:'Owned bounded snapshot'}]}]}},
 'spring-runtime/runtime-controls':{body:{kind:'message',text:'Start or stop bounded refresh polling in the controls view.'}}
});
const chrome={
 'gitlab/merge-requests':[button('refresh','Refresh','refresh')],
 'gitlab/merge-request':[button('refresh','Refresh','refresh')],
 'gitlab/gitlab-projects':[button('refresh','Refresh','refresh')],
 'spring-integration/structure':[button('refresh','Refresh','refresh')],
 'spring-beans/beans':[button('refresh','Refresh','refresh')],
 'spring-runtime/runtime-graph':[], 'ignore/templates':[],
 'spring-initializr/initializr':[button('more-actions','More Actions…','more-horizontal')],
 'openapi/endpoints':[button('refresh','Refresh','refresh'),button('routes:filter','Filter…','filter')],
 'openapi/compare':[button('refresh','Refresh','refresh')],
 'dependency-audit/vulnerabilities':[button('scan','Rescan','refresh')],
 'spring-web/endpoints':[button('refresh','Refresh','refresh')],
 'laravel/laravel':[button('reindex','Reindex','refresh')],
 'symfony/symfony':[button('reindex','Reindex','refresh')],
 'symfony/symfony-profiler':[button('refresh-profiler','Refresh Index Snapshot','refresh')],
 'kube-explorer/resource-details':[button('refresh','Refresh','refresh')]
};
const browser=await engines[engine].launch({headless:true});const receipt=[];
try {
 const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(()=>{window.sent=[];const listeners=[];window.openidea={postMessage:s=>window.sent.push(JSON.parse(s)),onMessage:f=>listeners.push(f)};window.receive=m=>listeners.forEach(f=>f(JSON.stringify(m)));});
 for(const plugin of readdirSync('plugins')) {
  if(process.env.UI_QA_PLUGIN && plugin !== process.env.UI_QA_PLUGIN) continue;
  if((process.env.UI_QA_EXCLUDE||'').split(',').includes(plugin))continue;
  const path=`plugins/${plugin}/src/ui_config.json`;if(!existsSync(path))continue;const config=JSON.parse(readFileSync(path,'utf8'));
  for(const [view,title]of Object.entries(config.views)) {
   const id=`${plugin}/${view}`,fixture=fixtures[id];assert.ok(fixture,`missing UI fixture for ${id}`);
   await page.goto(pathToFileURL(resolve(`plugins/${plugin}/web/resources/hub.html`)).href);
   const model={title,plugin:config.title,viewTabs:config.group?.tabs||[],activeView:config.group?view:undefined,body:fixture.body,toolbar:[...(chrome[id]||[]),...(fixture.toolbar||[]),{key:'unavailable',label:'Unavailable',icon:'trash',enabled:false}],actions:fixture.actions||config.actions.filter(action=>action.view===view).map(action=>({...action,replacesOpen:action.replaces_open===true})),fieldActions:config.field_actions||{},contextButtons:config.context_buttons?.[view],iconButtons:config.toolbar_icon_buttons?.[view]};
   const reply=()=>page.evaluate(model=>{const r=window.sent.at(-1);window.receive({...r,project:'/fixture',model});},model);await reply();
   for(const width of [320,768])for(const theme of ['light','dark']) {
    await page.setViewportSize({width,height:520});await page.evaluate(theme=>{document.documentElement.dataset.theme=theme;document.documentElement.style.setProperty('--openidea-bg',theme==='dark'?'rgb(24, 26, 27)':'rgb(245, 246, 247)');},theme);
    await page.waitForFunction(()=>[...document.querySelectorAll('#toolbar button:has(svg),.field-controls>button:has(svg)')].every(button=>button.title));
    assert.equal(await page.locator('body>header').count(),0,`${id}: native host owns title`);
    if(plugin==='ddev') {
     assert.equal(await page.locator('#toolbar button').count(),6,'DDEV keeps all setup actions');
     assert.equal(await page.locator('#toolbar button.ui-icon-button').count(),6,'DDEV toolbar stays compact');
     assert.equal(await page.locator('#toolbar button').evaluateAll(buttons=>new Set(buttons.map(button=>Math.round(button.getBoundingClientRect().top))).size),1,'DDEV actions fit one row at narrow dock widths');
    }
    assert.equal(await page.getByRole('button',{name:'Unavailable',exact:true}).count(),0,`${id}: unavailable action is hidden`);
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`${id}: no root horizontal overflow at ${width}`);
    assert.equal(await page.locator('body').evaluate(body=>getComputedStyle(body).backgroundColor),theme==='dark'?'rgb(24, 26, 27)':'rgb(245, 246, 247)',`${id}: host base surface`);
    const end=page.locator('#toolbar>.ui-toolbar-end').last();
    if(await end.count()){const box=await end.boundingBox();assert.ok(Math.abs(box.x+box.width-(width-7))<1,`${id}: trailing action is right-aligned`);}

    if(fixture.body.kind==='tree'||fixture.body.kind==='resources')assert.equal(await page.locator('tbody tr').first().evaluate(row=>row.getBoundingClientRect().height),24,`${id}: native row density`);
    if(width===768)await page.screenshot({path:resolve(output,`${engine}-${plugin}-${view}-${theme}.png`)});
   }
   if(fixture.body.kind==='tree'||fixture.body.kind==='resources') {
    const rows=page.locator('tbody tr');await rows.last().click({button:'right'});
    if(await page.evaluate(()=>window.sent.at(-1).event?.kind==='select'))await reply();
    assert.equal(await page.getByRole('menuitem',{name:'Refresh',exact:true}).count(),0,`${id}: view-wide refresh stays out of row menus`);
    assert.equal(await page.getByRole('menuitem',{name:'More Actions…',exact:true}).count(),0,`${id}: view-wide overflow stays out of row menus`);
    if(plugin==='ignore')assert.equal(await page.getByRole('menuitem',{name:'Add to Ignore File',exact:true}).isVisible(),true);
    if(plugin==='kube-explorer')assert.equal(await page.getByRole('menuitem',{name:'Open',exact:true}).count(),0,'Kubernetes Describe replaces generic Open');
    if(plugin==='laravel'||plugin==='symfony')assert.equal(await page.getByRole('menuitem',{name:'Clear Caches',exact:true}).count(),0,'cache actions stay in the toolbar');
    await page.screenshot({path:resolve(output,`${engine}-${plugin}-${view}-menu.png`)});
    await page.keyboard.press('Escape');
    if(fixture.body.kind==='tree' && fixture.body.items[0].expandable) {
     await rows.first().click({button:'right'});if(await page.evaluate(()=>window.sent.at(-1).event?.kind==='select'))await reply();
     assert.equal(await page.getByRole('menuitem',{name:'Collapse',exact:true}).isVisible(),true,'group action names its collapse behavior');
     assert.equal(await page.getByRole('menuitem',{name:'Open',exact:true}).count(),0);
     await page.keyboard.press('Escape');
    }
    if(plugin==='ignore') {
     model.body={...fixture.body,items:fixture.body.items.map(item=>item.key==='u:mine'?{...item,activationEnabled:false}:item)};
     await reply();await page.locator('tbody tr').last().click({button:'right'});await reply();
     assert.equal(await page.getByRole('menuitem',{name:'Add to Ignore File',exact:true}).count(),0,'unavailable template insertion stays hidden');
     assert.equal(await page.getByRole('menuitem',{name:'Delete User Template',exact:true}).isVisible(),true,'template management remains available');
     await page.keyboard.press('Escape');
    }
   }
   if(plugin==='spring-initializr') {
    model.body={kind:'tree',items:[{key:'dep:web',label:'Spring Web',activationLabel:'Select',activationIcon:'add'}]};
    model.toolbar=[button('search','Search…','search'),button('clearAll','Clear all','trash'),button('generate','Generate Project','export'),button('more-actions','More Actions…','more-horizontal')];
    await reply();await page.locator('tbody tr').click({button:'right'});
    assert.equal(await page.getByRole('menuitem',{name:'Select',exact:true}).isVisible(),true);
    for(const name of ['Open','Search…','Clear all','Generate Project','More Actions…'])assert.equal(await page.getByRole('menuitem',{name,exact:true}).count(),0,'dependency selector menu is contextual');
    await page.keyboard.press('Escape');
   }
   receipt.push({plugin,view,title,layouts:4,status:'passed'});
  }
 }
 assert.equal(receipt.length,Object.keys(fixtures).filter(id=>(!process.env.UI_QA_PLUGIN || id.startsWith(`${process.env.UI_QA_PLUGIN}/`)) && !(process.env.UI_QA_EXCLUDE||'').split(',').includes(id.split('/')[0])).length);assert.deepEqual(errors,[]);
 writeFileSync(resolve(output,`${engine}-receipt.json`),JSON.stringify(receipt,null,2)+'\n');
 console.log(`PASS ${engine}: all ${receipt.length} custom hubs, ${receipt.length*4} light/dark dock layouts, native icons/tooltips, row density, right-aligned overflow and contextual action policies`);
} finally {await browser.close();}
