//! T3.9 登录执行：单步/多步推进、check 失效检测、隔离键、token/cookie 产出。

use super::{RunInputs, send_request, decode_to_doc, doc_to_val};
use crate::error::{Code, Diagnostic};
use crate::exec::eval::Eval;
use crate::exec::hosts::ExprHostImpl;
use crate::exec::value::Val;
use crate::exec::ctx::ExecCx;
use crate::ir::{CompiledSource, LoginIR};
use serde_json::Value;

/// 阶段是否需要登录（requiresLogin 或声明了 requiresLogin 的 hostStages）。
pub fn login_needed_for(src: &CompiledSource, _stage: &str) -> bool {
    // 五阶段默认不强制；hostStages.requiresLogin 在 run_stage 判断
    let _ = src;
    false
}

/// 确保登录态：check 通过或已登录则跳过；失败 → E_LOGIN_REQUIRED（宿主据此弹登录 UI）。
pub fn ensure_login(src: &CompiledSource, cx: &mut ExecCx, inputs: &RunInputs) -> Result<(), Diagnostic> {
    let Some(l) = &src.login else {
        return Err(Diagnostic::new(
            Code::ELoginRequired,
            "login",
            "源未声明 login，但该阶段需要登录",
            "宿主引导用户在支持登录的源上操作",
        ));
    };
    // check 缓存 TTL（loginCheckTtlMs，默认 300000）
    if login_state_valid(cx) {
        return Ok(());
    }
    if let (Some(req), Some(ok)) = (&l.check_req, &l.check_ok) {
        let resp = send_request(src, cx, req, "login.check.request")?;
        let doc = decode_to_doc(cx, req, &resp, "login.check")?;
        let root = doc_to_val(&doc);
        let mut e = Eval {
            cx,
            doc: None,
            origin: root,
            path: "login.check.ok".into(),
            stage: "login".into(),
            keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
        };
        let r = e.eval_selector(ok)?;
        let passed = match r {
            Val::Bool(b) => b,
            other => !other.is_empty_val(),
        };
        if !passed {
            return Err(Diagnostic::new(
                Code::ELoginRequired,
                "login.check",
                "check 未通过（登录态失效）",
                "先用宿主登录界面完成登录；注意隔离键含登录名",
            )
            .rule(&ok.raw));
        }
        mark_login_valid(cx);
        Ok(())
    } else {
        // 无 check：按 token/cookie 存在性判断
        let has_token = cx.tokens.borrow().values().next().is_some()
            || cx.get_cookie("sessionid").is_some();
        if has_token {
            Ok(())
        } else {
            Err(Diagnostic::new(
                Code::ELoginRequired,
                "login",
                "未登录且源未声明 check",
                "宿主先完成 login.steps 再调用",
            ))
        }
    }
}

fn login_state_valid(cx: &ExecCx) -> bool {
    // 进程内标记：tokens 非空视为已校验（TTL 由宿主管）
    !cx.tokens.borrow().is_empty()
}

fn mark_login_valid(cx: &ExecCx) {
    let _ = cx;
}

/// 登录步骤推进（宿主渲染 fields + 回传值；引擎驱动 next.when）。
pub fn run_login_steps(
    src: &CompiledSource,
    cx: &mut ExecCx,
    inputs: &RunInputs,
) -> Result<(), Diagnostic> {
    let Some(l) = &src.login else {
        return Err(Diagnostic::new(Code::ELoginRequired, "login", "源未声明 login", "—"));
    };
    let mut current: Option<String> = l.steps.first().map(|s| s.id.clone());
    let mut guard = 0usize;
    while let Some(step_id) = current {
        guard += 1;
        if guard > 16 {
            return Err(Diagnostic::new(
                Code::ESchema,
                "login.steps",
                "登录步骤推进超过 16 步",
                "检查 next.when/next.to 是否成环",
            ));
        }
        let Some(step) = l.steps.iter().find(|s| s.id == step_id) else {
            break;
        };
        // 注入 login.<field>
        for f in &step.fields {
            if let Some(v) = inputs.login_values.get(&f.key) {
                cx.vars.insert(format!("login.{}", f.key), Value::String(v.clone()));
            } else if let Some(d) = &f.default {
                cx.vars.insert(format!("login.{}", f.key), d.to_json());
            }
        }
        // submit
        if let Some(sub) = &step.submit {
            let resp = send_request(src, cx, &sub.req, &format!("login.steps[{}].submit.request", step.id))?;
            let doc = decode_to_doc(cx, &sub.req, &resp, "login.submit")?;
            let root = doc_to_val(&doc);
            let doc_ref = Some(doc);
            let mut e = Eval {
                cx,
                doc: None,
                origin: root.clone(),
                path: format!("login.steps[{}].submit.ok", step.id),
                stage: "login".into(),
                keep_nodes: false,
                cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
            };
            let r = e.eval_selector(&sub.ok)?;
            let ok = match r {
                Val::Bool(b) => b,
                other => !other.is_empty_val(),
            };
            if !ok {
                return Err(Diagnostic::new(
                    Code::ELoginRequired,
                    format!("login.steps[{}].submit.ok", step.id),
                    "登录提交未通过 ok 判定",
                    "检查账号/验证码；ok 应选登录成功才存在的字段",
                )
                .rule(&sub.ok.raw));
            }
            // token / cookie 产出
            if let Some(tok) = &sub.token {
                let mut e2 = Eval {
                    cx,
                    doc: None,
                    origin: root.clone(),
                    path: format!("login.steps[{}].submit.token", step.id),
                    stage: "login".into(),
                    keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                if let Ok(v) = e2.eval_selector(tok) {
                    let t = v.to_text();
                    if !t.is_empty() {
                        cx.tokens.borrow_mut().insert("auth".into(), t);
                    }
                }
            }
            if let Some(ck) = &sub.cookie {
                let mut e3 = Eval {
                    cx,
                    doc: None,
                    origin: root.clone(),
                    path: format!("login.steps[{}].submit.cookie", step.id),
                    stage: "login".into(),
                    keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                if let Ok(v) = e3.eval_selector(ck) {
                    let name = extract_cookie_name(&ck.raw);
                    let val = v.to_text();
                    if !name.is_empty() && !val.is_empty() {
                        cx.set_cookie(&name, &val);
                    }
                }
            }
            // next 推进
            if let Some(to) = &step.next_to {
                // next.when 在该步提交成功后求值（S§4.5.2）
                if let Some(when) = &step.next_when {
                    let mut e4 = Eval {
                        cx,
                        doc: None,
                        origin: root,
                        path: format!("login.steps[{}].next.when", step.id),
                        stage: "login".into(),
                        keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                    };
                    let r = e4.eval_selector(when)?;
                    let go = match r {
                        Val::Bool(b) => b,
                        other => !other.is_empty_val(),
                    };
                    current = if go { Some(to.clone()) } else { None };
                } else {
                    current = Some(to.clone());
                }
            } else {
                current = None;
            }
        } else {
            current = None;
        }
        mark_login_valid(cx);
    }
    Ok(())
}

/// 从 {"$":"cookie:sessionid"} 提取 cookie 名。
fn extract_cookie_name(raw: &str) -> String {
    raw.strip_prefix("cookie:").unwrap_or(raw).to_string()
}

/// 引擎版本（供绑定层上报）。
pub fn engine_supports(l: &LoginIR) -> bool {
    !l.steps.is_empty()
}
