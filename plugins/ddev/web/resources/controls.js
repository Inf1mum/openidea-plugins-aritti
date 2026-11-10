/* Shared visual adapter. Existing listeners, values and Rust authority stay owned
 * by each view. Call enhance for a document or an embedded shadow root. */
(() => {
  "use strict";
  const stylesheet = new URL("./components.css", document.currentScript.src).href;
  const observers = new WeakMap();
  const automaticTitles = new WeakMap();
  function iconTooltip(button) {
    if(withinRaw(button) || button.hasAttribute("data-ui-tooltip")) return;
    if(!button.matches(".ui-icon-button,.tree-toggle,.ui-tree-chevron,[aria-label]") && !button.querySelector("svg,[data-ui-icon]")) return;
    const previous = automaticTitles.get(button);
    if(button.hasAttribute("title") && button.title !== previous) return;
    const root = button.getRootNode();
    const labelledBy = button.getAttribute("aria-labelledby")?.split(/\s+/).map(id=>root.getElementById(id)?.textContent || "").join(" ");
    const copy = button.cloneNode(true);
    copy.querySelectorAll("svg,[data-ui-icon],[aria-hidden=true]").forEach(node=>node.remove());
    const label = (button.getAttribute("aria-label") || labelledBy || copy.textContent || "").trim();
    if(label) {button.title = label;automaticTitles.set(button,label);}
  }
  function withinRaw(node) {
    let owner = node?.nodeType === 11 ? node.host : node;
    while(owner) {
      if(owner.closest?.('[data-ui-raw]')) return true;
      owner = owner.getRootNode?.().host;
    }
    return false;
  }
  function decorate(node) {
    if(node.nodeType !== 1 || withinRaw(node)) return;
    const tag = node.localName;
    const classes = {button:"ui-button",select:"ui-select",textarea:"ui-textarea",output:"ui-output",progress:"ui-progress",meter:"ui-meter",details:"ui-disclosure",dialog:"ui-dialog"};
    if(classes[tag]) node.classList.add(classes[tag]);
    if(tag === "button") {
      if(node.classList.contains("primary") || node.dataset.variant === "primary") node.classList.add("ui-button--primary");
      if(node.dataset.variant === "danger") node.classList.add("ui-button--danger");
    }
    if(tag === "input") {
      if(["button","submit","reset"].includes(node.type)) node.classList.add("ui-button");
      else if(["checkbox","radio"].includes(node.type)) {
        node.classList.add("ui-choice");
        if(node.getAttribute("role") === "switch") node.classList.add("ui-switch");
        else if(!node.parentElement?.classList.contains("ui-choice-control")) {
          // Keep the real input as the sole interactive/labelled control. The
          // source drawings extend beyond its 16px layout box (radio edges and
          // focus/error rings); a separate paint layer must not clip them.
          const focused = node.getRootNode().activeElement === node;
          const control = document.createElement("span");control.className = "ui-choice-control";
          const mark = document.createElement("span");mark.className = "ui-choice-mark";mark.setAttribute("aria-hidden", "true");
          node.before(control);control.append(node, mark);
          if(focused) node.focus({preventScroll:true});
        }
        node.closest("label")?.classList.add("ui-check");
      } else if(node.type === "range") node.classList.add("ui-range");
      else if(!["hidden","button","submit","reset","image","file","color"].includes(node.type)) node.classList.add("ui-input");
    }
    if(tag === "a" && node.hasAttribute("href")) node.classList.add("ui-link");
    if(node.hasAttribute("contenteditable")) node.classList.add("ui-editable");
  }
  function scan(root) {
    if(withinRaw(root)) return;
    decorate(root);
    root.querySelectorAll?.("button,input,select,textarea,output,progress,meter,details,dialog,a[href],[contenteditable]").forEach(decorate);
    window.uiKitIcons?.enhance(root);
    window.uiKitWidgets?.enhance(root);
    if(root.matches?.("button")) iconTooltip(root);
    root.querySelectorAll?.("button").forEach(iconTooltip);
  }
  // Embedded plugin pages own their menus. Cancel browser defaults without
  // stopping propagation, so kit context menus (including keyboard invocation)
  // still receive the same gesture. This policy also covers raw vendor content.
  function preventBrowserMenu(event) {
    if (!document.body?.classList.contains("ui-embedded-panel")) return;
    if (event.type === "contextmenu" || event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) event.preventDefault();
  }
  function enhance(root = document) {
    if(withinRaw(root)) return () => {};
    if(root instanceof ShadowRoot && !observers.has(root)) {
      const link = document.createElement("link");link.rel = "stylesheet";link.href = stylesheet;root.append(link);
    }
    scan(root);
    if(!observers.has(root)) {
      const observer = new MutationObserver(records => {
        for(const record of records) {
          if(record.type === "attributes") {
            if(record.target.matches("button")) iconTooltip(record.target);
            continue;
          }
          for(const node of record.addedNodes) scan(node);
          const button = record.target.nodeType === 1 ? record.target.closest("button") : record.target.parentElement?.closest("button");
          if(button) iconTooltip(button);
        }
        window.uiKitWidgets?.prune();
      });
      observer.observe(root, {childList:true,characterData:true,subtree:true,attributes:true,attributeFilter:["aria-label","aria-labelledby"]});observers.set(root,observer);
      root.addEventListener("contextmenu",preventBrowserMenu,true);
      root.addEventListener("keydown",preventBrowserMenu,true);
    }
    return () => {observers.get(root)?.disconnect();observers.delete(root);root.removeEventListener("contextmenu",preventBrowserMenu,true);root.removeEventListener("keydown",preventBrowserMenu,true);};
  }
  window.uiKitControls = Object.freeze({enhance});
  enhance(document);
})();
