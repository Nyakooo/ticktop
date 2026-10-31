use ticktop::{
    config::Config,
    instruments::*,
    market,
    model::{Period, Price},
    search::SearchIndex,
    storage::{UserState, bundled_catalog},
};

#[test]
fn etfs_and_indices_are_searchable_without_colliding_with_stocks() {
    let catalog = bundled_catalog();
    let index = SearchIndex::new(&catalog.stocks);
    for (query, id) in [
        ("上证指数", "sh000001"),
        ("深证成指", "sz399001"),
        ("恒生", "hkHSI"),
        ("科创50", "sh000688"),
        ("沪深300", "sh000300"),
        ("hsi", "hkHSI"),
        ("hkHSI", "hkHSI"),
        ("sz000001", "sz000001"),
        ("510300", "sh510300"),
        ("创业板ETF", "sz159915"),
        ("hengshengzhishu", "hkHSI"),
    ] {
        assert!(
            index.search(query, 30).iter().any(|s| s.id == id),
            "query {query} missing {id}"
        );
    }
    let results = index.search("000001", 30);
    assert!(results.iter().any(|s| s.id == "sh000001"));
    assert!(results.iter().any(|s| s.id == "sz000001"));
    assert!(index.search("ETF", 30).iter().all(|s| is_etf(&s.id)));
    assert_eq!(kind("sz000001"), "股票");
    assert_eq!(kind("sh000001"), "指数");
    assert!(is_etf("sh551000"));
}
#[test]
fn pinned_index_defaults_migration_and_validation() {
    let config: Config = toml::from_str("[refresh]\nquotes_secs=5").unwrap();
    assert_eq!(config.pinned_indices, vec!["sh000001"]);
    let pinned = parse_pinned("上证指数，深证成指,hkHSI 科创50 沪深300").unwrap();
    assert_eq!(
        pinned,
        vec!["sh000001", "sz399001", "hkHSI", "sh000688", "sh000300"]
    );
    assert!(parse_pinned("无").unwrap().is_empty());
    assert!(parse_pinned("").unwrap().is_empty());
    assert!(parse_pinned("上证指数,sh000001").is_err());
    assert!(parse_pinned("sh600519").is_err());
    assert!(parse_pinned("上证指数,深证成指,恒生指数,科创50,沪深300,创业板指,上证50").is_err());
    let c = Config {
        pinned_indices: pinned,
        ..Config::default()
    };
    c.validate().unwrap();
    let roundtrip: Config = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
    assert_eq!(roundtrip.pinned_indices, c.pinned_indices);
    let c = Config {
        pinned_indices: vec!["sh600519".into()],
        ..Config::default()
    };
    assert!(c.validate().is_err());
}
#[test]
fn etf_prices_preserve_mill_precision_and_index_points_have_correct_units() {
    assert_eq!(price("sh510300", Price(40650)), "4.065");
    assert_eq!(price("sz159915", Price(-1000)), "-0.100");
    assert_eq!(price("sh600519", Price(15280000)), "1528.00");
    assert_eq!(unit("hkHSI"), "点");
    assert_eq!(unit("sh510300"), "CNY");
}
#[test]
fn mainland_index_hong_kong_time_and_etf_quote_contracts() {
    let quotes = market::parse_quotes(include_str!("fixtures/indices-etfs.txt")).unwrap();
    assert_eq!(quotes.len(), 7);
    let hsi = quotes.iter().find(|q| q.id == "hkHSI").unwrap();
    assert!(hsi.source_time.to_rfc3339().ends_with("+08:00"));
    assert_eq!(hsi.price, Price(232944200));
    let etf = quotes.iter().find(|q| q.id == "sh510300").unwrap();
    assert_eq!(etf.price, Price(40650));
    assert_eq!(etf.change(), Price(-460));
}
#[test]
fn extended_watchlists_survive_reload_and_bad_etf_rows_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = UserState::default();
    state.add("hkHSI", 1).unwrap();
    state.add("sh510300", 1).unwrap();
    let path = dir.path().join("state.json");
    state.save(&path).unwrap();
    assert!(
        UserState::load(&path)
            .unwrap()
            .visible()
            .contains(&"hkHSI".to_string())
    );
    let mut stocks = vec![];
    merge(&mut stocks);
    assert_eq!(stocks.iter().filter(|s| is_etf(&s.id)).count(), 1715);
    assert!(parse_etfs(&serde_json::json!([{"symbol":"sh600519","name":"wrong"}])).is_err());
}
#[tokio::test]
#[ignore = "requires public network; opt-in index/ETF source verification"]
async fn live_indices_and_etf_quotes_and_history() {
    let http = market::client(&Config::default().network).unwrap();
    let ids: Vec<String> = [
        "sh000001", "sz399001", "sh000688", "sh000300", "hkHSI", "sh510300", "sz159915",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let quotes = market::quotes(&http, &ids).await.unwrap();
    assert!(ids.iter().all(|id| quotes.iter().any(|q| &q.id == id)));
    for id in ["hkHSI", "sh510300", "sh000001"] {
        for period in [Period::Intraday, Period::Week] {
            let series = market::history(&http, id, period).await.unwrap();
            assert!(!series.points.is_empty());
            println!(
                "{id} {:?} {} points / {}",
                period,
                series.points.len(),
                series.date
            );
        }
    }
}
