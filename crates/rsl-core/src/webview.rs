//! T2.7 WebView 端口：engine http/webview/auto、autoUpgradeIf 只升级一次、webJs 空返回 E_SCRIPT、
//! sniff 产出 var:sniffed。缺能力 → E_UNSUPPORTED（features:["webview"]）。

use crate::error::{Code, Diagnostic};
use crate::ports::{WebViewPort, WebViewRequest, WebViewResult};

/// engine 判定：auto = 先 HTTP，命中 autoUpgradeIf 再升级一次（S§9.3）。
pub enum EngineDecision {
    Http,
    WebView,
}

pub fn decide_auto(
    upgrade_if: Option<&crate::ir::SelectorIR>,
    doc_html: Option<&str>,
) -> EngineDecision {
    // autoUpgradeIf 在 HTTP 响应上求值：命中 → 升级（只升一次）
    if let Some(sel) = upgrade_if {
        if let crate::ir::BaseSel::Css(c) = &sel.base {
            if let Some(html) = doc_html {
                let d = crate::exec::value::HtmlDoc::parse(html);
                if let Ok(ids) = crate::selector::select_css(&d.html, None, c) {
                    if !ids.is_empty() {
                        return EngineDecision::WebView;
                    }
                }
            }
        }
    }
    EngineDecision::Http
}

pub fn render(
    port: &dyn WebViewPort,
    url: &str,
    headers: &[(String, String)],
    web_js: Option<&str>,
    sniff: Option<&str>,
    timeout_ms: i64,
) -> Result<WebViewResult, Diagnostic> {
    let r = port.render(&WebViewRequest {
        url: url.to_string(),
        headers: headers.to_vec(),
        web_js: web_js.map(String::from),
        sniff: sniff.map(String::from),
        timeout_ms,
    })?;
    // webJs 必须有非空返回（S§4.8），否则 E_SCRIPT
    if let Some(js) = web_js {
        if !js.is_empty() && r.body.trim().is_empty() {
            return Err(Diagnostic::new(
                Code::EScript,
                "request.webJs",
                "webJs 返回值为空",
                "webJs 必须返回非空值（历史上空返回导致死循环）",
            ));
        }
    }
    Ok(r)
}

/// 缺 webview 能力时的错误。
pub fn unsupported(path: &str) -> Diagnostic {
    Diagnostic::new(
        Code::EUnsupported,
        path,
        "宿主无 webview 能力",
        "requires.features 去掉 \"webview\"，或给 engine 改 \"http\"",
    )
}
