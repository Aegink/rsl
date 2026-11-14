// D1 验收：扩展协议冒烟（node，零依赖）。
// 用与 extension.js 相同的传输层（lib/rpc.js）驱动真实 rsl-lsp：
// initialize → didOpen → diagnostics → completion → codeAction → apply → 诊断消失。
// 用法：node test/protocol-smoke.js（RSL_LSP 环境变量可指定 server 路径）
'use strict';

const path = require('path');
const fs = require('fs');
const { LspClient } = require('../lib/rpc');

const SERVER = process.env.RSL_LSP
  || path.join(__dirname, '..', '..', '..', 'target', 'debug', 'rsl-lsp.exe');

const BROKEN = JSON.stringify({
  rsl: '2.0', id: 'vscode-smoke.example.com', name: '扩展冒烟站', type: 'novel',
  baseUrl: 'https://vscode-smoke.example.com',
  requires: { features: [], crypto: [] },
  stages: {
    search: {
      request: { url: 'https://vscode-smoke.example.com/search', params: { q: 'var:kw' } },
      out: {
        list: 'css:ul.list li',
        map: {
          name: { $: 'css:a.title', transform: [{ trim: true }] },
          url: { $: 'css:a.title', attr: 'href', transform: [{ absolute: true }] },
        },
        emptyOk: true,
      },
    },
    detail: {
      request: { url: 'var:book.urll' },
      out: { name: 'css:h1', tocUrl: { $: 'css:a.read', attr: 'href', optional: true } },
    },
    toc: {
      request: { url: 'var:book.tocUrl' },
      out: { list: 'css:#list li', map: {
        title: { $: 'css:a', transform: [{ trim: true }] },
        url: { $: 'css:a', attr: 'href', transform: [{ absolute: true }] },
      } },
    },
    content: {
      request: { url: 'var:chapter.url' },
      out: { content: { $: 'css:div#content', paragraphs: { split: 'br' } } },
    },
  },
}, null, 2);

function applyEdit(text, edit) {
  const lines = text.split('\n');
  const s = edit.range.start;
  const e = edit.range.end;
  const line = lines[s.line];
  lines[s.line] = line.slice(0, s.character) + edit.newText + line.slice(e.character);
  return lines.join('\n');
}

(async () => {
  const results = {};
  const client = new LspClient(SERVER);
  try {
    // 1) initialize
    const init = await client.request('initialize', {
      processId: process.pid, rootUri: null, capabilities: {},
    });
    results.initialize = Boolean(init.result && init.result.capabilities);
    client.notify('initialized', {});

    // 2) didOpen → diagnostics（含 E_REF + fix data）
    const URI = 'file:///vscode-smoke.rsl.json';
    client.notify('textDocument/didOpen', {
      textDocument: { uri: URI, languageId: 'json', version: 1, text: BROKEN },
    });
    const pub = await client.waitFor(
      (n) => n.method === 'textDocument/publishDiagnostics' && n.params.uri === URI,
      4000,
    );
    const eref = (pub.params.diagnostics ?? []).find((d) => d.code === 'E_REF');
    results.diagnostics = (pub.params.diagnostics ?? []).length;
    results.eref_with_fix = Boolean(eref && eref.data && eref.data.fix);

    // 3) codeAction → 编辑
    const ca = await client.request('textDocument/codeAction', {
      textDocument: { uri: URI },
      range: { start: eref.range.start, end: eref.range.start },
      context: { diagnostics: [eref] },
    });
    const action = (ca.result ?? [])[0];
    results.codeAction = Boolean(action && action.edit);
    let fixed = BROKEN;
    if (action) {
      for (const ed of (action.edit.changes[URI] ?? [])) fixed = applyEdit(fixed, ed);
    }

    // 4) didChange(全量) → E_REF 消失
    client.notify('textDocument/didChange', {
      textDocument: { uri: URI, version: 2 },
      contentChanges: [{ text: fixed }],
    });
    const pub2 = await client.waitFor(
      (n) => n.method === 'textDocument/publishDiagnostics' && n.params.uri === URI
        && n.params.textDocument === undefined
        && (n.params.version ?? 2) >= 2
        && (n.params.diagnostics ?? []).every((d) => d.code !== 'E_REF'),
      4000,
    ).catch(() => null);
    results.fix_resolves = Boolean(pub2);

    // 5) completion（命名空间）：首个 "$" 键位（其前无 transform 上下文）
    const firstDollar = BROKEN.indexOf('"$": "');
    const before = BROKEN.slice(0, firstDollar + '"$": "'.length);
    const pos = { line: before.split('\n').length - 1,
      character: before.slice(before.lastIndexOf('\n') + 1).length };
    const comp = await client.request('textDocument/completion', {
      textDocument: { uri: URI }, position: pos,
    });
    const items = Array.isArray(comp.result) ? comp.result : [];
    results.completion_ns = items.filter((i) => (i.label || '').endsWith(':')).length;

    await client.request('shutdown', null);
    client.notify('exit', null);
  } finally {
    client.end();
    await Promise.race([client.closed, new Promise((r) => setTimeout(r, 1500))]);
  }

  console.log(JSON.stringify(results, null, 2));
  const checks = ['initialize', 'eref_with_fix', 'codeAction', 'fix_resolves'];
  const fails = checks.filter((c) => !results[c]);
  const nsOk = results.completion_ns >= 15;
  if (!nsOk) fails.push('completion_ns');
  console.log('\n== rsl-vscode 协议冒烟 ==');
  console.log(fails.length === 0 && nsOk ? 'PASS 6/6' : `FAIL: ${fails}`);
  process.exit(fails.length === 0 && nsOk ? 0 : 1);
})();
