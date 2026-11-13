//! T6.3 JNI 桥：Kotlin RslSource ↔ rsl-core。与 C ABI 同语义（编译一次 → run_stage）。
//! 由 `cargo ndk` 交叉编译为 librsl_core.so。

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;
use std::sync::Mutex;

fn to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

struct EngineHandle {
    src: rsl_core::ir::CompiledSource,
}

unsafe fn read_str(env: &mut JNIEnv, s: &JString) -> String {
    let c: std::ffi::CString = env
        .get_string(s)
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_default()
        .into();
    c.into_string().unwrap_or_default()
}

/// 创建引擎（编译）。返回句柄；编译失败返回 0（错误经 lastError）。
#[no_mangle]
pub extern "system" fn Java_com_rsl_RslNative_nativeCreate(
    mut env: JNIEnv,
    _class: JClass,
    source_json: JString,
) -> jlong {
    let text = read_str(&mut env, &source_json);
    let out = rsl_core::compile(&text);
    match out.source {
        Some(src) => {
            rsl_core::script::setup_engine(&src);
            Box::into_raw(Box::new(EngineHandle { src })) as jlong
        }
        None => {
            LAST.with(|l| {
                *l.lock().unwrap() = out
                    .errors()
                    .first()
                    .map(|d| format!("{}|{}|{}", d.code.as_str(), d.path, d.message))
                    .unwrap_or_default()
            });
            0
        }
    }
}

/// 执行阶段。transport_json 为 {"responses":[…]}（fixture）或 "null"（真实 HTTP）。
#[no_mangle]
pub extern "system" fn Java_com_rsl_RslNative_nativeRunStage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    stage: JString<'local>,
    inputs_json: JString<'local>,
    transport_json: JString<'local>,
) -> jstring {
    let stage_name = read_str(&mut env, &stage);
    let inputs_json = read_str(&mut env, &inputs_json);
    let transport_json = read_str(&mut env, &transport_json);
    let handle = match unsafe { (handle as *mut EngineHandle).as_mut() } {
        Some(h) => h,
        None => return to_jstring(&mut env, "null"),
    };
    let transport: Box<dyn rsl_core::ports::Transport> = if transport_json != "null" {
        match serde_json::from_str::<serde_json::Value>(&transport_json) {
            Ok(v) => {
                let mut responses = vec![];
                if let Some(arr) = v["responses"].as_array() {
                    for r in arr {
                        let body = rsl_core::crypto::base64_decode(r["body_base64"].as_str().unwrap_or(""))
                            .unwrap_or_default();
                        responses.push(rsl_core::ports::HttpResponse {
                            status: r["status"].as_u64().unwrap_or(200) as u16,
                            headers: vec![("Content-Type".into(), r["content_type"].as_str().unwrap_or("text/html; charset=utf-8").into())],
                            body,
                            final_url: r["url"].as_str().unwrap_or("").into(),
                            ms: 0,
                        });
                    }
                }
                Box::new(rsl_core::ports::FixtureTransport { responses, idx: Default::default() })
            }
            Err(_) => Box::new(rsl_core::http::UreqTransport::new(&handle.src.http)),
        }
    } else {
        Box::new(rsl_core::http::UreqTransport::new(&handle.src.http))
    };
    let clock = rsl_core::ports::SystemClock;
    let cookies = rsl_core::ports::MemoryCookieStore::default();
    let inputs_v: serde_json::Value = serde_json::from_str(&inputs_json).unwrap_or(serde_json::Value::Null);
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
    let mut cx = rsl_core::exec::ctx::ExecCx::new(&handle.src, transport.as_ref(), &clock);
    cx.cookie_store = Some(&cookies);
    let mut reuse = rsl_core::stage::ResponseReuse::default();
    match rsl_core::stage::run_stage(&handle.src, &stage_name, &mut cx, &inputs, &mut reuse, &rsl_core::stage::HostCaps::default()) {
        Ok(r) => to_jstring(&mut env, &serde_json::to_string(&r.out).unwrap_or_default()),
        Err(d) => {
            LAST.with(|l| {
                *l.lock().unwrap() = format!("{}|{}|{}", d.code.as_str(), d.path, d.message);
            });
            to_jstring(&mut env, "null")
        }
    }
}

/// 释放引擎句柄。
#[no_mangle]
pub extern "system" fn Java_com_rsl_RslNative_nativeFree(_env: JNIEnv, _class: JClass, handle: jlong) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut EngineHandle));
        }
    }
}

/// 最后错误："code|path|message"。
#[no_mangle]
pub extern "system" fn Java_com_rsl_RslNative_lastError(mut env: JNIEnv) -> jstring {
    let msg = LAST.with(|l| l.lock().unwrap().clone()).unwrap_or_default();
    to_jstring(&mut env, &msg)
}

thread_local! {
    static LAST: Mutex<Option<String>> = const { Mutex::new(None) };
}
