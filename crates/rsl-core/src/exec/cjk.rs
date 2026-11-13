//! OpenCC 全表简繁转换（T4 变换 `t2s`/`s2t` 的数据源）。
//! 词表：`src/data/{t2s,s2t}_{chars,phrases}.txt`（key\tvalue，key 长度降序）。
//! 匹配：短语最长匹配优先，未命中逐字查单字表，未命中原样保留。
//! 数据来源：OpenCC（BYVoid）data/dictionary，Apache-2.0 / LGPL-2.1 双许可中的字典数据。

use std::collections::HashMap;
use std::sync::OnceLock;

const T2S_CHARS: &str = include_str!("../data/t2s_chars.txt");
const T2S_PHRASES: &str = include_str!("../data/t2s_phrases.txt");
const S2T_CHARS: &str = include_str!("../data/s2t_chars.txt");
const S2T_PHRASES: &str = include_str!("../data/s2t_phrases.txt");

struct Table {
    phrases: Vec<(String, String)>, // 长度降序（最长匹配）
    chars: HashMap<char, String>,
}

fn load(phrases_src: &str, chars_src: &str) -> Table {
    let mut phrases = vec![];
    for line in phrases_src.lines() {
        let mut it = line.splitn(2, '\t');
        if let (Some(k), Some(v)) = (it.next(), it.next()) {
            if !k.is_empty() && !v.is_empty() {
                phrases.push((k.to_string(), v.to_string()));
            }
        }
    }
    phrases.sort_by_key(|(k, _)| std::cmp::Reverse(k.chars().count()));
    let mut chars = HashMap::new();
    for line in chars_src.lines() {
        let mut it = line.splitn(2, '\t');
        if let (Some(k), Some(v)) = (it.next(), it.next()) {
            if let (Some(kc), Some(vc)) = (k.chars().next(), v.chars().next()) {
                chars.insert(kc, vc.to_string());
            }
        }
    }
    Table { phrases, chars }
}

fn tables() -> (&'static Table, &'static Table) {
    static T2S: OnceLock<Table> = OnceLock::new();
    static S2T: OnceLock<Table> = OnceLock::new();
    let t2s = T2S.get_or_init(|| load(T2S_PHRASES, T2S_CHARS));
    let s2t = S2T.get_or_init(|| load(S2T_PHRASES, S2T_CHARS));
    (t2s, s2t)
}

/// 繁→简（OpenCC TSCharacters/TSPhrases 全表）。
pub fn opencc_t2s(s: &str) -> String {
    let (t2s, _) = tables();
    convert(s, t2s)
}

/// 简→繁（OpenCC STCharacters/STPhrases 全表）。
pub fn opencc_s2t(s: &str) -> String {
    let (_, s2t) = tables();
    convert(s, s2t)
}

fn convert(s: &str, table: &Table) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() * 2);
    let mut i = 0usize;
    'outer: while i < chars.len() {
        // 短语最长匹配
        for (k, v) in &table.phrases {
            let kc: Vec<char> = k.chars().collect();
            if i + kc.len() <= chars.len() && chars[i..i + kc.len()] == kc[..] {
                out.push_str(v);
                i += kc.len();
                continue 'outer;
            }
        }
        // 单字
        let c = chars[i];
        match table.chars.get(&c) {
            Some(v) => out.push_str(v),
            None => out.push(c),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencc_t2s_basics() {
        assert_eq!(opencc_t2s("聽說"), "听说");
        assert_eq!(opencc_t2s("這裡對時點說話"), "这里对时点说话");
        assert_eq!(opencc_t2s("一目瞭然"), "一目了然"); // 短语表最长匹配命中
        assert_eq!(opencc_t2s("東"), "东");
        assert_eq!(opencc_t2s("軟體"), "软体"); // 单字映射（TW 词汇转换属 tw2sp 配置）
    }

    #[test]
    fn opencc_s2t_basics() {
        assert_eq!(opencc_s2t("软件开发"), "軟件開發"); // 短语表最长匹配命中
        assert_eq!(opencc_s2t("听"), "聽");
    }

    #[test]
    fn opencc_roundtrip_sample() {
        let text = "网絡運動連區員處專業報導應該環境產經濟發現讓與兩雖後視覺聽讀寫譯還風雲電視腦辦實現場館醫院";
        let simple = opencc_t2s(text);
        assert!(!simple.contains('網'));
        assert!(!simple.contains('運'));
    }

    #[test]
    fn opencc_passthrough_ascii_and_unknown() {
        assert_eq!(opencc_t2s("Hello 世界 123"), "Hello 世界 123");
    }
}
