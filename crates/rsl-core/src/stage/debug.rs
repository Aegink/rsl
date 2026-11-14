//! rsl-debug/1 调试产物（TASK-BOOK-4 T1）：单次阶段执行的四数据面——
//! 请求（协议标头）/ 响应（原始数据预览）/ 解析结果（out）/ 诊断+trace。
//! schema/trace.schema.json（rsl-trace/1）为同体例先例：独立 schema 文件 + 版本冻结（V4-D5）。
//! 响应体仅内存预览 ≤256KB，不落盘（S§17 / V4-D4）。

use crate::exec::ctx::{DebugRespInfo, ExecCx};

use crate::error::Diagnostic;
use crate::http::decode_response;
use crate::ir::{CompiledSource, RequestIR, RespKind};
use crate::ports::HttpResponse;
use crate::sugar::Seg;
use crate::stage::{RunInputs, StageResult};
use serde_json::{json, Value};

/// 响应体预览预算（V4-D4）：>256KB 只留前缀 + truncated 标记。
pub const DEBUG_PREVIEW_MAX: usize = 256 * 1024;

/// Seg 列表 → 模板展示串（原始地址形态）：`Lit` 原样、`Tmpl` 为 `${ns:expr}`。
pub fn segs_display(segs: &[Seg]) -> String {
    let mut s = String::new();
    for seg in segs {
        match seg {
            Seg::Lit(l) => s.push_str(l),
            Seg::Tmpl { ns, expr } => {
                if ns.is_empty() {
                    s.push_str(&format!("$({expr})"));
                } else {
                    s.push_str(&format!("${{{ns}:{expr}}}"));
                }
            }
        }
    }
    s
}

/// 请求模板形态（原始地址）：url 缺省时为 baseUrl。
pub fn request_template(req: &RequestIR, base_url: &str) -> String {
    match &req.url {
        Some(segs) => segs_display(segs),
        None => base_url.to_string(),
    }
}

/// 字符串截断到 max 字节（不留半个字符）。
fn trunc_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// 响应侧捕获：sha256 + 解码预览（≤256KB；bytes 声明只记哈希不预览）。
pub fn capture_response(req: &RequestIR, resp: &HttpResponse, detect: bool, path: &str) -> DebugRespInfo {
    let body_bytes = resp.body.len();
    let sha256 = crate::crypto::sha256_hex(&resp.body);
    let truncated = body_bytes > DEBUG_PREVIEW_MAX;
    let binary = req.response == RespKind::Bytes;
    let preview = if binary {
        None
    } else {
        let text = decode_response(
            &resp.body,
            req.charset.as_deref(),
            resp.header("content-type"),
            detect,
            path,
        )
        // 解码失败（charset 判定失误）时 lossy 兜底——调试器要展示真实数据而非空白
        .unwrap_or_else(|_| String::from_utf8_lossy(&resp.body).into_owned());
        Some(trunc_str(&text, DEBUG_PREVIEW_MAX))
    };
    DebugRespInfo {
        status: resp.status,
        headers: resp.headers.clone(),
        ms: resp.ms,
        final_url: resp.final_url.clone(),
        body_bytes,
        sha256,
        truncated,
        binary,
        preview,
    }
}

/// 组装 rsl-debug/1 产物（CLI/serve 共用；失败阶段也产出——诊断直通 T7）。
#[allow(clippy::too_many_arguments)]
pub fn build_artifact(
    src: &CompiledSource,
    source_path: &str,
    stage_name: &str,
    cx: &ExecCx,
    inputs: &RunInputs,
    result: &Result<StageResult, Diagnostic>,
    elapsed_ms: u64,
) -> Value {
    let records = cx.debug.borrow().clone();
    let last_req = records
        .last()
        .map(|r| serde_json::to_value(&r.request).unwrap_or(Value::Null))
        .unwrap_or(Value::Null);
    let last_resp = records
        .last()
        .and_then(|r| r.response.as_ref())
        .map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
        .unwrap_or(Value::Null);
    let requests: Vec<Value> = records
        .iter()
        .map(|r| {
            json!({
                "request": serde_json::to_value(&r.request).unwrap_or(Value::Null),
                "response": r.response.as_ref().map(|v| serde_json::to_value(v).unwrap_or(Value::Null)).unwrap_or(Value::Null),
            })
        })
        .collect();
    let (ok, out, partial) = match result {
        Ok(r) => (true, r.out.clone(), r.partial),
        Err(_) => (false, Value::Null, false),
    };
    let mut inputs_echo = serde_json::Map::new();
    if let Some(kw) = &inputs.kw {
        inputs_echo.insert("kw".into(), Value::String(kw.clone()));
    }
    if let Some(page) = inputs.page {
        inputs_echo.insert("page".into(), json!(page));
    }
    if !inputs.book.is_empty() {
        inputs_echo.insert(
            "book".into(),
            Value::Object(inputs.book.clone().into_iter().collect()),
        );
    }
    if !inputs.chapter.is_empty() {
        inputs_echo.insert(
            "chapter".into(),
            Value::Object(inputs.chapter.clone().into_iter().collect()),
        );
    }
    if let Some(g) = &inputs.group {
        inputs_echo.insert("group".into(), g.clone());
    }
    if !inputs.filter.is_empty() {
        inputs_echo.insert(
            "filter".into(),
            Value::Object(inputs.filter.clone().into_iter().collect()),
        );
    }
    let mut doc = json!({
        "schema": "rsl-debug/1",
        "source": source_path,
        "sourceId": src.id,
        "stage": stage_name,
        "ok": ok,
        "elapsed_ms": elapsed_ms,
        "inputs": Value::Object(inputs_echo),
        "request": last_req,
        "response": last_resp,
        "requests": requests,
        "out": out,
        "partial": partial,
        "trace": cx.trace.borrow().iter().collect::<Vec<_>>(),
        "diagnostics": cx.warnings.borrow().iter().collect::<Vec<_>>(),
    });
    if let Err(d) = result {
        doc["error"] = serde_json::to_value(d).unwrap_or(Value::Null);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segs_display_template_forms() {
        let segs = vec![
            Seg::Lit("https://a.example.com/s?q=".into()),
            Seg::Tmpl { ns: "var".into(), expr: "kw".into() },
        ];
        assert_eq!(segs_display(&segs), "https://a.example.com/s?q=${var:kw}");
        let bare = vec![Seg::Tmpl { ns: "".into(), expr: "page - 1".into() }];
        assert_eq!(segs_display(&bare), "$(page - 1)");
    }

    #[test]
    fn trunc_str_keeps_char_boundary() {
        let s = "斗破苍穹斗破苍穹".repeat(10);
        let t = trunc_str(&s, 13);
        // 13 字节落在第 5 个字符中间 → 回退到 12 字节 = 4 个字符
        assert!(t.chars().count() == 4, "边界回退到完整字符: {t}");
        assert!(trunc_str("abc", 10) == "abc");
    }
}
