//! Same-minute A-share turnover comparison. Source amounts are cumulative CNY.
use crate::model::Price;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Comparison {
    pub date: String,
    pub previous_date: String,
    pub minute: String,
    pub current_fen: i64,
    pub previous_fen: i64,
    pub fetched_at: DateTime<Utc>,
}
impl Comparison {
    pub fn delta_yi(&self) -> f64 {
        (self.current_fen - self.previous_fen) as f64 / 10_000_000_000.
    }
}
type Days = BTreeMap<String, BTreeMap<String, i64>>;
fn parse(value: &Value, id: &str) -> Result<Days> {
    if value["code"].as_i64() != Some(0) {
        bail!("turnover unavailable");
    }
    let mut days = Days::new();
    for row in value["data"][id]["data"]
        .as_array()
        .context("missing turnover days")?
    {
        let date = row["date"].as_str().context("missing trading date")?;
        NaiveDate::parse_from_str(date, "%Y%m%d")?;
        let mut minutes = BTreeMap::new();
        let mut previous = 0;
        for sample in row["data"].as_array().context("missing turnover minutes")? {
            let fields: Vec<_> = sample
                .as_str()
                .context("invalid turnover row")?
                .split_whitespace()
                .collect();
            if fields.len() < 4 {
                bail!("missing turnover amount");
            }
            let time = chrono::NaiveTime::parse_from_str(fields[0], "%H%M")?;
            let minute = time.format("%H%M").to_string();
            if !("0930"..="1130").contains(&minute.as_str())
                && !("1300"..="1500").contains(&minute.as_str())
            {
                continue;
            }
            let amount = Price::parse(fields[3])?.0;
            if amount < previous || amount % 100 != 0 {
                bail!("invalid cumulative amount");
            }
            previous = amount;
            if minutes.insert(minute, amount / 100).is_some() {
                bail!("duplicate minute");
            }
        }
        if !minutes.is_empty() && days.insert(date.into(), minutes).is_some() {
            bail!("duplicate trading date");
        }
    }
    Ok(days)
}
/// Compare the latest shared completed minute against the immediately preceding
/// trading session from each exchange. No interpolation, estimation, or calendar guess.
pub fn compare(sh: &Value, sz: &Value) -> Result<Option<Comparison>> {
    let sh = parse(sh, "sh000002")?;
    let sz = parse(sz, "sz399107")?;
    let Some(date) = sh.keys().next_back() else {
        return Ok(None);
    };
    if sz.keys().next_back() != Some(date) {
        return Ok(None);
    }
    let Some(previous_date) = sh.range(..date.clone()).next_back().map(|(d, _)| d) else {
        return Ok(None);
    };
    if sz.range(..date.clone()).next_back().map(|(d, _)| d) != Some(previous_date) {
        return Ok(None);
    }
    let a = &sh[date];
    let b = &sz[date];
    let old_a = &sh[previous_date];
    let old_b = &sz[previous_date];
    let completed = |minutes: &BTreeMap<String, i64>, minute: &str| {
        // The last live minute can be partial. Keep only minutes followed by
        // another sample, except the final minute at the lunch/closing boundary.
        matches!(minute, "1130" | "1500")
            || minutes
                .keys()
                .next_back()
                .is_some_and(|last| last.as_str() > minute)
    };
    let minute = a.keys().rev().find(|m| {
        b.contains_key(*m)
            && old_a.contains_key(*m)
            && old_b.contains_key(*m)
            && completed(a, m)
            && completed(b, m)
    });
    let Some(minute) = minute else {
        return Ok(None);
    };
    Ok(Some(Comparison {
        date: date.clone(),
        previous_date: previous_date.clone(),
        minute: format!("{}:{}", &minute[..2], &minute[2..]),
        current_fen: a[minute]
            .checked_add(b[minute])
            .context("turnover overflow")?,
        previous_fen: old_a[minute]
            .checked_add(old_b[minute])
            .context("turnover overflow")?,
        fetched_at: Utc::now(),
    }))
}
pub async fn fetch(http: &reqwest::Client) -> Result<Option<Comparison>> {
    let mut values = Vec::new();
    for id in ["sh000002", "sz399107"] {
        values.push(
            http.get("https://web.ifzq.gtimg.cn/appstock/app/day/query")
                .query(&[("code", id)])
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?,
        );
    }
    compare(&values[0], &values[1])
}
