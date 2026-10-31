//! Estimated cost distribution from forward-adjusted daily bars and turnover.
//! This model is not actual shareholder positions or an upstream CYQ feed.
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    pub id: String,
    pub date: String,
    pub fetched_at: DateTime<Utc>,
    pub days: usize,
    pub bins: Vec<(f64, f64)>,
    pub close: f64,
    pub seed_remaining: f64,
}
impl Distribution {
    pub fn average(&self) -> f64 {
        self.bins.iter().map(|(p, w)| p * w).sum()
    }
    pub fn quantile(&self, q: f64) -> f64 {
        let mut sum = 0.;
        for &(p, w) in &self.bins {
            sum += w;
            if sum >= q {
                return p;
            }
        }
        self.bins.last().map_or(0., |b| b.0)
    }
    pub fn profit(&self) -> f64 {
        self.bins
            .iter()
            .filter(|(p, _)| *p < self.close)
            .map(|(_, w)| w)
            .sum::<f64>()
            * 100.
    }
}
#[derive(Clone)]
struct Day {
    date: String,
    low: f64,
    high: f64,
    close: f64,
    turnover: f64,
}

pub async fn fetch(http: &reqwest::Client, id: &str) -> Result<Distribution> {
    ensure!(crate::model::valid_id(id), "invalid id");
    let secid = format!("{}.{}", if id.starts_with("sh") { 1 } else { 0 }, &id[2..]);
    let end = Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(28800).unwrap())
        .format("%Y%m%d")
        .to_string();
    let value: Value = http
        .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
        .query(&[
            ("secid", secid.as_str()),
            ("klt", "101"),
            ("fqt", "1"),
            ("lmt", "240"),
            ("end", end.as_str()),
            ("fields1", "f1,f2,f3,f4,f5,f6"),
            ("fields2", "f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse(id, &value)
}
pub fn parse(id: &str, value: &Value) -> Result<Distribution> {
    ensure!(
        value["data"]["code"].as_str() == id.get(2..),
        "wrong symbol"
    );
    let rows = value["data"]["klines"]
        .as_array()
        .context("missing daily bars")?;
    let mut days = Vec::new();
    for row in rows {
        let f: Vec<_> = row
            .as_str()
            .context("invalid daily row")?
            .split(',')
            .collect();
        ensure!(f.len() >= 11, "incomplete daily row");
        let number = |i: usize| -> Result<f64> {
            let n: f64 = f[i].parse()?;
            ensure!(n.is_finite(), "nonfinite bar");
            Ok(n)
        };
        chrono::NaiveDate::parse_from_str(f[0], "%Y-%m-%d")?;
        let day = Day {
            date: f[0].into(),
            low: number(4)?,
            high: number(3)?,
            close: number(2)?,
            turnover: number(10)? / 100.,
        };
        ensure!(
            day.low > 0.
                && day.high >= day.low
                && day.close >= day.low
                && day.close <= day.high
                && day.turnover >= 0.,
            "invalid daily bar"
        );
        if let Some(previous) = days.last() {
            let previous: &Day = previous;
            ensure!(previous.date < day.date, "unordered daily bars");
        }
        days.push(day);
    }
    estimate(id, &days)
}
fn estimate(id: &str, days: &[Day]) -> Result<Distribution> {
    let Some(last) = days.last() else {
        bail!("no daily bars")
    };
    let low = days.iter().map(|d| d.low).fold(f64::INFINITY, f64::min);
    let high = days
        .iter()
        .map(|d| d.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let step = ((high - low) / 200.).max(0.0001);
    let mut weights = vec![0.; 200];
    let mut seed_remaining = 1.;
    for (index, day) in days.iter().enumerate() {
        // The first available day seeds unknown older positions. Report its residual.
        let turnover = if index == 0 { 1. } else { day.turnover.min(1.) };
        if index > 0 {
            seed_remaining *= 1. - turnover;
        }
        let mut daily = vec![0.; 200];
        let center = (day.low + day.high + day.close) / 3.;
        for (i, weight) in daily.iter_mut().enumerate() {
            let price = low + (i as f64 + 0.5) * step;
            if price >= day.low && price <= day.high && day.high > day.low {
                *weight = if price <= center {
                    (price - day.low) / (center - day.low).max(1e-12)
                } else {
                    (day.high - price) / (day.high - center).max(1e-12)
                };
            }
        }
        let mut sum: f64 = daily.iter().sum();
        if sum <= 0. {
            daily[(((day.close - low) / step) as usize).min(199)] = 1.;
            sum = 1.;
        }
        for (old, new) in weights.iter_mut().zip(daily) {
            *old = *old * (1. - turnover) + new / sum * turnover;
        }
    }
    Ok(Distribution {
        id: id.into(),
        date: last.date.clone(),
        fetched_at: Utc::now(),
        days: days.len(),
        bins: weights
            .into_iter()
            .enumerate()
            .map(|(i, w)| (low + (i as f64 + 0.5) * step, w))
            .collect(),
        close: last.close,
        seed_remaining,
    })
}
pub fn demo(id: &str) -> Distribution {
    let close = crate::market::demo_quotes(&[id.into()])[0].price.as_f64();
    let days: Vec<_> = (0..240)
        .map(|i| {
            let center = close * (0.9 + 0.06 * (i as f64 / 30.).sin());
            Day {
                date: (Utc::now().date_naive() - chrono::Duration::days(239 - i)).to_string(),
                low: center * 0.98,
                high: center * 1.02,
                close: center,
                turnover: 0.03,
            }
        })
        .collect();
    estimate(id, &days).expect("valid demo bars")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn day(price: f64, turnover: f64) -> Day {
        Day {
            date: "2026-10-30".into(),
            low: price,
            high: price,
            close: price,
            turnover,
        }
    }
    #[test]
    fn turnover_relocates_mass_and_preserves_total() {
        let d = estimate("sz000001", &[day(10., 0.1), day(20., 0.25)]).unwrap();
        assert!((d.bins.iter().map(|b| b.1).sum::<f64>() - 1.).abs() < 1e-10);
        assert!((d.seed_remaining - 0.75).abs() < 1e-10);
        assert!(d.quantile(0.7) < 11. && d.quantile(0.9) > 19.);
        let d = estimate("sz000001", &[day(10., 1.), day(20., 1.5)]).unwrap();
        assert!(d.average() > 19. && d.seed_remaining == 0.);
    }
    #[test]
    fn suspended_and_flat_prices_remain_finite() {
        let d = estimate("sz000001", &[day(10., 0.), day(10., 0.)]).unwrap();
        assert!(d.average().is_finite());
        assert!((d.bins.iter().map(|b| b.1).sum::<f64>() - 1.).abs() < 1e-10);
    }
    #[test]
    fn parses_daily_bar_field_order() {
        let value = serde_json::json!({"data":{"code":"300750","klines":[
            "2025-11-04,374.75,370.64,378.19,365.69,239391,9091842152.00,3.30,-2.09,-7.91,0.56",
            "2025-11-05,364.54,380.39,384.69,363.69,311425,11937586811.00,5.67,2.63,9.75,0.73"
        ]}});
        let d = parse("sz300750", &value).unwrap();
        assert_eq!(d.days, 2);
        assert_eq!(d.date, "2025-11-05");
        assert_eq!(d.close, 380.39);
        assert!((d.seed_remaining - 0.9927).abs() < 1e-10);
        assert!((d.bins.iter().map(|b| b.1).sum::<f64>() - 1.).abs() < 1e-10);
    }
    #[test]
    fn rejects_missing_turnover_and_wrong_symbol() {
        let v = serde_json::json!({"data":{"code":"000001","klines":["2026-10-30,10,10,11,9,1,1,1,1,1,-"]}});
        assert!(parse("sz000001", &v).is_err());
        assert!(parse("sz000002", &v).is_err());
    }
}
