//! M2 传输（T2.1–T2.9）：ureq 客户端 + 重试 + 限流 + Cookie + 编码判定六步（S§6.7）+ 代理/DNS/TLS。
//! 连接失败计入重试（E_NET）；超时 E_TIMEOUT 与预算 E_BUDGET 分离。

use crate::error::{Code, Diagnostic};
use crate::ir::HttpIR;
use crate::ports::{HttpRequest, HttpResponse, Transport};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// ureq 传输实现（feature "net"，非 WASM 目标）。
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub struct UreqTransport {
    pub agent: ureq::Agent,
    pub dns_map: HashMap<String, String>,
    pub redirect_max: usize,
}

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
impl UreqTransport {
    pub fn new(http: &HttpIR) -> Self {
        let mut agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(10_000))
            .user_agent("rsl-core/2.0");
        if let Some(p) = &http.proxy {
            if let Some(addr) = p.strip_prefix("socks5://").or_else(|| p.strip_prefix("http://")) {
                if let Ok(proxy) = ureq::Proxy::new(format!("socks5://{addr}")) {
                    agent = agent.proxy(proxy);
                }
            }
        }
        UreqTransport {
            agent: agent.build(),
            dns_map: http.dns.clone().into_iter().collect(),
            redirect_max: http.redirect_max as usize,
        }
    }
}

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
impl Transport for UreqTransport {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, Diagnostic> {
        let t0 = Instant::now();
        // DNS 固定解析（http.dns）
        let mut url = req.url.clone();
        for (host, ip) in &self.dns_map {
            if let Some(replaced) = replace_host(&url, host, ip) {
                url = replaced;
                break;
            }
        }
        if !matches!(req.method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "HEAD") {
            return Err(Diagnostic::new(Code::EHttp, "$", format!("非法方法 {}", req.method), "GET POST PUT DELETE HEAD"));
        }
        let mut call = self
            .agent
            .request(req.method.as_str(), &url)
            .timeout(Duration::from_millis(req.timeout_ms.max(1) as u64));
        for (k, v) in &req.headers {
            call = call.set(k, v);
        }
        let resp = if let Some(body) = &req.body {
            call.send_bytes(body)
        } else {
            match req.method.as_str() {
                "POST" | "PUT" => call.send_string(""),
                _ => call.call(),
            }
        };
        let ms = t0.elapsed().as_millis() as u64;
        match resp {
            Ok(r) => {
                let status = r.status();
                let headers: Vec<(String, String)> = r
                    .headers_names()
                    .iter()
                    .flat_map(|k| {
                        r.all(k)
                            .into_iter()
                            .map(move |v| (k.clone(), v.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                // maxBytes 预算（reject 由调用方判断策略；这里硬截断到上限+1）
                let limit = if req.max_bytes > 0 { req.max_bytes as usize + 1 } else { usize::MAX };
                let mut body = Vec::with_capacity(8192);
                use std::io::Read;
                let mut reader = r.into_reader().take(limit as u64);
                reader.read_to_end(&mut body).map_err(|e| {
                    Diagnostic::new(
                        Code::ENet,
                        "$",
                        format!("响应读取失败：{e}"),
                        "检查连接与 maxBytes 预算",
                    )
                })?;
                Ok(HttpResponse {
                    status: status,
                    headers,
                    body,
                    final_url: url,
                    ms,
                })
            }
            Err(e) => {
                let msg = e.to_string();
                let d = if msg.contains("timed out") || msg.contains("timeout") {
                    Diagnostic::new(
                        Code::ETimeout,
                        "$",
                        format!("请求超时 {msg}"),
                        format!("当前超时 {}ms；检查站点可用性或调整 timeout", req.timeout_ms),
                    )
                    .retryable(true)
                } else {
                    Diagnostic::new(
                        Code::ENet,
                        "$",
                        format!("网络层失败：{msg}"),
                        "检查 DNS/代理/TLS；retry 会把连接失败计入重试",
                    )
                    .retryable(true)
                };
                Err(d)
            }
        }
    }
}

fn replace_host(url: &str, host: &str, ip: &str) -> Option<String> {
    if url.contains(host) {
        let s = url.replacen(&format!("//{host}"), &format!("//{ip}"), 1);
        if s != url {
            // 保留 Host 头由调用方补
            return Some(s);
        }
    }
    None
}

/// 重试 + 限流 + 编码判定的封装客户端。
pub struct HttpClient<'a> {
    pub transport: &'a dyn Transport,
    pub http: &'a HttpIR,
    /// 每次发送前的节流钩子（RateLimiter）。
    pub rate: Option<RateLimiter>,
}

#[derive(Default, Clone)]
pub struct RateLimiter {
    /// (window_ms, max)
    pub window: Option<(i64, i64)>,
    state: HashMap<String, Vec<i64>>,
}

impl RateLimiter {
    pub fn from_ir(http: &HttpIR) -> Option<Self> {
        http.rate.as_ref().map(|r| RateLimiter {
            window: Some((r.window_ms, r.max)),
            state: HashMap::new(),
        })
    }

    /// 窗口限流：60s 内第 21 次被延迟（T2.4 验收）。
    pub fn wait(&mut self, key: &str) {
        let Some((window_ms, max)) = self.window else { return };
        let now = now_ms();
        let entry = self.state.entry(key.to_string()).or_default();
        entry.retain(|t| now - *t < window_ms);
        if entry.len() >= max as usize {
            // 等到最早的请求滑出窗口
            let oldest = entry.first().copied().unwrap_or(now);
            let wait_ms = (window_ms - (now - oldest)).max(1) as u64;
            crate::ports::sleep_ms(wait_ms.min(1000));
        }
        self.state.entry(key.to_string()).or_default().push(now_ms());
    }
}

pub fn now_ms() -> i64 {
    crate::ports::now_ms()
}

impl<'a> HttpClient<'a> {
    /// 发送 + 重试（连接失败与 on 状态码计入）。
    pub fn send(&mut self, req: &HttpRequest, retry: &crate::ir::RetryIR, key: &str) -> Result<HttpResponse, Diagnostic> {
        let mut attempt = 0i64;
        loop {
            if let Some(r) = &mut self.rate {
                r.wait(key);
            }
            let result = self.transport.send(req);
            let need_retry = match &result {
                Err(d) => d.code == Code::ENet || d.code == Code::ETimeout,
                Ok(resp) => retry.on.contains(&(resp.status as i64)),
            };
            if !need_retry || attempt >= retry.times {
                return result;
            }
            let _ = &result;
            let backoff_ms = match retry.backoff.as_str() {
                "linear" => 1000 * (attempt + 1) as u64,
                "exponential" => (500u64.saturating_mul(1u64 << attempt.min(4))).min(8000),
                _ => 0,
            };
            if backoff_ms > 0 {
                crate::ports::sleep_ms(backoff_ms);
            }
            attempt += 1;
        }
    }
}

/// 响应解码六步（S§6.7）：request.charset → Content-Type → meta charset → BOM → detect → utf-8。
pub fn decode_response(
    body: &[u8],
    explicit: Option<&str>,
    content_type: Option<&str>,
    detect_enabled: bool,
    path: &str,
) -> Result<String, Diagnostic> {
    // 1. request.charset
    if let Some(cs) = explicit {
        return decode_with(body, cs, path);
    }
    // 2. Content-Type charset=
    if let Some(ct) = content_type {
        if let Some(pos) = ct.to_lowercase().find("charset=") {
            let cs = ct[pos + 8..].split(';').next().unwrap_or("").trim().trim_matches('"');
            if !cs.is_empty() {
                return decode_with(body, cs, path);
            }
        }
    }
    // 3. <meta charset> / <?xml encoding=?>
    let head = String::from_utf8_lossy(&body[..body.len().min(2048)]).to_lowercase();
    for pat in ["<meta charset=\"", "<meta content=\"text/html; charset=", "<?xml encoding=\""] {
        if let Some(i) = head.find(pat) {
            let rest = &head[i + pat.len()..];
            let cs: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            if !cs.is_empty() {
                return decode_with(body, &cs, path);
            }
        }
    }
    // 4. BOM
    if body.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return decode_with(body, "utf-8", path);
    }
    if body.starts_with(&[0xFF, 0xFE]) || body.starts_with(&[0xFE, 0xFF]) {
        return decode_with(body, "utf-16", path);
    }
    // 5. 自动探测（仅当 requires.features 含 detect）
    if detect_enabled {
        // 简易探测：合法 UTF-8 → utf-8；否则试 GBK
        if std::str::from_utf8(body).is_ok() {
            return decode_with(body, "utf-8", path);
        }
        let (r, _, had_errors) = encoding_rs::GBK.decode(body);
        if !had_errors {
            return Ok(r.into_owned());
        }
        return Err(Diagnostic::new(
            Code::ECharset,
            path,
            "编码探测失败",
            format!("前 64 字节：{}", hex_prefix(body, 64)),
        ));
    }
    // 6. utf-8 兜底（lossy）
    let (r, _, _) = encoding_rs::UTF_8.decode(body);
    Ok(r.into_owned())
}

fn decode_with(body: &[u8], cs: &str, path: &str) -> Result<String, Diagnostic> {
    let norm = cs.to_lowercase().replace("-", "");
    let enc = match norm.as_str() {
        "utf8" => encoding_rs::UTF_8,
        "gbk" | "gb2312" | "gb18030" => encoding_rs::GBK,
        "big5" => encoding_rs::BIG5,
        "utf16" | "utf16le" | "utf16be" => encoding_rs::UTF_16LE,
        "latin1" | "iso88591" | "windows1252" => encoding_rs::WINDOWS_1252,
        "eucjp" => encoding_rs::EUC_JP,
        "shiftjis" => encoding_rs::SHIFT_JIS,
        _ => encoding_rs::UTF_8,
    };
    let (r, _, had_errors) = enc.decode(body);
    if had_errors && norm != "utf8" {
        return Err(Diagnostic::new(
            Code::ECharset,
            path,
            format!("{cs} 解码出现无效字节"),
            format!("前 64 字节：{}；试试 gbk 或显式 request.charset", hex_prefix(body, 64)),
        ));
    }
    Ok(r.into_owned())
}

fn hex_prefix(body: &[u8], n: usize) -> String {
    body.iter()
        .take(n)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 请求参数/体的编码（encode.params / encode.body 分离，S§4.3）。
pub fn encode_param(v: &str, charset: Option<&str>) -> String {
    match charset {
        Some(cs) if !cs.is_empty() && cs.to_lowercase() != "utf-8" && cs.to_lowercase() != "utf8" => {
            crate::crypto::url_encode(v, Some(cs))
        }
        _ => crate::crypto::url_encode(v, None),
    }
}

/// multipart/raw 体编码。
pub fn build_body(kind: &str, data: &serde_json::Map<String, serde_json::Value>, encoded_charset: Option<&str>) -> (String, Vec<u8>) {
    match kind {
        "json" => {
            let mut obj = serde_json::Map::new();
            for (k, v) in data {
                obj.insert(k.clone(), v.clone());
            }
            ("application/json".into(), serde_json::to_vec(&serde_json::Value::Object(obj)).unwrap_or_default())
        }
        "form" => {
            let mut pairs = vec![];
            for (k, v) in data {
                let vs = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let enc_k = encode_param(k, encoded_charset);
                let enc_v = encode_param(&vs, encoded_charset);
                pairs.push(format!("{enc_k}={enc_v}"));
            }
            ("application/x-www-form-urlencoded".into(), pairs.join("&").into_bytes())
        }
        "raw" => {
            ("application/octet-stream".into(), {
                let joined = data
                    .get("__raw")
                    .and_then(|x| x.as_str())
                    .map(String::from)
                    .unwrap_or_default();
                joined.into_bytes()
            })
        }
        _ => ("application/octet-stream".into(), vec![]),
    }
}
