"""expr: 表达式语言的可运行原型（对齐 spec/EXPRESSIONS.md v2.0）

验证三件事：
1. 语法足以表达真实站点的分页与签名需求；
2. 编译期能拦下变量未定义、函数不存在、算法未声明、类型错误、超限；
3. 求值结果与预期一致。

这是规范验证用的最小实现；Rust 正式实现按 spec/EXPRESSIONS.md §8 重写。
"""
import base64
import hashlib
import hmac as _hmac
import random
import re
import time
import urllib.parse

# ---------- 词法 ----------
TOKEN_RE = re.compile(r"""
    (?P<num>\d+\.\d+|\d+)
  | (?P<str>'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")
  | (?P<op>==|!=|<=|>=|&&|\|\||[+\-*/%<>!?:(),])
  | (?P<ident>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<ws>\s+)
""", re.X)


class ExprError(Exception):
    def __init__(self, code, msg, pos=0):
        super().__init__(f"{code}: {msg} (pos {pos})")
        self.code = code


def tokenize(src):
    pos, out = 0, []
    while pos < len(src):
        m = TOKEN_RE.match(src, pos)
        if not m:
            raise ExprError("E_EXPR_SYNTAX", f"无法识别的字符 {src[pos]!r}", pos)
        kind = m.lastgroup
        if kind != "ws":
            out.append((kind, m.group(), pos))
        pos = m.end()
    out.append(("eof", "", pos))
    return out


# ---------- 语法（Pratt） ----------
class Parser:
    BIN = {
        "||": (1, "L"), "&&": (2, "L"),
        "==": (3, "L"), "!=": (3, "L"),
        "<": (4, "L"), "<=": (4, "L"), ">": (4, "L"), ">=": (4, "L"),
        "+": (5, "L"), "-": (5, "L"),
        "*": (6, "L"), "/": (6, "L"), "%": (6, "L"),
    }

    def __init__(self, src):
        self.toks = tokenize(src)
        self.i = 0
        self.depth = 0

    def peek(self):
        return self.toks[self.i]

    def next(self):
        t = self.toks[self.i]
        self.i += 1
        return t

    def expect(self, val):
        t = self.next()
        if t[1] != val:
            raise ExprError("E_EXPR_SYNTAX", f"期望 {val!r} 得到 {t[1]!r}", t[2])
        return t

    def parse(self):
        e = self.expr(0)
        if self.peek()[0] != "eof":
            t = self.peek()
            raise ExprError("E_EXPR_SYNTAX", f"多余的内容 {t[1]!r}", t[2])
        return e

    def expr(self, min_bp):
        self.depth += 1
        if self.depth > 32:
            raise ExprError("E_EXPR_LIMIT", "表达式嵌套超过 32 层")
        left = self.nud()
        while True:
            t = self.peek()
            if t[0] == "op" and t[1] == "?" and min_bp == 0:
                self.next()
                a = self.expr(0)
                self.expect(":")
                b = self.expr(0)
                left = ("ternary", left, a, b, t[2])
                continue
            if t[0] != "op" or t[1] not in self.BIN:
                break
            bp, _ = self.BIN[t[1]]
            if bp < min_bp:
                break
            self.next()
            right = self.expr(bp + 1)
            left = ("bin", t[1], left, right, t[2])
        self.depth -= 1
        return left

    def nud(self):
        t = self.next()
        kind, v, p = t
        if kind == "num":
            return ("num", float(v) if "." in v else int(v), p)
        if kind == "str":
            return ("str", _unescape(v[1:-1]), p)
        if kind == "op" and v == "(":
            e = self.expr(0)
            self.expect(")")
            return e
        if kind == "op" and v == "!":
            return ("unary", "!", self.expr(7), p)
        if kind == "op" and v == "-":
            return ("unary", "-", self.expr(7), p)
        if kind == "ident":
            if v == "true":
                return ("bool", True, p)
            if v == "false":
                return ("bool", False, p)
            if v == "null":
                return ("null", None, p)
            if self.peek()[1] == "(":
                self.next()
                args = []
                if self.peek()[1] != ")":
                    args.append(self.expr(0))
                    while self.peek()[1] == ",":
                        self.next()
                        args.append(self.expr(0))
                self.expect(")")
                return ("call", v, args, p)
            return ("var", v, p)
        raise ExprError("E_EXPR_SYNTAX", f"意外的记号 {v!r}", p)


def _unescape(s):
    """只处理规范允许的转义，其余原样保留（避免破坏中文）。"""
    out, i = [], 0
    table = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", "'": "'", '"': '"'}
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s):
            out.append(table.get(s[i + 1], s[i + 1]))
            i += 2
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


# ---------- 值转换 ----------
def _to_num(x):
    if isinstance(x, bool):
        return 1 if x else 0
    if isinstance(x, (int, float)):
        return x
    try:
        s = str(x)
        return float(s) if "." in s else int(s)
    except Exception:
        return 0


def _to_str(x):
    if x is None:
        return ""
    if isinstance(x, bool):
        return "true" if x else "false"
    if isinstance(x, float) and x.is_integer():
        return str(int(x))
    return str(x)


def _truthy(v):
    return not (v is None or v == "" or v == 0 or v is False or v == [])


_CJK = "零一二三四五六七八九"
_CJK_UNIT = {"十": 10, "百": 100, "千": 1000, "万": 10000}


def _cjk2num(s):
    s = str(s)
    m = re.search(r"[零一二三四五六七八九十百千万]+", s)
    if not m:
        return _to_num(s)
    total, section, digit = 0, 0, 0
    for ch in m.group():
        if ch in _CJK:
            digit = _CJK.index(ch)
        elif ch in _CJK_UNIT:
            unit = _CJK_UNIT[ch]
            if unit == 10000:
                section = (section + digit) * unit
                total += section
                section = 0
            else:
                section += (digit or 1) * unit
            digit = 0
    return total + section + digit


def _num2cjk(n):
    n = int(_to_num(n))
    if n == 0:
        return "零"
    units = ["", "十", "百", "千", "万"]
    digits, out, zero = list(str(n)), [], False
    for i, d in enumerate(digits):
        d = int(d)
        pos = len(digits) - i - 1
        if d == 0:
            zero = True
            continue
        if zero and out:
            out.append("零")
        zero = False
        out.append(_CJK[d] + units[pos] if pos < len(units) else "")
    return "".join(out)


# ---------- 函数表 ----------
NOW = None  # 测试时固定


def _now():
    return NOW if NOW is not None else int(time.time() * 1000)


PURE = {
    "min": lambda *a: min(a), "max": lambda *a: max(a),
    "abs": abs, "round": lambda n: int(round(n)),
    "ceil": lambda n: -int(-_to_num(n) // 1), "floor": lambda n: int(_to_num(n) // 1),
    "len": lambda x: len(x), "upper": lambda s: _to_str(s).upper(),
    "lower": lambda s: _to_str(s).lower(), "trim": lambda s: _to_str(s).strip(),
    "replace": lambda s, a, b: _to_str(s).replace(_to_str(a), _to_str(b)),
    "split": lambda s, sep: _to_str(s).split(_to_str(sep)),
    "join": lambda l, sep: _to_str(sep).join(_to_str(x) for x in l),
    "contains": lambda s, sub: _to_str(sub) in _to_str(s),
    "num": _to_num, "str": _to_str,
    "cjk2num": _cjk2num, "num2cjk": _num2cjk,
}

# name -> (feature, 实现, 需要的 crypto 算法 | None)
FEATURE_FN = {
    "now": ("time", _now, None),
    "nowDate": ("time", lambda *a: time.strftime(a[0] if a else "%Y-%m-%d",
                                                 time.localtime(_now() / 1000)), None),
    "rand": ("random", lambda *a: random.random() if not a else random.randint(0, int(a[0])), None),
    "md5": ("crypto", lambda s: hashlib.md5(_to_str(s).encode()).hexdigest(), "md5"),
    "sha1": ("crypto", lambda s: hashlib.sha1(_to_str(s).encode()).hexdigest(), "sha1"),
    "sha256": ("crypto", lambda s: hashlib.sha256(_to_str(s).encode()).hexdigest(), "sha256"),
    "sha512": ("crypto", lambda s: hashlib.sha512(_to_str(s).encode()).hexdigest(), "sha512"),
    "hmac": ("crypto", lambda alg, key, s: _hmac.new(
        _to_str(key).encode(), _to_str(s).encode(),
        {"hmac-sha1": hashlib.sha1, "hmac-sha256": hashlib.sha256}[_to_str(alg)]
    ).hexdigest(), "hmac-sha256"),
    "hexEncode": ("crypto", lambda s: _to_str(s).encode().hex(), "hex"),
    "hexDecode": ("crypto", lambda s: bytes.fromhex(_to_str(s)).decode(errors="replace"), "hex"),
    "base64Encode": ("crypto", lambda s: base64.b64encode(_to_str(s).encode()).decode(), "base64"),
    "base64Decode": ("crypto", lambda s: base64.b64decode(_to_str(s)).decode(errors="replace"), "base64"),
    "urlEncode": ("crypto", lambda s, cs="utf-8": urllib.parse.quote(_to_str(s).encode(cs)), "url-encode"),
}


# ---------- 编译期检查 ----------
def compile_expr(src, scope, features=(), crypto_algos=()):
    if len(src) > 512:
        raise ExprError("E_EXPR_LIMIT", "表达式超过 512 字符")
    ast = Parser(src).parse()
    _check(ast, set(scope), set(features), set(crypto_algos))
    return ast


def _check(node, scope, features, algos):
    t = node[0]
    if t == "var":
        if node[1] not in scope:
            raise ExprError("E_REF", f"变量 {node[1]!r} 在当前阶段不可见，可用：{sorted(scope)}", node[2])
    elif t == "call":
        name = node[1]
        if name in PURE:
            pass
        elif name in FEATURE_FN:
            feat, _, alg = FEATURE_FN[name]
            if feat not in features:
                raise ExprError("E_UNSUPPORTED", f"函数 {name}() 需要 features 声明 {feat!r}")
            if alg is not None and alg not in algos:
                raise ExprError("E_UNSUPPORTED", f"函数 {name}() 需要 requires.crypto 声明 {alg!r}")
        else:
            raise ExprError("E_EXPR_FN", f"未知函数 {name}()")
        for a in node[2]:
            _check(a, scope, features, algos)
    elif t == "bin":
        _check(node[2], scope, features, algos)
        _check(node[3], scope, features, algos)
    elif t == "unary":
        _check(node[2], scope, features, algos)
    elif t == "ternary":
        for a in node[1:4]:
            _check(a, scope, features, algos)


# ---------- 求值 ----------
def evaluate(ast, ctx):
    t = ast[0]
    if t in ("num", "str", "bool"):
        return ast[1]
    if t == "null":
        return None
    if t == "var":
        return ctx.get(ast[1])
    if t == "unary":
        v = evaluate(ast[2], ctx)
        return (not _truthy(v)) if ast[1] == "!" else -_to_num(v)
    if t == "ternary":
        return evaluate(ast[2], ctx) if _truthy(evaluate(ast[1], ctx)) else evaluate(ast[3], ctx)
    if t == "call":
        name = ast[1]
        args = [evaluate(a, ctx) for a in ast[2]]
        if name in PURE:
            return PURE[name](*args)
        return FEATURE_FN[name][1](*args)
    if t == "bin":
        op = ast[1]
        if op == "&&":
            return _truthy(evaluate(ast[2], ctx)) and _truthy(evaluate(ast[3], ctx))
        if op == "||":
            return _truthy(evaluate(ast[2], ctx)) or _truthy(evaluate(ast[3], ctx))
        l, r = evaluate(ast[2], ctx), evaluate(ast[3], ctx)
        if op == "+":
            if isinstance(l, str) or isinstance(r, str):
                return _to_str(l) + _to_str(r)
            return _to_num(l) + _to_num(r)
        if op == "-":
            return _to_num(l) - _to_num(r)
        if op == "*":
            return _to_num(l) * _to_num(r)
        if op == "/":
            d = _to_num(r)
            if d == 0:
                raise ExprError("E_TYPE", "除数为 0")
            return _to_num(l) / d
        if op == "%":
            return _to_num(l) % _to_num(r)
        if op == "==":
            return l == r or _to_str(l) == _to_str(r)
        if op == "!=":
            return not (l == r or _to_str(l) == _to_str(r))
        a, b = _to_num(l), _to_num(r)
        return {"<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
    raise ExprError("E_EXPR_SYNTAX", f"未知节点 {t}")


# ---------- 自测 ----------
if __name__ == "__main__":
    NOW = 1730000000000
    SCOPE = {"page", "kw", "ts", "let:sign", "book"}
    FEATS = ["time", "crypto"]
    ALGOS = ["md5", "sha1", "sha256", "hmac-sha256", "hex", "base64", "url-encode"]

    cases = [
        ("page - 1", {"page": 3}, 2),
        ("page * 20", {"page": 5}, 100),
        ("max(1, page - 1)", {"page": 1}, 1),
        ("ceil(page * 1.0 / 2)", {"page": 5}, 3),
        ("'第' + page + '页'", {"page": 3}, "第3页"),
        ("replace(kw, ' ', '+')", {"kw": "斗 破"}, "斗+破"),
        ("urlEncode(kw, 'gbk')", {"kw": "斗破"}, "%B6%B7%C6%C6"),
        ("page > 1 ? 'old' : 'first'", {"page": 1}, "first"),
        ("len(split('a,b,c', ','))", {}, 3),
        ("md5(kw + 'salt')", {"kw": "x"}, hashlib.md5(b"xsalt").hexdigest()),
        ("sha256('x')", {}, hashlib.sha256(b"x").hexdigest()),
        ("hmac('hmac-sha256', 'k', 'v')", {},
         _hmac.new(b"k", b"v", hashlib.sha256).hexdigest()),
        ("hexEncode('ab')", {}, "6162"),
        ("base64Decode('5paX56C0')", {}, "斗破"),
        ("kw == '' ? 0 : 1", {"kw": ""}, 0),
        ("!contains(kw, '赠') && page <= 3", {"kw": "斗破", "page": 2}, True),
        ("cjk2num('第一百二十三章')", {}, 123),
        ("num2cjk(123)", {}, "一百二十三"),
    ]
    ok = 0
    for src, ctx, expect in cases:
        ast = compile_expr(src, SCOPE, FEATS, ALGOS)
        got = evaluate(ast, ctx)
        good = got == expect
        ok += good
        print(f"{'OK ' if good else 'FAIL'} {src:42} => {got!r}")

    print("\n--- 编译期拦截 ---")
    errs = [
        ("pagee - 1", {}, (), (), "E_REF"),
        ("fooBar(1)", {}, (), (), "E_EXPR_FN"),
        ("md5('x')", {}, (), (), "E_UNSUPPORTED"),
        ("md5('x')", {}, ("crypto",), (), "E_UNSUPPORTED"),
        ("md5('x')", {}, ("crypto",), ("md5",), None),
        ("aesCbcDecrypt('x')", {}, (), (), "E_EXPR_FN"),
        ("1 +", {}, (), (), "E_EXPR_SYNTAX"),
        ("a.b", {}, (), (), "E_EXPR_SYNTAX"),
        ("'" + "x" * 600 + "'", {}, (), (), "E_EXPR_LIMIT"),
    ]
    bad = 0
    for src, ctx, feats, algos, code in errs:
        try:
            compile_expr(src, SCOPE, feats, algos)
            if code is None:
                print(f"OK   {src[:24]:24} 正常通过（预期）")
            else:
                bad += 1
                print(f"FAIL {src[:24]:24} 未拦截，预期 {code}")
        except ExprError as e:
            if code is None or e.code != code:
                bad += 1
                print(f"FAIL {src[:24]:24} 得到 {e.code}，预期 {code}")
            else:
                print(f"OK   {src[:24]:24} -> {e.code}")

    print(f"\n求值 {ok}/{len(cases)} 通过；编译期用例 {len(errs) - bad}/{len(errs)} 符合预期")
