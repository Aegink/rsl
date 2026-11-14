//! ExprHost（expr: 求值宿主）与 CtxApi（lib.ctx 受控接口）的 ExecCx 实现。

use super::ctx::{ExecCx, TraceEntry};
use super::value::Val;
use crate::error::{Code, Diagnostic};
use crate::ports::HttpRequest;
use serde_json::Value;
use std::collections::BTreeMap;

/// `expr:` 求值宿主（变量 + 需声明函数）。
pub struct ExprHostImpl<'x, 'c> {
    pub cx: &'x ExecCx<'c>,
    pub doc_val: Val,
}

impl<'x, 'c> ExprHostImpl<'x, 'c> {
    fn to_eval(&self, v: &Value) -> crate::expr::EVal {
        crate::expr::EVal::from_json(v)
    }

    /// 逐项作用域（S§3.3）：`item.*` 从当前列表项取值。
    fn var_from_origin(&self, path: &str) -> Option<Value> {
        let root = path.split('.').next()?;
        if root != "item" {
            return None;
        }
        match &self.doc_val {
            crate::exec::value::Val::Json(v) => {
                let mut cur = v;
                for part in path.split('.').skip(1) {
                    cur = cur.get(part)?;
                }
                Some(cur.clone())
            }
            _ => None,
        }
    }
}

impl<'x, 'c> crate::expr::ExprHost for ExprHostImpl<'x, 'c> {
    fn var(&self, ns: &str, path: &str) -> Option<crate::expr::EVal> {
        let v = match ns {
            "" | "var" => self
                .cx
                .get_var(path)
                .or_else(|| self.var_from_origin(path)),
            "let" | "session" => self.cx.session.borrow().get(path).cloned(),
            "token" => self.cx.tokens.borrow().get(path).cloned().map(Value::String),
            "cookie" => self.cx.get_cookie(path).map(Value::String),
            _ => None,
        }?;
        Some(self.to_eval(&v))
    }

    fn call_declared(
        &self,
        fid: crate::expr::FnId,
        args: &[crate::expr::EVal],
        path: &str,
    ) -> Result<crate::expr::EVal, Diagnostic> {
        use crate::expr::FnId;
        let s = |i: usize| args.get(i).map(|x| x.to_str()).unwrap_or_default();
        Ok(match fid {
            FnId::Now => {
                if !self.cx.src.requires.features.contains(&"time".to_string()) {
                    return Err(Diagnostic::new(
                        Code::EUnsupported,
                        path,
                        "feature `time` 未声明",
                        "requires.features 加 \"time\"",
                    ));
                }
                crate::expr::EVal::Num(self.cx.clock.now_ms() as f64)
            }
            FnId::Rand => {
                if !self.cx.src.requires.features.contains(&"random".to_string()) {
                    return Err(Diagnostic::new(
                        Code::EUnsupported,
                        path,
                        "feature `random` 未声明",
                        "requires.features 加 \"random\"",
                    ));
                }
                crate::expr::EVal::Num(self.cx.clock.rand())
            }
            FnId::Md5 | FnId::Sha1 | FnId::Sha256 => {
                let alg = fid.name();
                let out = crate::crypto::digest(alg, s(0).as_bytes())?;
                crate::expr::EVal::Str(hex::encode(out))
            }
            FnId::Hmac => {
                // hmac(alg, key, s)
                let alg = args.first().map(|x| x.to_str()).unwrap_or_else(|| "sha256".into());
                let key = s(1);
                let data = s(2);
                let hexs = crate::crypto::hmac_hex(&alg, key.as_bytes(), data.as_bytes())?;
                crate::expr::EVal::Str(hexs)
            }
            FnId::HexEncode => crate::expr::EVal::Str(hex::encode(s(0).as_bytes())),
            FnId::HexDecode => {
                let b = hex::decode(s(0).trim()).map_err(|e| {
                    Diagnostic::new(Code::EScript, path, format!("Hex 解码失败：{e}"), "输入必须是偶数位十六进制")
                })?;
                crate::expr::EVal::Str(String::from_utf8_lossy(&b).to_string())
            }
            FnId::Base64Encode => crate::expr::EVal::Str(crate::crypto::base64_encode(s(0).as_bytes())),
            FnId::Base64Decode => {
                let b = crate::crypto::base64_decode(&s(0))?;
                crate::expr::EVal::Str(String::from_utf8_lossy(&b).to_string())
            }
            FnId::UrlEncode => {
                let cs = args.get(1).map(|x| x.to_str());
                crate::expr::EVal::Str(crate::crypto::url_encode(&s(0), cs.as_deref()))
            }
            FnId::Cookie => {
                let name = s(0);
                match self.cx.get_cookie(&name) {
                    Some(v) => crate::expr::EVal::Str(v),
                    None => crate::expr::EVal::Str(String::new()),
                }
            }
            _ => crate::expr::EVal::Null,
        })
    }
}

/// `lib.ctx` 的 CtxApi 实现（走 ExecCx 端口）。
pub struct CtxHost<'x, 'c> {
    pub cx: &'x ExecCx<'c>,
}

impl<'x, 'c> crate::script::CtxApi for CtxHost<'x, 'c> {
    fn var(&self, name: &str) -> Option<Value> {
        self.cx.get_var(name)
    }
    fn config(&self, key: &str) -> Option<Value> {
        self.cx.vars.get(key).cloned()
    }
    fn http_get(&self, url: &str, headers: &[(String, String)]) -> Result<Value, Diagnostic> {
        let req = HttpRequest {
            method: "GET".into(),
            url: url.to_string(),
            headers: headers.to_vec(),
            body: None,
            timeout_ms: self.cx.src.limits.timeout_ms,
            max_bytes: self.cx.src.limits.max_bytes,
        };
        let resp = self.cx.transport.send(&req)?;
        let (text, _, _) = encoding_rs::UTF_8.decode(&resp.body);
        Ok(Value::String(text.into_owned()))
    }
    fn http_post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<Value, Diagnostic> {
        let req = HttpRequest {
            method: "POST".into(),
            url: url.to_string(),
            headers: headers.to_vec(),
            body: Some(body.as_bytes().to_vec()),
            timeout_ms: self.cx.src.limits.timeout_ms,
            max_bytes: self.cx.src.limits.max_bytes,
        };
        let resp = self.cx.transport.send(&req)?;
        let (text, _, _) = encoding_rs::UTF_8.decode(&resp.body);
        Ok(Value::String(text.into_owned()))
    }
    fn cache_get(&self, k: &str) -> Option<Value> {
        self.cx.cache.borrow().get(k).cloned()
    }
    fn cache_set(&self, k: &str, v: Value, _ttl: i64) {
        self.cx.cache.borrow_mut().insert(k.to_string(), v);
    }
    fn session_get(&self, k: &str) -> Option<Value> {
        self.cx.session.borrow().get(k).cloned()
    }
    fn session_set(&self, k: &str, v: Value) {
        self.cx.session.borrow_mut().insert(k.to_string(), v);
    }
    fn cookie_get(&self, k: &str) -> Option<String> {
        self.cx.get_cookie(k)
    }
    fn cookie_set(&self, k: &str, v: &str) {
        self.cx.set_cookie(k, v);
    }
    fn cookie_remove(&self, k: &str) {
        self.cx.cookies.borrow_mut().retain(|(name, _)| name != k);
    }
    fn log(&self, msg: &str) {
        self.cx.trace.borrow_mut().push(TraceEntry {
            stage: self.cx.stage.borrow().clone(),
            step: "ctx.log".into(),
            rule: msg.chars().take(80).collect(),
            matched: 0,
            ms: 0,
            bridge: 0,
            sample: None,
        });
    }
    fn crypto_digest(&self, alg: &str, data: &str) -> Result<String, Diagnostic> {
        Ok(hex::encode(crate::crypto::digest(alg, data.as_bytes())?))
    }
    fn crypto_symmetric(&self, alg: &str, mode: &str, key: &str, iv: &str, data: &str) -> Result<Value, Diagnostic> {
        // 约定：iv 与 data 都可以是 base64（优先解码）或原始字节串；key 始终 UTF-8
        let iv_bytes = crate::crypto::base64_decode(iv).unwrap_or_else(|_| iv.as_bytes().to_vec());
        let raw = crate::crypto::base64_decode(data).unwrap_or_else(|_| data.as_bytes().to_vec());
        let out = crate::crypto::symmetric(alg, mode, key.as_bytes(), &iv_bytes, &raw, None)?;
        Ok(Value::String(crate::crypto::base64_encode(&out)))
    }
    fn now(&self) -> Result<i64, Diagnostic> {
        if !self.cx.src.requires.features.contains(&"time".to_string()) {
            return Err(Diagnostic::new(
                Code::EUnsupported,
                "$",
                "feature `time` 未声明",
                "requires.features 加 \"time\"",
            ));
        }
        Ok(self.cx.clock.now_ms())
    }
    fn rand(&self) -> Result<f64, Diagnostic> {
        if !self.cx.src.requires.features.contains(&"random".to_string()) {
            return Err(Diagnostic::new(
                Code::EUnsupported,
                "$",
                "feature `random` 未声明",
                "requires.features 加 \"random\"",
            ));
        }
        Ok(self.cx.clock.rand())
    }
}

/// 六层状态快照（§12.1：可快照可重放）。
pub struct VarsSnapshot {
    pub vars: BTreeMap<String, Value>,
    pub session: BTreeMap<String, Value>,
    pub tokens: BTreeMap<String, String>,
}

impl VarsSnapshot {
    pub fn take(cx: &ExecCx) -> Self {
        VarsSnapshot {
            vars: cx.vars.clone(),
            session: cx.session.borrow().clone(),
            tokens: cx.tokens.borrow().clone(),
        }
    }
}

pub type EvalVal = Val;

/// 便捷：JSON 值 → Val。
pub fn val_of(v: Value) -> Val {
    match v {
        Value::String(s) => Val::Str(s),
        Value::Array(_) | Value::Object(_) => Val::Json(v),
        Value::Number(n) => Val::Num(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => Val::Bool(b),
        Value::Null => Val::Empty,
    }
}
