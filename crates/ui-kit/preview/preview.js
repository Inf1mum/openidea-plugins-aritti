"use strict";
const theme = document.getElementById("theme");
const workflow = document.getElementById("workflow");
function applyTheme() {
  document.documentElement.dataset.theme = theme.value;
  workflow.contentWindow?.postMessage({channel:"ui-kit-preview-theme",theme:theme.value}, "*");
}
theme.addEventListener("change", applyTheme);
workflow.addEventListener("load", applyTheme);
document.getElementById("reset-demo").addEventListener("click", () => {
  workflow.contentWindow?.postMessage({channel:"ui-kit-preview-reset"}, "*"); workflow.scrollIntoView({block:"center"});
});
document.getElementById("open-dialog").addEventListener("click", () => document.getElementById("sample-dialog").showModal());
document.getElementById("rows-demo").addEventListener("input", event => {
  document.getElementById("rows-value").value = `${event.target.value} rows`;
});
