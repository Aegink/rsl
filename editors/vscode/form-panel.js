// RSL 表单编辑器 host 侧（TASK-BOOK-3 H2 M1/M2）。
// M2 双向：文本 didChange → 表单刷新（聚焦路径由 WebView 守卫）；表单 apply → 文档全量替换
// （JSON 规范缩进重排，未建模键原样保留——语义由 form-model.applyFormState 保证）。
// 校验统一走 rsl-lsp 诊断（不另造校验，失败语义与手写一致）。
'use strict';

const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const M = require('./form/form-model');

class FormPanel {
  constructor(context) {
    this.context = context;
    this.panel = null;
    this.uri = null;       // 绑定的文档（null = M1 骨架模式）
    this.baseDoc = null;   // 当前基线文档对象
    this.applying = false; // 自己写入触发的变更不回推表单
  }

  /** 打开：有活动的 .rsl.json → M2；面板已存在且绑定过文档 → 回绑该文档（UX：
   *  用户在表单面板里按 F6/命令时不该掉进骨架模式）；否则 → M1 骨架。 */
  open(editor) {
    if (editor && /\.rsl\.json$/.test(editor.document.fileName)) {
      this._openFor(editor.document);
      return;
    }
    if (!editor && this.uri && this.panel) {
      const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === this.uri.toString());
      if (doc) { this._openFor(doc); return; }
    }
    this._openSkeleton();
  }


  _ensurePanel(title) {
    if (this.panel) {
      this.panel.title = title;
      return;
    }
    this.panel = vscode.window.createWebviewPanel(
      'rslForm', title, vscode.ViewColumn.Beside,
      { enableScripts: true, retainContextWhenHidden: true },
    );
    this.panel.webview.html = this._html();
    this.panel.onDidDispose(() => { this.panel = null; this.uri = null; }, null, this.context.subscriptions);
    this.panel.webview.onDidReceiveMessage(
      (msg) => this._onMessage(msg), null, this.context.subscriptions);
  }

  _openFor(doc) {
    let obj = null;
    try {
      obj = JSON.parse(doc.getText());
    } catch (e) {
      vscode.window.showErrorMessage(`RSL 表单：文档不是合法 JSON，无法载入表单（${e.message}）`);
      return;
    }
    this.uri = doc.uri;
    this.baseDoc = obj;
    this.applying = false;
    this._ensurePanel(`表单 · ${path.basename(doc.fileName)}`);
    this._sendInit(true, doc.fileName);
  }

  _openSkeleton() {
    this.uri = null;
    this.baseDoc = null;
    this._ensurePanel('表单 · 骨架（M1）');
    this._sendInit(false, '');
  }

  _sendInit(docMode, docName) {
    const state = M.extractFormState(this.baseDoc || M.skeleton());
    this.panel.webview.postMessage({
      type: 'init',
      cfg: this._cfg(),
      state,
      docMode,
      docName: docName ? path.basename(docName) : '',
      diags: docMode ? this._cachedDiags(this.uri) : [],
    });
  }

  _cfg() {
    if (!this._cfgCache) {
      const schemaPath = path.join(this.context.extensionPath, 'schema', 'rsl.schema.json');
      const schema = JSON.parse(fs.readFileSync(schemaPath, 'utf-8'));
      this._cfgCache = M.deriveFormConfig(schema);
    }
    return this._cfgCache;
  }

  _cachedDiags(uri) {
    return this._lastDiags && this._lastDiags.uri === uri.toString() ? this._lastDiags.list : [];
  }

  // ───────── WebView → host ─────────
  async _onMessage(msg) {
    if (!msg || !msg.state) return;
    if (msg.type === 'apply') {
      if (!this.uri) { this._generate(msg.state); return; }
      const doc = await vscode.workspace.openTextDocument(this.uri);
      const newText = this._buildText(this.baseDoc, msg.state);
      const edit = new vscode.WorkspaceEdit();
      const full = new vscode.Range(new vscode.Position(0, 0),
        new vscode.Position(doc.lineCount, doc.lineAt(doc.lineCount - 1).text.length));
      edit.replace(this.uri, full, newText);
      this.applying = true;
      const ok = await vscode.workspace.applyEdit(edit);
      this.applying = false;
        if (!ok) {
        vscode.window.showErrorMessage('RSL 表单：应用到文档失败（文档可能已被外部修改，请重新打开表单）');
      }
      return;
    }
    if (msg.type === 'generate') {
      this._generate(msg.state);
    }
  }

  /** M1/M2 共用：骨架或当前基线 + 表单值 → 新文档（未建模键保留）。 */
  _generate(state) {
    const base = this.baseDoc || M.skeleton();
    const newDoc = M.applyFormState(base, state);
    const text = this._buildText(base, state);
    vscode.workspace.openTextDocument({ content: text, language: 'json' })
      .then((d) => vscode.window.showTextDocument(d, vscode.ViewColumn.One))
      .then(() => {
        vscode.window.showInformationMessage(
          'RSL 表单：已生成 .rsl.json（另存为 *.rsl.json 后获得诊断与补全）');
      });
    void newDoc;
  }

  _buildText(baseDoc, state) {
    const newDoc = M.applyFormState(baseDoc, state);
    return JSON.stringify(newDoc, null, 2) + '\n';
  }

  // ───────── host → WebView ─────────
  /** 文档外部变更（didChange）→ 刷新表单；自己 apply 触发的跳过。 */
  onDocChanged(doc) {
    if (!this.panel || !this.uri || doc.uri.toString() !== this.uri.toString()) return;
    if (this.applying) return;
    let obj = null;
    try { obj = JSON.parse(doc.getText()); } catch (_) { return; }
    this.baseDoc = obj;
    this.panel.webview.postMessage({
      type: 'doc',
      state: M.extractFormState(obj),
      docName: path.basename(doc.fileName),
    });
  }

  /** LSP 诊断 → 表单顶栏（校验统一走语言服务）。 */
  onDiagnostics(uri, diagnostics) {
    if (!this.panel) return;
    this._lastDiags = { uri: uri.toString(), list: diagnostics };
    if (this.uri && uri.toString() === this.uri.toString()) {
      this.panel.webview.postMessage({ type: 'diags', diagnostics });
    }
  }

  _html() {
    const wv = this.panel.webview;
    const nonce = Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);
    const jsModel = wv.asWebviewUri(vscode.Uri.joinPath(this.context.extensionUri, 'form', 'form-model.js'));
    const jsUi = wv.asWebviewUri(vscode.Uri.joinPath(this.context.extensionUri, 'media', 'form.js'));
    const css = wv.asWebviewUri(vscode.Uri.joinPath(this.context.extensionUri, 'media', 'form.css'));
    return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src ${wv.cspSource}; script-src ${wv.cspSource};">
<link rel="stylesheet" href="${css}">
<title>RSL 表单编辑器</title>
</head>
<body>
<h1>RSL 书源表单</h1>
<div id="banner" class="banner"></div>
<div id="diags"></div>
<div id="fields"></div>
<div id="toolbar">
  <button id="btnApply">应用到文档</button>
  <button id="btnGenerate" class="secondary">生成源文件</button>
  <span class="muted">未建模内容（x- 字段、media、explore 等）在应用时原样保留。</span>
</div>
<script nonce="${nonce}" src="${jsModel}"></script>
<script nonce="${nonce}" src="${jsUi}"></script>
</body>
</html>`;
  }
}

module.exports = { FormPanel };
