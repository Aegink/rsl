//! T1.10 登录步骤图校验：id 唯一（E_SCHEMA）、next.to 存在（E_REF）、Kahn 拓扑判环（E_SCHEMA + 环路径）。

use crate::error::{Code, Diagnostic};
use std::collections::{BTreeMap, VecDeque};

/// 步骤图输入：按声明顺序的 (id, next_to)。
/// 返回拓扑序（id → 执行序号）。检查失败时返回错误诊断。
pub fn check_graph(
    steps: &[(String, Option<String>)],
    path: &str,
) -> Result<BTreeMap<String, usize>, Diagnostic> {
    // id 唯一
    let mut seen = BTreeMap::new();
    for (id, _) in steps {
        if id.is_empty() {
            return Err(Diagnostic::new(
                Code::ESchema,
                path,
                "login.steps[].id 不能为空",
                "每步给一个稳定 id，如 \"cred\" \"sms\"",
            ));
        }
        if seen.contains_key(id) {
            return Err(Diagnostic::new(
                Code::ESchema,
                path,
                format!("login.steps[].id 重复：`{id}`"),
                "id 唯一，否则 next.to 不可判定（lint #10）",
            ));
        }
        seen.insert(id.clone(), steps.iter().position(|(x, _)| x == id).unwrap_or(0));
    }
    let ids: std::collections::BTreeSet<&String> = steps.iter().map(|(id, _)| id).collect();
    // next.to 存在
    for (id, to) in steps {
        if let Some(t) = to {
            if !ids.contains(t) {
                let mut legal: Vec<&String> = ids.iter().copied().collect();
                legal.sort();
                let nearest = closest(t, &legal);
                return Err(Diagnostic::new(
                    Code::ERef,
                    path,
                    format!("步骤 `{id}` 的 next.to 指向不存在的 id `{t}`"),
                    format!(
                        "合法 id：{}{}",
                        legal
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                        nearest
                            .map(|n| format!("（最近似：{n}）"))
                            .unwrap_or_default()
                    ),
                ));
            }
        }
    }
    // Kahn 拓扑
    let n = steps.len();
    let mut indegree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, (_, to)) in steps.iter().enumerate() {
        if let Some(t) = to {
            if let Some(j) = steps.iter().position(|(x, _)| x == t) {
                adj[i].push(j);
                indegree[j] += 1;
            }
        }
    }
    let mut q: VecDeque<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = BTreeMap::new();
    let mut seq = 0usize;
    while let Some(i) = q.pop_front() {
        order.insert(steps[i].0.clone(), seq);
        seq += 1;
        for &j in &adj[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                q.push_back(j);
            }
        }
    }
    if order.len() != n {
        // 有环：找出环路径
        let cycle = find_cycle(steps);
        return Err(Diagnostic::new(
            Code::ESchema,
            path,
            format!("login.steps 步骤图有环：{}", cycle.join(" → ")),
            "步骤图必须是 DAG；末步省略 next 即结束，不要指回前面的步骤",
        ));
    }
    Ok(order)
}

fn find_cycle(steps: &[(String, Option<String>)]) -> Vec<String> {
    // 从任一剩余节点沿 next.to 走，直到重复访问
    let mut visited = std::collections::BTreeSet::new();
    let mut path: Vec<String> = vec![];
    let start = steps
        .iter()
        .find(|(id, _)| !visited.contains(id))
        .map(|(id, _)| id.clone())
        .unwrap_or_default();
    let mut cur = Some(start);
    while let Some(c) = cur {
        if path.contains(&c) {
            // 截取环
            let idx = path.iter().position(|x| x == &c).unwrap_or(0);
            let mut cyc: Vec<String> = path[idx..].to_vec();
            cyc.push(c);
            return cyc;
        }
        visited.insert(c.clone());
        path.push(c.clone());
        cur = steps
            .iter()
            .find(|(id, _)| id == &c)
            .and_then(|(_, t)| t.clone());
    }
    path
}

fn closest(target: &str, ids: &[&String]) -> Option<String> {
    // 简单编辑距离最近
    let mut best: Option<(usize, &String)> = None;
    for id in ids {
        let d = edit_distance(target, id);
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, id));
        }
    }
    best.filter(|(d, _)| *d <= 3).map(|(_, id)| id.clone())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1])
            };
        }
    }
    dp[a.len()][b.len()]
}
