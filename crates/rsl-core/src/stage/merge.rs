//! T3.12 多源合并（S§12.4 契约）：归一化 → 分档 → 折叠 → 跨档比较 → 排序 → 打分。
//! 宿主职责但语义固定——三端必须一致。

use crate::ir::{MergeIR, MergeIR as M};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 折叠键默认值。
pub fn default_key() -> Vec<String> {
    vec!["name".into(), "author".into()]
}

/// 归一化（S§12.4 规则 1）：trim / t2s / lower / stripPunct。
pub fn normalize_field(s: &str, ops: &[String]) -> String {
    let mut out = s.to_string();
    for op in ops {
        match op.as_str() {
            "trim" => out = out.trim().to_string(),
            "t2s" => out = crate::exec::eval::t2s(&out),
            "lower" => out = out.to_lowercase(),
            "stripPunct" => {
                out = out
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
            }
            _ => {}
        }
    }
    out
}

/// 匹配分档：exact（全相等）→ contains（互相包含）→ other。
pub fn bucket_of(a: &Value, b: &Value, key: &[String], ops: &[String]) -> &'static str {
    let fields_a = key_fields(a, key, ops);
    let fields_b = key_fields(b, key, ops);
    if fields_a == fields_b {
        return "exact";
    }
    let contains = fields_a
        .iter()
        .zip(fields_b.iter())
        .all(|(x, y)| x.contains(y.as_str()) || y.contains(x.as_str()));
    if contains {
        "contains"
    } else {
        "other"
    }
}

fn key_fields(v: &Value, key: &[String], ops: &[String]) -> Vec<String> {
    key.iter()
        .map(|k| {
            v.get(k)
                .and_then(|x| x.as_str())
                .map(|s| normalize_field(s, ops))
                .unwrap_or_default()
        })
        .collect()
}

/// 折叠键值（跨档比较用）。
fn fold_key(v: &Value, key: &[String], ops: &[String]) -> String {
    key_fields(v, key, ops).join("\u{1}")
}

/// 多源合并。entries: (来源名, 条目列表)。返回合并后的有序列表（每项含 origins/sources/score）。
pub fn merge_sources(
    entries: Vec<(String, Vec<Value>)>,
    m: &MergeIR,
) -> Vec<Value> {
    let key = if m.key.is_empty() { default_key() } else { m.key.clone() };
    let ops = if m.normalize.is_empty() { vec!["trim".to_string()] } else { m.normalize.clone() };
    let cross = m.cross_bucket;

    // 折叠：同源内 key 相等合并
    struct Item {
        origin: String,
        data: Value,
        bucket: &'static str,
        weight: f64,
    }
    let mut items: Vec<Item> = vec![];
    for (origin, list) in &entries {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for item in list {
            let k = fold_key(item, &key, &ops);
            let w = m.weight;
            if let Some(i) = seen.get(&k) {
                items[*i].origin = format!("{};{}", items[*i].origin, origin);
            } else {
                seen.insert(k, items.len());
                items.push(Item {
                    origin: origin.clone(),
                    data: item.clone(),
                    bucket: "other",
                    weight: w,
                });
            }
        }
    }
    // 分档
    for i in 0..items.len() {
        items[i].bucket = "other";
        for j in 0..items.len() {
            if i == j {
                continue;
            }
            let b = bucket_of(&items[i].data, &items[j].data, &key, &ops);
            if b == "exact" {
                items[i].bucket = "exact";
                break;
            }
        }
        if items[i].bucket == "other" {
            for j in 0..items.len() {
                if i == j {
                    continue;
                }
                if bucket_of(&items[i].data, &items[j].data, &key, &ops) == "contains" {
                    items[i].bucket = "contains";
                    break;
                }
            }
        }
    }
    // 跨档比较折叠（crossBucket: true 强制全局比较——不同档之间也按 key 合并）
    let mut merged: Vec<Item> = vec![];
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for it in items {
        let k = fold_key(&it.data, &key, &ops);
        if cross {
            if let Some(i) = index.get(&k) {
                merged[*i].origin = format!("{};{}", merged[*i].origin, it.origin);
                merged[*i].weight = merged[*i].weight.max(it.weight);
                continue;
            }
            index.insert(k, merged.len());
        }
        merged.push(it);
    }
    // 排序（S§12.4 规则 5）：档位 → sources 降序 → weight 降序 → 名称字典序
    let bucket_rank = |b: &str| -> usize {
        m.order.iter().position(|o| o == b).unwrap_or(99)
    };
    let mut rows: Vec<Value> = merged
        .into_iter()
        .map(|it| {
            let sources: Vec<&str> = it.origin.split(';').filter(|s| !s.is_empty()).collect();
            let score = bucket_rank(it.bucket) as f64 * 100.0 + sources.len() as f64 * 10.0 + it.weight;
            let mut data = it.data.clone();
            if let Value::Object(m) = &mut data {
                m.insert("origins".into(), json!(sources));
                m.insert("sources".into(), json!(sources.len()));
                m.insert("bucket".into(), json!(it.bucket));
                m.insert("score".into(), json!(score));
            }
            data
        })
        .collect();
    // 排序（S§12.4 规则 5）：档位升序 → sources 降序 → weight 降序 → 名称字典序
    let brank = |v: &Value| -> usize {
        v.get("bucket")
            .and_then(|b| b.as_str())
            .map(bucket_rank)
            .unwrap_or(99)
    };
    rows.sort_by(|a, b| {
        let (ra, rb) = (brank(a), brank(b));
        ra.cmp(&rb)
            .then_with(|| {
                let sa = a.get("sources").and_then(|x| x.as_u64()).unwrap_or(0);
                let sb = b.get("sources").and_then(|x| x.as_u64()).unwrap_or(0);
                sb.cmp(&sa)
            })
            .then_with(|| {
                let wa = a.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let wb = b.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0);
                wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .cmp(b.get("name").and_then(|x| x.as_str()).unwrap_or(""))
            })
    });
    // minScore 过滤
    if m.min_score > 0.0 {
        rows.retain(|r| r.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0) >= m.min_score);
    }
    rows
}

/// merge 断言支持：merged[0].sources == 3 之类在 testkit 用。
pub fn merged_count(rows: &[Value]) -> usize {
    rows.len()
}

// 占位导入已移除（无副作用）
