use serde_json::json;
use ticktop::{
    model::Period,
    overseas::{self, Session},
};
fn fixture() -> serde_json::Value {
    json!({"chart":{"result":[{"meta":{"symbol":"AAPL","instrumentType":"EQUITY","currency":"USD","regularMarketPrice":110.,"regularMarketTime":1793739600_i64,"previousClose":100.,"chartPreviousClose":70.,"currentTradingPeriod":{"pre":{"start":1793696400_i64,"end":1793716200_i64},"regular":{"start":1793716200_i64,"end":1793739600_i64},"post":{"start":1793739600_i64,"end":1793754000_i64}}},"timestamp":[1793696400_i64,1793716200_i64,1793716260_i64,1793739600_i64,1793754060_i64],"indicators":{"quote":[{"close":[102.,105.,null,112.,114.],"open":[101.,104.,null,111.,113.],"high":[103.,106.,null,113.,115.],"low":[100.,103.,null,110.,112.],"volume":[10,20,null,30,40]}]}}],"error":null}})
}
#[test]
fn source_sessions_nulls_and_change_baselines_are_preserved() {
    let s = overseas::parse_chart(&fixture(), "AAPL", Period::Intraday).unwrap();
    assert_eq!(s.bars.len(), 3); // Unknown interval is not assumed to be overnight.
    assert_eq!(s.bars[0].session, Session::Pre);
    assert_eq!(s.bars[1].session, Session::Regular);
    assert_eq!(s.bars[2].session, Session::Post);
    assert_eq!(s.regular.reference, Some(100.)); // Never use 70, the multi-day chart baseline.
    assert_eq!(s.quote(Session::Pre).unwrap().reference, Some(100.));
    assert_eq!(s.quote(Session::Post).unwrap().reference, Some(110.));
    assert_eq!(s.quote(Session::All).unwrap().price, 112.);
    assert!(s.quote(Session::Overnight).is_none());
    let mut bad = fixture();
    bad["chart"]["result"][0]["meta"]["symbol"] = json!("MSFT");
    assert!(overseas::parse_chart(&bad, "AAPL", Period::Intraday).is_err());
}
#[test]
fn source_intervals_override_dst_assumptions() {
    let mut v = fixture();
    let root = &mut v["chart"]["result"][0];
    root["meta"]["currentTradingPeriod"]["pre"]["start"] = json!(1793692800_i64);
    root["meta"]["currentTradingPeriod"]["pre"]["end"] = json!(1793712600_i64);
    root["meta"]["currentTradingPeriod"]["regular"]["start"] = json!(1793712600_i64);
    root["timestamp"][0] = json!(1793692800_i64);
    let s = overseas::parse_chart(&v, "AAPL", Period::Intraday).unwrap();
    assert_eq!(s.bars[0].session, Session::Pre);
    assert!(
        s.intervals
            .iter()
            .any(|i| i.start == 1793712600_i64 && i.session == Session::Regular)
    );
}
#[test]
fn overnight_json_is_symbol_scoped_latest_and_percent_unit_independent() {
    let quote = |symbol, time, price| json!({"symbol":symbol,"regularMarketPrice":{"raw":100.},"overnightMarketPrice":{"raw":price},"overnightMarketTime":{"raw":time},"overnightMarketChangePercent":{"raw":999.}});
    let page = format!(
        "<script>ignored()</script><script type=\"application/json\">{}</script>",
        json!({"body":json!({"quotes":[quote("AAPL",1000,102.),quote("MSFT",3000,500.),quote("AAPL",2000,103.)]}).to_string()})
    );
    let q = overseas::parse_night(&page, "AAPL").unwrap().unwrap();
    assert_eq!(q.time, 2000);
    assert_eq!(q.price, 103.);
    assert!((q.percent().unwrap() - 3.).abs() < 1e-9);
    assert!(overseas::parse_night(&page, "NVDA").unwrap().is_none());
    assert!(
        overseas::parse_night("<script>{bad}</script>", "AAPL")
            .unwrap()
            .is_none()
    );
    assert!(overseas::parse_night(&page, "../AAPL").is_err());
}
#[test]
fn subscriptions_and_groups_validate_portable_state() {
    let mut state = overseas::State::default();
    let id = state.group_named("长期关注").unwrap();
    assert_eq!(state.group_named("长期关注").unwrap(), id);
    state.groups[0].symbols.push("AAPL".into());
    state.group = id;
    state.ensure_selection();
    assert_eq!(state.visible(), ["AAPL"]);
    state.aliases.insert("AAPL".into(), "work".into());
    state.validate().unwrap();
    let loaded: overseas::State =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    loaded.validate().unwrap();
    state.subscriptions.push("../path".into());
    assert!(state.validate().is_err());
}
#[tokio::test]
#[ignore = "requires public network; verifies all overseas source contracts"]
async fn live_foreign_quotes_periods_and_optional_night() {
    let http = ticktop::market::client(&ticktop::config::Config::default().network).unwrap();
    for id in ["^N225", "^KS11", "^IXIC", "^GSPC", "AAPL", "NVDA", "TSLA"] {
        let s = overseas::fetch_chart(&http, id, Period::Intraday)
            .await
            .unwrap();
        assert!(s.regular.price > 0.);
        assert!(!s.bars.is_empty());
        println!(
            "{id} {} {} {} points",
            s.regular.price,
            overseas::source_time(s.regular.time),
            s.bars.len()
        );
    }
    for period in [Period::Day, Period::Week, Period::Month] {
        let s = overseas::fetch_chart(&http, "AAPL", period).await.unwrap();
        assert!(s.bars.iter().any(|b| b.open.is_some()));
    }
    if let Some(q) = overseas::fetch_night(&http, "AAPL").await.unwrap() {
        assert_eq!(q.session, Session::Overnight);
        println!("AAPL night {} {}", q.price, overseas::source_time(q.time));
    }
}

#[test]
fn malformed_chart_is_rejected_without_panics() {
    for value in [
        json!({}),
        json!({"chart":{"result":[{}]}}),
        json!({"chart":{"result":[{"meta":null}]}}),
    ] {
        assert!(overseas::parse_chart(&value, "AAPL", Period::Intraday).is_err());
    }
    let mut value = fixture();
    value["chart"]["result"][0]["indicators"] = json!(null);
    assert!(overseas::parse_chart(&value, "AAPL", Period::Intraday).is_err());
}

#[test]
fn index_directory_and_attention_list_support_search_order_pin_and_migration() {
    use ticktop::index_board;
    let directory = index_board::catalog();
    assert_eq!(directory.len(), 13);
    assert_eq!(index_board::search("hushen300")[0].id, "sh000300");
    assert_eq!(index_board::search("日经")[0].id, "^N225");
    assert!(index_board::search("not-an-index").is_empty());
    let mut state = overseas::State {
        index_all: false,
        ..Default::default()
    };
    state.follow_index("sh000300").unwrap();
    state.pin_index("sh000300").unwrap();
    assert_eq!(state.index_rows()[0], "sh000300");
    state.pin_index("sh000300").unwrap();
    let original = state.index_rows();
    state.move_index(false).unwrap();
    assert_ne!(state.index_rows(), original);
    state.remove_index("sh000300");
    assert!(!state.index_rows().iter().any(|id| id == "sh000300"));
    assert!(index_board::contains("sh000300"));
    assert!(state.follow_index("^UNKNOWN").is_err());
    state.validate().unwrap();
    let old_json = json!({"index_selected":"^KS11","index_period":"day"});
    let loaded: overseas::State = serde_json::from_value(old_json).unwrap();
    assert!(loaded.index_all);
    assert_eq!(loaded.index_watchlist.len(), 6);
    loaded.validate().unwrap();
    state.index_pinned.push("sh000300".into());
    assert!(state.validate().is_err());
}
#[test]
fn domestic_indices_keep_source_date_ohlc_and_exchange_units() {
    use ticktop::{index_board, market};
    let quote = market::demo_quotes(&["sh000001".into()]).remove(0);
    let mut series = market::demo_history("sh000001", Period::Intraday);
    series.date = "20261104".into();
    let s = index_board::snapshot(&quote, Some(&series)).unwrap();
    assert_eq!(s.regular.price, quote.price.as_f64());
    assert!(!s.bars.is_empty());
    assert_eq!(s.regular.source, "腾讯");
    let mut day = market::demo_history("sh000001", Period::Day);
    for point in &mut day.points {
        point.label = format!("2026-{}", point.label);
    }
    let s = index_board::snapshot(&quote, Some(&day)).unwrap();
    assert!(
        s.bars
            .iter()
            .all(|b| b.open.is_some() && b.high.unwrap() >= b.close)
    );
}
#[tokio::test]
#[ignore = "requires public network; verifies the combined domestic/foreign index directory"]
async fn live_index_directory_quotes_and_domestic_history() {
    use ticktop::{index_board, market};
    let http = market::client(&ticktop::config::Config::default().network).unwrap();
    let domestic = index_board::catalog()
        .iter()
        .filter(|(id, _)| ticktop::instruments::is_index(id))
        .map(|(id, _)| id.to_string())
        .collect::<Vec<_>>();
    let quotes = market::quotes(&http, &domestic).await.unwrap();
    assert_eq!(quotes.len(), domestic.len());
    for id in ["sh000001", "hkHSI"] {
        for period in [Period::Intraday, Period::Day] {
            let series = market::history(&http, id, period).await.unwrap();
            let quote = quotes.iter().find(|q| q.id == id).unwrap();
            let s = index_board::snapshot(quote, Some(&series)).unwrap();
            assert!(!s.bars.is_empty());
            println!("{id} {period:?} {} points", s.bars.len());
        }
    }
    for (id, _) in overseas::INDICES {
        let quote = overseas::fetch_index_quote(&http, id).await.unwrap();
        assert!(quote.regular.price > 0.);
        assert!(quote.bars.is_empty());
        println!("{id} {}", quote.regular.price);
    }
}

#[test]
fn single_day_quote_metadata_uses_exact_previous_close_only_for_one_day() {
    let mut v = fixture();
    let meta = &mut v["chart"]["result"][0]["meta"];
    meta.as_object_mut().unwrap().remove("previousClose");
    meta["range"] = json!("1d");
    meta["chartPreviousClose"] = json!(99.25);
    assert_eq!(
        overseas::parse_chart(&v, "AAPL", Period::Day)
            .unwrap()
            .regular
            .reference,
        Some(99.25)
    );
    v["chart"]["result"][0]["meta"]["range"] = json!("5d");
    assert!(
        overseas::parse_chart(&v, "AAPL", Period::Day)
            .unwrap()
            .regular
            .reference
            .is_none()
    );
}

#[test]
fn us_pins_and_manual_order_are_group_local_and_migrate_legacy_state() {
    let mut state = overseas::State::default();
    let group = state.group_named("观察").unwrap();
    state.groups[0].symbols = state.subscriptions.clone();
    state.toggle_us_pin("TSLA").unwrap();
    assert_eq!(state.visible(), ["TSLA", "AAPL", "NVDA"]);
    assert!(state.move_us_symbol("TSLA", true).is_err());
    state.toggle_us_pin("NVDA").unwrap();
    state.move_us_symbol("NVDA", true).unwrap();
    assert_eq!(state.visible(), ["TSLA", "NVDA", "AAPL"]);
    let all = state.visible();
    state.group = group;
    assert_eq!(state.visible(), ["AAPL", "NVDA", "TSLA"]);
    assert!(!state.us_is_pinned("TSLA"));
    state.move_us_symbol("TSLA", false).unwrap();
    state.toggle_us_pin("AAPL").unwrap();
    assert_eq!(state.visible(), ["AAPL", "TSLA", "NVDA"]);
    let before = state.visible();
    state.move_us_symbol("AAPL", false).unwrap(); // first row is a no-op
    state.move_us_symbol("NVDA", true).unwrap(); // last row is a no-op
    assert_eq!(state.visible(), before);
    assert!(state.toggle_us_pin("MSFT").is_err());
    state.validate().unwrap();
    let mut loaded: overseas::State =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(loaded.visible(), before);
    loaded.group = 0;
    assert_eq!(loaded.visible(), all);
    let mut old = serde_json::to_value(&state).unwrap();
    old.as_object_mut().unwrap().remove("us_pinned");
    let legacy: overseas::State = serde_json::from_value(old).unwrap();
    assert!(legacy.us_pinned.is_empty());
    legacy.validate().unwrap();
    loaded.us_pinned.insert(999, vec!["AAPL".into()]);
    assert!(loaded.validate().is_err());
    loaded.us_pinned.remove(&999);
    loaded.us_pinned.insert(group, vec!["MSFT".into()]);
    assert!(loaded.validate().is_err());
    loaded
        .us_pinned
        .insert(group, vec!["AAPL".into(), "AAPL".into()]);
    assert!(loaded.validate().is_err());
}

#[test]
fn nasdaq_premarket_uses_extended_reference_and_source_time() {
    let v = json!({"data":{"symbol":"NVDA","marketStatus":"Pre-Market","primaryData":{"lastSalePrice":"$176.49","netChange":"+7.60","lastTradeTimestamp":"Nov 11, 2026 4:57 AM ET"}}});
    let q = overseas::parse_nasdaq_quote(&v, "NVDA").unwrap();
    assert_eq!(q.session, Session::Pre);
    assert!((q.percent().unwrap() - 4.50).abs() < 0.02);
    assert_eq!(q.time, 1794391020);
}

#[test]
fn nasdaq_rejects_unverified_data_and_handles_summer_postmarket() {
    let mut v = json!({"data":{"symbol":"AAPL","marketStatus":"After-Hours","primaryData":{"lastSalePrice":"$110.00","netChange":"+10.00","lastTradeTimestamp":"Jul 10, 2026 4:30 PM ET"}}});
    let q = overseas::parse_nasdaq_quote(&v, "AAPL").unwrap();
    assert_eq!(q.session, Session::Post);
    assert_eq!(q.time, 1783715400);
    assert!((q.percent().unwrap() - 10.).abs() < 1e-9);
    assert!(overseas::parse_nasdaq_quote(&v, "NVDA").is_err());
    v["data"]["marketStatus"] = json!("unknown");
    assert!(overseas::parse_nasdaq_quote(&v, "AAPL").is_err());
}

#[tokio::test]
#[ignore = "requires public Nasdaq website endpoint"]
async fn live_nasdaq_quote() {
    let http = ticktop::market::client(&ticktop::config::Config::default().network).unwrap();
    let q = overseas::fetch_nasdaq_quote(&http, "NVDA").await.unwrap();
    assert!(q.price > 0. && q.reference.is_some());
    println!("{} {} {:?} {:?}", q.price, q.time, q.session, q.percent());
}

#[test]
fn current_premarket_does_not_select_later_cached_overnight() {
    use chrono::{TimeZone, Utc};
    let now = Utc.with_ymd_and_hms(2026, 11, 11, 10, 8, 0).unwrap();
    let mut snapshot = overseas::demo("NVDA", Period::Intraday);
    snapshot.intervals.clear();
    snapshot.phases.clear();
    let pre = overseas::Quote {
        price: 176.69,
        reference: Some(168.89),
        time: now.timestamp() - 60,
        session: Session::Pre,
        source: "Nasdaq".into(),
    };
    let night = overseas::Quote {
        price: 170.,
        reference: Some(168.89),
        time: now.timestamp(),
        session: Session::Overnight,
        source: "Yahoo".into(),
    };
    snapshot.phases.insert(Session::Pre, pre);
    assert_eq!(
        overseas::current_us_quote(Some(&snapshot), Some(&night), now)
            .unwrap()
            .session,
        Session::Pre
    );
    snapshot.phases.clear();
    snapshot.regular.session = Session::Regular;
    assert!(overseas::current_us_quote(Some(&snapshot), Some(&night), now).is_none());
}

#[test]
fn automatic_us_session_respects_dst_and_weekend() {
    use chrono::{TimeZone, Utc};
    for (date, expected) in [
        ((2026, 11, 11, 8, 59), Session::Overnight),
        ((2026, 11, 11, 9, 0), Session::Pre),
        ((2026, 7, 10, 8, 0), Session::Pre),
        ((2026, 7, 10, 13, 30), Session::Regular),
        ((2026, 7, 10, 20, 0), Session::Post),
    ] {
        let (y, m, d, h, min) = date;
        assert_eq!(
            overseas::current_us_session(Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()),
            Some(expected)
        );
    }
    assert_eq!(
        overseas::current_us_session(Utc.with_ymd_and_hms(2026, 11, 14, 15, 0, 0).unwrap()),
        None
    );
}

#[test]
fn yahoo_premarket_uses_last_regular_close_before_open() {
    let mut v = fixture();
    let root = &mut v["chart"]["result"][0];
    root["meta"]["regularMarketTime"] = json!(1793653200_i64);
    root["meta"]["regularMarketPrice"] = json!(100.);
    root["meta"]["previousClose"] = json!(111.5);
    root["timestamp"] = json!([1793696400_i64]);
    root["indicators"]["quote"][0]["close"] = json!([103.7]);
    let snapshot = overseas::parse_chart(&v, "AAPL", Period::Intraday).unwrap();
    assert!((snapshot.quote(Session::Pre).unwrap().percent().unwrap() - 3.7).abs() < 1e-9);
    v["chart"]["result"][0]
        .as_object_mut()
        .unwrap()
        .remove("timestamp");
    assert!(
        overseas::parse_chart(&v, "AAPL", Period::Intraday)
            .unwrap()
            .bars
            .is_empty()
    );
}

#[test]
fn nasdaq_extended_uses_trade_time_and_regular_close() {
    let v = json!({"data":{
        "previousInfo":" Market Close: $100.00",
        "lastUpdateInfo":["This page refreshes every 30 seconds.", "Data last updated Nov 11, 2026 05:10 AM ET."],
        "tradeDetailTable":{"rows":[{"time":"04:56:26","price":"$103.70"}]}
    }});
    let quote = overseas::parse_nasdaq_extended(&v, Session::Pre).unwrap();
    assert_eq!(quote.time, 1794390986);
    assert!((quote.percent().unwrap() - 3.7).abs() < 1e-9);
    assert!(overseas::parse_nasdaq_extended(&v, Session::Post).is_err());
}

#[test]
fn missing_current_quote_keeps_last_trade_and_its_original_session() {
    use chrono::{TimeZone, Utc};
    let now = Utc.with_ymd_and_hms(2026, 11, 11, 10, 8, 0).unwrap();
    let night = overseas::Quote {
        price: 174.07,
        reference: Some(168.89),
        time: 1794387598,
        session: Session::Overnight,
        source: "Yahoo / Blue Ocean ATS".into(),
    };
    assert!(overseas::current_us_quote(None, Some(&night), now).is_none());
    let quote = overseas::display_us_quote(None, Some(&night), now).unwrap();
    assert_eq!(quote.session, Session::Overnight);
    assert_eq!(quote.time, night.time);
}

#[test]
fn source_intervals_select_postmarket_after_an_early_close() {
    use chrono::{TimeZone, Utc};
    let at = |hour, minute| Utc.with_ymd_and_hms(2026, 11, 27, hour, minute, 0).unwrap();
    let mut snapshot = overseas::demo("NVDA", Period::Intraday);
    snapshot.intervals = vec![overseas::Interval {
        start: at(18, 0).timestamp(),
        end: at(22, 0).timestamp(),
        session: Session::Post,
    }];
    snapshot.phases.insert(
        Session::Post,
        overseas::Quote {
            time: at(18, 1).timestamp(),
            price: 101.,
            reference: Some(100.),
            session: Session::Post,
            source: "Yahoo".into(),
        },
    );
    assert_eq!(
        overseas::current_us_quote(Some(&snapshot), None, at(18, 5))
            .unwrap()
            .session,
        Session::Post,
    );
}

#[test]
fn previous_days_postmarket_does_not_use_todays_regular_price() {
    let mut v = fixture();
    let root = &mut v["chart"]["result"][0];
    root["meta"]["regularMarketTime"] = json!(1793826000_i64);
    let snapshot = overseas::parse_chart(&v, "AAPL", Period::Intraday).unwrap();
    assert_eq!(snapshot.quote(Session::Post).unwrap().reference, None);
}
