//! Local fuzzy lookup plus cancellable, debounced public US equity search.
use crate::{
    market::{MarketEvent, Request},
    model::Instrument,
    overseas,
    search::SearchIndex,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use tokio::sync::{mpsc, watch};

pub const DIRECT_NAME: &str = "按代码添加（行情源确认）";
const ENGLISH: &[(&str, &str)] = &[
    ("AAPL", "Apple"),
    ("NVDA", "Nvidia"),
    ("TSLA", "Tesla"),
    ("MSFT", "Microsoft"),
    ("AMZN", "Amazon"),
    ("GOOG", "Google Alphabet"),
    ("META", "Meta Facebook"),
    ("AMD", "Advanced Micro Devices"),
    ("INTC", "Intel"),
    ("BABA", "Alibaba"),
    ("PDD", "Pinduoduo"),
    ("BRK-B", "Berkshire Hathaway"),
    ("MU", "Micron"),
];

pub fn local(state: &overseas::State, query: &str) -> Vec<Instrument> {
    let mut names: BTreeMap<String, String> = overseas::STOCKS
        .iter()
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect();
    for id in &state.subscriptions {
        names.entry(id.clone()).or_insert_with(|| {
            state
                .us_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.clone())
        });
    }
    let mut entries: Vec<_> = names
        .iter()
        .map(|(id, name)| Instrument {
            id: id.clone(),
            name: name.clone(),
        })
        .collect();
    entries.extend(ENGLISH.iter().map(|(id, name)| Instrument {
        id: id.to_string(),
        name: name.to_string(),
    }));
    entries.extend(
        state
            .aliases
            .iter()
            .filter(|(id, _)| state.subscriptions.contains(id))
            .map(|(id, alias)| Instrument {
                id: id.clone(),
                name: alias.clone(),
            }),
    );
    let matches = if query.trim().is_empty() {
        entries
    } else {
        SearchIndex::new(&entries).search(query, 60)
    };
    let mut seen = HashSet::new();
    let rows = matches
        .into_iter()
        .filter(|s| seen.insert(s.id.clone()))
        .take(19)
        .map(|mut s| {
            s.name = names.get(&s.id).cloned().unwrap_or(s.name);
            s
        })
        .collect();
    merge(rows, vec![], query)
}

pub fn merge(local: Vec<Instrument>, online: Vec<Instrument>, query: &str) -> Vec<Instrument> {
    let mut seen = HashSet::new();
    let mut rows: Vec<_> = local
        .into_iter()
        .filter(|s| s.name != DIRECT_NAME)
        .chain(online)
        .filter(|s| seen.insert(s.id.clone()))
        .take(19)
        .collect();
    let id = query.trim().to_ascii_uppercase();
    if overseas::valid_symbol(&id) && !seen.contains(&id) {
        rows.push(Instrument {
            id,
            name: DIRECT_NAME.into(),
        });
    }
    rows
}

pub fn parse(value: &Value) -> Result<Vec<Instrument>> {
    let rows = value["quotes"]
        .as_array()
        .context("missing search results")?;
    let mut seen = HashSet::new();
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = row["symbol"].as_str()?;
            let exchange = row["exchange"].as_str()?;
            if row["quoteType"].as_str() != Some("EQUITY")
                || !matches!(
                    exchange,
                    "NMS" | "NGM" | "NCM" | "NYQ" | "ASE" | "PCX" | "BTS" | "PNK" | "OQB" | "OQX"
                )
                || !overseas::valid_symbol(id)
                || !seen.insert(id.to_owned())
            {
                return None;
            }
            let name = row["shortname"]
                .as_str()
                .or_else(|| row["longname"].as_str())?;
            if name.trim().is_empty()
                || name.chars().count() > 128
                || name.chars().any(char::is_control)
            {
                return None;
            }
            Some(Instrument {
                id: id.into(),
                name: name.trim().into(),
            })
        })
        .take(20)
        .collect())
}

pub async fn fetch(http: &reqwest::Client, query: &str) -> Result<Vec<Instrument>> {
    let value = http
        .get("https://query1.finance.yahoo.com/v1/finance/search")
        .query(&[
            ("q", query),
            ("quotesCount", "20"),
            ("newsCount", "0"),
            ("enableFuzzyQuery", "true"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    parse(&value)
}

async fn changed(rx: &mut watch::Receiver<Request>, current: &Request) -> Option<Request> {
    loop {
        rx.changed().await.ok()?;
        let next = rx.borrow_and_update().clone();
        if next.us_search != current.us_search
            || next.config.network != current.config.network
            || next.demo != current.demo
        {
            return Some(next);
        }
    }
}

pub async fn worker(mut rx: watch::Receiver<Request>, tx: mpsc::Sender<MarketEvent>) {
    let mut current = rx.borrow_and_update().clone();
    loop {
        if let Some((generation, query)) = current.us_search.clone() {
            let operation = async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                if current.demo {
                    return Ok(vec![]);
                }
                fetch(&crate::market::client(&current.config.network)?, &query).await
            };
            tokio::select! {
                next = changed(&mut rx, &current) => {
                    let Some(next) = next else { break }; current = next; continue;
                }
                result = operation => {
                    let (rows, error) = match result {
                        Ok(rows) => (rows, None),
                        Err(_) => (vec![], Some("在线搜索暂不可用，已保留本地结果，可按代码添加".to_string())),
                    };
                    if tx.send(MarketEvent::UsSearch { generation, query, rows, error }).await.is_err() { break; }
                }
            }
        }
        let Some(next) = changed(&mut rx, &current).await else {
            break;
        };
        current = next;
    }
}
