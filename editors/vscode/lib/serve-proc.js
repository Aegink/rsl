// rsl serve 子进程生命周期（TASK-BOOK-4 T6）。
// 纯 Node 模块（不依赖 vscode）——DebugPanel 与集成测试共用。
'use strict';

const { spawn } = require('child_process');
const crypto = require('crypto');
const http = require('http');

/** 随机 token（V4-D11：回环 + 随机端口 + token）。 */
function randomToken() {
  return crypto.randomBytes(16).toString('hex');
}

/** 解析 RSL_SERVE listening=127.0.0.1:PORT 行。 */
function parseListening(line) {
  const m = /listening=127\.0\.0\.1:(\d+)/.exec(line || '');
  return m ? parseInt(m[1], 10) : null;
}

/**
 * rsl serve 进程句柄。start() 在读到监听行后 resolve。
 * 停止语义：stop() 杀进程树（Windows 无 POSIX 信号，用 taskkill 兜底 + kill）。
 */
class ServeProc {
  /**
   * @param {{ exePath: string, webRoot?: string, port?: number, token?: string }} opts
   */
  constructor(opts) {
    this.exePath = opts.exePath;
    this.webRoot = opts.webRoot;
    this.token = opts.token || randomToken();
    this.port = opts.port || 0; // 0 = 内核随机端口
    this.child = null;
    this.listeningPort = null;
    this.exited = null; // { code, signal }
    this._stdoutBuf = '';
  }

  start() {
    return new Promise((resolve, reject) => {
      const args = ['serve', '--port', String(this.port), '--token', this.token];
      if (this.webRoot) args.push('--web', this.webRoot);
      let settled = false;
      this.child = spawn(this.exePath, args, { stdio: ['ignore', 'pipe', 'pipe'] });
      this.child.stdout.on('data', (chunk) => {
        this._stdoutBuf += chunk.toString();
        if (!settled) {
          for (const line of this._stdoutBuf.split('\n')) {
            const p = parseListening(line);
            if (p !== null) {
              this.listeningPort = p;
              settled = true;
              resolve({ port: p, url: `http://127.0.0.1:${p}`, token: this.token });
              return;
            }
          }
        }
      });
      this.child.stderr.on('data', (chunk) => { this._stdoutBuf += chunk.toString(); });
      this.child.on('exit', (code, signal) => {
        this.exited = { code, signal };
        if (!settled) {
          settled = true;
          reject(new Error(`rsl serve 提前退出 code=${code}：${this._stdoutBuf.slice(0, 400)}`));
        }
      });
      this.child.on('error', (err) => {
        if (!settled) {
          settled = true;
          reject(err);
        }
      });
      setTimeout(() => {
        if (!settled) {
          settled = true;
          reject(new Error(`rsl serve 启动超时（8s）：${this._stdoutBuf.slice(0, 400)}`));
        }
      }, 8000);
    });
  }

  get url() {
    return this.listeningPort ? `http://127.0.0.1:${this.listeningPort}` : null;
  }

  /** GET /health 探活。 */
  health(timeoutMs = 3000) {
    return new Promise((resolve) => {
      const req = http.get(
        { host: '127.0.0.1', port: this.listeningPort, path: `/health?token=${this.token}`, timeout: timeoutMs },
        (res) => {
          let body = '';
          res.on('data', (c) => { body += c; });
          res.on('end', () => {
            try { resolve({ status: res.statusCode, body: JSON.parse(body) }); }
            catch (_) { resolve({ status: res.statusCode, body }); }
          });
        },
      );
      req.on('error', () => resolve(null));
      req.on('timeout', () => { req.destroy(); resolve(null); });
    });
  }

  /** POST /debug/run（source_text 随请求携带；fixture 可选——离线重放）。 */
  run(payload, timeoutMs = 30000) {
    return new Promise((resolve, reject) => {
      const data = Buffer.from(JSON.stringify(payload));
      const req = http.request(
        {
          host: '127.0.0.1', port: this.listeningPort, path: `/debug/run?token=${this.token}`,
          method: 'POST', headers: { 'Content-Type': 'application/json', 'Content-Length': data.length },
          timeout: timeoutMs,
        },
        (res) => {
          let body = '';
          res.on('data', (c) => { body += c; });
          res.on('end', () => {
            try { resolve({ status: res.statusCode, body: JSON.parse(body) }); }
            catch (e) { reject(e); }
          });
        },
      );
      req.on('error', reject);
      req.on('timeout', () => { req.destroy(); reject(new Error('debug/run 超时')); });
      req.write(data);
      req.end();
    });
  }

  /** 停止（回收进程树）。 */
  stop() {
    return new Promise((resolve) => {
      if (!this.child || this.exited) { resolve(); return; }
      const child = this.child;
      if (process.platform === 'win32') {
        // Windows：taskkill /T 连子进程一起回收
        const t = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
        t.on('exit', () => resolve());
        t.on('error', () => resolve());
      } else {
        child.kill('SIGTERM');
      }
      const guard = setTimeout(() => { try { child.kill('SIGKILL'); } catch (_) { /* 已退出 */ } resolve(); }, 3000);
      child.on('exit', () => { clearTimeout(guard); resolve(); });
    });
  }
}

module.exports = { ServeProc, randomToken, parseListening };
