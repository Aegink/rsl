// 调试面板宿主（TASK-BOOK-4 T6）：spawn rsl serve（随机端口+token，工作区生命周期）
// → webview iframe 装前端（token 随查询串）→ didChange 防抖转发（T5）→ 停止即回收。
// 调试执行唯一在 Rust 内核（V4-D1）；扩展只做生命周期与文本桥。
'use strict';

const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const { ServeProc } = require('./lib/serve-proc');

/** 解析 rsl CLI 路径：显式配置 > 工作区 target/debug 回退 > PATH。 */
function resolveServePath() {
  const cfg = vscode.workspace.getConfiguration('rsl');
  const explicit = cfg.get('servePath');
  if (explicit && explicit !== 'rsl') return explicit;
  const wsRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (wsRoot) {
    const cands = [
      path.join(wsRoot, 'target', 'debug', 'rsl.exe'),
      path.join(wsRoot, 'target', 'debug', 'rsl'),
    ];
    const hit = cands.find((p) => fs.existsSync(p));
    if (hit) return hit;
  }
  return explicit || 'rsl';
}

/** web 调试前端目录（扩展 sibling；打包进 VSIX）。 */
function resolveWebRoot() {
  const cfg = vscode.workspace.getConfiguration('rsl');
  const explicit = cfg.get('webDebugRoot');
  if (explicit) return explicit;
  const sibling = path.join(__dirname, '..', 'web-debug');
  if (fs.existsSync(sibling)) return sibling;
  return undefined;
}

class DebugPanel {
  constructor(context) {
    this.context = context;
    this.serve = null; // ServeProc
    this.panel = null; // vscode WebviewPanel
    this.debounce = null;
    this.statusItem = null;
    this.editPartner = null; // T1 互通：表单面板（extension.js 注入）
  }

  /** T1 编辑↔调试互通：调试面板「✎ 编辑」→ 表单面板。 */
  setEditPartner(fp) {
    this.editPartner = fp;
  }

  /** 启动（或复用）serve + 打开调试面板。
   *  编辑器无活动 .rsl.json 时回退：上次调试的文档 → 工作区任一打开的 .rsl.json（webview 聚焦场景，T1）。 */
  async open(editor) {
    let doc = editor?.document;
    if (!doc && this.lastDoc && !this.lastDoc.isClosed) doc = this.lastDoc;
    if (!doc) {
      doc = vscode.workspace.textDocuments.find((d) => d.fileName.endsWith('.rsl.json'));
    }
    if (!doc || !doc.fileName.endsWith('.rsl.json')) {
      vscode.window.showInformationMessage('先打开一个 .rsl.json 源文件，再启动调试。');
      return;
    }
    if (!this.serve || this.serve.exited) {
      const webRoot = resolveWebRoot();
      this.serve = new ServeProc({ exePath: resolveServePath(), webRoot });
      try {
        const { port } = await this.serve.start();
        this.updateStatus(`调试器 :${port}`);
      } catch (e) {
        vscode.window.showErrorMessage(`rsl serve 启动失败：${e.message}`);
        this.serve = null;
        return;
      }
      this.context.subscriptions.push({ dispose: () => this.stop() });
    }
    if (!this.panel) {
      this.panel = vscode.window.createWebviewPanel(
        'rslDebug', 'RSL 调试器',
        vscode.ViewColumn.Beside,
        { enableScripts: true, retainContextWhenHidden: true },
      );
      this.panel.webview.html = this.html(this.serve.url, this.serve.token);
      // iframe load 完成会回发 ready → 重推当前编辑缓冲（防首帧丢失）；✎ 编辑 → 表单面板（T1 互通）
      this.panel.webview.onDidReceiveMessage((msg) => {
        if (msg && msg.kind === 'ready') {
          const active = vscode.window.activeTextEditor?.document;
          const doc = active && active.fileName.endsWith('.rsl.json') ? active : this.lastDoc;
          if (doc) this.pushSource(doc.getText());
        }
        if (msg && msg.kind === 'edit' && this.editPartner) {
          const doc = this.lastDoc ?? vscode.window.activeTextEditor?.document;
          if (doc) this.editPartner.open({ document: doc });
        }
      }, null, this.context.subscriptions);
      this.panel.onDidDispose(() => { this.panel = null; }, null, this.context.subscriptions);
    } else {
      this.panel.reveal();
    }
    this.lastDoc = doc;
    // 推当前编辑缓冲（免保存实时调试的第一帧）
    this.pushSource(doc.getText());
  }

  /** 编辑缓冲 → 前端（前端 800ms 防抖重跑，V4-D10）。 */
  onDocChanged(doc) {
    if (!this.panel || !doc.fileName.endsWith('.rsl.json')) return;
    clearTimeout(this.debounce);
    this.debounce = setTimeout(() => this.pushSource(doc.getText()), 800);
  }

  pushSource(text) {
    if (this.panel) this.panel.webview.postMessage({ type: 'rsl-source', text });
  }

  async stop() {
    clearTimeout(this.debounce);
    if (this.panel) { this.panel.dispose(); this.panel = null; }
    if (this.serve) {
      await this.serve.stop();
      this.serve = null;
    }
    this.updateStatus(null);
  }

  updateStatus(text) {
    if (!this.statusItem) {
      this.statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 90);
      this.statusItem.command = 'rsl.debug';
      this.context.subscriptions.push(this.statusItem);
    }
    if (text) {
      this.statusItem.text = `$(zap) ${text}`;
      this.statusItem.tooltip = 'RSL 调试器运行中（点击开关）';
      this.statusItem.show();
    } else {
      this.statusItem.hide();
    }
  }

  html(url, token) {
    // CSP：仅允许回环 iframe（H5：frame-src 127.0.0.1）
    const src = `${url}/?token=${encodeURIComponent(token)}`;
    return `<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; frame-src http://127.0.0.1:*; script-src 'unsafe-inline'; style-src 'unsafe-inline';">
<style>
  html,body { margin:0; height:100%; background:#1f2a3a; }
  iframe { border:0; width:100%; height:calc(100% - 34px); display:block; }
  .bar { height: 34px; display:flex; align-items:center; padding: 0 8px; }
  .bar button { background:#31415a; color:#dbe6f5; border:1px solid #46586f; border-radius:5px; padding:4px 12px; cursor:pointer; }
  .bar button:hover { background:#3d5170; }
</style></head>
<body>
<div class="bar"><button id="btn-edit" title="回到表单编辑（反复修改 + 反复调试）">✎ 编辑</button></div>
<iframe id="dbg" src="${src}" title="RSL 调试器"></iframe>
<script>
  const vscode = acquireVsCodeApi();
  const frame = document.getElementById('dbg');
  // didChange → iframe（T5：编辑缓冲推送）；✎ 编辑 → 表单（T1 互通）
  window.addEventListener('message', (e) => {
    if (e.data && e.data.type === 'rsl-source') {
      frame.contentWindow.postMessage({ type: 'rsl-source', text: e.data.text }, '*');
    }
  });
  frame.addEventListener('load', () => {
    vscode.postMessage({ kind: 'ready' });
  });
  document.getElementById('btn-edit').addEventListener('click', () => {
    vscode.postMessage({ kind: 'edit' });
  });
</script>
</body></html>`;
  }
}

module.exports = { DebugPanel, resolveServePath, resolveWebRoot };
