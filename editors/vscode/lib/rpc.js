// RSL LSP client 传输层（零依赖，与 rsl-lsp server 的手写帧协议同风格）。
// 供 extension.js 与 test/protocol-smoke.js 共用——协议改动只动这一处。
'use strict';

const { spawn } = require('child_process');

/** 裸 JSON-RPC over stdio 客户端。 */
class LspClient {
  /**
   * @param {string} cmd   rsl-lsp 可执行文件路径
   * @param {string[]} args
   */
  constructor(cmd, args = []) {
    this.proc = spawn(cmd, args, { stdio: ['pipe', 'pipe', 'pipe'] });
    this.nextId = 1;
    this.pending = new Map(); // id -> resolve
    this.notifications = []; // {method, params}
    this.closed = new Promise((resolve) => this.proc.on('exit', resolve));
    let buf = Buffer.alloc(0);
    this.proc.stdout.on('data', (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      for (;;) {
        const sep = buf.indexOf('\r\n\r\n');
        if (sep < 0) break;
        const header = buf.slice(0, sep).toString('ascii');
        const m = /Content-Length:\s*(\d+)/i.exec(header);
        if (!m) { buf = buf.slice(sep + 4); continue; }
        const len = parseInt(m[1], 10);
        if (buf.length < sep + 4 + len) break;
        const body = buf.slice(sep + 4, sep + 4 + len);
        buf = buf.slice(sep + 4 + len);
        this._dispatch(JSON.parse(body.toString('utf-8')));
      }
    });
    this.proc.stderr.on('data', () => { /* server 日志不进通知流 */ });
  }

  _dispatch(msg) {
    if (msg.id !== undefined && (this.pending.has(msg.id))) {
      const r = this.pending.get(msg.id);
      this.pending.delete(msg.id);
      r(msg);
    } else if (msg.method) {
      this.notifications.push(msg);
    }
  }

  /** 请求（等待响应）。 */
  request(method, params) {
    const id = this.nextId++;
    this._send({ jsonrpc: '2.0', id, method, params });
    return new Promise((resolve) => this.pending.set(id, resolve));
  }

  /** 通知（不等响应）。 */
  notify(method, params) {
    this._send({ jsonrpc: '2.0', method, params });
  }

  _send(msg) {
    const body = Buffer.from(JSON.stringify(msg), 'utf-8');
    this.proc.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.proc.stdin.write(body);
  }

  /** 等待满足谓词的通知（轮询，超时 reject）。 */
  async waitFor(pred, timeoutMs = 5000) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const hit = this.notifications.find(pred);
      if (hit) return hit;
      if (Date.now() > deadline) throw new Error('waitFor 超时');
      await new Promise((r) => setTimeout(r, 25));
    }
  }

  end() {
    try { this.proc.stdin.end(); } catch (_) { /* 已退出 */ }
  }
}

/** 光标定位：子串 → LSP 位置（UTF-16 code unit 列，与 server 一致）。 */
function findPos(text, needle, occurrence = 1) {
  let idx = -1;
  for (let i = 0; i < occurrence; i++) idx = text.indexOf(needle, idx + 1);
  if (idx < 0) return null;
  const line = text.slice(0, idx).split('\n').length - 1;
  const lineStart = text.lastIndexOf('\n', idx - 1) + 1;
  const seg = text.slice(lineStart, idx);
  return { line, character: seg.length + 1 };
}

module.exports = { LspClient, findPos };
