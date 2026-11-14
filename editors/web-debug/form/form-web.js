/* web 表单编辑器适配层（TASK-BOOK-5：浏览器内 编辑↔调试 闭环）。
 * 在 form.js 之前加载：stub acquireVsCodeApi（VS Code 专属 API），
 * 把 form 面板的 postMessage（apply/generate）转发给编辑台宿主（window 事件）。 */
(function () {
  'use strict';
  window.__webFormApi = {
    postMessage: function (msg) {
      window.dispatchEvent(new CustomEvent('webform-host', { detail: msg }));
    },
    getState: function () { return {}; },
    setState: function () {},
  };
  // form.js 顶层调用 acquireVsCodeApi()——返回同一 stub
  window.acquireVsCodeApi = function () { return window.__webFormApi; };
})();
