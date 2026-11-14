//! C5 引擎基准（TASK-BOOK-2 §3.3 C5 / §5.4）：三负载 ×（boa/quickjs）。
//! 负载取材真实站点 L3 真实链路：qmDecrypt（b64→AES-CBC 解密）、signParams（md5 链）、
//! cleanTitle（正则剥离链）。每次迭代 = 完整引擎调用（含上下文创建，同生产路径）。
//!
//! 运行：
//!   cargo bench -p rsl-core                                  # 仅 boa
//!   cargo bench -p rsl-core --features engine-quickjs        # boa + quickjs 对比
//! 报告归档 scratch/bench/。

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rsl_core::script::{set_engine, ScriptEngine};
use serde_json::{json, Value};

/// qmDecrypt 等价 ctx 负载：ctx.crypto.sym AES-CBC 解密 ×50 次。
const CTX_DECRYPT_SRC: &str = r#"
function qmDecryptBatch(ctx, items) {
  var out = [];
  for (var i = 0; i < items.length; i++) {
    var s = ctx.crypto.sym('aes-cbc', 'encrypt', items[i].key, items[i].iv, items[i].data);
    out.push(String(s).length);
  }
  return out.length;
}
"#;

/// signParams 等价 ctx 负载：4 参数排序拼接 + md5 ×1000。
const CTX_SIGN_SRC: &str = r#"
function signParams(ctx, params) {
  var acc = "";
  for (var i = 0; i < 1000; i++) {
    var keys = Object.keys(params).sort();
    var parts = [];
    for (var k = 0; k < keys.length; k++) {
      parts.push(keys[k] + "=" + params[keys[k]]);
    }
    acc += ctx.crypto.md5(parts.join("&"));
  }
  return acc.length;
}
"#;

/// cleanTitle 等价 pure 负载：正则剥离 + trim ×10000。
const PURE_CLEAN_SRC: &str = r#"
function cleanTitle(s) {
  var out = "";
  for (var i = 0; i < 10000; i++) {
    out += String(s).replace(/[《》\[\]]/g, '').replace(/\s+/g, ' ').trim().length;
  }
  return out.length;
}
"#;

fn make_items() -> Value {
    // 50 项：b64 数据 ~200 字节/项（真实站点章节 payload 量级）
    let mut items = vec![];
    for i in 0..50 {
        items.push(json!({
            "key": "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=", // 32 字节 base64
            "iv": "MDEyMzQ1Njc4OWFiY2RlZg==",                      // 16 字节 base64
            "data": "6060606060606060606060606060606060606060606060606060606060606060"
        }));
        let _ = i;
    }
    Value::Array(items)
}

fn bench_engine(c: &mut Criterion, name: &str, engine: std::rc::Rc<dyn ScriptEngine>) {
    set_engine(Some(engine.clone()));
    let items = make_items();

    c.benchmark_group(format!("{name}/qmDecrypt-50x-sym"))
        .throughput(criterion::Throughput::Elements(50))
        .bench_function("call_ctx", |b| {
            b.iter(|| {
                let args: Vec<Value> = vec![items.clone()];
                let r = engine.call_ctx(
                    "qmDecryptBatch",
                    &[black_box(&args[0])],
                    &TestHost,
                    "bench",
                );
                assert!(r.is_ok(), "qmDecrypt 失败: {:?}", r.err());
            })
        });

    let params = json!({"appId": "qm", "ts": "1699999999", "nonce": "abc123", "sign": "v1"});
    c.benchmark_group(format!("{name}/signParams-md5-1000x"))
        .throughput(criterion::Throughput::Elements(1000))
        .bench_function("call_ctx", |b| {
            b.iter(|| {
                let args: Vec<Value> = vec![params.clone()];
                let r = engine.call_ctx(
                    "signParams",
                    &[black_box(&args[0])],
                    &TestHost,
                    "bench",
                );
                assert!(r.is_ok(), "signParams 失败: {:?}", r.err());
            })
        });

    c.benchmark_group(format!("{name}/cleanTitle-10000x-regex"))
        .throughput(criterion::Throughput::Elements(10000))
        .bench_function("call_pure", |b| {
            b.iter(|| {
                let arg = json!("《斗破苍穹》第一千六百二十三章 [VIP] 大结局");
                let r = engine.call_pure("cleanTitle", &[black_box(&arg)], "bench");
                assert!(r.is_ok(), "cleanTitle 失败: {:?}", r.err());
            })
        });
}

/// 基准宿主：ctx.crypto.md5/sym 真实实现（digest 走 crypto 模块）。
struct TestHost;
impl rsl_core::script::CtxApi for TestHost {
    fn var(&self, _n: &str) -> Option<Value> { None }
    fn config(&self, _k: &str) -> Option<Value> { None }
    fn http_get(&self, _u: &str, _h: &[(String, String)]) -> Result<Value, rsl_core::error::Diagnostic> {
        Err(rsl_core::error::Diagnostic::new(
            rsl_core::error::Code::ENet, "bench", "基准不允许网络", "—"))
    }
    fn http_post(&self, _u: &str, _b: &str, _h: &[(String, String)]) -> Result<Value, rsl_core::error::Diagnostic> {
        Err(rsl_core::error::Diagnostic::new(
            rsl_core::error::Code::ENet, "bench", "基准不允许网络", "—"))
    }
    fn cache_get(&self, _k: &str) -> Option<Value> { None }
    fn cache_set(&self, _k: &str, _v: Value, _t: i64) {}
    fn session_get(&self, _k: &str) -> Option<Value> { None }
    fn session_set(&self, _k: &str, _v: Value) {}
    fn cookie_get(&self, _k: &str) -> Option<String> { None }
    fn cookie_set(&self, _k: &str, _v: &str) {}
    fn cookie_remove(&self, _k: &str) {}
    fn log(&self, _m: &str) {}
    fn crypto_digest(&self, alg: &str, data: &str) -> Result<String, rsl_core::error::Diagnostic> {
        let out = rsl_core::crypto::digest(alg, data.as_bytes())?;
        Ok(hex::encode(out))
    }
    fn crypto_symmetric(&self, alg: &str, mode: &str, key: &str, iv: &str, data: &str) -> Result<Value, rsl_core::error::Diagnostic> {
        // 与 ctx.crypto_sym 桥同构：key/iv base64 → raw（失败回退原文）
        let dec = |x: &str| -> Vec<u8> {
            rsl_core::crypto::base64_decode(x).unwrap_or_else(|_| x.as_bytes().to_vec())
        };
        rsl_core::crypto::symmetric(
            alg,
            mode,
            &dec(key),
            &dec(iv),
            &dec(data),
            Some("pkcs7"),
        )
        .map(|s| Value::String(String::from_utf8_lossy(&s).to_string()))
    }
    fn now(&self) -> Result<i64, rsl_core::error::Diagnostic> { Ok(0) }
    fn rand(&self) -> Result<f64, rsl_core::error::Diagnostic> { Ok(0.5) }
}

const CTX_SRC_ALL: &str = "/** merged */";

fn bench_boa(c: &mut Criterion) {
    let ctx = format!("{CTX_DECRYPT_SRC}
{CTX_SIGN_SRC}");
    let e = rsl_core::script::boa_engine::BoaEngine::new(PURE_CLEAN_SRC, &ctx, 1_000_000_000);
    bench_engine(c, "boa", std::rc::Rc::new(e));
}

#[cfg(feature = "engine-quickjs")]
fn bench_quickjs(c: &mut Criterion) {
    let ctx = format!("{CTX_DECRYPT_SRC}
{CTX_SIGN_SRC}");
    let e = rsl_core::script::quickjs_engine::QuickJsEngine::new(PURE_CLEAN_SRC, &ctx, 1_000_000_000);
    bench_engine(c, "quickjs", std::rc::Rc::new(e));
}

#[cfg(not(feature = "engine-quickjs"))]
fn bench_quickjs(_c: &mut Criterion) {}

criterion_group!(benches, bench_boa, bench_quickjs);
criterion_main!(benches);
