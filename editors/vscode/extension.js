// RSL VS Code 扩展（TASK-BOOK-3 D1）：
// 启动 rsl-lsp（stdio）→ 诊断 / hover / 补全 / 定义 / code action 全部来自语言服务。
// 传输层在 lib/rpc.js（零依赖手写 JSON-RPC，与 server 同风格）。
'use strict';

const vscode = require('vscode');
const { LspClient } = require('./lib/rpc');

let client = null;

function activate(context) {
  const cfg = vscode.workspace.getConfiguration('rsl');
  const lspPath = cfg.get('lspPath') || 'rsl-lsp';

  client = new LspClient(lspPath);
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
      diagnostics.map((d) => new vscode.Diagnostic(
        new vscode.Range(d.range.start.line, d.range.start.character,
          d.range.end.line, d.range.end.character),
        d.message,
        d.severity === 1 ? vscode.DiagnosticSeverity.Error
          : d.severity === 2 ? vscode.DiagnosticSeverity.Warning
            : vscode.DiagnosticSeverity.Information,
      )),
    );
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
  }, null, context.subscriptions);
  vscode.workspace.textDocuments.forEach(sync);

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
