//! T6.4 WASM 薄壳：浏览器侧用宿主 fetch/JS 作为传输端口（内核纯计算全在 rsl-core）。
//! 接口与 C ABI 对齐：create → run_stage → free。

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn version() -> String {
    format!("rsl-core {}", rsl_core::VERSION)
}

/// 编译校验：输入源 JSON 文本，返回诊断 JSON。
#[wasm_bindgen]
pub fn compile(source_json: &str) -> String {
    let out = rsl_core::compile(source_json);
    let ok = out.ok();
    let mut diags = vec![];
    for d in &out.diagnostics {
        diags.push(serde_json::json!({
            "code": d.code.as_str(),
            "severity": d.severity.to_string(),
            "path": d.path,
            "message": d.message,
            "hint": d.hint,
        }));
    }
    serde_json::json!({ "ok": ok, "diagnostics": diags }).to_string()
}

/// WASM 引擎：fixture 传输（宿主 fetch 端口由 JS 侧实现后经 responses 注入）。
#[wasm_bindgen]
pub struct RslEngine {
    src: rsl_core::ir::CompiledSource,
    responses: Vec<rsl_core::ports::HttpResponse>,
    idx: usize,
}

#[wasm_bindgen]
impl RslEngine {
    /// 创建引擎。transport_json: {"responses":[{url,status,content_type,body_base64}]} 或 "null"。
    #[wasm_bindgen(constructor)]
    pub fn new(source_json: &str, transport_json: &str) -> Result<RslEngine, JsValue> {
        let out = rsl_core::compile(source_json);
        let src = out.source.ok_or_else(|| JsValue::from_str("编译失败"))?;
        rsl_core::script::setup_engine(&src);
        let mut responses = vec![];
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(transport_json) {
            if let Some(arr) = v["responses"].as_array() {
                for r in arr {
                    let body = rsl_core::crypto::base64_decode(r["body_base64"].as_str().unwrap_or(""))
                        .unwrap_or_default();
                    responses.push(rsl_core::ports::HttpResponse {
                        status: r["status"].as_u64().unwrap_or(200) as u16,
                        headers: vec![(
                            "Content-Type".into(),
                            r["content_type"].as_str().unwrap_or("text/html; charset=utf-8").into(),
                        )],
                        body,
                        final_url: r["url"].as_str().unwrap_or("").into(),
                        ms: 0,
                    });
                }
            }
        }
        Ok(RslEngine { src, responses, idx: 0 })
    }

    /// 执行阶段。inputs_json: {"kw","page","book","chapter"}。
    /// 返回 {"ok":true,"out":…} 或 {"ok":false,"error":{code,path,message}}。
    pub fn run_stage(&mut self, stage: &str, inputs_json: &str) -> Result<String, JsValue> {
        let transport = rsl_core::ports::FixtureTransport {
            responses: self.responses.clone(),
            idx: std::sync::atomic::AtomicUsize::new(self.idx),
        };
        let clock = rsl_core::ports::SystemClock;
        let cookies = rsl_core::ports::MemoryCookieStore::default();
        let inputs_v: serde_json::Value =
            serde_json::from_str(inputs_json).unwrap_or(serde_json::Value::Null);
        let bmap = |k: &str| -> std::collections::BTreeMap<String, serde_json::Value> {
            inputs_v[k]
                .as_object()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default()
        };
        let inputs = rsl_core::stage::RunInputs {
            kw: inputs_v["kw"].as_str().map(String::from),
            page: inputs_v["page"].as_i64(),
            book: bmap("book"),
            chapter: bmap("chapter"),
            ..Default::default()
        };
        let mut cx = rsl_core::exec::ctx::ExecCx::new(&self.src, &transport, &clock);
        cx.cookie_store = Some(&cookies);
        let mut reuse = rsl_core::stage::ResponseReuse::default();
        match rsl_core::stage::run_stage(&self.src, stage, &mut cx, &inputs, &mut reuse, &rsl_core::stage::HostCaps::default()) {
            Ok(r) => {
                self.idx = transport.idx.load(std::sync::atomic::Ordering::SeqCst);
                Ok(serde_json::json!({ "ok": true, "out": r.out }).to_string())
            }
            Err(d) => Ok(serde_json::json!({
                "ok": false,
                "error": { "code": d.code.as_str(), "path": d.path, "message": d.message },
            }).to_string()),
        }
    }
}
