// RSL VS Code 扩展（TASK-BOOK-3 D1）：
// 启动 rsl-lsp（stdio）→ 诊断 / hover / 补全 / 定义 / code action 全部来自语言服务。
// 传输层在 lib/rpc.js（零依赖手写 JSON-RPC，与 server 同风格）。
'use strict';

const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const { LspClient } = require('./lib/rpc');
const { FormPanel } = require('./form-panel');
const { DebugPanel } = require('./debug-panel');

/** 解析 rsl-lsp 路径：显式配置 > 工作区 target/debug 回退（仓库根打开即开箱可用）> PATH。 */
function resolveLspPath() {
  const cfg = vscode.workspace.getConfiguration('rsl');
  const explicit = cfg.get('lspPath');
  if (explicit && explicit !== 'rsl-lsp') return explicit;
  const wsRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (wsRoot) {
    const cands = [
      path.join(wsRoot, 'target', 'debug', 'rsl-lsp.exe'),
      path.join(wsRoot, 'target', 'debug', 'rsl-lsp'),
    ];
    const hit = cands.find((p) => fs.existsSync(p));
    if (hit) return hit;
  }
  return explicit || 'rsl-lsp';
}

let client = null;

function activate(context) {
  try {
    activateInner(context);
  } catch (err) {
    vscode.window.showErrorMessage(`RSL 扩展激活失败：${err && err.message}`);
    throw err;
  }
}

function activateInner(context) {
  client = new LspClient(resolveLspPath());

  // H2 M1/M2：表单编辑器（同一面板承载两种模式）
  const formPanel = new FormPanel(context);
  // T6：调试面板（spawn rsl serve + webview iframe + didChange 转发）
  const debugPanel = new DebugPanel(context);
  // T1 编辑↔调试互通：互跳（反复修改 + 反复调试闭环）
  formPanel.setDebugPartner(debugPanel);
  debugPanel.setEditPartner(formPanel);
  context.subscriptions.push({
    dispose() {
      if (client) {
        client.request('shutdown', null).catch(() => {});
        client.notify('exit', null);
        client.end();
      }
    },
  });

  const docSelector = [
    { language: 'rsl', scheme: 'file', pattern: '**/*.rsl.json' },
    { language: 'rsl', scheme: 'untitled', pattern: '**/*.rsl.json' },
    { language: 'json', scheme: 'file', pattern: '**/*.rsl.json' },
    { language: 'json', scheme: 'untitled', pattern: '**/*.rsl.json' },
  ];

  // initialize + 能力宣告
  client.request('initialize', {
    processId: process.pid,
    rootUri: vscode.workspace.workspaceFolders?.[0]?.uri.toString() ?? null,
    capabilities: {},
  }).then(() => client.notify('initialized', {}));

  const publish = (e) => {
    const c = vscode.languages.createDiagnosticCollection('rsl');
    context.subscriptions.push(c);
    return c;
  };
  const diagCollection = publish();

  const pushDiags = (uri, diagnostics) => {
    diagCollection.set(
      vscode.Uri.parse(uri),
      diagnostics.map((d) => {
        const vd = new vscode.Diagnostic(
          new vscode.Range(d.range.start.line, d.range.start.character,
            d.range.end.line, d.range.end.character),
          d.message,
          d.severity === 1 ? vscode.DiagnosticSeverity.Error
            : d.severity === 2 ? vscode.DiagnosticSeverity.Warning
              : vscode.DiagnosticSeverity.Information,
        );
        // F2：透传 data.fix（快速修复依赖它回传 server 生成 TextEdit）
        if (d.data !== undefined) vd.data = d.data;
        vd.source = 'rsl';
        if (d.code !== undefined) vd.code = d.code;
        return vd;
      }),
    );
    // 表单面板诊断条（校验统一走语言服务）
    formPanel.onDiagnostics(uri, diagnostics);
  };

  client.onNotificationFallback = null; // 传输层直接轮询 notifications（lib/rpc.js）

  // 全量同步：didOpen/didChange/didSave → server publishDiagnostics
  const sync = (doc) => {
    if (!doc || !doc.fileName.endsWith('.rsl.json')) return;
    client.notify('textDocument/didOpen', {
      textDocument: {
        uri: doc.uri.toString(), languageId: 'json', version: doc.version, text: doc.getText(),
      },
    });
  };
  vscode.workspace.onDidOpenTextDocument(sync, null, context.subscriptions);
  vscode.workspace.onDidSaveTextDocument(sync, null, context.subscriptions);
  vscode.workspace.onDidChangeTextDocument((e) => {
    if (!e.contentChanges.length) return;
    client.notify('textDocument/didChange', {
      textDocument: { uri: e.document.uri.toString(), version: e.document.version },
      contentChanges: [{ text: e.document.getText() }],
    });
    // M2：文本 → 表单（面板内部会跳过自己 apply 触发的变更）
    formPanel.onDocChanged(e.document);
    // T5/T6：编辑缓冲 → 调试面板（免保存实时重跑，前端 800ms 防抖）
    debugPanel.onDocChanged(e.document);
  }, null, context.subscriptions);
  vscode.workspace.textDocuments.forEach(sync);

  context.subscriptions.push(
    vscode.commands.registerCommand('rsl.formEditor', () => {
      formPanel.open(vscode.window.activeTextEditor);
    }),
    vscode.commands.registerCommand('rsl.debug', async () => {
      if (debugPanel.serve && !debugPanel.serve.exited) {
        const pick = await vscode.window.showQuickPick(['打开调试器', '停止调试服务'], { title: 'RSL 调试' });
        if (pick === '打开调试器') await debugPanel.open(vscode.window.activeTextEditor);
        if (pick === '停止调试服务') await debugPanel.stop();
      } else {
        await debugPanel.open(vscode.window.activeTextEditor);
      }
    }),
  );

  // 轮询泵：通知 → 诊断集合（lib/rpc.js 的 notifications 数组）
  const pump = setInterval(() => {
    while (client && client.notifications.length) {
      const n = client.notifications.shift();
      if (n.method === 'textDocument/publishDiagnostics') {
        pushDiags(n.params.uri, n.params.diagnostics ?? []);
      }
    }
  }, 50);
  context.subscriptions.push({ dispose: () => clearInterval(pump) });

  // hover / completion / definition / codeAction → 直接转发请求
  const passthrough = (method) => async (doc, pos) => {
    const r = await client.request(method, {
      textDocument: { uri: doc.uri.toString() },
      position: { line: pos.line, character: pos.character },
    });
    return r.result ?? null;
  };
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(docSelector, { provideHover: passthrough('textDocument/hover') }),
    vscode.languages.registerCompletionItemProvider(docSelector, {
      provideCompletionItems: async (doc, pos) => {
        const r = await passthrough('textDocument/completion')(doc, pos);
        if (!Array.isArray(r)) return null;
        return r.map((it) => {
          const item = new vscode.CompletionItem(it.label, it.kind ?? vscode.CompletionItemKind.Text);
          item.detail = it.detail;
          return item;
        });
      },
    }, ':', '.', '"'),
    vscode.languages.registerDefinitionProvider(docSelector, { provideDefinition: passthrough('textDocument/definition') }),
    vscode.languages.registerCodeActionsProvider(docSelector, {
      provideCodeActions: async (doc, range, ctx) => {
        const r = await client.request('textDocument/codeAction', {
          textDocument: { uri: doc.uri.toString() },
          range: { start: range.start, end: range.end },
          context: { diagnostics: ctx.diagnostics.map((d) => ({
            range: { start: d.range.start, end: d.range.end },
            code: d.code, message: d.message, data: (d && d.data) || undefined,
          })) },
        });
        const actions = r.result ?? [];
        return actions.map((a) => {
          const action = new vscode.CodeAction(a.title, vscode.CodeActionKind.QuickFix);
          action.diagnostics = a.diagnostics;
          if (a.edit) {
            action.edit = new vscode.WorkspaceEdit();
            for (const [uri, edits] of Object.entries(a.edit.changes ?? {})) {
              action.edit.set(vscode.Uri.parse(uri), edits.map((e) =>
                vscode.TextEdit.replace(
                  new vscode.Range(e.range.start.line, e.range.start.character,
                    e.range.end.line, e.range.end.character),
                  e.newText,
                )));
            }
          }
          return action;
        });
      },
    }),
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
