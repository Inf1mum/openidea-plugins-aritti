/* Shared packaged view. Data and action ownership stay in the Rust plugins. */
(() => {
  "use strict";
  // The host transport broadcasts messages across windows. This opaque page
  // identity prevents wrong-frame rendering; it does not encrypt that transport.
  const frameBytes = new Uint8Array(16);
  window.crypto.getRandomValues(frameBytes);
  const frameId = Array.from(frameBytes, byte => byte.toString(16).padStart(2, "0")).join("");
  Object.defineProperty(window, "uiKitFrameId", { value: frameId, writable: false, configurable: false });
  const $ = id => document.getElementById(id);
  const state = { sequence: 0, project: null, model: null, section: "", pages: new Map(), selected: null, sort: null, busy: false, refreshPending: false, listeners: [] };
  const element = (tag, text, className) => { const node = document.createElement(tag); if (text != null) node.textContent = text; if (className) node.className = className; const control={button:"ui-button",input:"ui-input",select:"ui-select",textarea:"ui-textarea",output:"ui-output",table:"ui-table"}[tag]; if(control) node.classList.add(control); return node; };
  const queryState = () => { if (!state.pages.has(state.section)) state.pages.set(state.section, { query: "", facets: {}, page: 0, selected: null, sort: null }); return state.pages.get(state.section); };
  const notice = text => { $("notice").textContent = text || ""; $("notice").className = text ? "ui-alert ui-alert--error" : ""; };
  function send(op) {
    if (!window.openidea) { notice("Open this view in OpenIDEA to load project resources."); return; }
    state.busy = true; $("content").setAttribute("aria-busy", "true"); $("reload").setAttribute("aria-busy", "true");
    if($("request-progress")) $("request-progress").hidden = false;
    document.querySelectorAll("#toolbar button, #sections button, #details button").forEach(b => b.disabled = true);
    openidea.postMessage(JSON.stringify({ ...op, channel: "ui-kit", frameId, sequence: ++state.sequence, project: state.project }));
    return state.sequence;
  }
  const event = data => send({ op: "event", event: data });
  const formDrafts = new Map();
  const formScope = () => `${state.project}\0${state.section}`;
  function clearFormDrafts() { for(const draft of formDrafts.values()) clearTimeout(draft.timer); formDrafts.clear(); }
  function flushField(key) {
    const draft=formDrafts.get(key);if(!draft) return;
    clearTimeout(draft.timer);draft.timer=null;
    if(draft.scope !== formScope() || state.model?.body.kind !== "form") {formDrafts.delete(key);return;}
    if(draft.sentValue === draft.value) return;
    draft.sentValue=draft.value;draft.sequence=event({kind:"field",key,value:draft.value});
  }
  function flushForm() {for(const key of formDrafts.keys()) flushField(key);}
  // Model-free invalidation: fetch through this page's correlated bridge rather
  // than accepting state broadcast by another invocation or interrupting a write.
  function drainRefresh() {
    if (!state.refreshPending || state.busy) return;
    flushForm();
    if (state.busy) return;
    state.refreshPending = false;
    send({op:"render"});
  }

  function actionButton(button) {
    const b = element("button", button.label); b.type = "button"; b.disabled = !button.enabled;
    b.addEventListener("click", () => {flushForm();event({ kind: "button", key: button.key });}); return b;
  }
  function header() {
    const model = state.model; $("title").textContent = model.title; $("plugin").textContent = model.plugin;
    const sections = model.toolbar.filter(b => b.key.startsWith("section:"));
    state.section = sections.find(b => b.label.startsWith("●"))?.key || "main";
    $("sections").hidden = sections.length < 2; $("sections").replaceChildren(...sections.map(b => {
      const node = actionButton({ ...b, label: b.label.replace(/^●\s*/, "") });
      if (b.key === state.section) node.setAttribute("aria-current", "page"); return node;
    }));
    $("toolbar").replaceChildren(...model.toolbar.filter(b => !b.key.startsWith("section:")).map(actionButton));
    $("search").value = queryState().query;
  }
  function rowModel(body) {
    if (body.kind === "resources") {
      const identity = body.columns.findIndex(c => ["Path","Dependency","Package","Artifact","Name"].includes(c));
      return { columns: body.columns, facets: body.facets || [], rows: body.items.map(r => ({ ...r, label: r.label || r.cells[identity < 0 ? 0 : identity] })) };
    }
    if (body.kind === "table") return { columns: body.columns, facets: [], rows: body.rows.map((cells, i) => ({ key: String(i + (body.firstRow || 0)), row: i + (body.firstRow || 0), cells, label: cells[0] })) };
    return { columns: ["Resource", "Details"], facets: [], rows: (body.items || []).map(r => ({ ...r, cells: [r.label, r.detail || ""] })) };
  }
  function select(row, body) {
    const q = queryState(); q.selected = row.key; state.selected = row;
    const selected = $("details"); selected.hidden = false;
    const title = element("h2", row.label || row.cells[0]); const fields = element("dl");
    const heading = element("div", null, "detail-heading"), close = element("button", null, "detail-close ui-icon-button ui-icon-button--close");
    close.setAttribute("aria-label", "Close details");
    close.addEventListener("click", () => {
      selected.hidden = true; q.selected = null; state.selected = null;
      const current = document.querySelector('tbody tr[aria-selected="true"]');
      if(current) {current.setAttribute("aria-selected", "false"); current.focus();}
    });
    heading.append(title, close);
    const columns = rowModel(body).columns;
    columns.forEach((label,i) => { fields.append(element("dt", label), element("dd", row.cells[i] || "—")); });
    for (const fact of row.details || []) {
      if (typeof fact.label === "string" && typeof fact.value === "string") fields.append(element("dt",fact.label),element("dd",fact.value || "—"));
    }
    const actions = element("div", null, "actions"); const open = element("button", "Open");
    open.addEventListener("click", () => activate(row, body)); actions.append(open);
    const kind = row.key.split(":")[0];
    for (const a of state.model.actions || []) if (!a.kind || a.kind === kind) {
      const b = element("button", a.label); b.addEventListener("click", () => send({ op: "command", id: a.id, key: row.key })); actions.append(b);
    }
    selected.replaceChildren(heading, fields, actions);
  }
  function activate(row, body) { event(body.kind === "table" ? { kind: "table-activate", row: row.row } : { kind: "activate", key: row.key }); }
  function renderRows(body) {
    const { columns, facets, rows } = rowModel(body); const q = queryState();
    const filters = $("facets"); filters.replaceChildren();
    facets.forEach(column => {
      const label = element("label", columns[column], "ui-field"); const select = element("select"); select.setAttribute("aria-label", columns[column]);
      const values = [...new Set(rows.map(r => r.cells[column]).filter(Boolean))].sort();
      const all = element("option", `All (${values.length})`); all.value = ""; select.append(all);
      for (const value of values) { const option = element("option", value); option.value = value; select.append(option); }
      if (!values.includes(q.facets[column])) delete q.facets[column];
      select.value = q.facets[column] || ""; select.addEventListener("change", () => { q.facets[column] = select.value; q.page = 0; renderBody(); });
      label.append(select); filters.append(label);
    });
    const needle = q.query.trim().toLocaleLowerCase();
    let visible = rows.filter(r => (!needle || r.cells.join(" ").toLocaleLowerCase().includes(needle)) && facets.every(c => !q.facets[c] || q.facets[c] === r.cells[c]));
    if (body.kind === "tree" && !needle) {
      const byKey = new Map(rows.map(r => [r.key,r]));
      visible = visible.filter(r => { let parent = r.parent, seen = new Set(); while(parent && !seen.has(parent)) { seen.add(parent); const p = byKey.get(parent); if(p && !p.expanded) return false; parent = p?.parent; } return true; });
    }
    if (q.sort) visible.sort((a,b) => String(a.cells[q.sort.column] || "").localeCompare(String(b.cells[q.sort.column] || ""), undefined, { numeric: true }) * (q.sort.descending ? -1 : 1));
    const pages = Math.max(1,Math.ceil(visible.length / 100)); q.page = Math.min(q.page,pages-1);
    const table = element("table",null,body.kind === "resources" ? "resources" : ""); const head = element("thead"); const headings = element("tr");
    columns.forEach((label,column) => { const th = element("th"); const b = element("button", label + (q.sort?.column === column ? q.sort.descending ? " ↓" : " ↑" : ""));
      b.addEventListener("click", () => { q.sort = { column, descending: q.sort?.column === column && !q.sort.descending }; renderBody(); });
      th.setAttribute("scope","col"); if(q.sort?.column === column) th.setAttribute("aria-sort",q.sort.descending ? "descending":"ascending"); th.append(b); headings.append(th); });
    head.append(headings); table.append(head); const tbody = element("tbody");
    for (const row of visible.slice(q.page*100,(q.page+1)*100)) {
      const tr = element("tr"); tr.tabIndex = 0; tr.setAttribute("aria-selected",String(row.key === q.selected));
      row.cells.forEach((cell,i) => { const td = element("td", cell); if (body.kind === "resources" && columns[i] === "Method") { const badge = element("span", cell, "method ui-badge"); badge.dataset.method = cell; td.replaceChildren(badge); }
        else if(body.kind === "resources") td.className = "mono";
        if(["Kind","Type","Method","Severity","Ecosystem"].includes(columns[i]) && /^[\p{L}\p{N}_-]{1,24}$/u.test(String(cell))) td.classList.add("category");
        if (i === 0 && body.kind === "tree") {
          let parent = row.parent, depth = 0, seen = new Set(); const byKey = new Map(rows.map(r => [r.key,r]));
          while(parent && depth < 12 && !seen.has(parent)) {seen.add(parent); depth++; parent=byKey.get(parent)?.parent;}
          td.style.paddingLeft = `${10+depth*14}px`;
          if(row.expandable) { const toggle = element("button",row.expanded ? "▾":"▸","tree-toggle"); toggle.setAttribute("aria-label",`${row.expanded ? "Collapse":"Expand"} ${row.label}`); toggle.setAttribute("aria-expanded",String(!!row.expanded)); toggle.addEventListener("click",e => {e.stopPropagation(); event({kind:row.expanded ? "collapse":"expand",key:row.key});}); td.prepend(toggle); }
        }
        if(body.editable) { td.contentEditable = "true"; td.addEventListener("blur",() => { if(td.textContent !== cell) event({kind:"cell",row:row.row,column:i,value:td.textContent}); }); }
        tr.append(td);
      });
      tr.addEventListener("click", () => { select(row,body); document.querySelectorAll("tbody tr[aria-selected=true]").forEach(n=>n.setAttribute("aria-selected","false")); tr.setAttribute("aria-selected","true"); event(body.kind === "table" ? {kind:"table-select",row:row.row} : {kind:"select",key:row.key}); });
      tr.addEventListener("dblclick",() => activate(row,body)); tr.addEventListener("keydown",e => { if(e.target !== tr) return; if(e.key === "Enter") {e.preventDefault();activate(row,body);} else if(e.key === " ") {e.preventDefault();select(row,body);event(body.kind === "table" ? {kind:"table-select",row:row.row}:{kind:"select",key:row.key});} });
      tbody.append(tr);
    }
    table.append(tbody); const content = $("content");
    if(visible.length) content.replaceChildren(table);
    else {
      const empty = element("div", null, "empty ui-empty");
      empty.append(element("strong", rows.length ? "No matching resources" : "No resources found"), element("p", rows.length ? "Try a different search or clear the filters." : "Refresh after adding resources to this project."));
      const retry = element("button", rows.length ? "Reset filters" : "Refresh resources");
      retry.addEventListener("click", () => $(rows.length ? "clear" : "reload").click());
      empty.append(retry); content.replaceChildren(empty);
    }
    if (pages > 1) { const pager=element("div",null,"pager"); const prev=element("button","Previous"), next=element("button","Next"); prev.disabled=q.page===0;next.disabled=q.page===pages-1;prev.addEventListener("click",()=>{q.page--;renderBody();});next.addEventListener("click",()=>{q.page++;renderBody();});pager.append(prev,element("span",`${q.page+1} / ${pages}`),next);content.append(pager); }
    const selected = rows.find(r=>r.key===q.selected); if(selected) select(selected,body); else {$("details").hidden=true;q.selected=null;}
    $("status").textContent = `${visible.length} of ${rows.length} resources${body.totalRows > rows.length ? ` (${body.totalRows} available)` : ""}${state.model.status ? ` · ${state.model.status}`:""}`;
  }
  function renderForm(body) {
    const active=document.activeElement;
    const focus=active?.dataset?.fieldKey ? {key:active.dataset.fieldKey,start:active.selectionStart,end:active.selectionEnd,direction:active.selectionDirection} : null;
    const form = element("div",null,"form");
    for(const field of body.fields) {
      const value=field.value,label=element("label",field.label,"ui-field"); let input;
      if(value.kind === "label" || value.kind === "readonly-text") { input=element("output",value.kind === "label" ? value.data : value.data?.text); }
      else if(value.kind === "choice") {input=element("select");for(const text of value.options) {const o=element("option",text);o.value=text;input.append(o);}input.value=value.text;}
      else if(value.kind === "multiline") {input=element("textarea");input.value=value.text;}
      else if(value.kind === "checkbox") {input=element("input");input.type="checkbox";input.className="ui-choice";input.checked=value.checked;label.className="checkbox ui-check";}
      else if(value.kind === "text") {input=element("input");input.type="text";input.value=value.text;}
      else {input=element("output","Unsupported field type");}
      input.setAttribute("aria-label",field.label);input.dataset.fieldKey=field.key;
      if(value.kind === "text" || value.kind === "multiline") {
        const draft=formDrafts.get(field.key);if(draft?.scope === formScope()) input.value=draft.value;
        input.addEventListener("input",()=>{
          const previous=formDrafts.get(field.key);clearTimeout(previous?.timer);
          const next={...previous,scope:formScope(),value:input.value,timer:null};
          next.timer=setTimeout(()=>flushField(field.key),250);formDrafts.set(field.key,next);
        });
        input.addEventListener("change",()=>flushField(field.key));
        input.addEventListener("keydown",e=>{if(e.key === "Enter" && value.kind === "text") flushField(field.key);});
      } else if(value.kind === "checkbox" || value.kind === "choice") input.addEventListener("change",()=>{flushForm();event({kind:"field",key:field.key,value:value.kind === "checkbox" ? String(input.checked):input.value});});
      if(value.kind === "checkbox") label.prepend(input); else label.append(input);form.append(label);
    }
    $("content").replaceChildren(form);$("details").hidden=true;
    window.uiKitControls?.enhance(form);
    if(focus) {const next=Array.from(form.querySelectorAll("input,textarea,button[role=combobox]")).find(n=>n.dataset.fieldKey === focus.key);if(next){next.focus({preventScroll:true});if(focus.start != null && next.setSelectionRange) next.setSelectionRange(Math.min(focus.start,next.value.length),Math.min(focus.end,next.value.length),focus.direction || "none");}}
  }
  function renderHtml(body) {
    const host=element("div",null,"legacy");const shadow=host.attachShadow({mode:"open"});
    const parsed=new DOMParser().parseFromString(body.html,"text/html");parsed.querySelectorAll("script").forEach(s=>s.remove());
    for(const attribute of parsed.body.attributes) host.setAttribute(attribute.name,attribute.value);
    for(const style of parsed.querySelectorAll("style")) {style.textContent=style.textContent.replace(/:root|\bhtml\b|\bbody\b/g,":host");}
    shadow.append(...Array.from(parsed.head.children),...Array.from(parsed.body.childNodes));$("content").replaceChildren(host);$("details").hidden=true;
    // Syntax overlays have a matching pre/textarea geometry and must stay raw.
    shadow.querySelector("#src")?.setAttribute("data-ui-raw", "");
    window.uiKitControls?.enhance(shadow);
    const bridge={postMessage:value=>event({kind:"message",value}),onMessage:callback=>state.listeners.push(callback),saveFile:options=>window.openidea.saveFile(options),openFile:options=>window.openidea.openFile(options)};
    if(shadow.querySelector("svg .n[data-id]")) shadow.querySelectorAll(".n[data-id]").forEach(node=>{const open=()=>bridge.postMessage(node.dataset.id);node.addEventListener("click",open);node.addEventListener("keydown",e=>{if(e.key === "Enter" || e.key === " "){e.preventDefault();open();}});});
    else if(shadow.querySelector("#src")) window.uiKitLegacy?.kube(shadow,bridge);
    else if(shadow.querySelector(".file[data-new-path]")) window.uiKitLegacy?.review(shadow,bridge);
    state.listeners.push(message=>{if(message === "end") $("content").scrollTop=$("content").scrollHeight;});
  }
  function renderBody() {
    const body=state.model.body;state.listeners=[];$("filters").hidden=["form","html","message"].includes(body.kind);$("status").textContent=state.model.status || "";
    if(body.kind === "form") renderForm(body);
    else if(body.kind === "html") renderHtml(body);
    else if(body.kind === "message") {$("content").replaceChildren(element("div",body.text,"empty ui-empty"));$("details").hidden=true;}
    else renderRows(body);
  }
  $("search").addEventListener("input",()=>{const q=queryState();q.query=$("search").value;q.page=0;renderBody();});
  $("clear").addEventListener("click",()=>{const q=queryState();q.query="";q.facets={};q.page=0;$("search").value="";renderBody();});
  $("reload").addEventListener("click",()=>{const refresh=state.model?.toolbar.find(b=>["refresh","reindex"].includes(b.key) && b.enabled);if(refresh) event({kind:"button",key:refresh.key});else send({op:"render"});});
  window.openidea?.onMessage(text=>{
    let message;try{message=JSON.parse(text);}catch{state.listeners.forEach(callback=>callback(text));return;}
    if(message.channel === "ui-kit-refresh") {state.refreshPending=true;drainRefresh();return;}
    if(message.channel !== "ui-kit") {state.listeners.forEach(callback=>callback(text));return;}
    if(message.frameId !== frameId) return;
    if(message.sequence !== state.sequence) return;
    state.busy=false;$("content").setAttribute("aria-busy","false");$("reload").setAttribute("aria-busy","false");
    if($("request-progress")) $("request-progress").hidden = true;
    if(state.refreshPending) queueMicrotask(drainRefresh);
    if(message.error) {notice(message.error);if(state.model) {header();if(state.selected && !$("details").hidden) select(state.selected,state.model.body);}return;}
    const sameBody=state.project === message.project && JSON.stringify(state.model?.body) === JSON.stringify(message.model.body);
    const previousSection=state.section;
    if(state.project !== message.project) {state.pages.clear();state.selected=null;}
    const oldScope=formScope();let formRefresh=false;
    state.project=message.project;state.model=message.model;notice("");header();
    if(oldScope !== formScope() || state.model.body.kind !== "form") clearFormDrafts();
    else for(const [key,draft] of formDrafts) {
      const field=state.model.body.fields.find(f=>f.key === key && ["text","multiline"].includes(f.value.kind));
      if(!field || (draft.sequence <= message.sequence && draft.sentValue === draft.value)) {if(field && field.value.text !== draft.value) formRefresh=true;clearTimeout(draft.timer);formDrafts.delete(key);}
    }
    if(!sameBody || previousSection !== state.section || formRefresh) renderBody();
    else {$("status").textContent=state.model.status || $("status").textContent;if(state.selected && !$("details").hidden) select(state.selected,state.model.body);}
  });
  window.addEventListener("error",e=>notice(e.message || "The view could not be rendered."));
  send({op:"render"});
})();
