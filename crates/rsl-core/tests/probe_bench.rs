//! bench 负载探针。
use rsl_core::script::{boa_engine::BoaEngine, ScriptEngine};
use serde_json::json;

struct H;
impl rsl_core::script::CtxApi for H {
    fn var(&self, _n: &str) -> Option<serde_json::Value> { None }
    fn config(&self, _k: &str) -> Option<serde_json::Value> { None }
    fn http_get(&self, _u: &str, _h: &[(String, String)]) -> Result<serde_json::Value, rsl_core::error::Diagnostic> {
        Err(rsl_core::error::Diagnostic::new(rsl_core::error::Code::ENet, "p", "no net", "—"))
    }
    fn http_post(&self, _u: &str, _b: &str, _h: &[(String, String)]) -> Result<serde_json::Value, rsl_core::error::Diagnostic> {
        Err(rsl_core::error::Diagnostic::new(rsl_core::error::Code::ENet, "p", "no net", "—"))
    }
    fn cache_get(&self, _k: &str) -> Option<serde_json::Value> { None }
    fn cache_set(&self, _k: &str, _v: serde_json::Value, _t: i64) {}
    fn session_get(&self, _k: &str) -> Option<serde_json::Value> { None }
    fn session_set(&self, _k: &str, _v: serde_json::Value) {}
    fn cookie_get(&self, _k: &str) -> Option<String> { None }
    fn cookie_set(&self, _k: &str, _v: &str) {}
    fn cookie_remove(&self, _k: &str) {}
    fn log(&self, _m: &str) {}
    fn crypto_digest(&self, alg: &str, data: &str) -> Result<String, rsl_core::error::Diagnostic> {
        let out = rsl_core::crypto::digest(alg, data.as_bytes())?;
        Ok(hex::encode(out))
    }
    fn crypto_symmetric(&self, alg: &str, mode: &str, key: &str, iv: &str, data: &str) -> Result<serde_json::Value, rsl_core::error::Diagnostic> {
        let dec = |x: &str| -> Vec<u8> {
            rsl_core::crypto::base64_decode(x).unwrap_or_else(|_| x.as_bytes().to_vec())
        };
        rsl_core::crypto::symmetric(alg, mode, &dec(key), &dec(iv), &dec(data), Some("pkcs7"))
            .map(|s| serde_json::Value::String(String::from_utf8_lossy(&s).to_string()))
    }
    fn now(&self) -> Result<i64, rsl_core::error::Diagnostic> { Ok(0) }
    fn rand(&self) -> Result<f64, rsl_core::error::Diagnostic> { Ok(0.5) }
}

#[test]
fn probe_bench_load() {
    let ctx_src = r#"
function qmDecryptBatch(ctx, items) {
  var out = [];
  for (var i = 0; i < items.length; i++) {
    var s = ctx.crypto.sym('aes-cbc', 'encrypt', items[i].key, items[i].iv, items[i].data);
    out.push(String(s).length);
  }
  return out.length;
}
"#;
    let e = BoaEngine::new("function cleanTitle(s){ return String(s||'').trim(); }", ctx_src, 1_000_000_000);
    let items = json!([
        {"key": "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
         "iv": "MDEyMzQ1Njc4OWFiY2RlZg==",
         "data": "6060606060606060606060606060606060606060606060606060606060606060"}
    ]);
    let r = e.call_ctx("qmDecryptBatch", &[&items], &H, "probe");
    println!("boa qmDecryptBatch: {r:?}");
    assert!(r.is_ok());
}
