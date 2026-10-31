use crate::model::Instrument;
use pinyin::ToPinyinMulti;
use unicode_normalization::UnicodeNormalization;

fn normalize(s: &str) -> String {
    s.nfkc()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}
struct Entry {
    stock: Instrument,
    name: String,
    code: String,
    id: String,
    spellings: Vec<String>,
}
pub struct SearchIndex {
    entries: Vec<Entry>,
}
impl SearchIndex {
    pub fn new(stocks: &[Instrument]) -> Self {
        Self {
            entries: stocks
                .iter()
                .map(|stock| {
                    let name = normalize(&if crate::instruments::is_etf(&stock.id)
                        && !stock.name.to_uppercase().contains("ETF")
                    {
                        format!("{}ETF", stock.name)
                    } else {
                        stock.name.clone()
                    });
                    // Expand polyphonic names (银行 / 重庆 / 长江), bounded to avoid
                    // exponential growth. Both full spellings and initials are indexed.
                    let mut variants = vec![(String::new(), String::new())];
                    for c in name.chars() {
                        let mut syllables = c
                            .to_pinyin_multi()
                            .map(|m| {
                                m.into_iter()
                                    .map(|p| p.plain().to_owned())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_else(|| vec![c.to_string()]);
                        syllables.sort();
                        syllables.dedup();
                        variants = variants
                            .iter()
                            .flat_map(|(full, initials)| {
                                syllables.iter().map(move |s| {
                                    (
                                        format!("{full}{s}"),
                                        format!("{initials}{}", s.chars().next().unwrap_or(c)),
                                    )
                                })
                            })
                            .take(32)
                            .collect();
                    }
                    let mut spellings: Vec<_> = variants
                        .into_iter()
                        .flat_map(|(full, initials)| [full, initials])
                        .collect();
                    spellings.sort();
                    spellings.dedup();
                    Entry {
                        stock: stock.clone(),
                        code: normalize(stock.code()),
                        id: normalize(&stock.id),
                        name,
                        spellings,
                    }
                })
                .collect(),
        }
    }
    pub fn search(&self, query: &str, limit: usize) -> Vec<Instrument> {
        let q = normalize(query);
        if q.is_empty() {
            return Vec::new();
        }
        let qlen = q.chars().count();
        let numeric = q.bytes().all(|b| b.is_ascii_digit());
        let mut scored: Vec<_> = self
            .entries
            .iter()
            .filter_map(|e| {
                let fields = [
                    e.code.as_str(),
                    e.name.as_str(),
                    e.id.as_str(),
                    if crate::instruments::is_etf(&e.stock.id) {
                        "etf"
                    } else {
                        crate::instruments::kind(&e.stock.id)
                    },
                ]
                .into_iter()
                .chain(e.spellings.iter().map(String::as_str));
                let score = fields
                    .clone()
                    .filter_map(|s| {
                        if s == q {
                            Some(0)
                        } else if s.starts_with(&q) {
                            Some(10)
                        } else {
                            s.find(&q).map(|i| 30 + i.min(20))
                        }
                    })
                    .min()
                    .or_else(|| {
                        if qlen < 3 || numeric {
                            return None;
                        }
                        let max = if qlen >= 7 { 2 } else { 1 };
                        fields
                            .skip(1)
                            .filter_map(|s| {
                                if s.chars().count().abs_diff(qlen) > max {
                                    return None;
                                }
                                let d = strsim::levenshtein(s, &q);
                                (d <= max).then_some(80 + d)
                            })
                            .min()
                    })?;
                Some((score, &e.stock))
            })
            .collect();
        scored.sort_unstable_by(|a, b| (a.0, &a.1.id).cmp(&(b.0, &b.1.id)));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, s)| s.clone())
            .collect()
    }
}
