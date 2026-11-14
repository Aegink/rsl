/* RSL 调试器前端（TASK-BOOK-4 T3 五页×四tab + T4 链式 + T5 实时 + T7 诊断直通）
 * 零解析逻辑（V4-D1/D8）：只消费 rsl-debug/1 产物渲染 + 按契约字段编排链式注入。
 * 执行唯一在 Rust 内核（rsl serve /debug/run）。 */
'use strict';

const qs = new URLSearchParams(location.search);
const TOKEN = qs.get('token') || '';

const STAGE_LABELS = { search: '搜索', explore: '发现', detail: '详情', toc: '目录', content: '正文' };
const CHAIN_ORDER = ['search', 'explore', 'detail', 'toc', 'content'];
const NEEDS_BOOK = new Set(['detail', 'toc']);

const state = {
  sourceText: '',
  stage: 'search',
  tab: 'raw',
  rawMode: 'raw',
  pageIdx: 0,
  book: null, // 链式选书（out.map 契约字段：name/url/…）
  chapter: null,
  group: null,
  filter: {},
  artifacts: {}, // stage → 最近 rsl-debug/1 产物
  live: true,
  running: false,
};

const $ = (id) => document.getElementById(id);
const el = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};

// ───────────────────────── API ─────────────────────────

async function api(path, opts = {}) {
  const sep = path.includes('?') ? '&' : '?';
  const url = path + sep + 'token=' + encodeURIComponent(TOKEN);
  const res = await fetch(url, opts);
  if (!res.ok) {
    let detail = '';
    try { detail = (await res.json()).error || ''; } catch (_) { /* 忽略 */ }
    throw new Error(`HTTP ${res.status} ${detail}`);
  }
  return res.json();
}

// ───────────────────────── 运行（T2 消费端） ─────────────────────────

/** 离线重放 fixture（L2 语义：录制响应重放，调试不碰真实站点；Base64 支持 GBK/二进制） */
function fixturePayload() {
  if (!$('fx-on').checked) return null;
  const bodyText = $('fx-body').value;
  if (!bodyText) return null;
  const payload = {
    status: parseInt($('fx-status').value, 10) || 200,
    headers: { 'Content-Type': $('fx-ct').value || 'text/html; charset=utf-8' },
  };
  if ($('fx-b64').checked) payload.body_base64 = bodyText.trim();
  else payload.body = bodyText;
  return payload;
}

function chainInputs() {
  const inputs = {};
  if (state.stage === 'search') {
    const kw = $('in-kw').value.trim();
    const page = parseInt($('in-page').value, 10) || 1;
    if (kw) inputs.kw = kw;
    if (page > 1) inputs.page = page;
  } else if (state.stage === 'explore') {
    if (state.group) inputs.group = state.group;
    if (Object.keys(state.filter).length) inputs.filter = { ...state.filter };
  } else if (state.stage === 'detail' || state.stage === 'toc') {
    inputs.book = { ...state.book };
  } else if (state.stage === 'content') {
    inputs.chapter = { ...state.chapter };
  }
  return inputs;
}

/** 前置阶段检查（对齐阅读"前置阶段须先成功"；缺前置给指引而非报错） */
function checkPrerequisite() {
  if (NEEDS_BOOK.has(state.stage) && !state.book) {
    return '缺前置：先在「搜索」或「发现」里点选一本书（结果卡片可点）';
  }
  if (state.stage === 'toc' && state.book && !state.book.tocUrl && !hasChainOut('detail')) {
    return '缺前置：先在「详情」里跑出 tocUrl（或该书自带 tocUrl）';
  }
  if (state.stage === 'content' && !state.chapter) {
    return '缺前置：先在「目录」里点选一个章节';
  }
  return null;
}

function hasChainOut(stage) {
  return state.artifacts[stage] && state.artifacts[stage].ok;
}

async function runStage(opts = {}) {
  const preErr = checkPrerequisite();
  if (preErr) {
    if (opts.silent) return;
    showDiag([{ code: 'E_CHAIN', message: preErr, hint: '按链式轨迹自上而下点选' }], null);
    return;
  }
  if (state.running) return;
  if (!state.sourceText.trim()) {
    showDiag([{ code: 'E_NO_SOURCE', message: '还没有源文本', hint: '左侧粘贴 RSL 源（JSON）' }], null);
    return;
  }
  state.running = true;
  $('btn-run').disabled = true;
  $('run-status').innerHTML = '<span class="spinner"></span>运行中…';
  const inputs = chainInputs();
  try {
    const payload = {
      source_text: state.sourceText,
      stage: state.stage,
      inputs,
      label: $('label').textContent || 'web',
    };
    const fx = fixturePayload();
    if (fx) payload.fixture = fx;
    const artifact = await api('/debug/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    state.artifacts[state.stage] = artifact;
    state.pageIdx = 0;
    // 链式推进：detail 成功 → 把 out 合回选书（tocUrl 供 toc 用）
    if (state.stage === 'detail' && artifact.ok) {
      state.book = { ...state.book, ...(artifact.out || {}) };
      renderBookView();
    }
    renderAll();
    $('conn-dot').className = 'dot dot-on';
    $('conn-text').textContent = '已连接';
    $('run-status').textContent = artifact.ok
      ? `完成 ${artifact.elapsed_ms}ms`
      : '失败（见诊断）';
    refreshChainList();
  } catch (e) {
    $('conn-dot').className = 'dot dot-off';
    $('conn-text').textContent = '连接失败';
    showDiag([{ code: 'E_SERVE', message: String(e.message || e), hint: '确认 rsl serve 还在运行（VS Code 用户：重新运行「RSL: 调试源」）' }], null);
    $('run-status').textContent = '';
  } finally {
    state.running = false;
    $('btn-run').disabled = false;
  }
}

// ───────────────────────── 诊断（T7：直通 + 诚实降级） ─────────────────────────

function showDiag(diags, error, artifact) {
  const bar = $('diag-bar');
  bar.innerHTML = '';
  bar.className = 'diag-bar' + (error ? '' : diags && diags.length ? ' warn' : '');
  const items = [];
  if (error) items.push(error);
  for (const d of diags || []) items.push(d);
  if (!items.length) { bar.classList.add('hidden'); return; }
  bar.classList.remove('hidden');
  for (const d of items) {
    const chip = el('span', 'diag-chip' + (d.severity === 'Warning' ? ' warn' : ''));
    chip.textContent = `${d.code} · ${d.message}`;
    chip.title = `${d.path || ''}\n${d.hint || ''}`;
    bar.appendChild(chip);
  }
  // E_UNSUPPORTED 诚实降级（V4-D7）：webview/登录能力缺口如实呈现 + 引导
  const unsupported = (error && error.code === 'E_UNSUPPORTED') ||
    (diags || []).some((d) => d.code === 'E_UNSUPPORTED');
  const needsWebview = state.sourceDoc && ((state.sourceDoc.requires || {}).features || []).includes('webview');
  if (unsupported || (error && needsWebview)) {
    const box = el('div', 'diag-unsupported');
    box.textContent = '⚠ 该源需要 WebView/登录会话能力——桌面调试器没有浏览器渲染，无法真实执行此阶段（E_UNSUPPORTED，诚实拒绝而非模拟）。';
    bar.appendChild(box);
    const guide = el('div', 'diag-guide');
    guide.textContent = '引导：这是能力声明（requires.features）与宿主能力的诚实差异。要调试 webview/登录阶段，请使用带真 WebView 的宿主（App/浏览器内核）；调试器的其他阶段不受影响。';
    bar.appendChild(guide);
  }
}

// ───────────────────────── 四 tab 渲染 ─────────────────────────

function currentRecord(artifact) {
  const reqs = artifact.requests || [];
  const i = Math.min(state.pageIdx, Math.max(reqs.length - 1, 0));
  return reqs[i] || null;
}

function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function renderAll() {
  const artifact = state.artifacts[state.stage];
  renderPagePicker(artifact);
  renderTabs(artifact);
  renderDiagFromArtifact(artifact);
}

function renderDiagFromArtifact(artifact) {
  if (!artifact) { showDiag([], null); return; }
  showDiag(artifact.diagnostics || [], artifact.ok ? null : artifact.error, artifact);
}

function renderPagePicker(artifact) {
  const picker = $('page-picker');
  picker.innerHTML = '';
  const n = artifact && artifact.requests ? artifact.requests.length : 0;
  if (n <= 1) { picker.classList.add('hidden'); return; }
  picker.classList.remove('hidden');
  picker.appendChild(el('span', undefined, `本次共 ${n} 次请求（分页）`));
  const sel = document.createElement('select');
  for (let i = 0; i < n; i++) {
    const op = el('option', undefined, `第 ${i + 1} 次`);
    op.value = String(i);
    sel.appendChild(op);
  }
  sel.value = String(Math.min(state.pageIdx, n - 1));
  sel.onchange = () => { state.pageIdx = parseInt(sel.value, 10); renderTabs(artifact); };
  picker.appendChild(sel);
}

function renderTabs(artifact) {
  document.querySelectorAll('#tabs .tab').forEach((b) => {
    b.classList.toggle('active', b.dataset.tab === state.tab);
  });
  $('raw-tools').classList.toggle('hidden', state.tab !== 'raw');
  $('log-tools').classList.toggle('hidden', state.tab !== 'logs');
  const body = $('tab-body');
  body.innerHTML = '';
  if (!artifact) {
    body.appendChild(emptyState('尚无结果', '左侧「输入」区配好参数后点 ▶ 运行'));
    return;
  }
  if (state.tab === 'raw') renderRawTab(body, artifact);
  else if (state.tab === 'out') renderOutTab(body, artifact);
  else if (state.tab === 'head') renderHeadTab(body, artifact);
  else if (state.tab === 'logs') renderLogs();
  else renderVisualTab(body, artifact);
}

/** Tab1 原始数据：响应体解码预览（真实数据；美化/取消换行/还原 工具条） */
function renderRawTab(body, artifact) {
  const rec = currentRecord(artifact);
  const tools = $('raw-tools');
  tools.querySelectorAll('button').forEach((b) => {
    b.classList.toggle('active', b.dataset.rawmode === state.rawMode);
  });
  $('raw-meta').textContent = rec && rec.response
    ? `${fmtBytes(rec.response.body_bytes)} · ${rec.response.ms}ms`
    : '';
  if (!rec || !rec.response) {
    body.appendChild(emptyState('无响应', artifact.ok ? '本阶段未发起请求（纯常量/跳过）' : '网络层失败或请求未发出（诚实展示，无伪造数据）'));
    return;
  }
  const resp = rec.response;
  if (resp.binary) {
    body.appendChild(el('div', 'raw-truncated', `二进制响应（${fmtBytes(resp.body_bytes)}，parse=bytes）：sha256=${resp.sha256}。调试器不做二进制预览。`));
    return;
  }
  if (resp.truncated) {
    body.appendChild(el('div', 'raw-truncated', `响应体 ${fmtBytes(resp.body_bytes)} 超过 256KB 预览预算，仅显示前缀（完整原始体未落盘，sha256=${resp.sha256.slice(0, 16)}…）。`));
  }
  let text = resp.preview || '';
  if (state.rawMode === 'pretty') text = prettyAuto(text);
  else if (state.rawMode === 'unwrap') text = text.replace(/\s*\n\s*/g, '');
  const pre = el('pre', 'raw');
  pre.textContent = text;
  body.appendChild(pre);
}

function prettyAuto(text) {
  const t = text.trim();
  if (t.startsWith('{') || t.startsWith('[')) {
    try { return JSON.stringify(JSON.parse(t), null, 2); } catch (_) { /* 保留原文 */ }
  }
  // 简易 HTML 缩进：块级标签前换行
  return t
    .replace(/>\s*</g, '>\n<')
    .split('\n')
    .reduce(({ lines, depth }, line) => {
      if (/^<\//.test(line.trim())) depth = Math.max(depth - 1, 0);
      lines.push('  '.repeat(depth) + line.trim());
      if (/^<[^/!][^>]*[^/]>$/.test(line.trim()) && !/^<(br|img|input|meta|link|hr)/i.test(line.trim())) depth += 1;
      return { lines, depth };
    }, { lines: [], depth: 0 }).lines.join('\n');
}

/** Tab2 解析结果：out 契约值 JSON（美化） */
function renderOutTab(body, artifact) {
  const pre = el('pre', 'raw');
  pre.textContent = artifact.out === null ? '（失败：out 为空，见诊断顶栏）' : JSON.stringify(artifact.out, null, 2);
  body.appendChild(pre);
}

/** Tab3 协议标头：原始地址/请求地址/重定向/重试/引擎/方式/解析方式 + 两侧头 */
function renderHeadTab(body, artifact) {
  const rec = currentRecord(artifact);
  if (!rec) {
    body.appendChild(emptyState('无请求', artifact.ok ? '本阶段未发起请求（纯常量/跳过，request=null 合法）' : '请求未发出'));
    return;
  }
  const r = rec.request;
  const redirected = rec.response && rec.response.final_url && rec.response.final_url !== r.url;
  const table = el('table', 'head-table');
  const row = (k, v) => {
    const tr = el('tr');
    tr.appendChild(el('td', 'k', k));
    const td = el('td');
    if (typeof v === 'string' && v.length > 80) { const c = el('code'); c.textContent = v; td.appendChild(c); }
    else td.textContent = v;
    tr.appendChild(td);
    table.appendChild(tr);
  };
  row('原始地址', r.template || '（模板缺省 → baseUrl）');
  row('请求地址', r.url);
  row('重定向', redirected ? `是 → ${rec.response.final_url}` : '否');
  row('重试', `${r.attempts} 次（含首次）`);
  row('请求引擎', r.engine);
  row('请求方式', r.method);
  row('解析方式', r.parse);
  if (rec.response) {
    row('状态码', `${rec.response.status} · ${rec.response.ms}ms`);
    row('响应大小', fmtBytes(rec.response.body_bytes) + (rec.response.truncated ? '（预览截断）' : ''));
    row('响应 sha256', rec.response.sha256);
  } else {
    row('状态码', '（无响应——网络层失败）');
  }
  body.appendChild(table);

  const h = el('div', 'h-sub', '请求头（实际发送）');
  body.appendChild(h);
  body.appendChild(kvTable(r.headers));
  if (rec.response) {
    const h2 = el('div', 'h-sub', '响应头');
    body.appendChild(h2);
    body.appendChild(kvTable(rec.response.headers));
  }
}

function kvTable(pairs) {
  const t = el('table', 'head-table');
  for (const [k, v] of pairs || []) {
    const tr = el('tr');
    tr.appendChild(el('td', 'k', k));
    const td = el('td');
    const c = el('code');
    c.textContent = v;
    td.appendChild(c);
    tr.appendChild(td);
    t.appendChild(tr);
  }
  return t;
}

/** Tab4 可视界面：out → 四形态（V4-D8 纯前端渲染，数据全部来自解析结果） */
function renderVisualTab(body, artifact) {
  if (!artifact.ok) {
    body.appendChild(emptyState('解析失败', '诊断顶栏有详情；修复后重跑即可看到可视化'));
    return;
  }
  const stage = artifact.stage;
  if (stage === 'detail') return renderVisualDetail(body, artifact.out || {});
  if (stage === 'toc') return renderVisualToc(body, artifact.out || {});
  if (stage === 'content') return renderVisualContent(body, artifact.out || {});
  // search / explore / hostStages 列表型
  const list = (artifact.out || {}).list;
  if (Array.isArray(list)) return renderVisualCards(body, list, stage);
  body.appendChild(emptyState('无可视化形态', '该阶段输出不是列表契约（out.list）；数据见「解析结果」tab'));
}

function cardFromItem(item) {
  const m = (item && item.map) || {};
  const card = el('div', 'book-card');
  if (m.cover) {
    const img = document.createElement('img');
    img.src = m.cover;
    img.loading = 'lazy';
    img.onerror = () => img.remove();
    card.appendChild(img);
  }
  const info = el('div');
  info.style.flex = '1';
  info.appendChild(el('div', 'bc-title', m.name || '（无书名）'));
  if (m.author) info.appendChild(el('div', 'bc-line', `作者：${m.author}`));
  if (m.kind) info.appendChild(el('div', 'bc-line', `分类：${m.kind}`));
  if (m.wordCount) info.appendChild(el('div', 'bc-line', `字数：${m.wordCount}`));
  if (m.lastChapter) info.appendChild(el('div', 'bc-line', `最新：${m.lastChapter}`));
  if (m.updateTime) info.appendChild(el('div', 'bc-line', `更新：${m.updateTime}`));
  if (m.intro) info.appendChild(el('div', 'bc-intro', m.intro));
  card.appendChild(info);
  return card;
}

function renderVisualCards(body, list, stage) {
  if (!list.length) { body.appendChild(emptyState('列表为空', '解析成功但没有任何条目（emptyOk）')); return; }
  const grid = el('div', 'book-grid');
  const selectable = stage === 'search' || stage === 'explore';
  list.forEach((item, idx) => {
    const card = cardFromItem(item);
    card.dataset.idx = String(idx);
    if (selectable) {
      card.title = '点击选书 → 自动进入「详情」';
      card.onclick = () => pickBook(item, card);
      if (state.book && state.book.url && item.map && item.map.url === state.book.url) card.classList.add('selected');
    }
    grid.appendChild(card);
  });
  body.appendChild(grid);
  if (selectable) {
    body.appendChild(el('p', 'hint', '↑ 点选任意卡片：按 out.map 契约字段注入 var:book，自动前往「详情」阶段（链式传值）。'));
  }
}

function renderVisualDetail(body, out) {
  const page = el('div', 'detail-page');
  if (out.cover) {
    const img = document.createElement('img');
    img.src = out.cover;
    img.onerror = () => img.remove();
    page.appendChild(img);
  }
  const info = el('div');
  info.appendChild(el('h2', 'dp-name', out.name || '（无书名）'));
  if (out.author) info.appendChild(el('div', 'dp-line', `作者：${out.author}`));
  if (out.kind) info.appendChild(el('div', 'dp-line', `分类：${out.kind}`));
  if (out.lastChapter) info.appendChild(el('div', 'dp-line', `最新章节：${out.lastChapter}`));
  if (out.updateTime) info.appendChild(el('div', 'dp-line', `更新时间：${out.updateTime}`));
  if (out.intro) info.appendChild(el('div', 'dp-intro', out.intro));
  const actions = el('div', 'dp-actions');
  const btn = el('button', 'btn primary', out.tocUrl ? '前往目录 →' : '无目录入口（tocUrl 缺失）');
  btn.disabled = !out.tocUrl;
  btn.onclick = () => switchStage('toc');
  actions.appendChild(btn);
  info.appendChild(actions);
  page.appendChild(info);
  body.appendChild(page);
  body.appendChild(el('p', 'hint', '详情数据来自 out（var:book 已合并本页结果，目录按钮走链式）。'));
}

function renderVisualToc(body, out) {
  const list = out.list || [];
  if (!list.length) { body.appendChild(emptyState('目录为空', '解析成功但没有任何章节')); return; }
  const ol = el('ol', 'chapter-list');
  list.forEach((item) => {
    const m = (item && item.map) || {};
    const li = el('li', m.isVolume ? 'vol' : '');
    li.appendChild(el('span', undefined, m.title || '（无题名）'));
    if (m.isVip) li.appendChild(el('span', 'badge', 'VIP'));
    if (m.isPay) li.appendChild(el('span', 'badge pay', '付费'));
    if (!m.isVolume && m.url) {
      li.title = '点击选章 → 自动进入「正文」';
      li.onclick = () => pickChapter(item, li);
    }
    ol.appendChild(li);
  });
  body.appendChild(ol);
  body.appendChild(el('p', 'hint', '↑ 点选章节：契约字段注入 var:chapter，自动前往「正文」。'));
}

function renderVisualContent(body, out) {
  const page = el('div', 'content-page');
  page.appendChild(el('h2', undefined, out.title || '（无章节标题）'));
  const paras = Array.isArray(out.content) ? out.content : [String(out.content || '')];
  for (const p of paras) page.appendChild(el('p', undefined, p));
  body.appendChild(page);
}

// ───────────────────────── 链式会话（T4） ─────────────────────────

/** search/explore 点选条目 → var:book（只传 out.map 契约字段，前端不发明字段） */
function pickBook(item, cardEl) {
  const m = (item && item.map) || {};
  state.book = { ...m };
  if (cardEl) {
    document.querySelectorAll('.book-card.selected').forEach((c) => c.classList.remove('selected'));
    cardEl.classList.add('selected');
  }
  renderBookView();
  refreshChainList();
  switchStage('detail');
}

/** toc 点选章节 → var:chapter */
function pickChapter(item, li) {
  const m = (item && item.map) || {};
  state.chapter = { ...m };
  renderChapterView();
  refreshChainList();
  switchStage('content');
}

function renderBookView() {
  // 显隐由 renderInputs 按阶段统一控制；这里只刷新内容
  const v = $('book-view');
  v.innerHTML = '';
  if (state.book) {
    for (const [k, val] of Object.entries(state.book)) {
      const line = el('div');
      const b = el('b', undefined, k + '：');
      line.appendChild(b);
      line.appendChild(document.createTextNode(String(val)));
      v.appendChild(line);
    }
  }
}

function renderChapterView() {
  const v = $('chapter-view');
  v.innerHTML = '';
  if (state.chapter) {
    const line = el('div');
    const b = el('b', undefined, 'title：');
    line.appendChild(b);
    line.appendChild(document.createTextNode(String(state.chapter.title || '')));
    v.appendChild(line);
    const l2 = el('div');
    const b2 = el('b', undefined, 'url：');
    l2.appendChild(b2);
    l2.appendChild(document.createTextNode(String(state.chapter.url || '')));
    v.appendChild(l2);
  }
}

function refreshChainList() {
  const ol = $('chain-list');
  ol.innerHTML = '';
  for (const st of ['search', 'detail', 'toc', 'content']) {
    const li = el('li', hasChainOut(st) ? 'done' : 'empty');
    const a = state.artifacts[st];
    const when = a ? `（${a.ok ? '成功' : '失败'} ${a.elapsed_ms}ms）` : '';
    li.textContent = `${STAGE_LABELS[st]}${when}`;
    ol.appendChild(li);
  }
  const li2 = el('li', hasChainOut('explore') ? 'done' : 'empty');
  const a = state.artifacts.explore;
  li2.textContent = `发现（独立入口${a ? `，${a.ok ? '成功' : '失败'}` : ''}）`;
  ol.appendChild(li2);
}

function switchStage(stage) {
  state.stage = stage;
  document.querySelectorAll('.stage-tab').forEach((b) => b.classList.toggle('active', b.dataset.stage === stage));
  renderInputs();
  const artifact = state.artifacts[stage];
  if (artifact && artifact.ok) { renderAll(); refreshChainList(); return; }
  runStage();
}

// ───────────────────────── 阶段/输入区 ─────────────────────────

function renderInputs() {
  const st = state.stage;
  $('inputs-search').classList.toggle('hidden', st !== 'search');
  $('inputs-explore').classList.toggle('hidden', st !== 'explore');
  $('inputs-book').classList.toggle('hidden', st !== 'detail' && st !== 'toc');
  $('inputs-chapter').classList.toggle('hidden', st !== 'content');
  renderBookView();
  renderChapterView();
  const hints = {
    search: '输入搜索词直接运行；结果在「可视界面」点选进入详情。',
    explore: '发现是独立入口：点分组（groups 树）+ 改筛选（filters）→ 运行。',
    detail: '输入来自搜索/发现的选中条目（var:book）；先跑前置阶段。',
    toc: '目录入口是 var:book.tocUrl（来自详情解析）。',
    content: '正文地址是 var:chapter.url（来自目录点选）。',
  };
  $('stage-hint').textContent = hints[st] || '';
  if (st === 'explore') renderExploreControls();
}

/** explore：groups 树 + filters 控件（V4-D9：数据来自源声明，值走 RunInputs） */
function renderExploreControls() {
  const treeEl = $('groups-tree');
  const filtersEl = $('filters-panel');
  treeEl.innerHTML = '';
  filtersEl.innerHTML = '';
  const doc = state.sourceDoc;
  const explore = doc && doc.stages && doc.stages.explore;
  if (!explore) {
    treeEl.appendChild(el('div', 'hint', '该源没有 stages.explore（发现）声明。'));
    return;
  }
  // groups 两级树（R§6.3：有二级 groups 的项只作分组标题不请求）
  const renderNodes = (nodes, container, level) => {
    for (const node of nodes || []) {
      const hasKids = Array.isArray(node.groups) && node.groups.length;
      const n = el('div', 'node' + (hasKids ? ' group-title' : ''));
      n.textContent = node.title || '（未命名）';
      const selected = state.group && node.title === state.group.title;
      if (!hasKids) {
        if (selected) n.classList.add('selected');
        n.onclick = () => {
          state.group = { title: node.title, params: node.params || {} };
          renderExploreControls();
          runStage();
        };
      }
      container.appendChild(n);
      if (hasKids) {
        const kids = el('div', 'children');
        renderNodes(node.groups, kids, level + 1);
        container.appendChild(kids);
      }
    }
  };
  renderNodes(explore.groups, treeEl, 0);
  if (!explore.groups || !explore.groups.length) {
    treeEl.appendChild(el('div', 'hint', '源未声明 groups（分类树）；仍可直接用 filters 运行。'));
  }
  // filters（数据选择声明：options/default/persist）
  const filters = explore.filters || [];
  if (!filters.length) {
    filtersEl.appendChild(el('div', 'hint', '源未声明 filters（筛选）。'));
    return;
  }
  for (const f of filters) {
    const wrap = el('div', 'filter');
    const label = el('label', undefined, `${f.label || f.key}（var:filter.${f.key}）`);
    wrap.appendChild(label);
    const sel = document.createElement('select');
    for (const op of f.options || []) {
      const value = typeof op === 'string' ? op : op.value;
      const text = typeof op === 'string' ? op : (op.label || op.value);
      const o = el('option', undefined, text);
      o.value = value;
      sel.appendChild(o);
    }
    const current = state.filter[f.key];
    if (current !== undefined) sel.value = current;
    else if (f.default !== undefined) { sel.value = f.default; state.filter[f.key] = f.default; }
    sel.onchange = () => {
      state.filter[f.key] = sel.value;
      runStage();
    };
    wrap.appendChild(sel);
    filtersEl.appendChild(wrap);
  }
}

// ───────────────────────── 源文本 + 实时（T5） ─────────────────────────

let debounceTimer = null;
function setSourceText(text, opts = {}) {
  state.sourceText = text;
  if (!opts.keepEditor) $('source-text').value = text;
  try { state.sourceDoc = JSON.parse(text); } catch (_) { state.sourceDoc = null; }
  $('label').textContent = (state.sourceDoc && state.sourceDoc.name) || '';
  if (opts.autoRun && state.live) scheduleLiveRerun();
}

/** 防抖 800ms 实时重跑（V4-D10：实时=编辑端职责；暂停开关防打站点） */
function scheduleLiveRerun() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    if (!state.live) return;
    // search 且未输入搜索词时不自动跑（避免每次敲键都打到内核/站点）
    if (state.stage === 'search' && !$('in-kw').value.trim() && !state.artifacts.search) return;
    if (state.artifacts[state.stage]) runStage({ silent: true });
    else runStage();
  }, 800);
}

function renderPause() {
  $('btn-pause').textContent = state.live ? '⏸ 实时：开' : '▶ 实时：停';
  $('btn-pause').title = state.live
    ? '点击暂停自动重跑（编辑不再触发请求，防止高频打站点）'
    : '已暂停：编辑不会触发请求；点此恢复';
}

// ───────────────────────── 空态 ─────────────────────────

function emptyState(title, sub) {
  const d = el('div', 'empty');
  d.appendChild(el('div', 'big', '🔍'));
  d.appendChild(el('div', undefined, title));
  if (sub) d.appendChild(el('div', 'small muted', sub));
  return d;
}

// ───────────────────────── 事件绑定 ─────────────────────────

function bind() {
  document.querySelectorAll('.stage-tab').forEach((b) => {
    b.onclick = () => switchStage(b.dataset.stage);
  });
  document.querySelectorAll('#tabs .tab').forEach((b) => {
    b.onclick = () => { state.tab = b.dataset.tab; renderTabs(state.artifacts[state.stage]); };
  });
  document.querySelectorAll('#raw-tools [data-rawmode]').forEach((b) => {
    b.onclick = () => { state.rawMode = b.dataset.rawmode; renderTabs(state.artifacts[state.stage]); };
  });
  $('btn-run').onclick = () => runStage();
  $('btn-pause').onclick = () => { state.live = !state.live; renderPause(); };
  $('btn-clear-book').onclick = () => {
    state.book = null;
    renderBookView();
    refreshChainList();
    switchStage('search');
  };
  $('btn-clear-chapter').onclick = () => {
    state.chapter = null;
    renderChapterView();
    refreshChainList();
    switchStage('toc');
  };
  $('source-text').addEventListener('input', (e) => {
    state.sourceText = e.target.value;
    try { state.sourceDoc = JSON.parse(state.sourceText); } catch (_) { state.sourceDoc = null; }
    $('label').textContent = (state.sourceDoc && state.sourceDoc.name) || '';
    scheduleLiveRerun();
  });
  // ── TASK-BOOK-5：视图/主题/源库/日志事件 ──
  $('vtab-sources').onclick = function () { showView('sources'); };
  $('vtab-debug').onclick = function () { showView('debug'); };
  $('btn-theme').onclick = function () { cycleTheme(); };
  $('btn-src-import').onclick = function () {
    const st = importSourcesText($('src-import-text').value);
    $('src-import-status').textContent = st.msg;
    if (st.ok) { $('src-import-text').value = ''; renderSources(); }
  };
  $('btn-src-file').onclick = function () { $('src-import-file').click(); };
  $('src-import-file').addEventListener('change', function (e) {
    const f = e.target.files[0];
    if (!f) return;
    const reader = new FileReader();
    reader.onload = function () {
      const st = importSourcesText(String(reader.result));
      $('src-import-status').textContent = st.msg;
      if (st.ok) renderSources();
    };
    reader.readAsText(f);
    e.target.value = '';
  });
  $('src-search').addEventListener('input', function () { renderSources(); });
  $('btn-src-export').onclick = function () { exportSelected(); };
  $('btn-src-delete').onclick = function () { deleteSelected(); };
  document.querySelectorAll('.log-chip').forEach(function (c) {
    c.onclick = function () { logFilter = c.dataset.logcat; renderLogs(); };
  });
  $('btn-log-copy').onclick = function () {
    navigator.clipboard.writeText(logTextAll());
    $('btn-log-copy').textContent = '✓ 已复制';
    setTimeout(function () { $('btn-log-copy').textContent = '⧉ 复制全部'; }, 1200);
  };
  $('btn-log-export').onclick = function () { exportLogs(); };

  // T6：VS Code 扩展宿主推送编辑缓冲（didChange 转发）
  window.addEventListener('message', (e) => {
    const d = e.data;
    if (d && d.type === 'rsl-source' && typeof d.text === 'string') {
      setSourceText(d.text, { keepEditor: true, autoRun: true });
    }
  });
}

async function init() {
  bind();
  renderPause();
  renderInputs();
  refreshChainList();
  document.querySelectorAll('.stage-tab').forEach((b) => b.classList.toggle('active', b.dataset.stage === 'search'));
  applyTheme();
  if (window.matchMedia) {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  }
  try {
    const h = await api('/health');
    if (h.ok) {
      $('conn-dot').className = 'dot dot-on';
      $('conn-text').textContent = '已连接';
    }
  } catch (_) {
    $('conn-text').textContent = '连接失败（rsl serve 未运行？）';
  }
}

init();

// ═══════════════ TASK-BOOK-5：主题三层 / 视图切换 / 源库 / 日志 ═══════════════

// ── T5 主题：自动(跟随系统)/浅/深 循环 + localStorage 记忆 ──
const THEME_KEY = 'rsl.theme';
function applyTheme() {
  const mode = localStorage.getItem(THEME_KEY) || 'auto';
  let dark;
  if (mode === 'dark') dark = true;
  else if (mode === 'light') dark = false;
  else dark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
  const label = { auto: '自动', light: '浅色', dark: '深色' }[mode];
  const btn = $('btn-theme');
  if (btn) btn.textContent = '🌓 主题：' + label;
}
function cycleTheme() {
  const order = ['auto', 'light', 'dark'];
  const cur = localStorage.getItem(THEME_KEY) || 'auto';
  const next = order[(order.indexOf(cur) + 1) % order.length];
  localStorage.setItem(THEME_KEY, next);
  applyTheme();
}

// ── 视图切换（阅读单页骨架：源库 ↔ 调试器） ──
function showView(name) {
  $('view-sources').classList.toggle('hidden', name !== 'sources');
  $('view-debug').classList.toggle('hidden', name !== 'debug');
  $('vtab-sources').classList.toggle('active', name === 'sources');
  $('vtab-debug').classList.toggle('active', name === 'debug');
  if (name === 'sources') renderSources();
}

// ── T4 源库（localStorage；规则是数据——真源文件仍归用户文件系统） ──
const SRC_KEY = 'rsl.sources';
const SRC_USED_KEY = 'rsl.sources.used';

function lsSources() {
  try { return JSON.parse(localStorage.getItem(SRC_KEY) || '{}'); } catch (_) { return {}; }
}
function saveSources(map) {
  localStorage.setItem(SRC_KEY, JSON.stringify(map));
}
function touchUsed(id) {
  const used = JSON.parse(localStorage.getItem(SRC_USED_KEY) || '{}');
  used[id] = Date.now();
  localStorage.setItem(SRC_USED_KEY, JSON.stringify(used));
}
function usedAt(id) {
  const used = JSON.parse(localStorage.getItem(SRC_USED_KEY) || '{}');
  return used[id] || 0;
}

/** 导入解析：单对象或数组；最小校验（rsl/id），诊断细节交给调试器编译。 */
function importSourcesText(text) {
  let doc;
  try { doc = JSON.parse(text); } catch (e) { return { ok: false, msg: 'JSON 解析失败：' + e.message }; }
  const arr = Array.isArray(doc) ? doc : [doc];
  if (!arr.length) return { ok: false, msg: '空数组' };
  const map = lsSources();
  let added = 0;
  let updated = 0;
  for (const s of arr) {
    if (!s || typeof s !== 'object') return { ok: false, msg: '存在非对象条目' };
    if (!s.rsl || !s.id) return { ok: false, msg: '缺少 rsl / id 字段（不是 RSL 源？）' };
    if (map[s.id]) updated++; else added++;
    map[s.id] = { id: s.id, name: s.name || s.id, group: (s.meta && s.meta.group) || '', url: s.baseUrl || '', ts: Date.now(), enabled: true, json: s };
  }
  saveSources(map);
  return { ok: true, msg: '导入完成：新增 ' + added + '、更新 ' + updated };
}

function srcMatches(s, q) {
  if (!q) return true;
  if (q === '已启用') return s.enabled !== false;
  if (q === '已禁用') return s.enabled === false;
  if (q === '无分组') return !s.group;
  if (q.indexOf('group:') === 0) return (s.group || '').indexOf(q.slice(6)) >= 0;
  return (s.name + ' ' + s.url + ' ' + s.group).toLowerCase().indexOf(q.toLowerCase()) >= 0;
}

function renderSources() {
  const list = $('src-list');
  list.innerHTML = '';
  const q = ($('src-search').value || '').trim();
  const all = Object.values(lsSources()).sort(function (a, b) { return usedAt(b.id) - usedAt(a.id); });
  const shown = all.filter(function (s) { return srcMatches(s, q); });
  $('src-count').textContent = shown.length + ' / ' + all.length + ' 个源';
  if (!shown.length) {
    list.appendChild(el('li', 'src-empty', all.length ? '没有匹配的源（试试清除筛选）' : '还没有源——先在上方导入'));
    return;
  }
  for (const s of shown) {
    const li = el('li', 'src-item');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.className = 'src-check';
    cb.dataset.id = s.id;
    li.appendChild(cb);
    const meta = el('div', 'meta');
    meta.appendChild(el('div', 'nm', s.name || s.id));
    if (s.url) meta.appendChild(el('div', 'url', s.url));
    if (s.group) meta.appendChild(el('div', 'grp', '分组：' + s.group));
    li.appendChild(meta);
    const ops = el('div', 'ops');
    const bDebug = el('button', 'btn small primary', '▶ 调试');
    bDebug.title = '载入调试器（编辑缓冲即最新规则，免保存）';
    bDebug.onclick = function () { openSourceInDebug(s); };
    const bEdit = el('button', 'btn small', '✎ 编辑');
    bEdit.title = '载入左侧源文本区';
    bEdit.onclick = function () { openSourceInDebug(s, { editOnly: true }); };
    const bExp = el('button', 'btn small', '⇩ 导出');
    bExp.onclick = function () { exportOne(s); };
    const bDel = el('button', 'btn small', '✕');
    bDel.title = '从源库删除（不动你的源文件）';
    bDel.onclick = function () {
      const m = lsSources();
      delete m[s.id];
      saveSources(m);
      renderSources();
    };
    ops.appendChild(bDebug);
    ops.appendChild(bEdit);
    ops.appendChild(bExp);
    ops.appendChild(bDel);
    li.appendChild(ops);
    list.appendChild(li);
  }
}

function openSourceInDebug(s, opts) {
  opts = opts || {};
  touchUsed(s.id);
  const text = JSON.stringify(s.json, null, 2);
  state.sourceText = text;
  $('source-text').value = text;
  try { state.sourceDoc = JSON.parse(text); } catch (_) { state.sourceDoc = null; }
  $('label').textContent = (state.sourceDoc && state.sourceDoc.name) || '';
  showView('debug');
  refreshChainList();
  if (!opts.editOnly) runStage();
}

function download(name, content) {
  const blob = new Blob([content], { type: 'application/json' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

function exportOne(s) {
  download(s.id.replace(/[^\w.-]/g, '_') + '.rsl.json', JSON.stringify(s.json, null, 4));
}

function exportSelected() {
  const ids = Array.from(document.querySelectorAll('.src-check:checked')).map(function (c) { return c.dataset.id; });
  const all = lsSources();
  const chosen = (ids.length ? ids : Object.keys(all)).map(function (id) { return all[id] ? all[id].json : null; }).filter(Boolean);
  if (!chosen.length) return;
  const stamp = new Date().toISOString().slice(0, 16).replace(/[-:T]/g, '');
  download('RslSources_' + stamp + '.json', JSON.stringify(chosen, null, 4));
}

function deleteSelected() {
  const ids = Array.from(document.querySelectorAll('.src-check:checked')).map(function (c) { return c.dataset.id; });
  if (!ids.length) return;
  const map = lsSources();
  for (const id of ids) delete map[id];
  saveSources(map);
  renderSources();
}

// ── T3 调试日志：产物 → 分类行流（对齐阅读 [mm:ss]+符号 协议语义；官方无复制按钮，RSL 补齐） ──
let logFilter = 'all';
let logCache = [];

function buildLogLines() {
  const stages = ['search', 'explore', 'detail', 'toc', 'content'].filter(function (s) { return state.artifacts[s]; });
  const lines = [];
  let n = 0;
  for (const st of stages) {
    const a = state.artifacts[st];
    const base = n;
    n += a.elapsed_ms || 0;
    lines.push({ cat: 'stage', t: base, text: '︾ ' + (STAGE_LABELS[st] || st) + ' 阶段开始（' + (a.ok ? '成功' : '失败') + '，共 ' + a.elapsed_ms + 'ms）' });
    const reqs = a.requests || [];
    reqs.forEach(function (rec, i) {
      const r = rec.request || {};
      lines.push({ cat: 'req', t: base + (rec.response ? rec.response.ms : 0), text: '⇒ 请求#' + (i + 1) + ' ' + (r.method || 'GET') + ' ' + (r.url || '') + (r.attempts > 1 ? '（重试 ' + r.attempts + ' 次）' : '') });
      const resp = rec.response;
      if (resp) {
        lines.push({ cat: 'resp', t: base + resp.ms, text: '≡ 响应 ' + resp.status + ' · ' + fmtBytes(resp.body_bytes) + ' · ' + resp.ms + 'ms' + (resp.truncated ? ' · 预览截断' : '') + (resp.final_url && resp.final_url !== r.url ? ' · 重定向→' + resp.final_url : '') });
      } else {
        lines.push({ cat: 'err', t: base, text: '✖ 无响应（网络层失败或请求未发出）' });
      }
    });
    for (const tr of a.trace || []) {
      if (tr.step === 'ctx.log') lines.push({ cat: 'script', t: base + (tr.ms || 0), text: '◇ [脚本] ' + tr.rule });
      else lines.push({ cat: 'parse', t: base + (tr.ms || 0), text: '┌ ' + tr.rule + ' → 命中 ' + tr.matched + (tr.ms ? ' · ' + tr.ms + 'ms' : '') });
    }
    for (const d of a.diagnostics || []) {
      lines.push({ cat: 'diag', t: base, text: '⚠ [' + d.code + '] ' + d.message + '（' + (d.path || '') + '）' });
    }
    if (!a.ok && a.error) lines.push({ cat: 'err', t: base, text: '✖ [' + a.error.code + '] ' + a.error.message });
    lines.push({ cat: 'stage', t: n, text: '︽ ' + (STAGE_LABELS[st] || st) + ' 阶段' + (a.ok ? '完成' : '失败') });
  }
  return lines;
}

function fmtLogTime(ms) {
  const s = Math.floor(ms / 1000);
  const frac = ms % 1000;
  return '[' + String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0') + '.' + String(frac).padStart(3, '0') + ']';
}

function renderLogs() {
  logCache = buildLogLines();
  const body = $('tab-body');
  body.innerHTML = '';
  const tools = $('log-tools');
  tools.querySelectorAll('.log-chip').forEach(function (c) { c.classList.toggle('active', c.dataset.logcat === logFilter); });
  const shown = logFilter === 'all' ? logCache : logCache.filter(function (l) { return l.cat === logFilter; });
  const wrap = el('div', 'log-list');
  for (const l of shown) {
    const row = el('div', 'log-line ' + l.cat);
    row.appendChild(el('span', 't', fmtLogTime(l.t)));
    row.appendChild(el('span', 'm', l.text));
    const cp = el('button', 'log-copy', '⧉ 复制');
    cp.onclick = function () {
      navigator.clipboard.writeText(fmtLogTime(l.t) + ' ' + l.text);
      cp.textContent = '✓';
      setTimeout(function () { cp.textContent = '⧉ 复制'; }, 900);
    };
    row.appendChild(cp);
    wrap.appendChild(row);
  }
  if (!shown.length) wrap.appendChild(emptyState('暂无日志', '运行一个阶段后，这里会有带时间戳的完整调试过程'));
  body.appendChild(wrap);
}

function logTextAll() {
  return logCache.map(function (l) { return fmtLogTime(l.t) + ' ' + l.text; }).join('\n');
}

function exportLogs() {
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
  download('debug-log-' + stamp + '.txt', logTextAll());
}
