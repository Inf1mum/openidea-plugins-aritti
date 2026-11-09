/* Shared visual adapter. Existing listeners, values and Rust authority stay owned
 * by each view. Call enhance for a document or an embedded shadow root. */
(() => {
  "use strict";
  const stylesheet = new URL("./components.css", document.currentScript.src).href;
  const observers = new WeakMap();
  function decorate(node) {
    if(node.nodeType !== 1 || node.closest('[data-ui-raw]')) return;
    const tag = node.localName;
    const classes = {button:"ui-button",select:"ui-select",textarea:"ui-textarea",output:"ui-output",progress:"ui-progress",meter:"ui-meter",details:"ui-disclosure",dialog:"ui-dialog"};
    if(classes[tag]) node.classList.add(classes[tag]);
    if(tag === "button") {
      if(node.classList.contains("primary") || node.dataset.variant === "primary") node.classList.add("ui-button--primary");
      if(node.dataset.variant === "danger") node.classList.add("ui-button--danger");
    }
    if(tag === "input") {
      if(["checkbox","radio"].includes(node.type)) {
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
    decorate(root);
    root.querySelectorAll?.("button,input,select,textarea,output,progress,meter,details,dialog,a[href],[contenteditable]").forEach(decorate);
    window.uiKitIcons?.enhance(root);
    window.uiKitWidgets?.enhance(root);
  }
  function enhance(root = document) {
    if(root instanceof ShadowRoot && !observers.has(root)) {
      const link = document.createElement("link");link.rel = "stylesheet";link.href = stylesheet;root.append(link);
    }
    scan(root);
    if(!observers.has(root)) {
      const observer = new MutationObserver(records => {
        for(const record of records) for(const node of record.addedNodes) scan(node);
        window.uiKitWidgets?.prune();
      });
      observer.observe(root, {childList:true,subtree:true});observers.set(root,observer);
    }
    return () => {observers.get(root)?.disconnect();observers.delete(root);};
  }
  window.uiKitControls = Object.freeze({enhance});
  enhance(document);
})();
