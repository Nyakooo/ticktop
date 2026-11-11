//! Free public data adapters. Upstream endpoints are not a contractual real-time feed.
use crate::{
    config::{Config, Network, ProxyMode},
    model::*,
    storage::Catalog,
};
use anyhow::{Context, Result, bail};
use chrono::{Datelike, FixedOffset, Utc};
use encoding_rs::GBK;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

pub const CATALOG_URL: &str = "https://www.cninfo.com.cn/new/data/szse_stock.json";
#[derive(Clone, Debug)]
pub struct Request {
    pub overseas: Option<crate::overseas::Job>,
    pub us_search: Option<(u64, String)>,
    pub config: Config,
    pub symbols: Vec<String>,
    pub focus: Option<String>,
    pub period: Period,
    pub demo: bool,
    pub catalog_as_of: String,
    pub heatmap_enabled: bool,
    pub sector_category: crate::sectors::Category,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Quotes,
    Chart,
    Catalog,
    Chips,
    Sectors,
    Turnover,
    Overseas,
    Overnight,
}
pub enum MarketEvent {
    UsSearch {
        generation: u64,
        query: String,
        rows: Vec<Instrument>,
        error: Option<String>,
    },
    Overseas(Vec<crate::overseas::Snapshot>, Vec<String>),
    Overnight(String, Option<crate::overseas::Quote>),
    Quotes(Vec<Quote>),
    Turnover(Option<crate::turnover::Comparison>),
    Chart(Series),
    Catalog(Catalog),
    Chips(crate::chips::Distribution),
    Sectors(crate::sectors::Snapshot),
    Error {
        kind: Kind,
        message: String,
        retry_secs: u64,
    },
}

pub fn client(config: &Network) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .connect_timeout(Duration::from_secs(config.timeout_secs.min(10)))
        .user_agent(concat!("ticktop/", env!("CARGO_PKG_VERSION")));
    match config.proxy_mode {
        ProxyMode::System => {
            builder = builder.no_proxy();
            for proxy in system_proxies(|name| match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => anyhow::bail!("系统代理环境变量 {name} 不是有效 UTF-8，已禁止请求"),
            })? {
                builder = builder.proxy(proxy);
            }
        }
        ProxyMode::Direct => builder = builder.no_proxy(),
        ProxyMode::Custom => {
            builder = builder.no_proxy().proxy(
                reqwest::Proxy::all(&config.proxy_url)
                    .map_err(|_| anyhow::anyhow!("代理地址无效"))?,
            )
        }
    }
    builder
        .build()
        .map_err(|_| anyhow::anyhow!("无法创建网络客户端，请检查代理配置"))
}
/// Resolve environment proxies explicitly: reqwest's automatic resolver silently
/// discards malformed variables. Reject them instead of unexpectedly going direct.
pub fn system_proxies(get: impl Fn(&str) -> Result<Option<String>>) -> Result<Vec<reqwest::Proxy>> {
    let read = |upper: &str, lower: &str| -> Result<Option<String>> {
        let value = match get(upper)? {
            Some(value) => Some(value),
            None => get(lower)?,
        };
        Ok(value.filter(|value| !value.trim().is_empty()))
    };
    let http = read("HTTP_PROXY", "http_proxy")?;
    let https = read("HTTPS_PROXY", "https_proxy")?;
    // ALL_PROXY is only applicable when a protocol-specific proxy is absent.
    // Desktop sessions may export an unsupported legacy ALL_PROXY alongside
    // valid explicit HTTP/HTTPS settings; ignoring that unused value is not a bypass.
    let all = if http.is_some() && https.is_some() {
        None
    } else {
        read("ALL_PROXY", "all_proxy")?
    };
    let excluded = read("NO_PROXY", "no_proxy")?;
    let no_proxy = excluded.as_deref().and_then(reqwest::NoProxy::from_string);
    let mut proxies = Vec::new();
    for (name, value, scheme) in [
        ("HTTP_PROXY", http, 0),
        ("HTTPS_PROXY", https, 1),
        ("ALL_PROXY", all, 2),
    ] {
        let Some(raw) = value else { continue };
        let raw = raw.trim();
        let address = if raw.contains("://") {
            raw.to_string()
        } else {
            format!("http://{raw}")
        };
        let url = reqwest::Url::parse(&address)
            .map_err(|_| anyhow::anyhow!("系统代理环境变量 {name} 无效，已禁止请求"))?;
        if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
            || url.host_str().is_none()
        {
            anyhow::bail!("系统代理环境变量 {name} 无效，已禁止请求");
        }
        let proxy = match scheme {
            0 => reqwest::Proxy::http(address),
            1 => reqwest::Proxy::https(address),
            _ => reqwest::Proxy::all(address),
        }
        .map_err(|_| anyhow::anyhow!("系统代理环境变量 {name} 无效，已禁止请求"))?;
        proxies.push(proxy.no_proxy(no_proxy.clone()));
    }
    Ok(proxies)
}
/// Deliberately omit URLs and raw upstream bodies: they can contain proxy credentials.
pub fn safe_error(error: &anyhow::Error) -> String {
    if let Some(e) = error.downcast_ref::<reqwest::Error>() {
        if e.is_timeout() {
            "请求超时，请检查网络或代理".into()
        } else if let Some(s) = e.status() {
            format!("行情源 HTTP {}，稍后重试", s.as_u16())
        } else {
            "网络连接失败，请检查代理、DNS 或行情源".into()
        }
    } else {
        "行情数据格式异常或标的暂无数据，保留已有缓存".into()
    }
}
pub fn retry_delay(base: u64, failures: u32) -> u64 {
    base.saturating_mul(1u64 << failures.min(6))
        .min(base.max(300))
}

pub async fn quotes(http: &reqwest::Client, ids: &[String]) -> Result<Vec<Quote>> {
    let mut all = Vec::new();
    for chunk in ids.chunks(80) {
        if !chunk.iter().all(|s| valid_id(s)) {
            bail!("invalid stock id");
        }
        let bytes = http
            .get("https://qt.gtimg.cn/")
            .query(&[("q", chunk.join(","))])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let (text, _, errors) = GBK.decode(&bytes);
        if errors {
            bail!("invalid quote encoding");
        }
        all.extend(parse_quotes(&text)?);
        if ids.len() > 80 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    if !ids.is_empty() && all.is_empty() {
        bail!("empty quote response");
    }
    Ok(all)
}
pub fn parse_quotes(text: &str) -> Result<Vec<Quote>> {
    let mut parsed = Vec::new();
    for line in text.split(';') {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let Some(id) = key.trim().strip_prefix("v_") else {
            continue;
        };
        if !valid_id(id) {
            continue;
        }
        let fields: Vec<_> = value.trim().trim_matches('"').split('~').collect();
        if fields.len() < 35 || fields[1].is_empty() {
            continue;
        }
        let price = Price::parse(fields[3])?;
        if price.0 <= 0 {
            continue;
        }
        let previous_close = Price::parse(fields[4])?;
        if previous_close.0 <= 0 {
            continue;
        }
        parsed.push(Quote {
            id: id.into(),
            name: fields[1].into(),
            price,
            previous_close,
            open: Price::parse(fields[5])?,
            high: Price::parse(fields[33])?,
            low: Price::parse(fields[34])?,
            source_time: parse_cn_time(&fields[30].replace(['/', ':', ' '], ""))?,
            received_at: Utc::now(),
            volume_lots: mainland_number(&fields, 6, id),
            amount_wan: mainland_number(&fields, 37, id),
            turnover_percent: mainland_number(&fields, 38, id),
            volume_ratio: mainland_number(&fields, 49, id),
        });
    }
    Ok(parsed)
}
// Hong Kong fields use a different schema; never interpret them as mainland lots/CNY.
fn mainland_number(fields: &[&str], index: usize, id: &str) -> Option<f64> {
    if id.starts_with("hk") {
        return None;
    }
    fields
        .get(index)?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.)
}
pub async fn history(http: &reqwest::Client, id: &str, period: Period) -> Result<Series> {
    if !valid_id(id) {
        bail!("invalid stock id");
    }
    let value: Value = if period == Period::Intraday {
        http.get("https://web.ifzq.gtimg.cn/appstock/app/minute/query")
            .query(&[("code", id)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?
    } else {
        // Unadjusted prices: history and current quotes share the same currency/scale.
        let param = format!("{id},{},,,60,", period.source_key());
        http.get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
            .query(&[("param", param)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?
    };
    parse_history(id, period, &value)
}
pub fn parse_history(id: &str, period: Period, value: &Value) -> Result<Series> {
    if value["code"].as_i64() != Some(0) {
        bail!("history unavailable");
    }
    let node = &value["data"][id];
    let mut points = Vec::new();
    let date;
    if period == Period::Intraday {
        date = node["data"]["date"].as_str().unwrap_or("").to_string();
        let mut previous_volume = Some(0.);
        for item in node["data"]["data"]
            .as_array()
            .context("missing minute points")?
        {
            let f: Vec<_> = item
                .as_str()
                .context("invalid minute row")?
                .split_whitespace()
                .collect();
            if f.len() < 2 || f[0].len() != 4 || !f[0].bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let price = Price::parse(f[1])?;
            if price.0 > 0 {
                let cumulative = f
                    .get(2)
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.);
                let volume = cumulative
                    .zip(previous_volume)
                    .and_then(|(now, old)| (now >= old).then_some(now - old));
                previous_volume = cumulative;
                points.push(Point {
                    label: format!("{}:{}", &f[0][..2], &f[0][2..]),
                    price,
                    ohlc: None,
                    volume,
                });
            }
        }
    } else {
        let rows = node[period.source_key()]
            .as_array()
            .context("missing candle points")?;
        for row in rows {
            let date = row[0].as_str().context("missing day")?;
            let price = Price::parse(row[2].as_str().context("missing close")?)?;
            if price.0 > 0 {
                let open = Price::parse(row[1].as_str().context("missing open")?)?;
                let high = Price::parse(row[3].as_str().context("missing high")?)?;
                let low = Price::parse(row[4].as_str().context("missing low")?)?;
                if low.0 <= 0 || low > open.min(price) || high < open.max(price) {
                    bail!("invalid candle range");
                }
                let volume = row[5]
                    .as_str()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.);
                points.push(Point {
                    label: date.into(),
                    price,
                    ohlc: Some(Ohlc { open, high, low }),
                    volume,
                });
            }
        }
        date = points.last().map(|p| p.label.clone()).unwrap_or_default();
        let keep = 60;
        if points.len() > keep {
            points.drain(..points.len() - keep);
        }
    }
    if points.is_empty() {
        bail!("empty history");
    }
    Ok(Series {
        id: id.into(),
        period,
        date,
        points,
        fetched_at: Utc::now(),
    })
}
pub async fn catalog(http: &reqwest::Client) -> Result<Catalog> {
    let body: Value = http
        .get(CATALOG_URL)
        .header("Referer", "https://www.cninfo.com.cn/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut stocks = Vec::new();
    for row in body["stockList"].as_array().context("missing stock list")? {
        let Some(code) = row["code"].as_str() else {
            continue;
        };
        let Some(name) = row["zwjc"].as_str() else {
            continue;
        };
        if row["category"].as_str() != Some("A股")
            || code.len() != 6
            || !code.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let prefix = if code.starts_with('6') {
            "sh"
        } else if code.starts_with(['4', '8']) || code.starts_with("92") {
            "bj"
        } else {
            "sz"
        };
        stocks.push(Instrument {
            id: format!("{prefix}{code}"),
            name: name.into(),
        });
    }
    if stocks.len() < 4000 {
        bail!("incomplete A-share catalog");
    }
    let etf_source = match crate::instruments::fetch_etfs(http).await {
        Ok(etfs) => {
            stocks.extend(etfs);
            "Sina ETF live catalog"
        }
        Err(_) => "bundled ETF catalog (2026-10-31)",
    };
    crate::instruments::merge(&mut stocks);
    stocks.sort_by(|a, b| a.id.cmp(&b.id));
    stocks.dedup_by(|a, b| a.id == b.id);
    Ok(Catalog {
        as_of: Utc::now()
            .with_timezone(&FixedOffset::east_opt(28800).unwrap())
            .format("%Y-%m-%d")
            .to_string(),
        source: format!("{CATALOG_URL}; {etf_source}; supported index registry"),
        stocks,
    })
}
fn same_job(a: &Request, b: &Request, kind: Kind) -> bool {
    a.config.network == b.config.network
        && a.demo == b.demo
        && match kind {
            Kind::Quotes => {
                a.symbols == b.symbols
                    && a.config.refresh.quotes_secs == b.config.refresh.quotes_secs
            }
            Kind::Chart => {
                a.focus == b.focus
                    && a.period == b.period
                    && a.config.refresh.chart_secs == b.config.refresh.chart_secs
            }
            Kind::Overseas | Kind::Overnight => {
                a.overseas == b.overseas
                    && a.config.refresh.quotes_secs == b.config.refresh.quotes_secs
                    && a.config.refresh.chart_secs == b.config.refresh.chart_secs
            }
            Kind::Catalog => true,
            Kind::Turnover => a.config.refresh.quotes_secs == b.config.refresh.quotes_secs,
            Kind::Chips => a.focus == b.focus && a.heatmap_enabled == b.heatmap_enabled,
            Kind::Sectors => {
                a.heatmap_enabled == b.heatmap_enabled
                    && a.sector_category == b.sector_category
                    && a.config.refresh.sectors_secs == b.config.refresh.sectors_secs
            }
        }
}
pub fn spawn_workers(rx: watch::Receiver<Request>, tx: mpsc::Sender<MarketEvent>) {
    tokio::spawn(crate::us_search::worker(rx.clone(), tx.clone()));
    for kind in [
        Kind::Quotes,
        Kind::Chart,
        Kind::Catalog,
        Kind::Sectors,
        Kind::Chips,
        Kind::Turnover,
        Kind::Overseas,
        Kind::Overnight,
    ] {
        tokio::spawn(worker(rx.clone(), tx.clone(), kind));
    }
}
async fn next_job(
    rx: &mut watch::Receiver<Request>,
    current: &Request,
    kind: Kind,
) -> Option<Request> {
    loop {
        if rx.changed().await.is_err() {
            return None;
        }
        let next = rx.borrow_and_update().clone();
        if !same_job(current, &next, kind) {
            return Some(next);
        }
    }
}
async fn worker(mut rx: watch::Receiver<Request>, tx: mpsc::Sender<MarketEvent>, kind: Kind) {
    let mut request = rx.borrow_and_update().clone();
    let mut failures = 0;
    let mut deadline = tokio::time::Instant::now();
    let mut http = client(&request.config.network).ok();
    if kind == Kind::Catalog
        && request.catalog_as_of
            == Utc::now()
                .with_timezone(&FixedOffset::east_opt(28800).unwrap())
                .format("%Y-%m-%d")
                .to_string()
    {
        // A bundled or cached index already covers today. No startup network dependency.
        deadline += Duration::from_secs(86400);
    }
    loop {
        tokio::select! {
            next = next_job(&mut rx, &request, kind) => {
                let Some(next) = next else { break };
                if next.config.network != request.config.network { http = client(&next.config.network).ok(); }
                request = next;
                failures = 0;
                deadline = tokio::time::Instant::now();
                continue;
            },
            _=tokio::time::sleep_until(deadline)=>{}
        }
        let job = request.clone();
        // Separate quote, chart and catalog workers, each serial and bounded.
        // Cancel a stale request on configuration changes; no detached network jobs.
        let operation = async {
            if job.demo {
                return Ok(match kind {
                    Kind::Quotes => MarketEvent::Quotes(demo_quotes(&job.symbols)),
                    Kind::Turnover => MarketEvent::Turnover(None),
                    Kind::Overseas => {
                        let j = job.overseas.as_ref().context("no overseas page")?;
                        MarketEvent::Overseas(
                            j.ids
                                .iter()
                                .map(|id| {
                                    crate::overseas::demo(
                                        id,
                                        if j.focus.as_ref() == Some(id) {
                                            j.period
                                        } else {
                                            Period::Intraday
                                        },
                                    )
                                })
                                .collect(),
                            vec![],
                        )
                    }
                    Kind::Overnight => MarketEvent::Overnight(
                        job.overseas
                            .as_ref()
                            .and_then(|j| j.focus.clone())
                            .context("no focus")?,
                        None,
                    ),
                    Kind::Chart => MarketEvent::Chart(demo_history(
                        job.focus.as_deref().unwrap_or("sz300750"),
                        job.period,
                    )),
                    Kind::Chips => MarketEvent::Chips(crate::chips::demo(
                        job.focus.as_deref().unwrap_or("sz300750"),
                    )),
                    Kind::Catalog => return Err(anyhow::anyhow!("demo catalog uses bundled data")),
                    Kind::Sectors => {
                        MarketEvent::Sectors(crate::sectors::demo(job.sector_category))
                    }
                });
            }
            let h = http.as_ref().context("invalid network configuration")?;
            Ok(match kind {
                Kind::Overseas => {
                    use futures_util::{StreamExt, stream};
                    let j = job.overseas.as_ref().context("no overseas page")?;
                    let mut tasks = j
                        .ids
                        .iter()
                        .filter(|id| !crate::instruments::is_index(id))
                        .map(|id| (id.clone(), Period::Intraday))
                        .collect::<Vec<_>>();
                    if j.period != Period::Intraday
                        && let Some(id) = &j.focus
                        && !crate::instruments::is_index(id)
                    {
                        tasks.push((id.clone(), j.period));
                    }
                    if j.us {
                        tasks.sort_by_key(|(id, _)| j.focus.as_ref() != Some(id));
                    }
                    let mut results = stream::iter(tasks.into_iter().map(|(id, period)| {
                        let h = h.clone();
                        let quote_only = !j.us && j.focus.as_ref() != Some(&id);
                        async move {
                            let result = if quote_only {
                                crate::overseas::fetch_index_quote(&h, &id).await
                            } else if j.us && period == Period::Intraday {
                                let (chart, quote) = tokio::join!(
                                    crate::overseas::fetch_chart(&h, &id, period),
                                    crate::overseas::fetch_nasdaq_quote(&h, &id)
                                );
                                match (chart, quote) {
                                    (Ok(mut snapshot), Ok(quote)) => {
                                        crate::overseas::merge_us_quote(&mut snapshot, quote);
                                        Ok(snapshot)
                                    }
                                    (Ok(snapshot), Err(_)) => Ok(snapshot),
                                    (Err(_), Ok(quote)) => {
                                        let mut phases = std::collections::BTreeMap::new();
                                        if quote.session != crate::overseas::Session::Regular {
                                            phases.insert(quote.session, quote.clone());
                                        }
                                        Ok(crate::overseas::Snapshot {
                                            id: id.clone(),
                                            period,
                                            regular: quote,
                                            phases,
                                            bars: vec![],
                                            intervals: vec![],
                                            received_at: chrono::Utc::now(),
                                        })
                                    }
                                    (Err(error), Err(_)) => Err(error),
                                }
                            } else {
                                crate::overseas::fetch_chart(&h, &id, period).await
                            };
                            (id, result)
                        }
                    }))
                    .buffer_unordered(3);
                    let mut snapshots = vec![];
                    let mut errors = vec![];
                    let mut delivered = false;
                    while let Some((id, result)) = results.next().await {
                        match result {
                            Ok(s) if j.us => {
                                tx.send(MarketEvent::Overseas(vec![s], vec![])).await?;
                                delivered = true;
                            }
                            Ok(s) => snapshots.push(s),
                            Err(_) => errors.push(id),
                        }
                    }
                    let domestic = j
                        .ids
                        .iter()
                        .filter(|id| crate::instruments::is_index(id))
                        .cloned()
                        .collect::<Vec<_>>();
                    if !domestic.is_empty() {
                        match quotes(h, &domestic).await {
                            Ok(quotes) => {
                                for id in &domestic {
                                    if !quotes.iter().any(|q| q.id == *id) {
                                        errors.push(id.clone());
                                    }
                                }
                                for quote in quotes {
                                    snapshots.push(crate::index_board::snapshot(&quote, None)?);
                                    if j.focus.as_ref() == Some(&quote.id) {
                                        match history(h, &quote.id, j.period).await.and_then(|s| {
                                            crate::index_board::snapshot(&quote, Some(&s))
                                        }) {
                                            Ok(s) => snapshots.push(s),
                                            Err(_) => errors.push(quote.id.clone()),
                                        }
                                    }
                                }
                            }
                            Err(_) => errors.extend(domestic),
                        }
                    }
                    if snapshots.is_empty() && !delivered {
                        bail!("foreign data unavailable");
                    }
                    MarketEvent::Overseas(snapshots, errors)
                }
                Kind::Overnight => {
                    let id = job
                        .overseas
                        .as_ref()
                        .and_then(|j| j.focus.as_deref())
                        .context("no focus")?;
                    MarketEvent::Overnight(id.into(), crate::overseas::fetch_night(h, id).await?)
                }
                Kind::Quotes => MarketEvent::Quotes(quotes(h, &job.symbols).await?),
                Kind::Turnover => MarketEvent::Turnover(crate::turnover::fetch(h).await?),
                Kind::Chart => MarketEvent::Chart(
                    history(h, job.focus.as_deref().context("no selection")?, job.period).await?,
                ),
                Kind::Catalog => MarketEvent::Catalog(catalog(h).await?),
                Kind::Chips => MarketEvent::Chips(
                    crate::chips::fetch(h, job.focus.as_deref().context("no selection")?).await?,
                ),
                Kind::Sectors => {
                    MarketEvent::Sectors(crate::sectors::fetch(h, job.sector_category).await?)
                }
            })
        };
        if (kind == Kind::Chips
            && (job.focus.as_ref().is_none_or(|id| {
                crate::instruments::is_index(id) || crate::instruments::is_etf(id)
            }) || job.heatmap_enabled))
            || (kind == Kind::Chart && job.focus.is_none())
            || (kind == Kind::Catalog && job.demo)
            || (kind == Kind::Sectors && !job.heatmap_enabled)
            || (kind == Kind::Overseas && job.overseas.as_ref().is_none_or(|j| j.ids.is_empty()))
            || (kind == Kind::Overnight
                && job
                    .overseas
                    .as_ref()
                    .is_none_or(|j| !j.us || j.focus.is_none()))
        {
            deadline = tokio::time::Instant::now() + Duration::from_secs(86400);
            continue;
        }
        let result: Result<MarketEvent> = tokio::select! {
            result=operation=>result,
            next = next_job(&mut rx, &request, kind) => {
                let Some(next) = next else { break };
                if next.config.network != request.config.network { http = client(&next.config.network).ok(); }
                request = next;
                failures = 0;
                deadline = tokio::time::Instant::now();
                continue;
            }
        };
        let base = match kind {
            Kind::Overseas => request
                .config
                .refresh
                .quotes_secs
                .max(request.config.refresh.chart_secs)
                .max(30),
            Kind::Overnight => request.config.refresh.quotes_secs.max(60),
            Kind::Quotes => request.config.refresh.quotes_secs,
            Kind::Turnover => request.config.refresh.quotes_secs.max(60),
            Kind::Chart => request.config.refresh.chart_secs,
            Kind::Catalog => 86400,
            Kind::Chips => 1800,
            Kind::Sectors => request.config.refresh.sectors_secs,
        };
        let delay = match result {
            Ok(event) => {
                failures = 0;
                if tx.send(event).await.is_err() {
                    break;
                }
                base
            }
            Err(err) => {
                failures += 1;
                let delay = retry_delay(if kind == Kind::Catalog { 60 } else { base }, failures);
                if tx
                    .send(MarketEvent::Error {
                        kind,
                        message: if http.is_none() {
                            "网络/代理配置无效，请检查设置或代理环境变量".into()
                        } else {
                            safe_error(&err)
                        },
                        retry_secs: delay,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                delay
            }
        };
        deadline = tokio::time::Instant::now() + Duration::from_secs(delay);
    }
}

pub fn demo_quotes(ids: &[String]) -> Vec<Quote> {
    let names: std::collections::HashMap<_, _> = crate::storage::bundled_catalog()
        .stocks
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    ids.iter()
        .map(|id| {
            let base = match id.as_str() {
                "sh000001" => 3599_0200,
                "sz399001" => 108337800,
                "hkHSI" => 232944200,
                "sh000688" => 1160_8400,
                "sh000300" => 3985_5000,
                id if crate::instruments::is_index(id) => 2500_0000,
                id if crate::instruments::is_etf(id) => 4_0650,
                "sz300750" => 268_5000,
                "sh600519" => 1528_0000,
                "sh688981" => 96_3500,
                "sz002594" => 286_7000,
                "sh600036" => 39_8200,
                _ => 52_1600,
            };
            let change = if id == "sh601318" || id == "sh600036" {
                -0.012
            } else {
                0.018
            };
            let previous = (base as f64 / (1.0 + change)) as i64;
            Quote {
                id: id.clone(),
                name: names.get(id).cloned().unwrap_or_else(|| id.clone()),
                price: Price(base),
                previous_close: Price(previous),
                open: Price(previous),
                high: Price((base as f64 * 1.012) as i64),
                low: Price((base as f64 * 0.982) as i64),
                source_time: Utc::now().with_timezone(&FixedOffset::east_opt(28800).unwrap()),
                received_at: Utc::now(),
                volume_lots: Some(123456.),
                amount_wan: Some(1250000.),
                turnover_percent: Some(1.23),
                volume_ratio: Some(1.56),
            }
        })
        .collect()
}
pub fn demo_history(id: &str, period: Period) -> Series {
    let quote = demo_quotes(&[id.into()]).remove(0);
    let base = quote.price.as_f64();
    let previous = quote.previous_close.as_f64();
    let n = if period == Period::Intraday { 242 } else { 60 };
    let points = (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64;
            let value = previous
                + (base - previous) * t
                + base
                    * ((t * 18.).sin() * 0.003 + (t * 81.).sin() * 0.001)
                    * (std::f64::consts::PI * t).sin();
            Point {
                label: if period == Period::Intraday {
                    let minutes = if i <= 120 { 570 + i } else { 780 + i - 121 };
                    format!("{:02}:{:02}", minutes / 60, minutes % 60)
                } else {
                    let day = Utc::now().date_naive()
                        - chrono::Duration::days(
                            (n - 1 - i) as i64
                                * match period {
                                    Period::Week => 7,
                                    Period::Month => 30,
                                    _ => 1,
                                },
                        );
                    format!("{:02}-{:02}", day.month(), day.day())
                },
                price: Price((value * 10000.) as i64),
                ohlc: (period != Period::Intraday).then(|| {
                    let open = value + base * (t * 70.).sin() * 0.004;
                    Ohlc {
                        open: Price((open * 10000.) as i64),
                        high: Price(((open.max(value) + base * 0.003) * 10000.) as i64),
                        low: Price(((open.min(value) - base * 0.003) * 10000.) as i64),
                    }
                }),
                volume: Some(
                    (1000. + (t * 51.).sin().abs() * 5000.)
                        * if period == Period::Intraday { 1. } else { 100. },
                ),
            }
        })
        .collect();
    Series {
        id: id.into(),
        period,
        date: Utc::now().format("%Y-%m-%d").to_string(),
        points,
        fetched_at: Utc::now(),
    }
}
