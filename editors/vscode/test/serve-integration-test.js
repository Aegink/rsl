// T6 集成测试（TASK-BOOK-4）：ServeProc 生命周期 spawn → health → run → 回收。
// 零依赖 node 直跑；rsl 二进制从工作区 target/debug 定位。
'use strict';

const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const { ServeProc } = require('../lib/serve-proc');

let pass = 0;
let fail = 0;
function t(name, cond, detail) {
  if (cond) {
    pass++;
    console.log(`  ok  ${name}`);
  } else {
    fail++;
    console.log(`FAIL  ${name}  ${detail || ''}`);
  }
}

function findRsl() {
  const exe = process.platform === 'win32' ? 'rsl.exe' : 'rsl';
  // 编辑器目录向上找仓库根 target/debug
  let dir = path.resolve(__dirname, '..', '..', '..');
  const cand = path.join(dir, 'target', 'debug', exe);
  if (fs.existsSync(cand)) return cand;
  const which = spawnSync(process.platform === 'win32' ? 'where' : 'which', [exe], { encoding: 'utf-8' });
  if (which.status === 0 && which.stdout.trim()) return which.stdout.trim().split(/\r?\n/)[0];
  return null;
}

async function main() {
  const exe = findRsl();
  t('找到 rsl 二进制', !!exe, '先 cargo build -p rsl-cli');
  if (!exe) process.exit(1);

  const webRoot = path.resolve(__dirname, '..', '..', 'web-debug');
  const proc = new ServeProc({ exePath: exe, webRoot: fs.existsSync(webRoot) ? webRoot : undefined });

  // 1) spawn → 监听行解析
  const started = await proc.start();
  t('serve 启动（随机端口解析）', Number.isInteger(started.port) && started.port > 0, JSON.stringify(started));
  t('url 回环', proc.url.startsWith('http://127.0.0.1:'));

  // 2) health
  const h = await proc.health();
  t('health ok', h && h.status === 200 && h.body.ok === true, JSON.stringify(h));

  // 3) 无 token → 401（token 模式生效）
  const raw = await new Promise((resolve) => {
    require('http').get({ host: '127.0.0.1', port: started.port, path: '/health' }, (r) => resolve(r.statusCode))
      .on('error', () => resolve(0));
  });
  t('无 token → 401', raw === 401, String(raw));

  // 4) debug/run（fixture 离线重放）→ 四数据面
  const srcText = fs.readFileSync(path.resolve(__dirname, '..', '..', '..', 'examples', 'novel-html.rsl.json'), 'utf-8');
  const fixtureHtml = fs.readFileSync(
    path.resolve(__dirname, '..', '..', '..', 'examples', 'fixtures', 'detail.fixture.html'), 'utf-8');
  const r = await proc.run({
    source_text: srcText,
    stage: 'detail',
    label: 'vscode-test',
    inputs: { book: { url: 'https://example.com/book/1' } },
    fixture: { status: 200, headers: { 'Content-Type': 'text/html; charset=utf-8' }, body: fixtureHtml },
  });
  t('run HTTP 200 + schema', r.status === 200 && r.body.schema === 'rsl-debug/1', String(r.status));
  t('四数据面齐（request/response/out/trace）',
    r.body.request && r.body.response && r.body.out && Array.isArray(r.body.trace)
    && r.body.out.name === '斗破苍穹',
    JSON.stringify(r.body.out || {}).slice(0, 120));

  // 5) 回收干净
  const pid = proc.child.pid;
  await proc.stop();
  await new Promise((res) => setTimeout(res, 300));
  const alive = process.platform === 'win32'
    ? spawnSync('tasklist', ['/FI', `PID eq ${pid}`], { encoding: 'utf-8' }).stdout.includes(String(pid))
    : fs.existsSync(`/proc/${pid}`);
  t('进程回收', !alive, `pid=${pid} 仍存活`);
  t('stop 幂等（二次调用不炸）', await proc.stop() === undefined);

  console.log(`\nserve-integration: ${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
