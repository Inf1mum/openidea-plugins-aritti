/* Session-owned process controls. All execution is validated by the Rust plugin. */
(() => {
  window.uiKitManagedProcesses = {mount(root, bridge) {
    const $ = id => root.querySelector(`#${id}`);
    const make = (tag, text) => {const node=document.createElement(tag);if(text!=null)node.textContent=text;return node;};
    let disposed=false,busy=false,token=null,jobs=[],timer=null,revision=0,pendingRevision=0,pending="";
    function controls(){
      const command=$("process-command").value;
      root.querySelectorAll("[data-process-serve]").forEach(node=>node.hidden=command!=="serve");
      root.querySelectorAll("[data-process-queue]").forEach(node=>node.hidden=command!=="queue:work");
      $("process-preview").disabled=busy;$("process-start").disabled=busy||!token;$("process-refresh").disabled=busy;
    }
    function send(request){if(disposed||busy)return false;clearTimeout(timer);busy=true;pending=request.op;pendingRevision=revision;controls();bridge.postMessage(JSON.stringify(request));return true;}
    function changed(){revision++;token=null;$("process-argv").textContent="Preview the updated command before starting.";controls();}
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
    $("process-refresh").addEventListener("click",()=>send({op:"poll"}));
    function render(){
      const list=$("process-jobs");list.replaceChildren();
      if(!jobs.length)list.append(make("p","No managed processes in this session."));
      for(const job of jobs){
        const article=make("article"),heading=make("strong",`${job.command} · ${job.runner} · ${job.running?"Running":job.stopped?"Stopped":`Exited (${job.status??"unknown"})`}`);
        article.append(heading);if(job.targetLabel)article.append(make("p",job.targetLabel));
        if(job.running){const stop=make("button","Stop"),icon=make("span");icon.dataset.uiIcon="stop";icon.setAttribute("aria-hidden","true");stop.prepend(icon);stop.className="ui-button process-stop";stop.type="button";stop.disabled=busy;stop.setAttribute("aria-label",`Stop ${job.command}`);stop.addEventListener("click",()=>send({op:"stop",id:job.id}));article.append(stop);}
        const output=make("div");output.className="process-output";const stdout=make("pre",job.stdout||"No output yet.");stdout.tabIndex=0;stdout.setAttribute("aria-label",`${job.command} output`);output.append(stdout);
        if(job.stderr){const stderr=make("pre",job.stderr);stderr.tabIndex=0;stderr.setAttribute("aria-label",`${job.command} standard error`);output.append(stderr);}
        if(job.truncated)output.append(make("small","Output clipped to the session capture limit."));article.append(output);list.append(article);
      }
      window.uiKitControls?.enhance(root);
    }
    bridge.onMessage(raw=>{
      if(disposed)return;let reply;try{reply=JSON.parse(raw);}catch{return;}
      const operation=pending;pending="";busy=false;$("process-error").textContent=reply.error||"";
      if(reply.preview&&pendingRevision===revision){token=reply.preview.token;$("process-argv").textContent=JSON.stringify({program:reply.preview.program,argv:reply.preview.argv,target:reply.preview.targetLabel},null,2);}
      if(Array.isArray(reply.jobs))jobs=reply.jobs;
      if(operation==="run")token=null;controls();render();
      if(!reply.error&&jobs.some(job=>job.running))timer=setTimeout(()=>send({op:"poll"}),1000);
    });
    controls();send({op:"state"});
    return ()=>{disposed=true;clearTimeout(timer);};
  }};
})();
