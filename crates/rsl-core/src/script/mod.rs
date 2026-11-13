//! T4.4–T4.6 脚本引擎端口 + `ctx` 受控接口。裁决 D6：JS 引擎用 boa（纯 Rust，全平台一致；
//! QuickJS 为规范参考实现）。两档隔离语义不变：pure 无 ctx/无 IO；ctx 十接口 + 受预算。

#[cfg(feature = "engine-js")]
pub mod boa_engine;

use crate::error::Diagnostic;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

/// `lib.ctx` 函数可用的受控接口（S§7.2 十接口，REFERENCE §11.3 展开为 11 项）。
pub trait CtxApi {
    fn var(&self, name: &str) -> Option<Value>;
    fn config(&self, key: &str) -> Option<Value>;
    fn http_get(&self, url: &str, headers: &[(String, String)]) -> Result<Value, Diagnostic>;
    fn http_post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<Value, Diagnostic>;
    fn cache_get(&self, k: &str) -> Option<Value>;
    fn cache_set(&self, k: &str, v: Value, ttl: i64);
    fn session_get(&self, k: &str) -> Option<Value>;
    fn session_set(&self, k: &str, v: Value);
    fn cookie_get(&self, k: &str) -> Option<String>;
    fn cookie_set(&self, k: &str, v: &str);
    fn cookie_remove(&self, k: &str);
    fn log(&self, msg: &str);
    fn crypto_digest(&self, alg: &str, data: &str) -> Result<String, Diagnostic>;
    fn crypto_symmetric(&self, alg: &str, mode: &str, key: &str, iv: &str, data: &str) -> Result<Value, Diagnostic>;
    fn now(&self) -> Result<i64, Diagnostic>;
    fn rand(&self) -> Result<f64, Diagnostic>;
}

/// 两档 JS 引擎端口。
pub trait ScriptEngine {
    /// 调 lib.pure 函数（无 ctx、无 IO）。
    fn call_pure(&self, fn_name: &str, args: &[&Value], path: &str) -> Result<Value, Diagnostic>;
    /// 调 lib.ctx 函数（首参 ctx）。
    fn call_ctx(
        &self,
        fn_name: &str,
        args: &[&Value],
        host: &dyn CtxApi,
        path: &str,
    ) -> Result<Value, Diagnostic>;
    fn has_fn(&self, name: &str) -> bool;
    /// libSteps 消耗（确定性计量）。
    fn steps_used(&self) -> i64;
}

thread_local! {
    static GLOBAL_ENGINE: RefCell<Option<Rc<dyn ScriptEngine>>> = const { RefCell::new(None) };
}

/// stage/宿主 注入引擎（线程内有效；boa Context 非 Send）。
pub fn set_engine(e: Option<Rc<dyn ScriptEngine>>) {
    GLOBAL_ENGINE.with(|g| *g.borrow_mut() = e);
}

pub fn get_engine() -> Option<Rc<dyn ScriptEngine>> {
    GLOBAL_ENGINE.with(|g| g.borrow().clone())
}

/// 从编译产物创建并注入引擎（CLI/宿主入口调用一次）。
/// 无 engine-js feature（WASM）：引擎由宿主 JS 环境充当，set_engine(None)。
pub fn setup_engine(src: &crate::ir::CompiledSource) {
    #[cfg(feature = "engine-js")]
    {
        let engine = boa_engine::BoaEngine::new(
            &src.lib.pure,
            &src.lib.ctx,
            src.limits.lib_steps,
        );
        set_engine(Some(std::rc::Rc::new(engine)));
    }
    #[cfg(not(feature = "engine-js"))]
    {
        let _ = src;
        set_engine(None);
    }
}
