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
pub const STOCKS: [(&str, &str); 13] = [
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
    ("MU", "美光"),
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
    pub us_pinned: BTreeMap<u64, Vec<String>>,
    pub us_names: BTreeMap<String, String>,
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
            us_pinned: BTreeMap::new(),
            us_names: BTreeMap::new(),
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
        let mut rows = if self.group == 0 {
            self.subscriptions.clone()
        } else {
            self.groups
                .iter()
                .find(|g| g.id == self.group)
                .map(|g| g.symbols.clone())
                .unwrap_or_default()
        };
        rows.sort_by_key(|id| !self.us_is_pinned(id));
        rows
    }
    pub fn us_is_pinned(&self, id: &str) -> bool {
        self.us_pinned
            .get(&self.group)
            .is_some_and(|ids| ids.iter().any(|s| s == id))
    }
    pub fn toggle_us_pin(&mut self, id: &str) -> Result<()> {
        if !self.visible().iter().any(|s| s == id) {
            bail!("标的不在当前美股分组");
        }
        let ids = self.us_pinned.entry(self.group).or_default();
        if ids.iter().any(|s| s == id) {
            ids.retain(|s| s != id);
        } else {
            ids.push(id.into());
        }
        Ok(())
    }
    pub fn move_us_symbol(&mut self, id: &str, down: bool) -> Result<()> {
        let visible = self.visible();
        let i = visible
            .iter()
            .position(|s| s == id)
            .context("标的不在当前美股分组")?;
        let next = if down { i + 1 } else { i.saturating_sub(1) };
        if next == i || next >= visible.len() {
            return Ok(());
        }
        if self.us_is_pinned(id) != self.us_is_pinned(&visible[next]) {
            bail!("置顶项与普通项分别排序，按 t 切换置顶");
        }
        let rows = if self.group == 0 {
            &mut self.subscriptions
        } else {
            &mut self
                .groups
                .iter_mut()
                .find(|g| g.id == self.group)
                .context("美股分组不存在")?
                .symbols
        };
        let a = rows.iter().position(|s| s == id).unwrap();
        let b = rows.iter().position(|s| s == &visible[next]).unwrap();
        rows.swap(a, b);
        Ok(())
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
        for (id, name) in &self.us_names {
            if !self.subscriptions.contains(id)
                || name.trim().is_empty()
                || name.chars().count() > 128
                || name.chars().any(char::is_control)
            {
                bail!("美股名称无效");
            }
        }
        for (group, pins) in &self.us_pinned {
            let members = if *group == 0 {
                &self.subscriptions
            } else {
                &self
                    .groups
                    .iter()
                    .find(|g| g.id == *group)
                    .context("美股置顶分组不存在")?
                    .symbols
            };
            let mut seen = HashSet::new();
            if pins
                .iter()
                .any(|id| !members.contains(id) || !seen.insert(id))
            {
                bail!("美股置顶标的无效或重复");
            }
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
            let quotes = || self.phases.values().chain(std::iter::once(&self.regular));
            let latest = quotes().max_by_key(|q| (q.time, q.session))?;
            quotes()
                .filter(|q| q.source == "Nasdaq" && q.time >= latest.time - 300)
                .max_by_key(|q| q.time)
                .or(Some(latest))
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
/// Select only quotes belonging to the current New York trading window.
pub fn current_us_session(now: DateTime<Utc>) -> Option<Session> {
    use chrono::{Datelike, Timelike, Weekday};
    let local = now.with_timezone(&chrono_tz::America::New_York);
    let day = local.weekday();
    let minutes = local.hour() * 60 + local.minute();
    let weekday = !matches!(day, Weekday::Sat | Weekday::Sun);
    match minutes {
        0..240 if weekday => Some(Session::Overnight),
        240..570 if weekday => Some(Session::Pre),
        570..960 if weekday => Some(Session::Regular),
        960..1200 if weekday => Some(Session::Post),
        1200..1440 if !matches!(day, Weekday::Fri | Weekday::Sat) => Some(Session::Overnight),
        _ => None,
    }
}

pub(crate) fn us_session_at(snapshot: Option<&Snapshot>, now: DateTime<Utc>) -> Option<Session> {
    snapshot
        .and_then(|s| {
            s.intervals
                .iter()
                .find(|w| (w.start..w.end).contains(&now.timestamp()))
        })
        .map(|w| w.session)
        .or_else(|| current_us_session(now))
}

fn new_york_date(time: i64) -> Option<chrono::NaiveDate> {
    DateTime::from_timestamp(time, 0).map(|time| {
        time.with_timezone(&chrono_tz::America::New_York)
            .date_naive()
    })
}

pub fn current_us_quote<'a>(
    snapshot: Option<&'a Snapshot>,
    night: Option<&'a Quote>,
    now: DateTime<Utc>,
) -> Option<&'a Quote> {
    let session = us_session_at(snapshot, now)?;
    let candidate = if session == Session::Overnight {
        night
    } else {
        snapshot.and_then(|s| s.quote(session))
    }?;
    let in_source_window = candidate.session == session
        && candidate.time <= now.timestamp()
        && snapshot.is_some_and(|s| {
            s.intervals.iter().any(|w| {
                w.session == session
                    && (w.start..w.end).contains(&now.timestamp())
                    && (w.start..w.end).contains(&candidate.time)
            })
        });
    (in_source_window || current_us_quote_in_window(candidate, session, now)).then_some(candidate)
}

fn current_us_quote_in_window(candidate: &Quote, session: Session, now: DateTime<Utc>) -> bool {
    use chrono::Timelike;
    if candidate.session != session || candidate.time > now.timestamp() {
        return false;
    }
    let local = now.with_timezone(&chrono_tz::America::New_York);
    let Some(quoted) = DateTime::from_timestamp(candidate.time, 0) else {
        return false;
    };
    let quoted = quoted.with_timezone(&chrono_tz::America::New_York);
    let same_day = quoted.date_naive() == local.date_naive();
    match session {
        Session::Pre => {
            same_day && (4 * 60..9 * 60 + 30).contains(&(quoted.hour() * 60 + quoted.minute()))
        }
        Session::Regular => {
            same_day && (9 * 60 + 30..16 * 60).contains(&(quoted.hour() * 60 + quoted.minute()))
        }
        Session::Post => {
            same_day && (16 * 60..20 * 60).contains(&(quoted.hour() * 60 + quoted.minute()))
        }
        Session::Overnight => {
            let age = now.timestamp() - candidate.time;
            age < 8 * 3600 && (quoted.hour() >= 20 || quoted.hour() < 4)
        }
        Session::All => false,
    }
}
/// Fall back to the last known trade, preserving its original session and time.
pub fn display_us_quote<'a>(
    snapshot: Option<&'a Snapshot>,
    night: Option<&'a Quote>,
    now: DateTime<Utc>,
) -> Option<&'a Quote> {
    current_us_quote(snapshot, night, now).or_else(|| {
        snapshot
            .into_iter()
            .flat_map(|s| s.phases.values().chain(std::iter::once(&s.regular)))
            .chain(night.filter(|q| {
                DateTime::from_timestamp(q.time, 0).and_then(current_us_session)
                    == Some(Session::Overnight)
            }))
            .filter(|q| q.time <= now.timestamp())
            .max_by_key(|q| q.time)
    })
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
    let q = &a["indicators"]["quote"][0];
    let (timestamps, close): (&[Value], &[Value]) =
        match (a["timestamp"].as_array(), q["close"].as_array()) {
            (Some(timestamps), Some(close)) => (timestamps, close),
            (None, _)
                if a["timestamp"].is_null()
                    && expected == "EQUITY"
                    && period == Period::Intraday =>
            {
                (&[], &[])
            }
            _ => bail!("missing chart timestamps or closes"),
        };
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
                    reference: match (session, new_york_date(t) == new_york_date(time)) {
                        (Session::Pre, true) => reference,
                        (Session::Pre, false) if t > time => Some(regular_price),
                        (Session::Post, true) => Some(regular_price),
                        _ => None,
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
/// Nasdaq's website quote contains a price, change and timestamp for the active session.
pub fn parse_nasdaq_quote(value: &Value, id: &str) -> Result<Quote> {
    use chrono::TimeZone;
    let data = &value["data"];
    if data["symbol"].as_str() != Some(id) {
        bail!("symbol mismatch");
    }
    let session = match data["marketStatus"].as_str().unwrap_or("") {
        "Pre-Market" => Session::Pre,
        "After-Hours" => Session::Post,
        "Market Open" | "Market Closed" | "Closed" => Session::Regular,
        _ => bail!("unknown market status"),
    };
    let row = &data["primaryData"];
    let number = |key: &str| -> Option<f64> {
        row[key]
            .as_str()?
            .replace(['$', ',', '%'], "")
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
    };
    let price = number("lastSalePrice")
        .filter(|n| *n > 0.)
        .context("missing price")?;
    let change = number("netChange").context("missing change")?;
    let reference = (price - change > 0.)
        .then_some(price - change)
        .context("invalid reference")?;
    let timestamp = row["lastTradeTimestamp"]
        .as_str()
        .context("missing timestamp")?
        .trim_start_matches("Closed at ")
        .trim_end_matches(" ET");
    let local = chrono::NaiveDateTime::parse_from_str(timestamp, "%b %d, %Y %l:%M %p")?;
    let time = chrono_tz::America::New_York
        .from_local_datetime(&local)
        .single()
        .context("ambiguous source time")?
        .timestamp();
    Ok(Quote {
        price,
        reference: Some(reference),
        time,
        session,
        source: "Nasdaq".into(),
    })
}

pub fn parse_nasdaq_extended(value: &Value, session: Session) -> Result<Quote> {
    use chrono::TimeZone;
    let data = &value["data"];
    let updated = data["lastUpdateInfo"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .find_map(|row| row.strip_prefix("Data last updated ")?.strip_suffix(" ET."))
        })
        .context("missing extended session date")?;
    let updated = chrono::NaiveDateTime::parse_from_str(updated, "%b %d, %Y %l:%M %p")?;
    let row = &data["tradeDetailTable"]["rows"][0];
    let traded = chrono::NaiveTime::parse_from_str(
        row["time"]
            .as_str()
            .context("missing extended trade time")?,
        "%H:%M:%S",
    )?;
    let time = chrono_tz::America::New_York
        .from_local_datetime(&updated.date().and_time(traded))
        .single()
        .context("ambiguous source time")?
        .timestamp();
    anyhow::ensure!(
        current_us_session(DateTime::from_timestamp(time, 0).context("invalid source time")?)
            == Some(session),
        "trade outside extended session"
    );
    let number = |value: &str| value.replace(['$', ','], "").parse::<f64>().ok();
    let price = row["price"]
        .as_str()
        .and_then(number)
        .filter(|p| p.is_finite() && *p > 0.)
        .context("missing extended trade price")?;
    let reference = data["previousInfo"]
        .as_str()
        .and_then(|text| text.rsplit_once('$'))
        .and_then(|(_, price)| number(price.trim()))
        .or_else(|| {
            let mut fields = data["infoTable"]["rows"][0]["consolidated"]
                .as_str()?
                .split_whitespace();
            let price = number(fields.next()?)?;
            let change = number(fields.next()?)?;
            Some(price - change)
        })
        .filter(|p| p.is_finite() && *p > 0.);
    Ok(Quote {
        price,
        reference,
        time,
        session,
        source: "Nasdaq".into(),
    })
}

pub(crate) fn merge_us_quote(snapshot: &mut Snapshot, mut quote: Quote) {
    if quote.session == Session::Post
        && quote.reference.is_none()
        && snapshot.regular.time <= quote.time
        && new_york_date(snapshot.regular.time) == new_york_date(quote.time)
    {
        quote.reference = Some(snapshot.regular.price);
    }
    if quote.time > Utc::now().timestamp() {
        return;
    }
    if snapshot
        .quote(quote.session)
        .is_some_and(|existing| existing.time > quote.time)
    {
        return;
    }
    if quote.session == Session::Regular {
        snapshot.regular = quote;
    } else {
        snapshot.phases.insert(quote.session, quote);
    }
}

async fn nasdaq_data(
    http: &reqwest::Client,
    id: &str,
    endpoint: &str,
    market: Option<&str>,
) -> Result<Value> {
    if !valid_symbol(id) {
        bail!("invalid symbol");
    }
    let mut url = reqwest::Url::parse("https://api.nasdaq.com/api/quote/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid url"))?
        .pop_if_empty()
        .push(id)
        .push(endpoint);
    url.query_pairs_mut().append_pair("assetclass", "stocks");
    if let Some(market) = market {
        url.query_pairs_mut().append_pair("markettype", market);
    }
    Ok(http
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .header("Accept", "application/json")
        .header("Origin", "https://www.nasdaq.com")
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?)
}

pub async fn fetch_nasdaq_quote(http: &reqwest::Client, id: &str) -> Result<Quote> {
    let session = current_us_session(Utc::now());
    let market = match session {
        Some(Session::Pre) => Some("pre"),
        Some(Session::Post) => Some("post"),
        _ => None,
    };
    let info = nasdaq_data(http, id, "info", None)
        .await
        .and_then(|data| parse_nasdaq_quote(&data, id));
    if let (Some(session), Some(market)) = (session, market) {
        if info.as_ref().is_ok_and(|quote| {
            current_us_quote_in_window(quote, session, Utc::now())
                && Utc::now().timestamp() - quote.time <= 180
        }) {
            return info;
        }
        if let Ok(data) = nasdaq_data(http, id, "extended-trading", Some(market)).await
            && let Ok(quote) = parse_nasdaq_extended(&data, session)
            && current_us_quote_in_window(&quote, session, Utc::now())
            && !info.as_ref().is_ok_and(|primary| {
                current_us_quote_in_window(primary, session, Utc::now())
                    && primary.time >= quote.time
            })
        {
            return Ok(quote);
        }
    }
    info
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
