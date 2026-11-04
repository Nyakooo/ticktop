//! Shared index directory and the user's portable attention list.
use crate::{
    model::Instrument,
    overseas::{self, State},
    search::SearchIndex,
};
use anyhow::{Result, bail};

pub fn catalog() -> Vec<(&'static str, &'static str)> {
    let mut rows = crate::instruments::INDICES
        .iter()
        .chain(overseas::INDICES.iter())
        .copied()
        .collect::<Vec<_>>();
    rows.sort_by_key(|(id, _)| match region(id) {
        "内地" => 0,
        "香港" => 1,
        "日本" => 2,
        "韩国" => 3,
        _ => 4,
    });
    rows
}
pub fn contains(id: &str) -> bool {
    crate::instruments::is_index(id) || overseas::INDICES.iter().any(|(code, _)| *code == id)
}
pub fn region(id: &str) -> &'static str {
    if id.starts_with("sh") || id.starts_with("sz") {
        "内地"
    } else if id == "hkHSI" {
        "香港"
    } else {
        match id {
            "^N225" => "日本",
            "^KS11" => "韩国",
            _ => "美国",
        }
    }
}
pub fn search(query: &str) -> Vec<Instrument> {
    let stocks = catalog()
        .into_iter()
        .map(|(id, name)| Instrument {
            id: id.into(),
            name: name.into(),
        })
        .collect::<Vec<_>>();
    SearchIndex::new(&stocks).search(query, 13)
}
impl State {
    pub fn index_rows(&self) -> Vec<String> {
        if self.index_all {
            catalog().iter().map(|(id, _)| id.to_string()).collect()
        } else {
            let mut rows = self.index_watchlist.clone();
            rows.sort_by_key(|id| !self.index_pinned.contains(id));
            rows
        }
    }
    pub fn ensure_index_selection(&mut self) {
        let rows = self.index_rows();
        if !rows.contains(&self.index_selected)
            && let Some(id) = rows.first()
        {
            self.index_selected = id.clone();
        }
    }
    pub fn follow_index(&mut self, id: &str) -> Result<()> {
        if !contains(id) {
            bail!("请从支持的指数目录中选择");
        }
        if !self.index_watchlist.iter().any(|s| s == id) {
            self.index_watchlist.push(id.into());
        }
        self.index_selected = id.into();
        Ok(())
    }
    pub fn remove_index(&mut self, id: &str) {
        self.index_watchlist.retain(|s| s != id);
        self.index_pinned.retain(|s| s != id);
        self.ensure_index_selection();
    }
    pub fn pin_index(&mut self, id: &str) -> Result<()> {
        self.follow_index(id)?;
        if self.index_pinned.iter().any(|s| s == id) {
            self.index_pinned.retain(|s| s != id);
        } else {
            self.index_pinned.push(id.into());
        }
        Ok(())
    }
    pub fn move_index(&mut self, down: bool) -> Result<()> {
        if self.index_all {
            bail!("Tab 切换到我的关注后排序");
        }
        let rows = self.index_rows();
        let Some(i) = rows.iter().position(|id| id == &self.index_selected) else {
            return Ok(());
        };
        let next = if down { i + 1 } else { i.saturating_sub(1) };
        if next == i || next >= rows.len() {
            return Ok(());
        }
        if self.index_pinned.contains(&rows[i]) != self.index_pinned.contains(&rows[next]) {
            bail!("置顶项与普通项分别排序，按 t 切换置顶");
        }
        let a = self
            .index_watchlist
            .iter()
            .position(|id| id == &rows[i])
            .unwrap();
        let b = self
            .index_watchlist
            .iter()
            .position(|id| id == &rows[next])
            .unwrap();
        self.index_watchlist.swap(a, b);
        Ok(())
    }
    pub fn validate_indices(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        if self
            .index_watchlist
            .iter()
            .any(|id| !contains(id) || !seen.insert(id))
        {
            bail!("指数关注含无效或重复代码");
        }
        seen.clear();
        if self
            .index_pinned
            .iter()
            .any(|id| !self.index_watchlist.contains(id) || !seen.insert(id))
        {
            bail!("指数置顶列表无效");
        }
        Ok(())
    }
}

pub fn snapshot(
    quote: &crate::model::Quote,
    series: Option<&crate::model::Series>,
) -> anyhow::Result<overseas::Snapshot> {
    use chrono::{FixedOffset, NaiveDate, NaiveTime, TimeZone};
    use overseas::{Bar, Interval, Quote, Session, Snapshot};
    let zone = FixedOffset::east_opt(28800).unwrap();
    let mut bars = vec![];
    if let Some(series) = series {
        for point in &series.points {
            let date_time = if series.period == crate::model::Period::Intraday {
                let day = NaiveDate::parse_from_str(&series.date, "%Y%m%d")
                    .or_else(|_| NaiveDate::parse_from_str(&series.date, "%Y-%m-%d"))?;
                day.and_time(NaiveTime::parse_from_str(&point.label, "%H:%M")?)
            } else {
                NaiveDate::parse_from_str(&point.label, "%Y-%m-%d")?
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            };
            let time = zone
                .from_local_datetime(&date_time)
                .single()
                .ok_or_else(|| anyhow::anyhow!("invalid source date"))?
                .timestamp();
            bars.push(Bar {
                time,
                close: point.price.as_f64(),
                open: point.ohlc.as_ref().map(|o| o.open.as_f64()),
                high: point.ohlc.as_ref().map(|o| o.high.as_f64()),
                low: point.ohlc.as_ref().map(|o| o.low.as_f64()),
                volume: point.volume,
                session: Session::Regular,
            });
        }
    }
    let date = quote.source_time.date_naive();
    let start = zone
        .from_local_datetime(&date.and_hms_opt(9, 30, 0).unwrap())
        .single()
        .unwrap()
        .timestamp();
    let end = zone
        .from_local_datetime(
            &date
                .and_hms_opt(if quote.id == "hkHSI" { 16 } else { 15 }, 0, 0)
                .unwrap(),
        )
        .single()
        .unwrap()
        .timestamp();
    Ok(Snapshot {
        id: quote.id.clone(),
        period: series.map_or(crate::model::Period::Intraday, |s| s.period),
        regular: Quote {
            price: quote.price.as_f64(),
            reference: Some(quote.previous_close.as_f64()),
            time: quote.source_time.timestamp(),
            session: Session::Regular,
            source: "腾讯".into(),
        },
        phases: Default::default(),
        bars,
        intervals: vec![Interval {
            start,
            end,
            session: Session::Regular,
        }],
        received_at: chrono::Utc::now(),
    })
}
