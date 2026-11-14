// RSL 表单编辑器纯模型（TASK-BOOK-3 H2 M1/M2）。
// 无 VS Code / Node 依赖：host（form-panel.js）与 WebView（media/form.js）共用，
// node 单测直接 require。核心不变量：
//   1. apply 只写"建模字段"，未建模内容（x- 字段、media、explore、复杂块）原样保留；
//   2. extract → apply 对同一文档往返幂等；
//   3. 选择器 = 命名空间 + 表达式（裸命名空间无冒号）；transform 步骤 = {key, args}。
'use strict';

// 18 命名空间（与 rsl-core sugar::NAMESPACES / LSP NS_DOCS 同步；改动需三处一致）
const NAMESPACES = ['css', 'xpath', 'json', 're', 'text', 'owntext', 'html', 'tag',
  'attr', 'const', 'var', 'let', 'session', 'token', 'cookie', 'cache', 'lib', 'expr'];
const BARE_NS = ['text', 'owntext', 'html', 'tag'];
// 31 管道步骤键（与 rsl-core / LSP STEP_KEYS 同步）
const STEP_KEYS = ['trim', 'replace', 'regex', 'match', 'prepend', 'append', 'absolute',
  'urlencode', 'urldecode', 'base64', 'hex', 'html', 'json', 'date', 'number',
  'numChapter', 'cjkNumber', 't2s', 's2t', 'slice', 'join', 'split', 'zip', 'protect',
  'font', 'unique', 'sort', 'dropEmpty', 'js', 'parseTime', 'decode'];

// ───────────────────────── 选择器 ─────────────────────────

/** "css:a.title" → {ns:'css', expr:'a.title'}；"text" → {ns:'text', expr:''}；空 → null。 */
function parseSelector(v) {
  if (typeof v !== 'string' || v.trim() === '') return null;
  const s = v.trim();
  if (BARE_NS.includes(s)) return { ns: s, expr: '' };
  const i = s.indexOf(':');
  if (i > 0 && NAMESPACES.includes(s.slice(0, i))) {
    return { ns: s.slice(0, i), expr: s.slice(i + 1) };
  }
  // 未知前缀/裸字符串：按无命名空间处理，原样进表达式（保留信息不丢）
  return { ns: '', expr: s };
}

function serializeSelector(sel) {
  if (!sel) return null;
  const expr = (sel.expr || '').trim();
  if (!expr && !BARE_NS.includes(sel.ns)) return null;
  if (BARE_NS.includes(sel.ns)) return sel.ns;
  if (!sel.ns) return expr;
  return `${sel.ns}:${expr}`;
}

// ───────────────────────── 管道步骤 ─────────────────────────

/** 文档形态 [ {"trim":true}, {"replace":{...}} ] → [{key,args}] */
function parseSteps(list) {
  if (!Array.isArray(list)) return [];
  const out = [];
  for (const it of list) {
    if (typeof it === 'string') { out.push({ key: it, args: null }); continue; }
    if (it && typeof it === 'object') {
      const key = Object.keys(it)[0];
      if (key) out.push({ key, args: it[key] === true ? null : it[key] });
    }
  }
  return out;
}

function serializeSteps(steps) {
  const out = [];
  for (const st of steps || []) {
    if (!st.key) continue;
    out.push(st.args === null || st.args === undefined || st.args === '' || st.args === true
      ? { [st.key]: true }
      : { [st.key]: st.args });
  }
  return out;
}

// ───────────────────────── 配置派生（schema → 表单配置） ─────────────────────────

const WIDGET = { string: 'text', integer: 'number', number: 'number', boolean: 'checkbox',
  array: 'list', object: 'group' };

function resolveRef(schema, ref) {
  if (typeof ref !== 'string' || !ref.startsWith('#/')) return undefined;
  let cur = schema;
  for (const part of ref.slice(2).split('/')) {
    cur = cur && cur[part];
  }
  return cur;
}

function widgetFor(prop, schema) {
  let p = prop;
  const seen = new Set();
  while (p && p.$ref && !seen.has(p.$ref)) {
    seen.add(p.$ref);
    p = resolveRef(schema, p.$ref) || {};
  }
  if (!p || typeof p !== 'object') return 'text';
  if (p.enum) return 'select';
  const t = Array.isArray(p.type) ? p.type[0] : p.type;
  if (t === 'string' && p.format === 'uri') return 'url';
  return WIDGET[t] || 'text';
}

function fieldEntry(name, prop, schema, required) {
  const raw = prop && prop.$ref ? resolveRef(schema, prop.$ref) : prop;
  const target = raw && typeof raw === 'object' ? raw : {};
  const entry = {
    field: name,
    widget: widgetFor(prop, schema),
    required: Boolean(required),
    description: (prop && prop.description) || '',
  };
  if (prop && prop.enum) entry.options = prop.enum;
  if (prop && prop.$ref) entry.$def = prop.$ref.split('/')[2];
  const sub = target.properties;
  if (sub && typeof sub === 'object' && Object.keys(sub).length) {
    entry.fields = Object.entries(sub).map(([k, v]) =>
      fieldEntry(k, v, schema, (target.required || []).includes(k)));
  }
  return entry;
}

/** schema → 表单配置（嵌套一层展开 + 命名空间/步骤键常量）。 */
function deriveFormConfig(schema) {
  const defs = schema.$defs || {};
  return {
    schema_version: schema.$id || '2.0',
    namespaces: NAMESPACES,
    bare_ns: BARE_NS,
    step_keys: STEP_KEYS,
    fields: Object.entries(schema.properties || {})
      .map(([name, prop]) => fieldEntry(name, prop, schema, (schema.required || []).includes(name))),
  };
}

// ───────────────────────── 建模路径（表单投影范围 S） ─────────────────────────

const SEARCH_MAP_FIELDS = ['name', 'url', 'author', 'kind', 'cover', 'wordCount', 'lastChapter', 'updateTime'];
const DETAIL_OUT_FIELDS = ['name', 'author', 'intro', 'kind', 'cover', 'lastChapter', 'tocUrl', 'wordCount', 'updateTime'];
const TOC_MAP_FIELDS = ['title', 'url', 'isVolume', 'isPay', 'isVip'];
const SPLIT_MODES = ['br', 'block', 'auto', 'line'];

// 新手文案：每个建模字段的中文标签（zh）、一句话说明（help）、示例占位符（ph）。
// 门禁：tests/form-model-test.js 断言全部路径都有 zh/help——文案缺失即红。
const ZH = {
  'id': { zh: '源 ID', help: '全局唯一标识，建议用反向域名（把 example.com 换成你的站点域名）', ph: 'mysite.example.com' },
  'name': { zh: '源名称', help: '书源列表里显示的名字', ph: '我的书源' },
  'type': { zh: '源类型', help: 'novel = 小说 / comic = 漫画 / video = 视频' },
  'baseUrl': { zh: '站点首页', help: '站点根地址；规则里的相对链接会基于它补全', ph: 'https://www.mysite.example.com' },
  'lang': { zh: '语言', help: '如 zh-cn；可留空', ph: 'zh-cn' },
  'version': { zh: '版本', help: '源的版本号；可留空', ph: '1.0' },
  'requires.features': { zh: '能力开关', help: '用到才勾选：js=脚本、webview=浏览器渲染、font=字体反爬、tls=指纹、detect=检测、captcha=验证码。宿主按声明决定能否运行' },
  'requires.crypto': { zh: '加密算法', help: '规则里用到的加解密算法，勾选声明' },
  'vars': { zh: '变量表', help: '源内可复用的变量；规则里用 ${var:名字} 取值（如搜索词 kw）' },
  'http.headers': { zh: '请求头', help: '发给站点的 HTTP 头；常配 User-Agent 伪装浏览器' },
  'http.timeout': { zh: '请求超时(ms)', help: '单个请求的最长等待毫秒数；不填用宿主默认', ph: '15000' },
  'stages.search.request.url': { zh: '搜索接口地址', help: '搜索页/搜索接口；模板 ${var:kw} 会被替换成搜索词', ph: 'https://www.mysite.example.com/search?q=${var:kw}' },
  'stages.detail.request.url': { zh: '详情接口地址', help: '书籍详情页；${var:book.url} 是 search 里解析出的书籍链接', ph: 'var:book.url' },
  'stages.toc.request.url': { zh: '目录接口地址', help: '章节目录页；${var:book.tocUrl} 来自 detail 解析', ph: 'var:book.tocUrl' },
  'stages.content.request.url': { zh: '正文接口地址', help: '章节正文页；${var:chapter.url} 来自 toc 解析', ph: 'var:chapter.url' },
  'stages.search.out.list': { zh: '列表选择器', help: '在搜索结果里选出"每一本书"的重复元素（列表项）', ph: 'ul.list li' },
  'stages.search.out.map.name': { zh: '书名', help: '列表项里的书名元素', ph: 'a.title' },
  'stages.search.out.map.url': { zh: '详情链接（必填）', help: '列表项里的详情页链接；配 absolute 步骤把相对地址补全' },
  'stages.search.out.map.author': { zh: '作者', help: '列表项里的作者；没有就点"移除"', ph: 'span.author' },
  'stages.search.out.map.kind': { zh: '分类', help: '如"玄幻""都市"', ph: 'span.kind' },
  'stages.search.out.map.cover': { zh: '封面图', help: '封面图片地址', ph: 'img' },
  'stages.search.out.map.wordCount': { zh: '字数', help: '总字数（可用 number 步骤取数值）', ph: 'span.words' },
  'stages.search.out.map.lastChapter': { zh: '最新章节', help: '最新一章标题', ph: 'span.latest' },
  'stages.search.out.map.updateTime': { zh: '更新时间', help: '最近更新时间', ph: 'span.time' },
  'stages.detail.out.name': { zh: '书名', help: '详情页的书名元素', ph: 'h1' },
  'stages.detail.out.author': { zh: '作者', help: '详情页的作者元素', ph: 'span.author' },
  'stages.detail.out.intro': { zh: '简介', help: '书籍简介所在元素', ph: 'div#intro' },
  'stages.detail.out.kind': { zh: '分类', help: '分类标签', ph: 'span.kind' },
  'stages.detail.out.cover': { zh: '封面图', help: '封面图片', ph: 'img.cover' },
  'stages.detail.out.lastChapter': { zh: '最新章节', help: '最新一章', ph: 'a.last' },
  'stages.detail.out.tocUrl': { zh: '目录页链接', help: '进入章节目录的入口；没有目录页就配成和详情页一样', ph: 'a.read' },
  'stages.detail.out.wordCount': { zh: '字数', help: '总字数', ph: 'span.words' },
  'stages.detail.out.updateTime': { zh: '更新时间', help: '最近更新时间', ph: 'span.time' },
  'stages.toc.out.list': { zh: '列表选择器', help: '在目录页里选出"每一章"的重复元素', ph: '#list li' },
  'stages.toc.out.map.title': { zh: '章节标题', help: '章节名所在元素', ph: 'a' },
  'stages.toc.out.map.url': { zh: '章节链接（必填）', help: '章节正文页链接', ph: 'a' },
  'stages.toc.out.map.isVolume': { zh: '是否分卷', help: '该行是卷名而非章节时命中', ph: 'span.vol' },
  'stages.toc.out.map.isPay': { zh: '是否付费', help: '付费章节标记', ph: 'i.pay' },
  'stages.toc.out.map.isVip': { zh: '是否 VIP', help: 'VIP 章节标记', ph: 'i.vip' },
  'stages.content.out.content': { zh: '正文元素', help: '章节正文所在的元素；split 决定怎么切成段落', ph: 'div#content' },
  'stages.search.paginate.next': { zh: '下一页链接', help: '列表页"下一页"元素；翻到没有就自动停', ph: 'a.next' },
  'stages.search.paginate.max': { zh: '最大页数', help: '翻页上限（防止打爆站点）', ph: '10' },
  'login.type': { zh: '登录方式', help: 'form=表单登录 / webview=打开网页登录 / qr=扫码 / token=令牌 / cookie=Cookie' },
  'login.webview.entry': { zh: '登录页地址', help: '宿主打开这个网页让用户手动登录', ph: 'https://www.mysite.example.com/login' },
  'login.webview.success': { zh: '登录成功判定', help: '登录成功后才出现的元素（如头像）；宿主据此确认', ph: 'div.user-avatar' },
  'login.relogin.max': { zh: '自动重登次数', help: '登录失效后自动重试次数（上限 2；0 = 不自动重登）', ph: '1' },
  'login.check.request.url': { zh: '登录检测地址', help: '请求它来判断登录态是否还有效', ph: 'https://www.mysite.example.com/user/info' },
  'login.check.ok': { zh: '已登录判定', help: '已登录时才存在的字段（json 选择器最常用）', ph: '$.user.id' },
  'login.fields': { zh: '登录字段', help: '登录表单需要的输入项（key 必填；secret=密码类脱敏）' },
  'login.submit.request.url': { zh: '登录提交地址', help: '表单提交的接口地址', ph: 'https://www.mysite.example.com/login' },
  'login.submit.ok': { zh: '提交成功判定', help: '登录提交成功后出现的字段', ph: '$.success' },
  'lib.pure': { zh: '纯函数脚本', help: '无副作用的辅助函数（清洗/格式化）；管道里用 { "js": "lib:函数名" } 调用' },
  'lib.ctx': { zh: '上下文脚本', help: '需要 ctx 能力（加解密/请求）的函数；谨慎使用' },
};

/** 表单建模的全部叶子（UI 渲染与 extract/apply 共用同一张表）。
 *  新手文案（zh/help/ph）由 ZH 表提供，缺失即抛错——文案完整性有门禁锁定。 */
function modeledPaths() {
  const paths = [];
  const mk = (path, kind, extra = {}) => {
    const z = ZH[path];
    if (!z || !z.zh) {
      throw new Error(`新手文案缺失：${path}（ZH 表必须全覆盖，新手第一次要看懂）`);
    }
    return { path, kind, label: path, zh: z.zh, help: z.help || '', ph: z.ph || '', ...extra };
  };
  for (const k of ['id', 'name', 'type', 'baseUrl', 'lang', 'version']) {
    paths.push(mk(k, k === 'type' ? 'enum:novel,comic,video' : 'text'));
  }
  paths.push(mk('requires.features', 'multi', { options: ['js', 'webview', 'font', 'tls', 'detect', 'captcha'] }));
  paths.push(mk('requires.crypto', 'multi', { options: ['md5', 'sha1', 'sha256', 'sha512', 'aes', 'des'] }));
  paths.push(mk('vars', 'kv'));
  paths.push(mk('http.headers', 'kv'));
  paths.push(mk('http.timeout', 'number'));

  for (const st of ['search', 'detail', 'toc', 'content']) {
    paths.push(mk(`stages.${st}.request.url`, 'text'));
  }
  paths.push(mk('stages.search.out.list', 'selector'));
  paths.push(mk('stages.toc.out.list', 'selector'));
  for (const f of SEARCH_MAP_FIELDS) {
    paths.push(mk(`stages.search.out.map.${f}`, 'rule',
      f === 'url' ? { requiredField: true } : {}));
  }
  for (const f of DETAIL_OUT_FIELDS) {
    paths.push(mk(`stages.detail.out.${f}`, 'rule',
      f === 'tocUrl' ? { requiredField: true } : {}));
  }
  for (const f of TOC_MAP_FIELDS) {
    paths.push(mk(`stages.toc.out.map.${f}`, 'rule',
      f === 'url' ? { requiredField: true } : {}));
  }
  paths.push(mk('stages.content.out.content', 'rule', { splitModes: SPLIT_MODES }));
  paths.push(mk('stages.search.paginate.next', 'selector'));
  paths.push(mk('stages.search.paginate.max', 'number'));
  paths.push(mk('login.type', 'enum:form,cookie,token,qr,webview'));
  paths.push(mk('login.webview.entry', 'text'));
  paths.push(mk('login.webview.success', 'selector'));
  paths.push(mk('login.relogin.max', 'number'));
  paths.push(mk('login.check.request.url', 'text'));
  paths.push(mk('login.check.ok', 'selector'));
  paths.push(mk('login.fields', 'kvLoginFields'));
  paths.push(mk('login.submit.request.url', 'text'));
  paths.push(mk('login.submit.ok', 'selector'));
  paths.push(mk('lib.pure', 'code'));
  paths.push(mk('lib.ctx', 'code'));
  return paths;
}

/** 分区标题与新手说明（UI 按此渲染步骤化引导）。 */
const GROUPS = [
  { title: '第 1 步 · 元信息（这个源是什么）', match: (p) => ['id', 'name', 'type', 'baseUrl', 'lang', 'version'].includes(p.path),
    desc: '必填：源 ID、名称、类型、站点首页。其他可留空。' },
  { title: '第 2 步 · 能力声明（用到什么才勾什么）', match: (p) => p.path.startsWith('requires.'),
    desc: '没勾的能力宿主可以拒绝运行——这正是"跨端一致"的保证。' },
  { title: '第 3 步 · 变量与请求配置（可以先跳过）', match: (p) => p.path === 'vars' || p.path.startsWith('http.'),
    desc: 'vars 里放搜索词等变量；http.headers 常配 User-Agent。' },
  { title: '第 4 步 · search 搜索（四阶段契约）', match: (p) => p.path.startsWith('stages.search.'),
    desc: '目标：给出 [书名, 详情链接] 列表。核心是 列表 + 书名 + 详情链接 三项，其他可选。' },
  { title: '第 5 步 · detail 详情（四阶段契约）', match: (p) => p.path.startsWith('stages.detail.'),
    desc: '目标：书籍信息 + 目录入口 tocUrl（点进目录的链接）。' },
  { title: '第 6 步 · toc 目录（四阶段契约）', match: (p) => p.path.startsWith('stages.toc.'),
    desc: '目标：[章节标题, 章节链接] 列表。' },
  { title: '第 7 步 · content 正文（四阶段契约）', match: (p) => p.path.startsWith('stages.content.'),
    desc: '目标：章节正文。split=br 表示按 <br> 分段。' },
  { title: '可选 · 登录（站点要登录才配）', match: (p) => p.path.startsWith('login.'),
    desc: '表单登录最常用；网页登录（webview）由宿主打开页面让用户手动操作。' },
  { title: '可选 · lib 脚本（加解密/清洗函数）', match: (p) => p.path.startsWith('lib.'),
    desc: '纯函数放 pure；需要加密等能力的放 ctx（记得勾能力开关里的 js）。' },
];

// ───────────────────────── 规则字段（selector + optional/default/transform/split） ─────────────────────────

const RULE_MODELED_KEYS = ['$', 'optional', 'default', 'transform', 'paragraphs'];

/** map 字段（字符串或对象）→ {selector, optional, default, transform, split, extras} | null
 *  extras = 未建模键（attr/css 前缀参数等）——apply 时原样并回，保证不丢数据。 */
function parseRule(v) {
  if (v === undefined || v === null) return null;
  if (typeof v === 'string') {
    const sel = parseSelector(v);
    return sel ? { selector: sel, optional: false, default: null, transform: [], split: null, extras: {} } : null;
  }
  if (typeof v === 'object') {
    const sel = parseSelector(v.$);
    const transform = parseSteps(v.transform);
    if (!sel && transform.length === 0) return null;
    const extras = {};
    for (const [k, val] of Object.entries(v)) {
      if (!RULE_MODELED_KEYS.includes(k)) extras[k] = cloneJson(val);
    }
    return {
      selector: sel || { ns: '', expr: '' },
      optional: Boolean(v.optional),
      default: typeof v.default === 'string' ? v.default : null,
      transform,
      split: v.paragraphs && typeof v.paragraphs === 'object' ? (v.paragraphs.split || null) : null,
      extras,
    };
  }
  return null;
}

function serializeRule(rule) {
  if (!rule) return null;
  const selStr = serializeSelector(rule.selector);
  const steps = serializeSteps(rule.transform);
  if (!selStr && steps.length === 0) return null;
  const hasMeta = rule.optional || (rule.default !== null && rule.default !== undefined)
    || steps.length > 0 || rule.split
    || (rule.extras && Object.keys(rule.extras).length > 0);
  if (!hasMeta) return selStr;
  const obj = {};
  if (selStr) obj.$ = selStr;
  for (const [k, val] of Object.entries(rule.extras || {})) { obj[k] = cloneJson(val); }
  if (steps.length) obj.transform = steps;
  if (rule.optional) obj.optional = true;
  if (rule.default !== null && rule.default !== undefined) obj.default = rule.default;
  if (rule.split) obj.paragraphs = { split: rule.split };
  return obj;
}

function kvPairs(v) {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return [];
  return Object.entries(v)
    .filter(([, val]) => typeof val === 'string')
    .map(([k, val]) => ({ k, v: val }));
}

function getPath(doc, path) {
  let cur = doc;
  for (const seg of path.split('.')) {
    if (cur === null || cur === undefined || typeof cur !== 'object') return undefined;
    cur = cur[seg];
  }
  return cur;
}

// ───────────────────────── extract：文档 → 表单状态 ─────────────────────────

/** 文档 → 表单状态：{ path: {value, present}, … }（未建模内容不进入状态）。 */
function extractFormState(doc) {
  const state = {};
  if (!doc || typeof doc !== 'object') return state;
  for (const m of modeledPaths()) {
    const raw = getPath(doc, m.path);
    const present = raw !== undefined;
    let value = null;
    switch (true) {
      case m.kind === 'kv':
        value = kvPairs(raw);
        break;
      case m.kind === 'kvLoginFields':
        value = Array.isArray(raw)
          ? raw.filter((f) => f && typeof f.key === 'string')
            .map((f) => ({ key: f.key, label: f.label || '', secret: Boolean(f.secret), def: typeof f.default === 'string' ? f.default : '' }))
          : [];
        break;
      case m.kind === 'multi':
        value = present && Array.isArray(raw) ? raw.filter((x) => typeof x === 'string') : [];
        break;
      case m.kind === 'selector':
        value = present ? parseSelector(raw) : null;
        break;
      case m.kind === 'rule':
        value = present ? parseRule(raw) : null;
        break;
      case m.kind.startsWith('enum:'):
        value = present && typeof raw === 'string' && m.kind.slice(5).split(',').includes(raw) ? raw : null;
        break;
      case m.kind === 'number':
        value = present && typeof raw === 'number' ? raw : null;
        break;
      default: // text / code
        value = present && typeof raw === 'string' ? raw : null;
    }
    state[m.path] = { value, present };
  }
  return state;
}

// ───────────────────────── apply：表单状态 → 文档（保留未建模） ─────────────────────────

function cloneJson(v) {
  return v === undefined ? undefined : JSON.parse(JSON.stringify(v));
}

function setPath(doc, path, value) {
  const segs = path.split('.');
  let cur = doc;
  if (value === undefined) {
    // 删除：不创建任何父容器
    for (let i = 0; i < segs.length - 1; i++) {
      cur = cur ? cur[segs[i]] : undefined;
      if (cur === null || typeof cur !== 'object') return doc;
    }
    if (cur && typeof cur === 'object') delete cur[segs[segs.length - 1]];
    return doc;
  }
  for (let i = 0; i < segs.length - 1; i++) {
    if (typeof cur[segs[i]] !== 'object' || cur[segs[i]] === null || Array.isArray(cur[segs[i]])) {
      cur[segs[i]] = {};
    }
    cur = cur[segs[i]];
  }
  cur[segs[segs.length - 1]] = value;
  return doc;
}

/** 把表单状态合并进文档（返回新文档对象）。语义：
 *  - value=null / '' → 删除该建模叶子（显式清空才删；state 里没出现的路径不动）
 *  - 有值 → set（伴随容器按需创建）
 *  - 未建模键一律不动；本方触碰过的空容器回收。 */
function applyFormState(doc, state) {
  const out = cloneJson(doc) || {};
  const touched = new Set();
  for (const [path, entry] of Object.entries(state)) {
    if (!entry || entry.value === undefined) continue;
    const segs = path.split('.');
    for (let i = 1; i < segs.length; i++) touched.add(segs.slice(0, i).join('.'));
    const v = entry.value;
    if (v === null || v === '') { setPath(out, path, undefined); continue; }
    if (path === 'vars' || path === 'http.headers') {
      const obj = {};
      for (const p of v) { if (p.k) obj[p.k] = p.v; }
      const keys = Object.keys(obj).length;
      if (keys) setPath(out, path, obj);
      else if (entry.present) setPath(out, path, undefined); // 原有但被清空 → 删；本就无 → 不造
      continue;
    }
    if (path === 'requires.features' || path === 'requires.crypto') {
      // 多选空数组 = 显式声明"不用该能力"（按需付费公理），永远写入不删
      setPath(out, path, v.filter(Boolean));
      continue;
    }
    if (path === 'login.fields') {
      const arr = v.filter((f) => f.key).map((f) => {
        const o = { key: f.key };
        if (f.label) o.label = f.label;
        if (f.secret) o.secret = true;
        if (f.def) o.default = f.def;
        return o;
      });
      if (arr.length) setPath(out, path, arr);
      else if (entry.present) setPath(out, path, undefined);
      continue;
    }
    if (v && typeof v === 'object' && 'selector' in v) { // rule 字段
      setPath(out, path, serializeRule(v) ?? undefined);
      continue;
    }
    if (v && typeof v === 'object' && 'ns' in v) { // 纯选择器叶子
      setPath(out, path, serializeSelector(v) ?? undefined);
      continue;
    }
    setPath(out, path, v);
  }
  // 空容器回收：只回收本次触碰过、且已无键的容器（按深→浅，一次过）
  const pruneList = ['stages.search.paginate', 'stages.search.out.map', 'stages.toc.out.map',
    'stages.detail.out', 'stages.search.out', 'stages.toc.out',
    'stages.search.request', 'stages.detail.request', 'stages.toc.request', 'stages.content.request',
    'login.webview', 'login.relogin', 'login.check', 'login.submit', 'login',
    'http.headers', 'http.timeout', 'http', 'requires', 'lib.pure', 'lib.ctx', 'lib', 'vars'];
  const isEmptyObj = (x) => x !== null && typeof x === 'object' && !Array.isArray(x) && Object.keys(x).length === 0;
  for (const p of pruneList) {
    const related = Array.from(touched).some((t) => t === p || t.startsWith(p + '.'));
    if (!related) continue;
    const segs = p.split('.');
    let parent = out;
    let cur = out;
    for (const s of segs) {
      if (cur === undefined || typeof cur !== 'object') { cur = undefined; break; }
      parent = cur;
      cur = cur[s];
    }
    if (isEmptyObj(cur)) {
      delete parent[segs[segs.length - 1]];
    }
  }
  return out;
}

// ───────────────────────── M1 骨架 ─────────────────────────

/** 最小合法骨架源（与 Python generate_form.py 的 gen_skeleton 同构）。 */
function skeleton() {
  return {
    rsl: '2.0',
    id: 'form-skel.example.com',
    name: '表单骨架站（生成物）',
    type: 'novel',
    baseUrl: 'https://form-skel.example.com',
    requires: { features: [], crypto: [] },
    stages: {
      search: {
        request: { url: 'https://form-skel.example.com/search', params: { q: 'var:kw' } },
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
        request: { url: 'var:book.url' },
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
  };
}

const API = {
  NAMESPACES, BARE_NS, STEP_KEYS, SEARCH_MAP_FIELDS, DETAIL_OUT_FIELDS, TOC_MAP_FIELDS, SPLIT_MODES,
  GROUPS, ZH,
  parseSelector, serializeSelector, parseSteps, serializeSteps,
  deriveFormConfig, modeledPaths, extractFormState, applyFormState, skeleton,
  parseRule, serializeRule,
};

// UMD：node 测试 require / WebView <script> 全局两用
if (typeof module !== 'undefined' && module.exports) {
  module.exports = API;
} else {
  window.RslFormModel = API;
}
