/* Terminal launch controls. All execution is validated by the Rust plugin. */
(() => {
  window.uiKitManagedProcesses = {mount(root, bridge) {
    const t=(key,fallback,args={})=>window.uiKitI18n?.t(`process.${key}`,fallback,args)??fallback.replace(/\{(\w+)\}/g,(token,key)=>Object.hasOwn(args,key)?String(args[key]):token);
    const $ = id => root.querySelector(`#${id}`);
    const make = (tag, text) => {const node=document.createElement(tag);if(text!=null)node.textContent=text;return node;};
    let disposed=false,busy=false,token=null,revision=0,pendingRevision=0,pending="";
    function controls(){
      const command=$("process-command").value;
      root.querySelectorAll("[data-process-serve]").forEach(node=>node.hidden=command!=="serve");
      root.querySelectorAll("[data-process-queue]").forEach(node=>node.hidden=command!=="queue:work");
      $("process-preview").disabled=busy;$("process-start").disabled=busy||!token;
    }
    function send(request){if(disposed||busy)return false;busy=true;pending=request.op;pendingRevision=revision;controls();bridge.postMessage(JSON.stringify(request));return true;}
    function changed(){revision++;token=null;$("process-argv").textContent=t("previewUpdated","Preview the updated command before starting.");controls();}
    root.querySelector("form").addEventListener("submit",event=>event.preventDefault());
    root.querySelector("form").addEventListener("input",changed);
    root.querySelector("form").addEventListener("change",changed);
    $("process-preview").addEventListener("click",()=>{
      token=null;const command=$("process-command").value,options={};
      if(command==="serve"){options.host=$("process-host").value;options.port=Number($("process-port").value);}
      if(command==="queue:work"){if($("process-connection").value)options.connection=$("process-connection").value;if($("process-queue").value)options.queue=$("process-queue").value;}
      send({op:"preview",command,runner:$("process-runner").value,options});
    });
    $("process-start").addEventListener("click",()=>{if(!token)return;const id=globalThis.crypto?.randomUUID?.()||`process-${Date.now()}-${Math.random().toString(36).slice(2)}`;send({op:"run",id,token});token=null;});
    bridge.onMessage(raw=>{
      if(disposed)return;let reply;try{reply=JSON.parse(raw);}catch{return;}
      const operation=pending;pending="";busy=false;$("process-error").textContent=reply.error||"";
      if(reply.preview&&pendingRevision===revision){token=reply.preview.token;$("process-argv").textContent=JSON.stringify({program:reply.preview.program,argv:reply.preview.argv,target:reply.preview.targetLabel},null,2);}
      if(operation==="run")token=null;controls();
    });
    controls();send({op:"state"});
    return ()=>{disposed=true;};
  }};
})();
