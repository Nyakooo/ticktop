use serde_json::{Value, json};
use ticktop::turnover;
fn source(id: &str, previous: &str) -> Value {
    json!({"code":0,"data":{id:{"data":[
        {"date":"20261102","data":["0930 10 1 100.00","0931 10 2 200.10","0932 10 3 250.00"]},
        {"date":previous,"data":["0930 10 1 80.00","0931 10 2 150.20","0932 10 3 300.00"]}
    ]}}})
}
#[test]
fn previous_trading_session_and_completed_minute_use_exact_cents() {
    let a = source("sh000002", "20261030");
    let b = source("sz399107", "20261030");
    let c = turnover::compare(&a, &b).unwrap().unwrap();
    assert_eq!(c.date, "20261102");
    assert_eq!(c.previous_date, "20261030");
    assert_eq!(c.minute, "09:31"); // 09:32 is still a partial live minute.
    assert_eq!(c.current_fen, 40020);
    assert_eq!(c.previous_fen, 30040);
}
#[test]
fn mismatch_missing_or_corrupt_data_never_produces_a_judgement() {
    let a = source("sh000002", "20261030");
    let b = source("sz399107", "20261029");
    assert!(turnover::compare(&a, &b).unwrap().is_none());
    let mut b = source("sz399107", "20261030");
    b["data"]["sz399107"]["data"][0]["date"] = json!("20261103");
    assert!(turnover::compare(&a, &b).unwrap().is_none());
    let mut b = source("sz399107", "20261030");
    b["data"]["sz399107"]["data"][0]["data"] = json!(["0930 10 1 100.00"]);
    assert!(turnover::compare(&a, &b).unwrap().is_none());
    for bad in ["0931 10 2 NaN", "0931 10 2 -5", "0931 10 2 50", "0931 10 2"] {
        let mut b = source("sz399107", "20261030");
        b["data"]["sz399107"]["data"][0]["data"][1] = json!(bad);
        assert!(turnover::compare(&a, &b).is_err());
    }
}
#[test]
fn closing_minutes_and_shrinking_turnover_are_supported() {
    let mut a = source("sh000002", "20261030");
    let mut b = source("sz399107", "20261030");
    for (v, id) in [(&mut a, "sh000002"), (&mut b, "sz399107")] {
        v["data"][id]["data"][0]["data"][2] = json!("1500 10 3 250.00");
        v["data"][id]["data"][1]["data"][2] = json!("1500 10 3 300.00");
    }
    let c = turnover::compare(&a, &b).unwrap().unwrap();
    assert_eq!(c.minute, "15:00");
    assert!(c.delta_yi() < 0.);
}
#[tokio::test]
#[ignore = "requires public network; opt-in same-minute turnover verification"]
async fn live_same_minute_turnover() {
    let h = ticktop::market::client(&ticktop::config::Config::default().network).unwrap();
    let c = turnover::fetch(&h)
        .await
        .unwrap()
        .expect("no comparable turnover data");
    assert!(c.previous_date < c.date && c.current_fen > 0 && c.previous_fen > 0);
    println!(
        "{} vs {} at {}: current {:.2}亿, previous {:.2}亿, delta {:+.2}亿",
        c.date,
        c.previous_date,
        c.minute,
        c.current_fen as f64 / 1e10,
        c.previous_fen as f64 / 1e10,
        c.delta_yi()
    );
}
