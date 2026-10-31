use ticktop::sectors::{self, Category};

#[test]
fn provider_sector_average_is_parsed_without_using_watchlist_quotes() {
    let text = r#"var sector={"hangye_ZA01":"hangye_ZA01,农业,15,7.36,-0.08,-1.1456189027119,441467505,2826017810","hangye_ZJ01":"hangye_ZJ01,银行,42,12.1,0.1,1.25,100,200"};"#;
    let snapshot = sectors::parse(Category::Industry, text).unwrap();
    assert_eq!(snapshot.sectors.len(), 2);
    assert_eq!(snapshot.sectors[0].name, "农业");
    assert_eq!(snapshot.sectors[0].stock_count, 15);
    assert!((snapshot.sectors[0].change_pct.unwrap() + 1.1456189027119).abs() < 1e-10);
    assert_eq!(snapshot.sectors[1].change_pct, Some(1.25));
}
#[test]
fn classification_and_missing_values_are_explicit() {
    let text = r#"var sector={"gn_ai":"gn_ai,人工智能,30,0,0,--,0,0"};"#;
    let snapshot = sectors::parse(Category::Concept, text).unwrap();
    assert_eq!(snapshot.sectors[0].change_pct, None);
    assert!(sectors::parse(Category::Industry, text).is_err());
    for text in [
        "{}",
        "not json",
        r#"{"gn_ai":"gn_ai,人工智能,30,0,0,NaN"}"#,
        r#"{"gn_ai":"gn_ai,人工智能,30"}"#,
    ] {
        assert!(sectors::parse(Category::Concept, text).is_err());
    }
}
#[tokio::test]
#[ignore = "requires live free sector source"]
async fn live_industry_and_concept_snapshots() {
    let client = ticktop::market::client(&ticktop::config::Network::default()).unwrap();
    for category in [Category::Industry, Category::Concept] {
        let snapshot = sectors::fetch(&client, category).await.unwrap();
        assert!(snapshot.sectors.len() >= 20);
        eprintln!(
            "{}: {} sectors; {}",
            category.label(),
            snapshot.sectors.len(),
            snapshot.sectors[0].name
        );
    }
}
