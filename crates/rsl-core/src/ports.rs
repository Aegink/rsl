//! 端口抽象（T2.1"HTTP 客户端抽象 + 注入点"）：传输、WebView、Cookie、时钟/随机。
//! 内核只依赖端口；CLI 提供 ureq 实现，WASM/宿主提供 fetch/JS 实现——三端一致的前提。

use crate::error::Diagnostic;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: i64,
    /// maxBytes 预算（响应超限按 on 策略）。
    pub max_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: String,
    /// 实际耗时（ms）——trace 用。
    pub ms: u64,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        let lname = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lname)
            .map(|(_, v)| v.as_str())
    }
}

pub trait Transport: Send + Sync {
    /// 发送请求。网络层失败 → E_NET；超时 → E_TIMEOUT；非 2xx 由调用方映射 E_HTTP。
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, Diagnostic>;
}

#[derive(Debug, Clone)]
pub struct WebViewRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub web_js: Option<String>,
    pub sniff: Option<String>,
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WebViewResult {
    /// 渲染后的 DOM/内容（webJs 有返回值时是其返回值，否则是 outerHTML）。
    pub body: String,
    /// 嗅探到的资源 URL（网络请求日志匹配 sniff 正则）。
    pub sniffed: Vec<String>,
}

pub trait WebViewPort: Send + Sync {
    fn render(&self, req: &WebViewRequest) -> Result<WebViewResult, Diagnostic>;
}

/// Cookie 存储（登录态隔离键由宿主保管，内核按键取值）。
pub trait CookieStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str, persist: bool);
    fn remove(&self, key: &str);
}

/// 统一毫秒时钟入口：wasm32 走 js_sys Date（std::time 在该目标 panic）。
pub fn now_ms() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// sleep：native 用 thread::sleep；wasm32 无法阻塞（事件循环），跳过。
pub fn sleep_ms(_ms: u64) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::sleep(std::time::Duration::from_millis(_ms));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = _ms;
    }
}

/// 时钟与随机（确定性测试注入固定值；S§7.3）。
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
    fn rand(&self) -> f64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        now_ms()
    }
    fn rand(&self) -> f64 {
        // 不依赖外部 crate：用纳秒熵；rand() 只在声明 random feature 后可用
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        (n % 1_000_000) as f64 / 1_000_000.0
    }
}

pub struct FixedClock {
    pub now: i64,
    pub seed: u64,
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.now
    }
    fn rand(&self) -> f64 {
        // xorshift（确定性）
        let mut x = self.seed | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 1_000_000) as f64 / 1_000_000.0
    }
}

/// 内存 Cookie 存储（CLI/测试用）。
#[derive(Default)]
pub struct MemoryCookieStore {
    pub map: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

/// 协作式取消令牌（S§12.5 / TASK-BOOK-2 A5）：宿主持有句柄，可跨线程置位；
/// generation 记录取消轮次（复位后递增），供宿主区分多轮执行。
#[derive(Clone, Default)]
pub struct CancelToken {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    /// 复位（新一轮执行前）；generation 保留历史轮次。
    pub fn reset(&self) {
        self.flag.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl CookieStore for MemoryCookieStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()
    }
    fn set(&self, key: &str, value: &str, _persist: bool) {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), value.to_string());
    }
    fn remove(&self, key: &str) {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }
}

/// 内存传输（L2 fixture 重放）。
pub struct FixtureTransport {
    pub responses: Vec<HttpResponse>,
    pub idx: std::sync::atomic::AtomicUsize,
}

impl Transport for FixtureTransport {
    fn send(&self, _req: &HttpRequest) -> Result<HttpResponse, Diagnostic> {
        let i = self
            .idx
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self
            .responses
            .get(i)
            .or_else(|| self.responses.last())
            .cloned()
            .unwrap_or(HttpResponse {
                status: 200,
                headers: vec![],
                body: vec![],
                final_url: String::new(),
                ms: 0,
            }))
    }
}
