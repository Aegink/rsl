//! T1.11 merge 契约校验：key 字段必须存在于 search.out.map（E_REF）；normalize/order 枚举（E_ENUM）。

use crate::error::{Code, Diagnostic};

pub const NORMALIZE_KEYS: &[&str] = &["trim", "t2s", "lower", "stripPunct"];
pub const ORDER_KEYS: &[&str] = &["exact", "contains", "weight", "sources", "other"];

pub fn check_merge(
    key: &[String],
    normalize: &[String],
    order: &[String],
    search_map_keys: Option<&std::collections::BTreeSet<String>>,
) -> Vec<Diagnostic> {
    let mut diags = vec![];
    if let Some(keys) = search_map_keys {
        for k in key {
            if !keys.contains(k) {
                diags.push(Diagnostic::new(
                    Code::ERef,
                    "merge.key",
                    format!("merge.key 字段 `{k}` 不在 stages.search.out.map 里"),
                    format!("search.out.map 声明的字段：{}", {
                        let mut v: Vec<&String> = keys.iter().collect();
                        v.sort();
                        v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")
                    }),
                ));
            }
        }
    }
    for n in normalize {
        if !NORMALIZE_KEYS.contains(&n.as_str()) {
            diags.push(Diagnostic::new(
                Code::EEnum,
                "merge.normalize",
                format!("`{n}` 不是合法的归一化方式"),
                format!("合法值：{}", NORMALIZE_KEYS.join(" ")),
            ));
        }
    }
    for o in order {
        if !ORDER_KEYS.contains(&o.as_str()) {
            diags.push(Diagnostic::new(
                Code::EEnum,
                "merge.order",
                format!("`{o}` 不是合法的档位"),
                format!("合法值：{}", ORDER_KEYS.join(" ")),
            ));
        }
    }
    diags
}
