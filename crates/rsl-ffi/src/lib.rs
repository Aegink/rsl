//! T6.1 C ABI：宿主（Swift/Kotlin）经 `rsl.h` 调内核。
//! 句柄模型：`rsl_engine_new` 创建 → `rsl_engine_run_stage` 执行 → `rsl_string_free` 释放。
//! 字符串一律 UTF-8；返回 JSON 文本；`NULL` 表示失败（错误经 `rsl_last_error` 取）。

use std::ffi::{c_char, CStr, CString};
use std::sync::Mutex;

fn to_cs(s: String) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

unsafe fn from_cs<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// 每线程最后错误（C ABI 无 Result；诊断经此传出）。
thread_local! {
    static LAST_ERROR: Mutex<Option<String>> = const { Mutex::new(None) };
}

fn set_err(d: &rsl_core::error::Diagnostic) {
    LAST_ERROR.with(|l| {
        *l.lock().unwrap() = Some(format!("{}|{}|{}", d.code.as_str(), d.path, d.message));
    });
}

fn take_err() -> String {
    LAST_ERROR.with(|l| l.lock().unwrap().take().unwrap_or_else(|| "unknown".into()))
}

/// 引擎句柄：持有编译产物 + 注入点。
pub struct Engine {
    pub src: rsl_core::ir::CompiledSource,
    pub transport: Box<dyn rsl_core::ports::Transport>,
    pub clock: Box<dyn rsl_core::ports::Clock>,
    pub cookies: rsl_core::ports::MemoryCookieStore,
}

impl Engine {
    fn new_cx(&mut self) -> rsl_core::exec::ctx::ExecCx<'_> {
        let mut cx = rsl_core::exec::ctx::ExecCx::new(&self.src, self.transport.as_ref(), self.clock.as_ref());
        cx.cookie_store = Some(&self.cookies);
        cx
    }
}

/// 创建引擎（编译一份源）。失败返回 NULL。
/// `transport_json`：`{"responses":[{"url","status","content_type","body_base64"}]}` = fixture 传输；
/// `null` = 真实 HTTP。
#[no_mangle]
pub extern "C" fn rsl_engine_new(source_json: *const c_char, transport_json: *const c_char) -> *mut Engine {
    let text = match unsafe { from_cs(source_json) } {
        Some(t) => t,
        None => return std::ptr::null_mut(),
    };
    let out = rsl_core::compile(text);
    let Some(src) = out.source else {
        let msg = out
            .errors()
            .first()
            .map(|d| format!("{}|{}|{}", d.code.as_str(), d.path, d.message))
            .unwrap_or_default();
        LAST_ERROR.with(|l| *l.lock().unwrap() = Some(msg));
        return std::ptr::null_mut();
    };
    rsl_core::script::setup_engine(&src);
    let transport: Box<dyn rsl_core::ports::Transport> = match unsafe { from_cs(transport_json) } {
        Some(tj) if !tj.is_empty() && tj != "null" => {
            match serde_json::from_str::<serde_json::Value>(tj) {
                Ok(v) => {
                    let mut responses = vec![];
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
                    Box::new(rsl_core::ports::FixtureTransport {
                        responses,
                        idx: Default::default(),
                    })
                }
                Err(_) => Box::new(rsl_core::http::UreqTransport::new(&src.http)),
            }
        }
        _ => Box::new(rsl_core::http::UreqTransport::new(&src.http)),
    };
    Box::into_raw(Box::new(Engine {
        src,
        transport,
        clock: Box::new(rsl_core::ports::SystemClock),
        cookies: rsl_core::ports::MemoryCookieStore::default(),
    }))
}

/// 执行一个阶段。`inputs_json`: {"kw","page","book":{…},"chapter":{…}}。返回 JSON 结果或 NULL。
#[no_mangle]
pub unsafe extern "C" fn rsl_engine_run_stage(
    engine: *mut Engine,
    stage: *const c_char,
    inputs_json: *const c_char,
) -> *mut c_char {
    let Some(eng) = engine.as_mut() else {
        return std::ptr::null_mut();
    };
    let Some(stage_name) = from_cs(stage) else {
        return std::ptr::null_mut();
    };
    let inputs_v: serde_json::Value = from_cs(inputs_json)
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
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
    let src_ptr: *const rsl_core::ir::CompiledSource = &eng.src;
    let mut cx = eng.new_cx();
    let mut reuse = rsl_core::stage::ResponseReuse::default();
    let src_ref = unsafe { &*src_ptr };
    match rsl_core::stage::run_stage(src_ref, stage_name, &mut cx, &inputs, &mut reuse, &rsl_core::stage::HostCaps::default()) {
        Ok(r) => to_cs(serde_json::to_string(&r.out).unwrap_or_default()),
        Err(d) => {
            set_err(&d);
            std::ptr::null_mut()
        }
    }
}

/// 释放引擎。
#[no_mangle]
pub unsafe extern "C" fn rsl_engine_free(engine: *mut Engine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// 编译校验（不创建引擎）。返回 JSON：{"ok":bool,"diagnostics":[…]}。
#[no_mangle]
pub extern "C" fn rsl_compile(json: *const c_char) -> *mut c_char {
    let src = match unsafe { from_cs(json) } {
        Some(t) => t,
        None => return std::ptr::null_mut(),
    };
    let out = rsl_core::compile(src);
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
    to_cs(serde_json::json!({ "ok": ok, "diagnostics": diags }).to_string())
}

/// 取最后一次错误的 `code|path|message`（engine 方法返回 NULL 时调用）。
#[no_mangle]
pub extern "C" fn rsl_last_error() -> *mut c_char {
    to_cs(take_err())
}

/// 释放 rsl_* 返回的字符串。
#[no_mangle]
pub unsafe extern "C" fn rsl_string_free(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p));
    }
}

/// 引擎版本。
#[no_mangle]
pub extern "C" fn rsl_version() -> *mut c_char {
    to_cs(format!("rsl-core {}", rsl_core::VERSION))
}
