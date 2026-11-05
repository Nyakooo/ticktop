use serde_json::json;
use ticktop::{model::Instrument, overseas, us_search};

#[test]
fn local_lookup_supports_chinese_english_pinyin_codes_aliases_and_typos() {
    let mut state = overseas::State::default();
    state.subscriptions.push("MSFT".into());
    state.aliases.insert("MSFT".into(), "office".into());
    for (query, id) in [
        ("英伟", "NVDA"),
        ("yingweida", "NVDA"),
        ("ywd", "NVDA"),
        ("nvida", "NVDA"),
        ("Micorsoft", "MSFT"),
        ("office", "MSFT"),
        ("int", "INTC"),
        ("微 软", "MSFT"),
        ("micron", "MU"),
        ("美光", "MU"),
    ] {
        assert_eq!(us_search::local(&state, query)[0].id, id, "{query}");
    }
    let direct = us_search::local(&state, "ZZZZ");
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].id, "ZZZZ");
    assert_eq!(direct[0].name, us_search::DIRECT_NAME);
    assert!(us_search::local(&state, "未知公司名称").is_empty());
    assert_eq!(
        Instrument {
            id: "MSFT".into(),
            name: "微软".into()
        }
        .code(),
        "MSFT"
    );
}

#[test]
fn online_results_exclude_foreign_listings_futures_etfs_and_invalid_names() {
    let row = |id, exchange, kind, name| json!({"symbol":id,"exchange":exchange,"quoteType":kind,"shortname":name});
    let value = json!({"quotes":[row("MU","NMS","EQUITY","Micron"),
        row("MU.TO","TOR","EQUITY","Canadian listing"), row("MU","NMS","EQUITY","duplicate"),
        row("MU=F","CME","FUTURE","Future"), row("SOXX","NGM","ETF","ETF"),
        row("BAD","NYQ","EQUITY","bad\nname"), row("IBM","NYQ","EQUITY","IBM")]});
    let rows = us_search::parse(&value).unwrap();
    assert_eq!(
        rows.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["MU", "IBM"]
    );
    assert!(us_search::parse(&json!({})).is_err());
    let merged = us_search::merge(
        us_search::local(&overseas::State::default(), "IBM"),
        rows,
        "IBM",
    );
    let ibm = merged.iter().filter(|s| s.id == "IBM").collect::<Vec<_>>();
    assert_eq!(ibm.len(), 1);
    assert_eq!(ibm[0].name, "IBM");
}

#[tokio::test(start_paused = true)]
async fn search_worker_debounces_and_stops_when_dialog_or_app_closes() {
    use std::time::Duration;
    use ticktop::{
        app::App,
        config::{Config, Paths},
        market::MarketEvent,
    };
    use tokio::{
        sync::{mpsc, watch},
        time::{sleep, timeout},
    };
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        Config::default(),
        Paths::resolve(Some(&dir.path().to_path_buf())).unwrap(),
        false,
        true,
    )
    .unwrap();
    let mut request = app.request();
    request.us_search = Some((1, "micro".into()));
    let (tx, rx) = watch::channel(request.clone());
    let (events, mut received) = mpsc::channel(4);
    let task = tokio::spawn(us_search::worker(rx, events));
    sleep(Duration::from_millis(50)).await;
    request.us_search = Some((2, "micron".into()));
    tx.send_replace(request.clone());
    assert!(
        timeout(Duration::from_millis(200), received.recv())
            .await
            .is_err()
    );
    assert!(
        matches!(timeout(Duration::from_secs(2), received.recv()).await.unwrap(),
        Some(MarketEvent::UsSearch {generation: 2, query, error: None, ..}) if query == "micron")
    );
    request.us_search = Some((3, "apple".into()));
    tx.send_replace(request.clone());
    sleep(Duration::from_millis(30)).await;
    request.us_search = None;
    tx.send_replace(request);
    assert!(
        timeout(Duration::from_millis(400), received.recv())
            .await
            .is_err()
    );
    drop(tx);
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
#[ignore = "requires public network; verifies free US equity search source"]
async fn live_us_search_returns_the_correct_us_listing() {
    let http = ticktop::market::client(&ticktop::config::Config::default().network).unwrap();
    for (query, id) in [("Micron", "MU"), ("Nvidia", "NVDA"), ("Apple", "AAPL")] {
        let rows = us_search::fetch(&http, query).await.unwrap();
        assert!(rows.iter().any(|s| s.id == id), "{query}: {rows:?}");
        println!("{query}: {rows:?}");
    }
}
