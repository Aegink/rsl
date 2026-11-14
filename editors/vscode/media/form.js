// RSL 表单编辑器 WebView 端（M1 生成 / M2 双向同步 + 新手引导文案层）。
// 依赖 window.RslFormModel（form/form-model.js UMD 全局）与 vscode.postMessage。
// 协议：in  = {type:'init'|'doc'|'diags', …}；out = {type:'apply'|'generate', state}。
// 新手约定：所有字段渲染 中文标签 + 一句话说明 + 示例占位符（来自模型 ZH 表，有门禁锁定）；
// 分区按"第 N 步"组织；顶部横幅给三步指引 + 必填缺口 chips（可点击跳转对应分区）。
'use strict';

(function () {
  const M = window.RslFormModel;
  const vscode = acquireVsCodeApi();
  let cfg = null;
  let state = null;
  let docMode = false;
  let docName = '';
  let modePaths = [];
  let focusedPath = null;  // 外部 doc 更新时不覆盖正在编辑的路径
  let openSections = new Set(); // 用户手动展开过的分区（跨 refresh 保留）

  // ───────── 基础工具 ─────────
  function el(tag, attrs = {}, ...children) {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k === 'class') e.className = v;
      else if (k === 'text') e.textContent = v;
      else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
      else e.setAttribute(k, v);
    }
    for (const c of children) { if (c) e.appendChild(c); }
    return e;
  }
  function trackFocus(input, path) {
    input.addEventListener('focus', () => { focusedPath = path; });
    input.addEventListener('blur', () => { if (focusedPath === path) focusedPath = null; });
  }
  function cssId(path) {
    return path.replace(/[^A-Za-z0-9]/g, '_');
  }

  // ───────── 字段标签（中文 + 核心徽标） ─────────
  function fieldLabel(m) {
    const lbl = el('label', { class: 'lbl', text: m.zh });
    if (m.requiredField) {
      lbl.appendChild(el('span', { class: 'badge', text: '核心' }));
    }
    lbl.title = m.path;
    return lbl;
  }

  // ───────── 选择器控件 ─────────
  function selectorWidget(path, sel, onPair, placeholder) {
    const sel2 = sel || { ns: 'css', expr: '' };
    const nsSel = el('select', { class: 'ns', title: '命名空间：css=HTML / json=JSON / re=正则 / var=变量 …' });
    nsSel.appendChild(el('option', { value: '', text: '前缀' }));
    for (const ns of cfg.namespaces) {
      nsSel.appendChild(el('option', { value: ns, text: ns + ':' }));
    }
    nsSel.value = sel2.ns || '';
    const bare = M.BARE_NS.includes(nsSel.value);
    const expr = el('input', { type: 'text', class: 'expr',
      placeholder: placeholder || '表达式' });
    expr.value = sel2.expr || '';
    if (bare) { expr.value = ''; expr.disabled = true; }
    const sync = () => onPair({ ns: nsSel.value, expr: expr.value });
    nsSel.addEventListener('change', () => {
      const isBare = M.BARE_NS.includes(nsSel.value);
      expr.disabled = isBare;
      if (isBare) expr.value = '';
      sync();
    });
    expr.addEventListener('input', sync);
    trackFocus(expr, path);
    nsSel.addEventListener('focus', () => { focusedPath = path; });
    nsSel.addEventListener('blur', () => { if (focusedPath === path) focusedPath = null; });
    return el('span', { class: 'row', style: 'flex:1 1 auto; margin:0;' }, nsSel, expr);
  }

  // ───────── 步骤列表（transform） ─────────
  function stepsWidget(path) {
    const wrap = el('div', { class: 'steps' });
    const render = () => {
      wrap.textContent = '';
      (state[path].value.transform || []).forEach((st, i) => {
        const sel = el('select', {});
        for (const k of cfg.step_keys) {
          sel.appendChild(el('option', { value: k, text: k }));
        }
        sel.value = st.key;
        sel.addEventListener('change', () => {
          state[path].value.transform[i].key = sel.value;
        });
        const args = el('input', { type: 'text',
          placeholder: '参数 JSON（可空；如 {"from":"《","to":""}）' });
        args.value = st.args === null || st.args === undefined ? '' : JSON.stringify(st.args);
        args.addEventListener('input', () => {
          let parsed = null;
          if (args.value.trim() !== '') {
            try { parsed = JSON.parse(args.value); } catch (_) { parsed = args.value; }
          }
          state[path].value.transform[i].args = parsed;
        });
        trackFocus(args, `${path}#step${i}`);
        const del = el('button', { class: 'icon-btn', text: '×',
          onclick: () => { state[path].value.transform.splice(i, 1); render(); } });
        wrap.appendChild(el('div', { class: 'step-row' }, sel, args, del));
      });
    };
    render();
    wrap._render = render;
    const add = el('button', { class: 'icon-btn', text: '+ 加工步骤',
      title: '管道加工：trim 去空白 / absolute 补全链接 / replace 替换 / number 取数值 …',
      onclick: () => {
        state[path].value.transform.push({ key: 'trim', args: null });
        wrap._render();
      } });
    return el('div', {}, wrap, el('div', { class: 'row' }, add,
      el('span', { class: 'muted', text: '常用：trim 去空白 · absolute 补全链接 · replace 替换 · number 取数值' })));
  }

  // ───────── 规则字段（selector + optional/default/transform/split） ─────────
  function ruleWidget(m) {
    const path = m.path;
    const holder = el('div', { class: 'rule-field' });
    const render = () => {
      holder.textContent = '';
      const v = state[path].value;
      const head = el('div', { class: 'row' });
      head.appendChild(fieldLabel(m));
      if (!v) {
        const enable = el('button', { class: 'icon-btn', text: '+ 添加这个字段',
          onclick: () => {
            state[path].value = { selector: { ns: 'css', expr: '' }, optional: false,
              default: null, transform: [], split: null };
            state[path].present = true;
            render();
          } });
        head.appendChild(enable);
        holder.appendChild(head);
        holder.appendChild(el('div', { class: 'desc', text: m.help }));
        return;
      }
      head.appendChild(selectorWidget(path, v.selector,
        (pair) => { state[path].value.selector = pair; }, m.ph));
      const opt = el('input', { type: 'checkbox', title: 'optional：该字段在页面上可能不存在（缺失不报错）' });
      opt.checked = Boolean(v.optional);
      opt.addEventListener('change', () => { state[path].value.optional = opt.checked; });
      head.appendChild(opt);
      head.appendChild(el('span', { class: 'check-label', text: '可缺失' }));
      const def = el('input', { type: 'text', class: 'expr', style: 'max-width:110px;',
        placeholder: '缺省值' });
      def.value = v.default || '';
      def.addEventListener('input', () => {
        state[path].value.default = def.value === '' ? null : def.value;
      });
      trackFocus(def, `${path}#default`);
      head.appendChild(def);
      const del = el('button', { class: 'icon-btn', text: '移除',
        onclick: () => { state[path].value = null; state[path].present = true; render(); } });
      head.appendChild(del);
      holder.appendChild(head);
      holder.appendChild(el('div', { class: 'desc', text: m.help }));
      if (v.transform) {
        holder.appendChild(stepsWidget(path));
      }
      if (m.splitModes) {
        const sp = el('select', { title: '正文分段方式：br=按 <br> / block=按块元素 / auto / line' });
        sp.appendChild(el('option', { value: '', text: '分段：—' }));
        for (const md of m.splitModes) sp.appendChild(el('option', { value: md, text: md }));
        sp.value = v.split || '';
        sp.addEventListener('change', () => { state[path].value.split = sp.value || null; });
        holder.appendChild(el('div', { class: 'row' },
          el('label', { class: 'lbl', text: '正文分段方式' }), sp));
      }
    };
    render();
    holder._render = render;
    return holder;
  }

  // ───────── kv 列表 ─────────
  function kvWidget(m) {
    const path = m.path;
    const holder = el('div', {});
    const render = () => {
      holder.textContent = '';
      (state[path].value || []).forEach((p, i) => {
        const kIn = el('input', { type: 'text', placeholder: '键（如 kw）' });
        kIn.value = p.k;
        kIn.addEventListener('input', () => { state[path].value[i].k = kIn.value; });
        const vIn = el('input', { type: 'text', placeholder: '值' });
        vIn.value = p.v;
        vIn.addEventListener('input', () => { state[path].value[i].v = vIn.value; });
        trackFocus(kIn, `${path}#${i}.k`);
        trackFocus(vIn, `${path}#${i}.v`);
        const del = el('button', { class: 'icon-btn', text: '×',
          onclick: () => { state[path].value.splice(i, 1); render(); } });
        holder.appendChild(el('div', { class: 'kv-row' }, kIn, vIn, del));
      });
    };
    render();
    holder._render = render;
    const add = el('button', { class: 'icon-btn', text: '+ 添加',
      onclick: () => { state[path].value.push({ k: '', v: '' }); holder._render(); } });
    return el('div', {}, holder, el('div', { class: 'row' }, add));
  }

  // ───────── 登录字段列表 ─────────
  function loginFieldsWidget(path) {
    const holder = el('div', {});
    const render = () => {
      holder.textContent = '';
      (state[path].value || []).forEach((f, i) => {
        const key = el('input', { type: 'text', placeholder: 'key（如 username）' });
        key.value = f.key;
        key.addEventListener('input', () => { state[path].value[i].key = key.value; });
        const lab = el('input', { type: 'text', placeholder: '界面显示名（如 账号）' });
        lab.value = f.label;
        lab.addEventListener('input', () => { state[path].value[i].label = lab.value; });
        const def = el('input', { type: 'text', placeholder: '预填值' });
        def.value = f.def;
        def.addEventListener('input', () => { state[path].value[i].def = def.value; });
        const sec = el('input', { type: 'checkbox', title: 'secret：密码类字段（界面脱敏）' });
        sec.checked = f.secret;
        sec.addEventListener('change', () => { state[path].value[i].secret = sec.checked; });
        trackFocus(key, `${path}#${i}`);
        trackFocus(lab, `${path}#${i}l`);
        trackFocus(def, `${path}#${i}d`);
        const del = el('button', { class: 'icon-btn', text: '×',
          onclick: () => { state[path].value.splice(i, 1); render(); } });
        holder.appendChild(el('div', { class: 'kv-row' }, key, lab, def, sec, del));
      });
    };
    render();
    holder._render = render;
    const add = el('button', { class: 'icon-btn', text: '+ 字段',
      onclick: () => { state[path].value.push({ key: '', label: '', secret: false, def: '' }); holder._render(); } });
    return el('div', {}, holder, el('div', { class: 'row' }, add));
  }

  // ───────── 简单字段 ─────────
  function simpleWidget(m) {
    const path = m.path;
    const v = state[path];
    if (m.kind.startsWith('enum:')) {
      const sel = el('select', {});
      sel.appendChild(el('option', { value: '', text: '— 请选择 —' }));
      for (const o of m.kind.slice(5).split(',')) {
        sel.appendChild(el('option', { value: o, text: o }));
      }
      sel.value = (v.value || '');
      sel.addEventListener('change', () => { state[path].value = sel.value || null; state[path].present = true; });
      trackFocus(sel, path);
      return sel;
    }
    if (m.kind === 'multi') {
      const wrap = el('span', { class: 'row', style: 'flex:1; margin:0;' });
      const cur = new Set(v.value || []);
      for (const o of m.options) {
        const cb = el('input', { type: 'checkbox', title: o });
        cb.checked = cur.has(o);
        cb.addEventListener('change', () => {
          if (cb.checked) cur.add(o); else cur.delete(o);
          state[path].value = Array.from(cur).sort();
          state[path].present = true;
        });
        wrap.appendChild(cb);
        wrap.appendChild(el('span', { class: 'check-label', text: o }));
      }
      return wrap;
    }
    const input = el('input', { type: m.kind === 'number' ? 'number' : 'text', class: 'full',
      placeholder: m.ph || '' });
    input.value = v.value === null || v.value === undefined ? '' : String(v.value);
    input.addEventListener('input', () => {
      let nv = input.value;
      if (m.kind === 'number') nv = input.value === '' ? null : Number(input.value);
      state[path].value = nv === '' ? null : nv;
      state[path].present = true;
    });
    trackFocus(input, path);
    return input;
  }

  // ───────── 新手横幅：三步指引 + 必填缺口 ─────────
  function isEmptyVal(p) {
    if (!state[p]) return true;
    const v = state[p].value;
    if (!state[p].present && (v === null || (Array.isArray(v) && v.length === 0))) return true;
    if (v === null || v === '') return true;
    if (v && typeof v === 'object' && 'selector' in v) {
      return !M.serializeSelector(v.selector);
    }
    return false;
  }

  function missingCore() {
    const need = [];
    const checks = [
      ['id', '源 ID'], ['name', '源名称'], ['type', '源类型'], ['baseUrl', '站点首页'],
      ['stages.search.out.list', 'search 列表'],
      ['stages.search.out.map.name', 'search 书名'],
      ['stages.search.out.map.url', 'search 详情链接'],
      ['stages.detail.out.name', 'detail 书名'],
      ['stages.toc.out.list', 'toc 列表'],
      ['stages.toc.out.map.title', 'toc 章节标题'],
      ['stages.toc.out.map.url', 'toc 章节链接'],
      ['stages.content.out.content', 'content 正文'],
    ];
    for (const [path, name] of checks) {
      if (isEmptyVal(path)) need.push([name, path]);
    }
    return need;
  }

  function sectionIdOf(path) {
    if (['id', 'name', 'type', 'baseUrl'].some((p) => path === p)) return 'sec-meta';
    if (path.startsWith('stages.search.')) return 'sec-search';
    if (path.startsWith('stages.detail.')) return 'sec-detail';
    if (path.startsWith('stages.toc.')) return 'sec-toc';
    if (path.startsWith('stages.content.')) return 'sec-content';
    return null;
  }

  function renderBanner() {
    const b = document.getElementById('banner');
    b.textContent = '';
    b.className = 'banner';
    if (!docMode) {
      b.className = 'banner mode-skel';
      b.appendChild(el('div', { text: '骨架模式：当前没有打开的 .rsl.json。分区已按四阶段契约备好空壳，填完点【生成源文件】即可得到完整源。' }));
    } else {
      b.appendChild(el('div', { text: `正在编辑：${docName}` }));
    }
    b.appendChild(el('div', { class: 'guide',
      text: '新手三步：① 核对第 1 步元信息 → ② 展开第 4~7 步，把选择器对准真实页面元素 → ③ 点底部按钮写入/生成，保存后用 rsl lint 检查。' }));
    b.appendChild(el('div', { class: 'guide',
      text: '选择器 = 命名空间 + 表达式：css: 选 HTML（css:a.title = class 含 title 的链接）；json: 选 JSON（json:$.data.list）；18 种命名空间见输入时的补全菜单。' }));
    const missing = missingCore();
    if (missing.length) {
      const row = el('div', { class: 'missing' });
      row.appendChild(el('span', { class: 'muted', text: '还缺：' }));
      for (const [name, path] of missing) {
        const chip = el('button', { class: 'icon-btn chip', text: name });
        chip.addEventListener('click', () => {
          const sid = sectionIdOf(path);
          const sec = sid ? document.getElementById(sid) : null;
          if (sec) {
            sec.open = true;
            sec.scrollIntoView({ behavior: 'smooth', block: 'start' });
          }
        });
        row.appendChild(chip);
      }
      b.appendChild(row);
    } else {
      b.appendChild(el('div', { class: 'ok', text: '✓ 核心字段齐全：写入后应能过编译（以诊断为准）。' }));
    }
  }

  // ───────── 渲染主体 ─────────
  function render() {
    const root = document.getElementById('fields');
    root.textContent = '';
    for (const g of M.GROUPS) {
      const items = modePaths.filter(g.match);
      if (!items.length) continue;
      const body = el('div', { class: 'body' });
      if (g.desc) body.appendChild(el('div', { class: 'group-desc', text: g.desc }));
      const hasSel = items.some((m) => m.kind === 'rule' || m.kind === 'selector');
      if (hasSel) {
        body.appendChild(el('div', { class: 'group-desc muted',
          text: '写法：命名空间 + 表达式。css:a.title = 选 class 含 title 的链接；json:$.x = 取 JSON 字段。' }));
      }
      for (const m of items) {
        const isRule = m.kind === 'rule';
        const isKv = m.kind === 'kv' || m.kind === 'kvLoginFields';
        const isCode = m.kind === 'code';
        const isMulti = m.kind === 'multi';
        if (isRule) {
          body.appendChild(ruleWidget(m));
          continue;
        }
        if (m.kind === 'selector') {
          // 纯选择器字段（列表/下一页）：命名空间下拉 + 表达式输入
          const srow = el('div', { class: 'row' });
          srow.appendChild(fieldLabel(m));
          srow.appendChild(selectorWidget(m.path, state[m.path].value,
            (pair) => { state[m.path].value = pair; state[m.path].present = true; }, m.ph));
          body.appendChild(srow);
          body.appendChild(el('div', { class: 'desc', text: m.help }));
          continue;
        }
        const row = el('div', { class: 'row' });
        row.appendChild(fieldLabel(m));
        if (isKv) {
          body.appendChild(row);
          body.appendChild(m.kind === 'kv' ? kvWidget(m) : loginFieldsWidget(m.path));
        } else if (isCode) {
          body.appendChild(row);
          const ta = el('textarea', { class: 'code', placeholder: m.ph || 'function NAME( … ) { … }' });
          ta.value = state[m.path].value || '';
          ta.addEventListener('input', () => { state[m.path].value = ta.value === '' ? null : ta.value; });
          trackFocus(ta, m.path);
          body.appendChild(ta);
          body.appendChild(el('div', { class: 'desc', text: m.help }));
        } else {
          row.appendChild(simpleWidget(m));
          body.appendChild(row);
          body.appendChild(el('div', { class: 'desc', text: m.help }));
        }
      }
      const sec = el('details', { class: 'sec', id: secIdOfGroup(g) });
      sec.open = openSections.has(g.title) || g.title.includes('第 1 步') || g.title.includes('第 4 步');
      sec.addEventListener('toggle', () => {
        if (sec.open) openSections.add(g.title); else openSections.delete(g.title);
      });
      sec.appendChild(el('summary', { text: g.title }));
      sec.appendChild(body);
      root.appendChild(sec);
    }
  }

  function secIdOfGroup(g) {
    if (g.title.includes('第 1 步')) return 'sec-meta';
    if (g.title.includes('search')) return 'sec-search';
    if (g.title.includes('detail')) return 'sec-detail';
    if (g.title.includes('toc')) return 'sec-toc';
    if (g.title.includes('content')) return 'sec-content';
    return 'sec-' + cssId(g.title);
  }

  // ───────── 诊断条 ─────────
  function renderDiags(diags) {
    const bar = document.getElementById('diags');
    bar.textContent = '';
    if (!diags || !diags.length) {
      bar.appendChild(el('div', { class: 'ok', text: '✓ 无诊断（编译通过）' }));
      return;
    }
    for (const d of diags.slice(0, 12)) {
      const sev = d.severity === 2 ? 'warn' : '';
      bar.appendChild(el('div', { class: 'd ' + sev,
        text: `[${d.code}] ${d.message}（位置：${d.path}）—— ${d.hint || ''}` }));
    }
    if (diags.length > 12) {
      bar.appendChild(el('div', { class: 'muted', text: `…共 ${diags.length} 条诊断` }));
    }
  }

  // ───────── 消息处理 ─────────
  window.addEventListener('message', (ev) => {
    const msg = ev.data;
    if (msg.type === 'init') {
      cfg = msg.cfg;
      modePaths = M.modeledPaths();
      docMode = msg.docMode;
      docName = msg.docName || '';
      state = msg.state;
      renderBanner();
      render();
      renderDiags(msg.diags || []);
      document.getElementById('btnApply').style.display = docMode ? '' : 'none';
    } else if (msg.type === 'doc') {
      state = mergeExternal(state, msg.state);
      docName = msg.docName || docName;
      renderBanner();
      render();
    } else if (msg.type === 'diags') {
      renderDiags(msg.diags);
    }
  });

  /** 外部文档更新合并：聚焦路径保留本地编辑值，其余采用外部值。 */
  function mergeExternal(local, external) {
    const out = {};
    for (const [path, entry] of Object.entries(external)) {
      const focused = focusedPath === path
        || (focusedPath && path.startsWith(focusedPath.split('#')[0]) && focusedPath.includes('#'));
      if (focused && local[path] && local[path].present) {
        out[path] = local[path];
      } else {
        out[path] = entry;
      }
    }
    return out;
  }

  document.getElementById('btnApply').addEventListener('click', () => {
    vscode.postMessage({ type: 'apply', state });
  });
  document.getElementById('btnGenerate').addEventListener('click', () => {
    vscode.postMessage({ type: 'generate', state });
  });
})();
