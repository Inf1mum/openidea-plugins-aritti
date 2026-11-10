/* The host protocol is invoked by this packaged view; Rust owns snapshot data. */
(() => {
  "use strict";
  window.openidea?.onMessage(text => {
    let message;
    try { message = JSON.parse(text); } catch { return; }
    const frame = window.uiKitFrameId;
    if ((message.channel === "gitlab-content-open" || message.channel === "gitlab-content-reply")
        && typeof frame === "string" && /^[0-9a-f]{32}$/.test(frame) && message.frameId === frame) {
      const expected = message.channel === "gitlab-content-open" ? "open-content-tab" : "content-tab-reply";
      if (message.payload?.op === expected) window.openidea.postMessage(JSON.stringify(message.payload));
    } else if (message.op === "content-tab-error") {
      window.dispatchEvent(new CustomEvent("ui-kit-notification", {detail:{message:"The snapshot tab could not be opened. Use the same Open Snapshot action (MR, diff or job trace) again to retry.",warning:false}}));
    }
    // content-tab-action goes directly to Rust via the host. Do not dispatch it twice.
  });
})();
