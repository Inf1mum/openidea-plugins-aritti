/* Standalone fixture only. Never packaged into a plugin or used for authority. */
(() => {
  window.addEventListener("message", event => {
    if(event.source !== window.parent) return;
    if(event.data?.channel === "ui-kit-preview-theme" && ["light","dark"].includes(event.data.theme)) document.documentElement.dataset.theme = event.data.theme;
    if(event.data?.channel === "ui-kit-preview-reset") document.getElementById("clear").click();
  });
  const listeners = []; let section = "routes";
  const routes = {kind:"resources",columns:["Method","Path","Controller","Handler"],facets:[0,2],items:[
    {key:"route:0",cells:["GET","/orders","OrderController","index"]},
    {key:"route:1",cells:["POST","/orders","OrderController","store"]},
    {key:"route:2",cells:["DELETE","/orders/{id}","OrderController","destroy"]},
    {key:"route:3",cells:["GET","/customers/{id}","CustomerController","show"]}
  ]};
  const settings = {kind:"form",fields:[{key:"base",label:"Base URL",value:{kind:"text",text:"http://localhost:8080"}},{key:"inherit",label:"Include inherited resources",value:{kind:"checkbox",checked:true}},{key:"preview",label:"Command preview",value:{kind:"readonly-text",data:{text:"php artisan route:list --json"}}}]};
  window.openidea = {
    onMessage: callback => listeners.push(callback),
    postMessage: text => {
      const request = JSON.parse(text);
      if(request.event?.key?.startsWith("section:")) section = request.event.key.slice(8);
      if(request.event?.kind === "field") {
        const field = settings.fields.find(f => f.key === request.event.key);
        if(field?.value.kind === "text") field.value.text = request.event.value;
        if(field?.value.kind === "checkbox") field.value.checked = request.event.value === "true";
      }
      const model = {title:section === "routes" ? "Routes" : "Settings",plugin:"Sample plugin · Preview data",toolbar:[{key:"section:routes",label:`${section === "routes" ? "● " : ""}Routes`,enabled:true},{key:"section:settings",label:`${section === "settings" ? "● " : ""}Settings`,enabled:true}],actions:[{id:"copy",label:"Copy as curl",kind:"route"}],body:section === "routes" ? routes : settings,status:"Sample data · No project actions are executed"};
      const error = request.op === "command" || ["activate","table-activate"].includes(request.event?.kind) ? "Preview only. Open the plugin in OpenIDEA to run source and command actions." : null;
      setTimeout(() => listeners.forEach(callback => callback(JSON.stringify({channel:"ui-kit",frameId:request.frameId,sequence:request.sequence,project:"/sample",model,error}))), 180);
    }
  };
})();
