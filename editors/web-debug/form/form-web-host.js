/* web 表单编辑器宿主（TASK-BOOK-5）：视图控制 + lint 校验 + 写回源库。
 * 依赖 app.js 的全局（$ / state / lsSources / saveSources / showView / runStage 等）。 */
(function () {
  'use strict';

  let editTarget = null; // {id, json} 当前编辑的源库条目
  let lintTimer = null;
  let lastDiags = [];

  function formCfg() {
    const schema = window.__rslLangSchema;
    if (!schema || !window.RslFormModel) return null;
    return window.RslFormModel.deriveFormConfig(schema);
  }

  /** 源库「✎ 编辑」入口：载入源 → 表单渲染。 */
  function openSourceInEditor(entry) {
    editTarget = entry;
    if (window.appShowView) window.appShowView('editor');
    const cfg = formCfg();
    if (!cfg) return;
    const json = entry.json;
    const st = window.RslFormModel.extractFormState(json);
    window.postMessage({
      type: 'init',
      cfg,
      state: st,
      docMode: true,
      docName: entry.name || entry.id,
      diags: lastDiags,
    }, '*');
    document.getElementById('ed-name').textContent = entry.name || entry.id;
    lintSoon();
  }

  /** 载入裸源文本（非源库条目，如粘贴的新源）→ 先登记临时条目。 */
  function openJsonInEditor(json) {
    const id = json.id || ('adhoc-' + Date.now());
    const entry = { id, name: json.name || id, group: '', url: json.baseUrl || '', ts: Date.now(), enabled: true, json };
    const map = lsSources();
    if (!map[id]) { map[id] = entry; saveSources(map); }
    openSourceInEditor(map[id]);
  }

  /** lint：只编译诊断（同内核，杜绝前端平行校验）。500ms 防抖。 */
  function lintSoon() {
    clearTimeout(lintTimer);
    lintTimer = setTimeout(function () {
      if (!editTarget) return;
      const text = JSON.stringify(editTarget.json, null, 2);
      fetch('/debug/lint?token=' + encodeURIComponent(TOKEN), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source_text: text }),
      }).then(function (r) { return r.json(); }).then(function (d) {
        lastDiags = (d && d.diagnostics) || [];
        window.postMessage({ type: 'diags', diags: lastDiags }, '*');
        const elx = document.getElementById('ed-lint');
        const errs = lastDiags.filter(function (x) { return x.severity === 1; }).length;
        elx.textContent = errs
          ? '✗ ' + errs + ' 个错误 / ' + lastDiags.length + ' 条诊断'
          : '✓ 无错误（' + lastDiags.length + ' 条诊断）';
        elx.style.color = errs ? 'var(--danger)' : 'var(--ok)';
      }).catch(function (e) {
        document.getElementById('ed-lint').textContent = '校验失败：' + e;
      });
    }, 500);
  }

  // form 面板 → 宿主消息（apply = 表单写回；debug = 带当前源去调试）
  window.addEventListener('webform-host', function (ev) {
    const msg = ev.detail || {};
    if (msg.type === 'apply' && editTarget) {
      const newDoc = window.RslFormModel.applyFormState(editTarget.json, msg.state);
      editTarget.json = newDoc;
      editTarget.name = newDoc.name || editTarget.name;
      editTarget.url = newDoc.baseUrl || editTarget.url;
      editTarget.ts = Date.now();
      const map = lsSources();
      map[editTarget.id] = editTarget;
      saveSources(map);
      lintSoon();
      const st = document.getElementById('ed-status');
      st.textContent = '✓ 已应用到源库（' + new Date().toLocaleTimeString() + '）';
      setTimeout(function () { st.textContent = ''; }, 2500);
      // 编辑→调试（T1 闭环）：写回后带着最新 JSON 跳调试器
      if (window.__pendingDebug) {
        window.__pendingDebug = false;
        openSourceInDebug(editTarget);
      }
    }
    if (msg.type === 'generate' && editTarget) {
      const newDoc = window.RslFormModel.applyFormState(editTarget.json, msg.state);
      download(editTarget.id.replace(/[^\w.-]/g, '_') + '.rsl.json', JSON.stringify(newDoc, null, 4));
    }
    if (msg.type === 'debug' && editTarget) {
      openSourceInDebug(editTarget); // 编辑↔调试互跳（免保存——编辑缓冲即最新规则）
    }
  });

  // 顶部按钮
  window.addEventListener('DOMContentLoaded', function () {
    const bApply = document.getElementById('ed-apply');
    if (bApply) bApply.onclick = function () { window.postMessage({ type: 'webform-trigger-apply' }, '*'); };
  });

  // 暴露给 app.js
  window.webFormEditor = { openSourceInEditor, openJsonInEditor };
})();
