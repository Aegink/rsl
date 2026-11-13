//! T1.6 正则子集检查（S§3.4）：RE2 子集。反向引用 / 环视 / 递归 → E_REGEX_UNSUPPORTED（给等效改写）；
//! 其余语法错误 → E_REGEX。

use crate::error::{Code, Diagnostic};

/// 检查一段正则源。`flags` 只允许 m/s/i。
pub fn check(source: &str, flags: &str, path: &str, rule: &str) -> Result<(), Diagnostic> {
    for f in flags.chars() {
        if !matches!(f, 'm' | 's' | 'i') {
            return Err(Diagnostic::new(
                Code::ESchema,
                path,
                format!("正则 flag `{f}` 不合法"),
                "flags 只允许 m s i（仅 re: 有效）",
            )
            .rule(rule));
        }
    }
    // 不支持构造预扫描（`regex` crate 会拒，但要区分码）
    let ch: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            let n = ch[i + 1];
            if n.is_ascii_digit() && n != '0' {
                return Err(Diagnostic::new(
                    Code::ERegexUnsupported,
                    path,
                    format!("反向引用 `\\{n}` 不受支持（RE2 子集）"),
                    "改用 transform.regex 的捕获组 $1；\"第 n 个匹配\"用 transform.match",
                )
                .rule(rule));
            }
            i += 2;
            continue;
        }
        if ch[i] == '(' && i + 1 < ch.len() && ch[i + 1] == '?' {
            let rest: String = ch[i..(i + 4).min(ch.len())].iter().collect();
            if rest.starts_with("(?=")
                || rest.starts_with("(?!")
                || rest.starts_with("(?<")
                || rest.starts_with("(?R")
                || rest.starts_with("(?&")
                || rest.starts_with("(?/")
            {
                let kind = if rest.starts_with("(?<") {
                    "lookbehind"
                } else if rest.starts_with("(?=") || rest.starts_with("(?!") {
                    "lookahead"
                } else {
                    "递归"
                };
                return Err(Diagnostic::new(
                    Code::ERegexUnsupported,
                    path,
                    format!("{kind} 不受支持（RE2 子集）"),
                    "改用 transform.regex 的捕获组 $1：先匹配整段再取组，如 (?<=id=)\\d+ → id=(\\d+) + replace:$1",
                )
                .rule(rule));
            }
        }
        i += 1;
    }
    // 真编译（regex crate 天然满足 RE2 语义）
    let mut pattern = String::new();
    if flags.contains('m') {
        pattern.push_str("(?m)");
    }
    if flags.contains('s') {
        pattern.push_str("(?s)");
    }
    if flags.contains('i') {
        pattern.push_str("(?i)");
    }
    pattern.push_str(source);
    match regex::Regex::new(&pattern) {
        Ok(_) => Ok(()),
        Err(e) => Err(Diagnostic::new(
            Code::ERegex,
            path,
            format!("正则语法非法：{}", e),
            "补括号/方括号；JSON 里反斜杠双写（\\d → \\\\d）",
        )
        .rule(rule)),
    }
}

/// 判断字符串是否带 re: 前缀（replace.from 的正则形式）。
pub fn has_re_prefix(s: &str) -> bool {
    s.starts_with("re:")
}
