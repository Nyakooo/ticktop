use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instrument {
    pub id: String,
    pub name: String,
}
impl Instrument {
    pub fn code(&self) -> &str {
        self.id.get(2..).unwrap_or(&self.id)
    }
    pub fn exchange(&self) -> &str {
        match self.id.get(..2) {
            Some("sh") => "沪",
            Some("sz") => "深",
            Some("bj") => "北",
            Some("hk") => "港",
            _ => "?",
        }
    }
}
pub fn valid_id(id: &str) -> bool {
    id == "hkHSI"
        || (id.len() == 8
            && matches!(id.get(..2), Some("sh" | "sz" | "bj"))
            && id
                .get(2..)
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit())))
}

/// Prices or index points in ten-thousandths. Never parse decimal prices via binary float.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(pub i64);
impl Price {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (negative, digits) = match value.strip_prefix('-') {
            Some(v) => (true, v),
            None => (false, value),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
            || frac.len() > 4
        {
            bail!("invalid decimal price");
        }
        let whole: i64 = whole.parse()?;
        let frac: i64 = if frac.is_empty() {
            0
        } else {
            frac.parse::<i64>()? * 10_i64.pow(4 - frac.len() as u32)
        };
        let raw = whole
            .checked_mul(10_000)
            .and_then(|x| x.checked_add(frac))
            .context("price overflow")?;
        Ok(Self(if negative { -raw } else { raw }))
    }
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 10_000.0
    }
}
impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = self.0.unsigned_abs();
        write!(
            f,
            "{}{}.{:02}",
            if self.0 < 0 { "-" } else { "" },
            raw / 10_000,
            raw % 10_000 / 100
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub id: String,
    pub name: String,
    pub price: Price,
    pub previous_close: Price,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub source_time: DateTime<FixedOffset>,
    pub received_at: DateTime<Utc>,
    /// Mainland volume in lots (100 shares), amount in CNY ten-thousands.
    #[serde(default)]
    pub volume_lots: Option<f64>,
    #[serde(default)]
    pub amount_wan: Option<f64>,
    #[serde(default)]
    pub turnover_percent: Option<f64>,
    #[serde(default)]
    pub volume_ratio: Option<f64>,
}
impl Quote {
    pub fn change(&self) -> Price {
        Price(self.price.0 - self.previous_close.0)
    }
    pub fn percent(&self) -> f64 {
        if self.previous_close.0 <= 0 {
            0.0
        } else {
            (self.price.0 - self.previous_close.0) as f64 / self.previous_close.0 as f64 * 100.0
        }
    }
    pub fn age_secs(&self) -> i64 {
        (Utc::now() - self.source_time.with_timezone(&Utc))
            .num_seconds()
            .max(0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    #[default]
    Intraday,
    Week,
    Month,
}
impl Period {
    pub const ALL: [Self; 3] = [Self::Intraday, Self::Week, Self::Month];
    pub fn next(self, reverse: bool) -> Self {
        let i = Self::ALL.iter().position(|x| *x == self).unwrap_or(0);
        Self::ALL[(i + if reverse { 2 } else { 1 }) % 3]
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Intraday => "日内",
            Self::Week => "1周",
            Self::Month => "1月",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Intraday => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub label: String,
    pub price: Price,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    pub period: Period,
    pub date: String,
    pub points: Vec<Point>,
    pub fetched_at: DateTime<Utc>,
}

pub fn parse_cn_time(s: &str) -> Result<DateTime<FixedOffset>> {
    let naive =
        NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M%S").context("invalid source timestamp")?;
    FixedOffset::east_opt(8 * 3600)
        .unwrap()
        .from_local_datetime(&naive)
        .single()
        .context("invalid China timestamp")
}
