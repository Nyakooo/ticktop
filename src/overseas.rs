//! Foreign market data. Session identity comes from source intervals, never a fixed UTC offset.
use crate::model::Period;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub const INDICES: [(&str, &str); 4] = [
    ("^N225", "日经225"),
    ("^KS11", "韩国综合"),
    ("^IXIC", "纳斯达克综合"),
    ("^GSPC", "标普500"),
];
pub const STOCKS: [(&str, &str); 12] = [
    ("AAPL", "苹果"),
    ("NVDA", "英伟达"),
    ("TSLA", "特斯拉"),
    ("MSFT", "微软"),
    ("AMZN", "亚马逊"),
    ("GOOG", "谷歌"),
    ("META", "Meta"),
    ("AMD", "超微半导体"),
    ("INTC", "英特尔"),
    ("BABA", "阿里巴巴"),
    ("PDD", "拼多多"),
    ("BRK-B", "伯克希尔B"),
];
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    #[default]
    Watch,
    Indices,
    Us,
    Heatmap,
}
impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Self::Watch => "自选",
            Self::Indices => "指数",
            Self::Us => "美股",
            Self::Heatmap => "云图",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Session {
    #[default]
    All,
    Pre,
    Regular,
    Post,
    Overnight,
}
impl Session {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Pre,
        Self::Regular,
        Self::Post,
        Self::Overnight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全时段",
            Self::Pre => "盘前",
            Self::Regular => "盘中",
            Self::Post => "盘后",
            Self::Overnight => "夜盘",
        }
    }
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % 5]
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    pub page: Page,
    pub return_page: Page,
    pub index_selected: String,
    pub index_all: bool,
    pub index_detail: bool,
    pub index_watchlist: Vec<String>,
    pub index_pinned: Vec<String>,
    pub index_period: Period,
    pub us_selected: Option<String>,
    pub us_period: Period,
    pub session: Session,
    pub subscriptions: Vec<String>,
    pub groups: Vec<crate::storage::Group>,
    pub group: u64,
    pub aliases: BTreeMap<String, String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            page: Page::Watch,
            return_page: Page::Watch,
            index_selected: "^N225".into(),
            index_all: true,
            index_detail: false,
            index_watchlist: ["sh000001", "hkHSI", "^N225", "^KS11", "^IXIC", "^GSPC"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            index_pinned: vec![],
            index_period: Period::Intraday,
            us_selected: Some("AAPL".into()),
            us_period: Period::Intraday,
            session: Session::All,
            subscriptions: vec!["AAPL".into(), "NVDA".into(), "TSLA".into()],
            groups: vec![],
            group: 0,
            aliases: BTreeMap::new(),
        }
    }
}
pub fn valid_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 15
        && s.as_bytes()[0].is_ascii_uppercase()
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}
pub fn name(id: &str) -> &str {
    INDICES
        .iter()
        .chain(crate::instruments::INDICES.iter())
        .chain(STOCKS.iter())
        .find(|(s, _)| *s == id)
        .map_or(id, |(_, n)| *n)
}
impl State {
    pub fn visible(&self) -> Vec<String> {
        if self.group == 0 {
            self.subscriptions.clone()
        } else {
            self.groups
                .iter()
                .find(|g| g.id == self.group)
                .map(|g| g.symbols.clone())
                .unwrap_or_default()
        }
    }
    pub fn selected(&self) -> Option<&str> {
        if self.page == Page::Indices {
            if self.index_rows().is_empty() {
                None
            } else {
                Some(&self.index_selected)
            }
        } else {
            self.us_selected.as_deref()
        }
    }
    pub fn period(&self) -> Period {
        if self.page == Page::Indices {
            self.index_period
        } else {
            self.us_period
        }
    }
    pub fn ensure_selection(&mut self) {
        self.ensure_index_selection();
        let rows = self.visible();
        if !self.us_selected.as_ref().is_some_and(|s| rows.contains(s)) {
            self.us_selected = rows.first().cloned();
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_indices()?;
        if !crate::index_board::contains(&self.index_selected) {
            bail!("无效海外指数");
        }
        let mut seen = HashSet::new();
        if self.subscriptions.len() > 100
            || self
                .subscriptions
                .iter()
                .any(|s| !valid_symbol(s) || !seen.insert(s))
        {
            bail!("美股订阅代码无效、重复或超过100只");
        }
        if self
            .us_selected
            .as_ref()
            .is_some_and(|s| !self.subscriptions.contains(s))
        {
            bail!("美股选择不在订阅列表");
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for g in &self.groups {
            let mut symbols = HashSet::new();
            if g.id == 0
                || !ids.insert(g.id)
                || !valid_label(&g.name)
                || !names.insert(&g.name)
                || g.symbols
                    .iter()
                    .any(|s| !self.subscriptions.contains(s) || !symbols.insert(s))
            {
                bail!("美股分组无效");
            }
        }
        if self.group != 0 && !ids.contains(&self.group) {
            bail!("美股分组不存在");
        }
        for (id, alias) in &self.aliases {
            if !(self.subscriptions.contains(id) || crate::index_board::contains(id))
                || !valid_label(alias)
            {
                bail!("海外别名无效");
            }
        }
        Ok(())
    }
    pub fn group_named(&mut self, name: &str) -> Result<u64> {
        let name = name.trim();
        if !valid_label(name) {
            bail!("名称需为1–24个可见字符");
        }
        if let Some(g) = self.groups.iter().find(|g| g.name == name) {
            return Ok(g.id);
        }
        let id = self
            .groups
            .iter()
            .map(|g| g.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .context("分组过多")?;
        self.groups.push(crate::storage::Group {
            id,
            name: name.into(),
            symbols: vec![],
        });
        Ok(id)
    }
}
pub fn valid_label(s: &str) -> bool {
    !s.trim().is_empty()
        && s.chars().count() <= 24
        && !s.chars().any(|c| c.is_control())
        && unicode_width::UnicodeWidthStr::width(s) > 0
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub ids: Vec<String>,
    pub focus: Option<String>,
    pub period: Period,
    pub us: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub price: f64,
    pub reference: Option<f64>,
    pub time: i64,
    pub session: Session,
    pub source: String,
}
impl Quote {
    pub fn percent(&self) -> Option<f64> {
        self.reference
            .filter(|v| *v > 0.)
            .map(|v| (self.price / v - 1.) * 100.)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    pub time: i64,
    pub close: f64,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub volume: Option<f64>,
    pub session: Session,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interval {
    pub start: i64,
    pub end: i64,
    pub session: Session,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub period: Period,
    pub regular: Quote,
    pub phases: BTreeMap<Session, Quote>,
    pub bars: Vec<Bar>,
    pub intervals: Vec<Interval>,
    pub received_at: DateTime<Utc>,
}
impl Snapshot {
    pub fn quote(&self, session: Session) -> Option<&Quote> {
        if session == Session::All {
            self.phases
                .values()
                .chain(std::iter::once(&self.regular))
                .max_by_key(|q| (q.time, q.session))
        } else if session == Session::Regular {
            Some(&self.regular)
        } else {
            self.phases.get(&session)
        }
    }
    pub fn bars(&self, session: Session) -> Vec<&Bar> {
        self.bars
            .iter()
            .filter(|b| session == Session::All || b.session == session)
            .collect()
    }
}
fn positive(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite() && *n > 0. && *n < 1e12)
}
fn raw(v: &Value) -> &Value {
    v.get("raw").unwrap_or(v)
}
fn intervals(value: &Value, session: Session, out: &mut Vec<Interval>) {
    if let (Some(start), Some(end)) = (value["start"].as_i64(), value["end"].as_i64()) {
        if start < end {
            out.push(Interval {
                start,
                end,
                session,
            });
        }
        return;
    }
    if let Some(rows) = value.as_array() {
        for row in rows {
            intervals(row, session, out);
        }
    }
}
pub fn parse_chart(value: &Value, id: &str, period: Period) -> Result<Snapshot> {
    if !valid_symbol(id) && !INDICES.iter().any(|(s, _)| *s == id) {
        bail!("invalid foreign symbol");
    }
    let a = &value["chart"]["result"][0];
    if !a.is_object() {
        bail!("empty chart");
    }
    let meta = &a["meta"];
    if meta["symbol"].as_str() != Some(id) {
        bail!("symbol mismatch");
    }
    let expected = if id.starts_with('^') {
        "INDEX"
    } else {
        "EQUITY"
    };
    if meta["instrumentType"].as_str() != Some(expected) {
        bail!("not a supported stock/index");
    }
    if expected == "EQUITY" && meta["currency"].as_str() != Some("USD") {
        bail!("not a USD equity");
    }
    let regular_price = positive(&meta["regularMarketPrice"]).context("missing price")?;
    let time = meta["regularMarketTime"]
        .as_i64()
        .filter(|t| DateTime::from_timestamp(*t, 0).is_some())
        .context("missing source time")?;
    let reference = positive(&meta["previousClose"])
        .or_else(|| positive(&meta["regularMarketPreviousClose"]))
        .or_else(|| {
            (meta["range"].as_str() == Some("1d"))
                .then(|| positive(&meta["chartPreviousClose"]))
                .flatten()
        })
        .or_else(|| {
            meta["regularMarketChangePercent"]
                .as_f64()
                .filter(|p| p.is_finite() && *p > -100.)
                .map(|p| regular_price / (1. + p / 100.))
        });
    let regular = Quote {
        price: regular_price,
        reference,
        time,
        session: Session::Regular,
        source: "Yahoo".into(),
    };
    let mut windows = vec![];
    for (key, session) in [
        ("pre", Session::Pre),
        ("regular", Session::Regular),
        ("post", Session::Post),
    ] {
        if let Some(v) = a
            .get("tradingPeriods")
            .or_else(|| meta.get("tradingPeriods"))
        {
            intervals(&v[key], session, &mut windows);
        }
        intervals(&meta["currentTradingPeriod"][key], session, &mut windows);
    }
    windows.sort_by_key(|w| (w.start, w.end));
    windows.dedup_by_key(|w| (w.start, w.end, w.session));
    let timestamps = a
        .get("timestamp")
        .and_then(Value::as_array)
        .context("missing timestamps")?;
    let q = &a["indicators"]["quote"][0];
    let close = q["close"].as_array().context("missing closes")?;
    if timestamps.len() != close.len() {
        bail!("chart length mismatch");
    }
    let mut bars = vec![];
    let mut phases = BTreeMap::new();
    for (i, t) in timestamps.iter().enumerate() {
        let Some(t) = t
            .as_i64()
            .filter(|t| DateTime::from_timestamp(*t, 0).is_some())
        else {
            continue;
        };
        let Some(price) = positive(&close[i]) else {
            continue;
        };
        let session = if period != Period::Intraday || id.starts_with('^') {
            Session::Regular
        } else {
            let Some(w) = windows.iter().find(|w| t >= w.start && t < w.end) else {
                continue;
            };
            w.session
        };
        if bars.last().is_some_and(|b: &Bar| b.time >= t) {
            bail!("non-monotonic chart");
        }
        let field = |key: &str| q[key].get(i).and_then(positive);
        let volume = q["volume"]
            .get(i)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0. && *v < 1e18);
        let (open, high, low) = (field("open"), field("high"), field("low"));
        let valid_ohlc = match (open, high, low) {
            (Some(o), Some(h), Some(l)) => l <= o.min(price) && h >= o.max(price) && l <= h,
            _ => false,
        };
        bars.push(Bar {
            time: t,
            close: price,
            open: open.filter(|_| valid_ohlc),
            high: high.filter(|_| valid_ohlc),
            low: low.filter(|_| valid_ohlc),
            volume,
            session,
        });
        if period == Period::Intraday && session != Session::Regular {
            // Extended-hours changes are relative to the last regular close, not a multi-day chart baseline.
            phases.insert(
                session,
                Quote {
                    price,
                    reference: if session == Session::Pre && t < time {
                        reference
                    } else {
                        Some(regular_price)
                    },
                    time: t,
                    session,
                    source: "Yahoo".into(),
                },
            );
        }
    }
    Ok(Snapshot {
        id: id.into(),
        period,
        regular,
        phases,
        bars,
        intervals: windows,
        received_at: Utc::now(),
    })
}
pub async fn fetch_chart(http: &reqwest::Client, id: &str, period: Period) -> Result<Snapshot> {
    if !valid_symbol(id) && !INDICES.iter().any(|(s, _)| *s == id) {
        bail!("invalid symbol");
    }
    let mut url = reqwest::Url::parse("https://query1.finance.yahoo.com/v8/finance/chart/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid url"))?
        .pop_if_empty()
        .push(id);
    let (interval, range) = match period {
        Period::Intraday => ("1m", "1d"),
        Period::Day => ("1d", "3mo"),
        Period::Week => ("1wk", "1y"),
        Period::Month => ("1mo", "5y"),
    };
    let value = http
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Ticktop")
        .query(&[
            ("interval", interval),
            ("range", range),
            ("includePrePost", "true"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    parse_chart(&value, id, period)
}
/// Yahoo embeds its server responses as JSON scripts, whose body is itself JSON.
/// Scope extraction to exact symbols and choose the latest timestamp; related stocks and
/// differently-scaled percent fields must never leak into the selected quote.
pub fn parse_night(html: &str, id: &str) -> Result<Option<Quote>> {
    fn walk(v: &Value, id: &str, best: &mut Option<Quote>, depth: usize) {
        if depth > 18 {
            return;
        }
        match v {
            Value::Object(m) => {
                if m.get("symbol").and_then(Value::as_str) == Some(id) {
                    let p = m.get("overnightMarketPrice").and_then(|v| positive(raw(v)));
                    let t = m.get("overnightMarketTime").and_then(|v| raw(v).as_i64());
                    if let (Some(price), Some(time)) = (p, t)
                        && DateTime::from_timestamp(time, 0).is_some()
                        && best.as_ref().is_none_or(|q| time > q.time)
                    {
                        let reference = m.get("regularMarketPrice").and_then(|v| positive(raw(v)));
                        *best = Some(Quote {
                            price,
                            reference,
                            time,
                            session: Session::Overnight,
                            source: "Yahoo / Blue Ocean ATS".into(),
                        });
                    }
                }
                for v in m.values() {
                    walk(v, id, best, depth + 1);
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v, id, best, depth + 1);
                }
            }
            Value::String(s) if s.starts_with('{') || s.starts_with('[') => {
                if let Ok(v) = serde_json::from_str::<Value>(s) {
                    walk(&v, id, best, depth + 1);
                }
            }
            _ => {}
        }
    }
    if !valid_symbol(id) {
        bail!("invalid US symbol");
    }
    let mut best = None;
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        rest = &rest[start + 7..];
        let Some(start) = rest.find('>') else { break };
        rest = &rest[start + 1..];
        let Some(end) = rest.find("</script>") else {
            break;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&rest[..end]) {
            walk(&value, id, &mut best, 0);
        }
        rest = &rest[end + 9..];
    }
    Ok(best)
}
pub async fn fetch_night(http: &reqwest::Client, id: &str) -> Result<Option<Quote>> {
    if !valid_symbol(id) {
        bail!("invalid symbol");
    }
    let mut response = http
        .get(format!("https://finance.yahoo.com/quote/{id}/"))
        .header("User-Agent", "Mozilla/5.0 Ticktop")
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 4_000_000 {
            bail!("oversized quote page");
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_night(std::str::from_utf8(&bytes)?, id)
}
pub fn source_time(t: i64) -> String {
    DateTime::from_timestamp(t, 0)
        .map(|t| {
            t.with_timezone(&FixedOffset::east_opt(28800).unwrap())
                .format("%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "--".into())
}
pub fn demo(id: &str, period: Period) -> Snapshot {
    let price = match id {
        "^N225" => 62188.63,
        "^KS11" => 5683.8,
        "^IXIC" => 22000.,
        "^GSPC" => 6400.,
        "AAPL" => 225.4,
        "NVDA" => 140.2,
        "TSLA" => 250.8,
        _ => 100.,
    };
    let time = Utc::now().timestamp();
    let step = if period == Period::Intraday {
        60
    } else {
        86400
    };
    let bars = (0..60)
        .map(|i| {
            let close = price * (1. + ((i as f64) * 0.3).sin() * 0.003);
            Bar {
                time: time - (59 - i) * step,
                close,
                open: Some(close * 0.999),
                high: Some(close * 1.002),
                low: Some(close * 0.998),
                volume: Some(1000. + i as f64 * 10.),
                session: Session::Regular,
            }
        })
        .collect();
    Snapshot {
        id: id.into(),
        period,
        regular: Quote {
            price,
            reference: Some(price * 0.99),
            time,
            session: Session::Regular,
            source: "演示".into(),
        },
        phases: BTreeMap::new(),
        bars,
        intervals: vec![Interval {
            start: time - 59 * step,
            end: time + step,
            session: Session::Regular,
        }],
        received_at: Utc::now(),
    }
}

/// A one-day daily bar provides index quote metadata without downloading its minute history.
pub async fn fetch_index_quote(http: &reqwest::Client, id: &str) -> Result<Snapshot> {
    if !INDICES.iter().any(|(code, _)| *code == id) {
        bail!("unsupported foreign index");
    }
    let mut url = reqwest::Url::parse("https://query1.finance.yahoo.com/v8/finance/chart/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid url"))?
        .pop_if_empty()
        .push(id);
    let value = http
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Ticktop")
        .query(&[("interval", "1d"), ("range", "1d")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let mut quote = parse_chart(&value, id, Period::Day)?;
    quote.period = Period::Intraday;
    quote.bars.clear();
    Ok(quote)
}
