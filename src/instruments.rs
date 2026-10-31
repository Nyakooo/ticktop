//! Instrument identity, index registry and the free exchange-traded fund catalog.
use crate::model::{Instrument, Price};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub const INDICES: &[(&str, &str)] = &[
    ("sh000001", "上证指数"),
    ("sz399001", "深证成指"),
    ("hkHSI", "恒生指数"),
    ("sh000688", "科创50"),
    ("sh000300", "沪深300"),
    ("sz399006", "创业板指"),
    ("sh000016", "上证50"),
    ("sh000905", "中证500"),
    ("sh000852", "中证1000"),
];
pub fn is_index(id: &str) -> bool {
    INDICES.iter().any(|(code, _)| *code == id)
}
pub fn is_etf(id: &str) -> bool {
    id.len() == 8
        && ((id.starts_with("sh")
            && ["51", "52", "53", "55", "56", "58"]
                .iter()
                .any(|prefix| id[2..].starts_with(prefix)))
            || id.starts_with("sz15"))
}
pub fn kind(id: &str) -> &'static str {
    if is_index(id) {
        "指数"
    } else if is_etf(id) {
        "ETF"
    } else {
        "股票"
    }
}
pub fn decimals(id: &str) -> usize {
    if is_etf(id) { 3 } else { 2 }
}
pub fn unit(id: &str) -> &'static str {
    if is_index(id) { "点" } else { "CNY" }
}
pub fn price(id: &str, value: Price) -> String {
    let precision = decimals(id);
    let raw = value.0.unsigned_abs();
    format!(
        "{}{}.{:0width$}",
        if value.0 < 0 { "-" } else { "" },
        raw / 10000,
        raw % 10000 / if precision == 3 { 10 } else { 100 },
        width = precision
    )
}
pub fn resolve_index(input: &str) -> Option<&'static str> {
    let input = input.trim();
    INDICES
        .iter()
        .find(|(id, name)| {
            id.eq_ignore_ascii_case(input) || *name == input || id[2..].eq_ignore_ascii_case(input)
        })
        .map(|(id, _)| *id)
}
pub fn parse_pinned(input: &str) -> Result<Vec<String>> {
    if matches!(input.trim(), "" | "无" | "none") {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for item in input
        .split(|c: char| c == ',' || c == '，' || c.is_whitespace())
        .filter(|s| !s.is_empty())
    {
        let id =
            resolve_index(item).context("常驻指数请输入支持的指数名称或代码（按 i 查看提示）")?;
        ensure!(!ids.iter().any(|s| s == id), "常驻指数不能重复");
        ids.push(id.to_string());
    }
    ensure!(ids.len() <= 6, "最多常驻 6 个指数");
    Ok(ids)
}
pub fn merge(stocks: &mut Vec<Instrument>) {
    let bundled: crate::storage::Catalog =
        serde_json::from_str(include_str!("../assets/etfs.json")).expect("validated ETF catalog");
    let mut seen: std::collections::HashSet<String> = stocks.iter().map(|s| s.id.clone()).collect();
    for stock in bundled
        .stocks
        .into_iter()
        .chain(INDICES.iter().map(|(id, name)| Instrument {
            id: (*id).into(),
            name: (*name).into(),
        }))
    {
        if seen.insert(stock.id.clone()) {
            stocks.push(stock);
        }
    }
    stocks.sort_by(|a, b| a.id.cmp(&b.id));
}
const ETF_BASE: &str = "https://vip.stock.finance.sina.com.cn/quotes_service/api/json_v2.php/";
pub async fn fetch_etfs(http: &reqwest::Client) -> Result<Vec<Instrument>> {
    let count: Value = http
        .get(format!("{ETF_BASE}Market_Center.getHQNodeStockCount"))
        .query(&[("node", "etf_hq_fund")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let count = count
        .as_str()
        .context("invalid ETF count")?
        .parse::<usize>()?;
    ensure!((1..=5000).contains(&count), "invalid ETF count");
    let mut stocks = Vec::new();
    for page in 1..=count.div_ceil(100) {
        let rows: Value = http
            .get(format!("{ETF_BASE}Market_Center.getHQNodeData"))
            .query(&[
                ("node", "etf_hq_fund"),
                ("page", &page.to_string()),
                ("num", "100"),
                ("sort", "symbol"),
                ("asc", "1"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        stocks.extend(parse_etfs(&rows)?);
        if page < count.div_ceil(100) {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    stocks.sort_by(|a, b| a.id.cmp(&b.id));
    stocks.dedup_by(|a, b| a.id == b.id);
    ensure!(stocks.len() == count, "incomplete ETF catalog");
    Ok(stocks)
}
pub fn parse_etfs(value: &Value) -> Result<Vec<Instrument>> {
    value
        .as_array()
        .context("invalid ETF list")?
        .iter()
        .map(|row| {
            let id = row["symbol"].as_str().context("missing ETF id")?;
            let name = row["name"].as_str().context("missing ETF name")?;
            ensure!(
                crate::model::valid_id(id) && is_etf(id) && !name.trim().is_empty(),
                "invalid ETF row"
            );
            Ok(Instrument {
                id: id.into(),
                name: name.into(),
            })
        })
        .collect()
}
