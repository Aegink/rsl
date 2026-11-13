//! T5.1 fixture 录制：`rsl record` 落盘响应（状态码、头、体、编码），供 L2 离线重放。

use crate::error::Diagnostic;
use crate::exec::ctx::ExecCx;
use crate::ir::{CompiledSource, RequestIR, RespKind};
use crate::ports::HttpResponse;
use serde_json::json;
use std::path::Path;

/// 录制一个阶段的全部响应为 fixture JSON 文件。
pub struct RecordedResponse {
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub body_base64: String,
    pub ms: u64,
}

impl RecordedResponse {
    pub fn from(resp: &HttpResponse) -> Self {
        RecordedResponse {
            url: resp.final_url.clone(),
            status: resp.status,
            content_type: resp.header("content-type").unwrap_or("").to_string(),
            body_base64: crate::crypto::base64_encode(&resp.body),
            ms: resp.ms,
        }
    }
}

/// 录制阶段响应：构建阶段首个请求并发送，落盘 `fixtures/<name>.fixture.json`。
pub fn record_stage(
    src: &CompiledSource,
    cx: &mut ExecCx,
    stage_name: &str,
    out_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, Diagnostic> {
    let stage = src
        .stage(stage_name)
        .ok_or_else(|| Diagnostic::new(crate::error::Code::EEnum, stage_name, format!("阶段 {stage_name} 不存在"), "—"))?;
    let req = stage.request.clone();
    let Some(req) = req else {
        return Err(Diagnostic::new(
            crate::error::Code::ESchema,
            stage_name,
            "该阶段无 request（skip/direct 阶段无可录制响应）",
            "对有 request 的阶段录制",
        ));
    };
    let resp = crate::stage::send_request(src, cx, &req, &format!("stages.{stage_name}.request"))?;
    let rec = RecordedResponse::from(&resp);
    let name = format!("{}_{}", src.id.replace(['.', '/'], "_"), stage_name);
    let path = out_dir.join(format!("{name}.fixture.json"));
    let doc = json!({
        "recorded_at": cx.clock.now_ms(),
        "kind": kind_of(&req),
        "url": rec.url,
        "status": rec.status,
        "headers": { "Content-Type": rec.content_type },
        "body_base64": rec.body_base64,
    });
    std::fs::create_dir_all(out_dir).ok();
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| {
        Diagnostic::new(
            crate::error::Code::EScript,
            path.display().to_string(),
            format!("fixture 写盘失败：{e}"),
            "检查目录权限",
        )
    })?;
    Ok(vec![path])
}

fn kind_of(req: &RequestIR) -> &'static str {
    match req.response {
        RespKind::Json => "json",
        RespKind::Bytes => "bytes",
        _ => "html",
    }
}

/// 从 fixture 文件重建 HttpResponse（L2 重放）。
/// 支持两种格式：record 产物（JSON 信封含 body_base64）与裸 HTML/JSON 原始响应。
pub fn load_fixture(path: &Path) -> Result<HttpResponse, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    let is_envelope = path.extension().and_then(|e| e.to_str()) == Some("json")
        && raw.starts_with(b"{")
        && serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("body_base64").cloned())
            .is_some();
    if is_envelope {
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("fixture JSON 非法: {e}"))?;
        let body = crate::crypto::base64_decode(v["body_base64"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        return Ok(HttpResponse {
            status: v["status"].as_u64().unwrap_or(200) as u16,
            headers: vec![(
                "Content-Type".into(),
                v["headers"]["Content-Type"].as_str().unwrap_or("").to_string(),
            )],
            body,
            final_url: v["url"].as_str().unwrap_or("").to_string(),
            ms: 0,
        });
    }
    // 裸响应：按扩展名给 Content-Type
    let ct = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        _ => "text/html; charset=utf-8",
    };
    Ok(HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), ct.into())],
        body: raw,
        final_url: String::new(),
        ms: 0,
    })
}
