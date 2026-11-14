// 表单模型单测（M1/M2 核心不变量）。零依赖，node 直跑：
//   node test/form-model-test.js
'use strict';

const fs = require('fs');
const path = require('path');
const M = require('../form/form-model');

let pass = 0;
let fail = 0;
function t(name, fn) {
  try {
    fn();
    pass++;
    console.log(`  ok  ${name}`);
  } catch (e) {
    fail++;
    console.log(`FAIL  ${name}\n      ${e.message}`);
  }
}
function eq(a, b, msg) {
  const ja = JSON.stringify(a);
  const jb = JSON.stringify(b);
  if (ja !== jb) throw new Error(`${msg || 'not equal'}\n      got:      ${ja}\n      expected: ${jb}`);
}
const schema = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'schema', 'rsl.schema.json'), 'utf-8'));

// ── 选择器 ──
t('sel: parse/serialize 常规', () => {
  eq(M.parseSelector('css:a.title'), { ns: 'css', expr: 'a.title' });
  eq(M.serializeSelector({ ns: 'css', expr: 'a.title' }), 'css:a.title');
});
t('sel: 裸命名空间无冒号', () => {
  eq(M.parseSelector('text'), { ns: 'text', expr: '' });
  eq(M.serializeSelector({ ns: 'text', expr: '' }), 'text');
});
t('sel: 未知前缀保留信息', () => {
  eq(M.parseSelector('weird:x'), { ns: '', expr: 'weird:x' });
  eq(M.serializeSelector({ ns: '', expr: 'weird:x' }), 'weird:x');
});
t('sel: 空 → null', () => {
  eq(M.parseSelector(''), null);
  eq(M.parseSelector(null), null);
  eq(M.serializeSelector({ ns: 'css', expr: '' }), null);
});

// ── 步骤 ──
t('steps: parse/serialize 往返', () => {
  const doc = [{ trim: true }, { replace: { from: 'a', to: 'b' } }, 'trim'];
  eq(M.serializeSteps(M.parseSteps(doc)), [{ trim: true }, { replace: { from: 'a', to: 'b' } }, { trim: true }]);
});

// ── 配置派生 ──
t('config: 从捆绑 schema 派生，顶层字段 ≥ 15，含 stages/l ogin 嵌套', () => {
  const cfg = M.deriveFormConfig(schema);
  if (cfg.fields.length < 15) throw new Error(`顶层字段只有 ${cfg.fields.length}`);
  const names = cfg.fields.map((f) => f.field);
  for (const k of ['rsl', 'id', 'name', 'type', 'baseUrl', 'requires', 'stages', 'login', 'http', 'vars', 'lib']) {
    if (!names.includes(k)) throw new Error(`缺 ${k}`);
  }
  const stages = cfg.fields.find((f) => f.field === 'stages');
  if (!stages.fields || !stages.fields.length) throw new Error('stages 未展开子字段');
});
t('config: 命名空间 18 + 步骤键 31', () => {
  const cfg = M.deriveFormConfig(schema);
  eq(cfg.namespaces.length, 18);
  eq(cfg.step_keys.length, 31);
});

// ── extract/apply ──
const DEMO = {
  rsl: '2.0', id: 't.example.com', name: '测试站', type: 'novel',
  baseUrl: 'https://t.example.com',
  requires: { features: [], crypto: [] },
  'x-custom': { keep: true },
  vars: { kw: '' },
  stages: {
    search: {
      request: { url: 'https://t.example.com/search', params: { q: 'var:kw' } },
      out: {
        list: 'css:ul li',
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

t('M2: extract → apply 幂等（未动表单时文档不变）', () => {
  const st = M.extractFormState(DEMO);
  const out = M.applyFormState(DEMO, st);
  eq(out, DEMO, '往返必须逐键一致');
});
t('M2: 未建模字段原样保留（x-custom / emptyOk / params）', () => {
  const st = M.extractFormState(DEMO);
  st.name = { value: '改名站', present: true };
  const out = M.applyFormState(DEMO, st);
  eq(out['x-custom'], { keep: true });
  eq(out.stages.search.out.emptyOk, true);
  eq(out.stages.search.request.params, { q: 'var:kw' });
  eq(out.name, '改名站');
});
t('M2: 改 name + 加 transform 步骤 → apply 正确写回', () => {
  const st = M.extractFormState(DEMO);
  st['stages.detail.out.name'] = {
    value: { selector: { ns: 'css', expr: 'h1.title' }, optional: false, default: null,
      transform: [{ key: 'trim', args: null }, { key: 'replace', args: { from: '《', to: '' } }] },
    present: true,
  };
  const out = M.applyFormState(DEMO, st);
  eq(out.stages.detail.out.name, {
    $: 'css:h1.title',
    transform: [{ trim: true }, { replace: { from: '《', to: '' } }],
  });
});
t('M2: 清空 rule 字段 → 删除该键，其余保留', () => {
  const st = M.extractFormState(DEMO);
  st['stages.detail.out.name'] = { value: null, present: true };
  const out = M.applyFormState(DEMO, st);
  if ('name' in out.stages.detail.out) throw new Error('name 应被删除');
  eq(out.stages.detail.out.tocUrl, { $: 'css:a.read', attr: 'href', optional: true });
});
t('M2: content split 修改保留在同一对象', () => {
  const st = M.extractFormState(DEMO);
  st['stages.content.out.content'].value.split = 'block';
  const out = M.applyFormState(DEMO, st);
  eq(out.stages.content.out.content.paragraphs, { split: 'block' });
  eq(out.stages.content.out.content.$, 'css:div#content');
});
t('M2: features 空数组=显式声明（不删）', () => {
  const st = M.extractFormState(DEMO);
  const out = M.applyFormState(DEMO, st);
  eq(out.requires.features, []);
});
t('M2: features 勾选 js+webview → 写回', () => {
  const st = M.extractFormState(DEMO);
  st['requires.features'] = { value: ['js', 'webview'], present: true };
  const out = M.applyFormState(DEMO, st);
  eq(out.requires.features, ['js', 'webview']);
});
t('M2: vars 增删', () => {
  const st = M.extractFormState(DEMO);
  st.vars = { value: [{ k: 'kw', v: '斗' }, { k: 'page', v: '1' }], present: true };
  let out = M.applyFormState(DEMO, st);
  eq(out.vars, { kw: '斗', page: '1' });
  st.vars = { value: [], present: true };
  out = M.applyFormState(DEMO, st);
  if ('vars' in out) throw new Error('清空后 vars 应删除');
});
t('M2: login.webview 契约写入 + 清空回收', () => {
  const st = M.extractFormState(DEMO);
  st['login.type'] = { value: 'webview', present: true };
  st['login.webview.entry'] = { value: 'https://t.example.com/login', present: true };
  st['login.webview.success'] = { value: { ns: 'css', expr: 'div.user' }, present: true };
  st['login.relogin.max'] = { value: 1, present: true };
  let out = M.applyFormState(DEMO, st);
  eq(out.login, {
    type: 'webview',
    webview: { entry: 'https://t.example.com/login', success: 'css:div.user' },
    relogin: { max: 1 },
  });
  st['login.webview.entry'] = { value: null, present: true };
  st['login.webview.success'] = { value: null, present: true };
  st['login.relogin.max'] = { value: null, present: true };
  out = M.applyFormState(DEMO, st);
  if ('webview' in out.login || 'relogin' in out.login) throw new Error('空容器应回收');
});
t('M2: paginate 写入', () => {
  const st = M.extractFormState(DEMO);
  st['stages.search.paginate.next'] = { value: { ns: 'css', expr: 'a.next' }, present: true };
  st['stages.search.paginate.max'] = { value: 10, present: true };
  const out = M.applyFormState(DEMO, st);
  eq(out.stages.search.paginate, { next: 'css:a.next', max: 10 });
});
t('M2: x-jslib 等根级未建模块不受影响', () => {
  const doc = { ...DEMO, 'x-jslib': [{ url: 'file://libs/lib.js' }] };
  const st = M.extractFormState(doc);
  st.name = { value: 'n', present: true };
  const out = M.applyFormState(doc, st);
  eq(out['x-jslib'], [{ url: 'file://libs/lib.js' }]);
});

// ── E1 explore 编辑露出 ──
const EXPLORE_DOC = {
  rsl: '2.0', id: 'e.example.com', name: '发现站', type: 'novel',
  baseUrl: 'https://e.example.com',
  requires: { features: [], crypto: [] },
  stages: {
    explore: {
      filters: [
        { key: 'order', label: '排序', default: 'hot', persist: true,
          options: [{ value: 'hot', label: '最热' }, { value: 'new', label: '最新' }] },
        { key: 'len', label: '字数', default: 1, options: ['all', '100'] },
      ],
      groups: [
        { title: '玄幻', params: { cat: 'xuanhuan' } },
        { title: '排行榜', url: 'https://e.example.com/rank', groups: [
          { title: '月票榜', params: { cat: 'rank', by: 'vote' } },
        ] },
      ],
      request: { url: 'https://e.example.com/list', params: { cat: 'var:group.params.cat' } },
      out: {
        list: 'css:.list .item',
        map: {
          name: { $: 'css:h3', transform: [{ trim: true }] },
          url: { $: 'css:a', attr: 'href', transform: [{ absolute: true }] },
        },
      },
    },
  },
};

t('E1: explore extract → apply 幂等（未动表单逐键一致，extras 保真）', () => {
  const st = M.extractFormState(EXPLORE_DOC);
  eq(M.applyFormState(EXPLORE_DOC, st), EXPLORE_DOC, 'explore 往返必须逐键一致（含 group.url / filter.persist）');
});
t('E1: filters 未建模键保真 + default 类型保真', () => {
  const doc = JSON.parse(JSON.stringify(EXPLORE_DOC));
  doc.stages.explore.filters[0].style = 'chip'; // 未建模键
  const st = M.extractFormState(doc);
  const out = M.applyFormState(doc, st);
  eq(out.stages.explore.filters[0].style, 'chip');
  eq(out.stages.explore.filters[1].default, 1, '数字 default 不应变字符串');
});
t('E1: filters 改选项文本 → 重建 options', () => {
  const st = M.extractFormState(EXPLORE_DOC);
  st['stages.explore.filters'].value[0].optionsText = 'hot|最热\nvote|月票';
  const out = M.applyFormState(EXPLORE_DOC, st);
  eq(out.stages.explore.filters[0].options, [{ value: 'hot', label: '最热' }, { value: 'vote', label: '月票' }]);
});
t('E1: groups 文本编辑 → 二级树重建 + extras 按路径并回', () => {
  const st = M.extractFormState(EXPLORE_DOC);
  // 模拟 UI（groupsWidget）：文本变更置 roundTrip=false（未编辑信号消失）
  st['stages.explore.groups'].value = {
    text: '玄幻 | cat=xuanhuan\n排行榜/月票榜 | cat=rank, by=vote\n排行榜/收藏榜 | cat=rank, by=fav',
    raw: st['stages.explore.groups'].value.raw,
    roundTrip: false,
  };
  const out = M.applyFormState(EXPLORE_DOC, st);
  const g = out.stages.explore.groups;
  eq(g.length, 2);
  eq(g[1].url, 'https://e.example.com/rank', '父节点未建模键 url 并回');
  eq(g[1].groups.length, 2);
  eq(g[1].groups[1], { title: '收藏榜', params: { cat: 'rank', by: 'fav' } });
});
t('E1: explore out.map 改选择器写回 + params 保留', () => {
  const st = M.extractFormState(EXPLORE_DOC);
  st['stages.explore.out.map.name'] = {
    value: { selector: { ns: 'css', expr: 'h3.name' }, optional: false, default: null, transform: [] },
    present: true,
  };
  const out = M.applyFormState(EXPLORE_DOC, st);
  eq(out.stages.explore.out.map.name, 'css:h3.name');
  eq(out.stages.explore.request.params, { cat: 'var:group.params.cat' });
});
t('E1: explore 全路径有文案（ZH 门禁自动覆盖）', () => {
  for (const m of M.modeledPaths().filter((p) => p.path.startsWith('stages.explore.'))) {
    if (!m.zh || !m.help) throw new Error(`${m.path} 缺 zh/help`);
  }
});

// ── M1 骨架 ──
t('M1: skeleton 必备键齐全 + 往返幂等', () => {
  const sk = M.skeleton();
  for (const k of ['rsl', 'id', 'name', 'type', 'baseUrl', 'requires', 'stages']) {
    if (!(k in sk)) throw new Error(`缺 ${k}`);
  }
  for (const stg of ['search', 'detail', 'toc', 'content']) {
    if (!sk.stages[stg]) throw new Error(`缺阶段 ${stg}`);
  }
  const st = M.extractFormState(sk);
  eq(M.applyFormState(sk, st), sk, '骨架往返必须一致');
});

// ── 新手文案门禁（UX 即不变量）──
t('ux: 全部建模路径有中文标签 + 说明（新手第一次要看懂）', () => {
  for (const m of M.modeledPaths()) {
    if (!m.zh) throw new Error(`${m.path} 缺中文标签 zh`);
    if (!m.help) throw new Error(`${m.path} 缺说明 help`);
  }
});
t('ux: GROUPS 覆盖全部路径且有说明', () => {
  const covered = new Set();
  for (const g of M.GROUPS) {
    if (!g.title || !g.desc) throw new Error(`分区缺 title/desc：${g.title}`);
    for (const m of M.modeledPaths().filter(g.match)) covered.add(m.path);
  }
  const all = M.modeledPaths().map((m) => m.path);
  const missing = all.filter((p) => !covered.has(p));
  if (missing.length) throw new Error(`GROUPS 未覆盖：${missing.join(', ')}`);
});

console.log(`\nform-model: ${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
