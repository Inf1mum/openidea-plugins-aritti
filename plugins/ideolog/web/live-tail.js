(() => {
  "use strict";
  const $ = id => document.getElementById(id);
  const random = () => {const bytes=new Uint8Array(16);crypto.getRandomValues(bytes);return Array.from(bytes,n=>n.toString(16).padStart(2,"0")).join("");};
  const frame=random();let nonce=random(),sequence=0,active=false,busy=false,timer=null,rows=[],disposed=false,intersecting=true;
  const visible=()=>!disposed&&!document.hidden&&intersecting;
  function clear(){if(timer!==null)clearTimeout(timer);timer=null;}
  function controls(){$("start").disabled=busy||active||!visible();$("stop").disabled=!active;}
  function send(op){clear();if(!window.openidea){$("status").textContent="Open this panel in OpenIDEA to follow a project log.";return;}if(op==="start"){nonce=random();sequence=0;}busy=true;controls();window.openidea.postMessage(JSON.stringify({channel:"ideolog-tail",frame,nonce,sequence:++sequence,op}));}
  function render(){const query=$("filter").value.toLocaleLowerCase(),invert=$("invert").checked;const shown=rows.filter(line=>!query||(line.toLocaleLowerCase().includes(query)!==invert));const fragment=document.createDocumentFragment();for(const line of shown){const tr=document.createElement("tr"),td=document.createElement("td");td.className="tail-line";td.textContent=line;tr.append(td);fragment.append(tr);}$("rows").replaceChildren(fragment);$("count").textContent=`${shown.length} / ${rows.length} recent complete lines · at most 512 lines / 128 KiB retained`;if($("follow").checked)$("lines").scrollTop=$("lines").scrollHeight;}
  function arm(){clear();if(active&&!busy&&visible())timer=setTimeout(()=>send("tick"),1000);}
  window.openidea?.onMessage(raw=>{let reply;try{reply=typeof raw==="string"?JSON.parse(raw):raw;}catch{return;}if(disposed||reply.channel!=="ideolog-tail"||reply.frame!==frame||reply.nonce!==nonce||reply.sequence!==sequence)return;busy=false;active=reply.model?.active===true;rows=Array.isArray(reply.model?.rows)?reply.model.rows.filter(line=>typeof line==="string"):[];$("status").textContent=reply.model?.status||"";$("path").textContent=reply.model?.path||"";render();controls();if(active&&!visible())send("stop");else arm();});
  $("start").addEventListener("click",()=>send("start"));$("stop").addEventListener("click",()=>send("stop"));$("filter").addEventListener("input",render);$("invert").addEventListener("change",render);$("follow").addEventListener("change",render);
  function visibility(){clear();if(!visible()&&(active||busy))send("stop");else if(visible())arm();controls();}
  document.addEventListener("visibilitychange",visibility);
  if(window.IntersectionObserver){new IntersectionObserver(entries=>{intersecting=entries.some(entry=>entry.isIntersecting);visibility();}).observe(document.documentElement);}
  window.addEventListener("pagehide",()=>{clear();if(active||busy)send("stop");disposed=true;});
  send("hello");
})();
