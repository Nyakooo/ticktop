use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use ticktop::{
    app::{App, Modal},
    config::{Config, Mode, Paths, ProxyMode, parse_interval},
    market::{self, MarketEvent},
    model::{Period, Price, valid_id},
    search::SearchIndex,
    storage::{Cache, UserState, bundled_catalog},
    ui,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    let app = App::new(Config::default(), paths, false, true).unwrap();
    (dir, app)
}
fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let b = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = b[(x, y)].symbol();
                line.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn decimal_prices_are_exact_and_reject_invalid_values() {
    assert_eq!(Price::parse("268.50").unwrap().0, 2_685_000);
    assert_eq!(Price::parse("0.0001").unwrap().0, 1);
    assert_eq!(Price::parse("-5.72").unwrap().to_string(), "-5.72");
    for s in [
        "NaN",
        "1e4",
        "1.12345",
        "1.2.3",
        "999999999999999999999",
        "",
    ] {
        assert!(Price::parse(s).is_err(), "{s}");
    }
}
#[test]
fn stock_ids_are_validated_without_unicode_panics() {
    assert!(valid_id("bj920002"));
    assert!(!valid_id("沪6005"));
    assert!(!valid_id("xx600519"));
    assert!(!valid_id("sh60051x"));
}
#[test]
fn local_search_supports_chinese_code_pinyin_initials_and_typos() {
    let catalog = bundled_catalog();
    assert!(catalog.stocks.len() > 5000);
    let index = SearchIndex::new(&catalog.stocks);
    for q in [
        "宁德",
        "300750",
        "0750",
        "ningde",
        "ndsd",
        "ningdeshidai",
        "ningdeshidia",
        "宁德时带",
    ] {
        let results = index.search(q, 10);
        assert!(results.iter().any(|s| s.id == "sz300750"), "query {q}");
    }
    assert_eq!(index.search("300750", 10)[0].id, "sz300750");
    assert!(index.search("", 10).is_empty());
}
#[test]
fn exact_name_ranks_ahead_of_partial_names() {
    let stocks = vec![
        ticktop::model::Instrument {
            id: "sh600001".into(),
            name: "中国银行".into(),
        },
        ticktop::model::Instrument {
            id: "sh600002".into(),
            name: "银行".into(),
        },
    ];
    assert_eq!(
        SearchIndex::new(&stocks).search("银行", 10)[0].id,
        "sh600002"
    );
}
#[test]
fn groups_are_independent_and_aliases_survive_removal() {
    let mut state = UserState::default();
    let alias = state.alias("sz300750");
    let g = state.create_group("长期观察").unwrap();
    state.add("sz300750", g).unwrap();
    state.group = g;
    state.remove("sz300750");
    assert!(state.watchlist.contains(&"sz300750".into()));
    assert!(state.groups[0].symbols.contains(&"sz300750".into()));
    state.group = 0;
    state.remove("sz300750");
    assert!(
        !state
            .groups
            .iter()
            .any(|g| g.symbols.contains(&"sz300750".into()))
    );
    state.add("sz300750", 0).unwrap();
    assert_eq!(state.alias("sz300750"), alias);
    assert!(state.create_group("长期观察").is_err());
    state.delete_group(g);
    assert!(state.watchlist.contains(&"sz300750".into()));
}
#[test]
fn state_round_trip_preserves_mode_period_groups_and_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let state = UserState {
        mode: Mode::Minimal,
        period: Period::Month,
        group: 2,
        heatmap: true,
        ..UserState::default()
    };
    state.save(&path).unwrap();
    let loaded = UserState::load(&path).unwrap();
    assert_eq!(loaded.mode, Mode::Minimal);
    assert_eq!(loaded.period, Period::Month);
    assert_eq!(loaded.group, 2);
    assert_eq!(loaded.selected, state.selected);
    assert!(loaded.heatmap);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn corrupt_state_is_not_silently_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"not json").unwrap();
    assert!(UserState::load(&path).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "not json");
}
#[test]
fn tencent_quote_fixture_has_exact_prices_and_source_time() {
    let mut fields = vec![""; 35];
    fields[1] = "宁德时代";
    fields[3] = "229.07";
    fields[4] = "229.14";
    fields[5] = "226.86";
    fields[30] = "20261029161445";
    fields[33] = "232.11";
    fields[34] = "225.45";
    let response = format!(
        "v_sz300750=\"{}\";\nv_pv_none_match=\"1\";",
        fields.join("~")
    );
    let q = market::parse_quotes(&response).unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].price, Price(2290700));
    assert_eq!(q[0].change(), Price(-700));
    assert_eq!(q[0].source_time.offset().local_minus_utc(), 28800);
    assert_eq!(q[0].name, "宁德时代");
}
#[test]
fn intraday_lunch_break_uses_ordered_trading_points() {
    let v = serde_json::json!({"code":0,"data":{"sz300750":{"data":{"date":"20261029","data":["1130 229.01 50","1300 229.10 51"]}}}});
    let s = market::parse_history("sz300750", Period::Intraday, &v).unwrap();
    assert_eq!(s.points.len(), 2);
    assert_eq!(s.points[1].label, "13:00");
    assert_eq!(s.points[1].price, Price(2291000));
    assert!(market::parse_history("sz300750", Period::Week, &v).is_err());
}
#[test]
fn historical_daily_prices_are_not_adjusted_or_invented() {
    let v = serde_json::json!({"code":0,"data":{"sz300750":{"day":[["2026-10-28","233.34","229.14"],["2026-10-29","226.86","229.07"]]}}});
    let s = market::parse_history("sz300750", Period::Week, &v).unwrap();
    assert_eq!(s.points.len(), 2);
    assert_eq!(s.points[0].price, Price(2291400));
    assert_eq!(s.date, "2026-10-29");
}
#[test]
fn refresh_intervals_and_backoff_are_bounded() {
    assert_eq!(parse_interval("1m").unwrap(), 60);
    assert_eq!(parse_interval("5s").unwrap(), 5);
    for s in ["0", "1s", "999999999999999999999m", "3601", "no"] {
        assert!(parse_interval(s).is_err());
    }
    assert_eq!(market::retry_delay(5, 1), 10);
    assert_eq!(market::retry_delay(5, 9), 300);
    assert_eq!(market::retry_delay(600, 2), 600);
}
#[test]
fn proxy_credentials_are_not_exposed() {
    let mut config = Config::default();
    config.network.proxy_mode = ProxyMode::Custom;
    config.network.proxy_url = "socks5h://alice:secret@localhost:1080".into();
    config.validate().unwrap();
    assert!(market::client(&config.network).is_ok());
    let text = format!("{:?}", config);
    assert!(!text.contains("secret"));
    assert!(!ui::redact_proxy(&config.network.proxy_url).contains("alice"));
    assert!(!ui::redact_proxy(&config.network.proxy_url).contains("secret"));
    config.network.proxy_url = "file:///tmp/proxy".into();
    assert!(config.validate().is_err());
}
#[test]
fn sqlite_cache_keeps_quotes_and_series_separate() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join("cache.sqlite3")).unwrap();
    let quotes = market::demo_quotes(&["sz300750".into()]);
    cache.write("quotes", &quotes).unwrap();
    assert_eq!(cache.quotes().unwrap()[0].id, "sz300750");
    let s = market::demo_history("sz300750", Period::Week);
    cache.write("series:sz300750:week", &s).unwrap();
    assert_eq!(
        cache
            .series("sz300750", Period::Week)
            .unwrap()
            .unwrap()
            .points
            .len(),
        5
    );
    assert!(cache.series("sz300750", Period::Month).unwrap().is_none());
}
#[test]
fn switching_modes_keeps_group_stock_and_period() {
    let (_dir, mut app) = app();
    app.user.group = 2;
    app.ensure_selection();
    let before = (app.user.group, app.user.selected.clone(), app.user.period);
    app.key(key(KeyCode::Char('m')));
    assert_eq!(app.user.mode, Mode::Minimal);
    assert_eq!(
        before,
        (app.user.group, app.user.selected.clone(), app.user.period)
    );
    app.key(key(KeyCode::Char('h')));
    assert!(app.user.heatmap);
    assert_eq!(app.user.mode, Mode::Minimal);
}
#[test]
fn search_accepts_chinese_and_does_not_trigger_global_shortcuts() {
    let (_dir, mut app) = app();
    app.key(key(KeyCode::Char('/')));
    for c in "宁德".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    match &app.modal {
        Some(Modal::Search { results, .. }) => assert_eq!(results[0].id, "sz300750"),
        _ => panic!("search missing"),
    };
    app.key(key(KeyCode::Char('q')));
    assert!(!app.quit);
    app.key(key(KeyCode::Esc));
    assert!(app.modal.is_none());
}
#[test]
fn add_search_selects_target_group_and_persists() {
    let (_dir, mut app) = app();
    app.key(key(KeyCode::Char('a')));
    for c in "000001".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(Modal::AddTo { .. })));
    app.key(key(KeyCode::Down));
    app.key(key(KeyCode::Enter));
    assert_eq!(app.user.group, 1);
    assert!(app.user.groups[0].symbols.contains(&"sz000001".into()));
    let loaded = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    assert!(loaded.watchlist.contains(&"sz000001".into()));
}
#[test]
fn removal_requires_confirmation_and_cancel_preserves_stock() {
    let (_dir, mut app) = app();
    let id = app.user.selected.clone().unwrap();
    app.key(key(KeyCode::Char('d')));
    assert!(app.user.watchlist.contains(&id));
    app.key(key(KeyCode::Esc));
    assert!(app.user.watchlist.contains(&id));
    app.key(key(KeyCode::Char('d')));
    app.key(key(KeyCode::Char('y')));
    assert!(!app.user.watchlist.contains(&id));
}
#[test]
fn pause_freezes_display_and_resume_applies_latest_quote() {
    let (_dir, mut app) = app();
    let ids = app.user.watchlist.clone();
    let quotes = market::demo_quotes(&ids);
    app.on_market(MarketEvent::Quotes(quotes.clone()));
    let id = &ids[0];
    let old = app.quotes[id].price;
    app.key(key(KeyCode::Char(' ')));
    let mut changed = quotes;
    changed[0].price = Price(1234500);
    app.on_market(MarketEvent::Quotes(changed));
    assert_eq!(app.quotes[id].price, old);
    app.key(key(KeyCode::Char(' ')));
    assert_eq!(app.quotes[id].price, Price(1234500));
}
#[test]
fn every_mode_renders_at_multiple_terminal_sizes_without_leaking_names() {
    let (_dir, mut app) = app();
    let quotes = market::demo_quotes(&app.user.watchlist);
    app.on_market(MarketEvent::Quotes(quotes));
    app.on_market(MarketEvent::Chart(market::demo_history(
        "sz300750",
        Period::Intraday,
    )));
    for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16), (30, 10)] {
        for mode in [Mode::Full, Mode::Minimal] {
            for heatmap in [false, true] {
                app.user.mode = mode;
                app.user.heatmap = heatmap;
                let rendered = screen(&app, w, h);
                if mode == Mode::Minimal && w >= 46 {
                    assert!(!rendered.contains("宁德时代"));
                    assert!(!rendered.contains("重点关注"));
                    assert!(!rendered.contains("300750"));
                }
            }
        }
    }
}
#[test]
fn modal_inputs_and_empty_groups_render_without_panics() {
    let (_dir, mut app) = app();
    let id = app.user.create_group("空组").unwrap();
    app.user.group = id;
    app.ensure_selection();
    for modal in [
        Modal::Help { offset: 0 },
        Modal::Groups { index: 0 },
        Modal::GroupName {
            id: None,
            input: "中文测试".into(),
        },
        Modal::Settings {
            draft: app.config.clone(),
            field: 3,
            input: "http://alice:secret@localhost:7890".into(),
        },
    ] {
        app.modal = Some(modal);
        let rendered = screen(&app, 80, 24);
        assert!(!rendered.contains("secret"));
    }
    app.modal = None;
    assert!(screen(&app, 80, 24).contains("当前分组为空"));
}
#[test]
#[ignore = "manual performance check; run with --release --ignored --nocapture"]
fn bundled_catalog_search_is_fast_enough_for_typing() {
    let catalog = bundled_catalog();
    let start = std::time::Instant::now();
    let index = SearchIndex::new(&catalog.stocks);
    let build = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..10 {
        let _ = index.search("ndsd", 20);
    }
    let per_query = start.elapsed() / 10;
    eprintln!("index build {build:?}; per search {per_query:?}");
    assert!(per_query < std::time::Duration::from_millis(100));
}

#[test]
fn older_source_quotes_never_replace_newer_quotes_even_while_paused() {
    let (_dir, mut app) = app();
    let ids = app.user.watchlist.clone();
    let quotes = market::demo_quotes(&ids);
    let mut newer = quotes.clone();
    newer[0].price = Price(3456700);
    newer[0].source_time += chrono::Duration::minutes(1);
    app.key(key(KeyCode::Char(' ')));
    app.on_market(MarketEvent::Quotes(newer));
    app.on_market(MarketEvent::Quotes(quotes));
    app.key(key(KeyCode::Char(' ')));
    assert_eq!(app.quotes[&ids[0]].price, Price(3456700));
}
#[test]
fn malformed_config_error_does_not_echo_proxy_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    std::fs::write(
        &paths.config,
        "[network]\nproxy_url = \"http://alice:private-password@localhost\n",
    )
    .unwrap();
    let error = paths.load_config().unwrap_err();
    assert!(!format!("{error:#}").contains("private-password"));
}
#[test]
fn settings_save_refresh_interval_without_restarting() {
    let (_dir, mut app) = app();
    app.request_dirty = false;
    app.key(key(KeyCode::Char(',')));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.key(key(KeyCode::Char('1')));
    app.key(key(KeyCode::Char('0')));
    app.key(key(KeyCode::Enter));
    assert!(app.modal.is_none());
    assert_eq!(app.config.refresh.quotes_secs, 10);
    assert!(app.request_dirty);
    assert_eq!(app.paths.load_config().unwrap().0.refresh.quotes_secs, 10);
}

#[test]
fn pinyin_search_handles_polyphonic_company_names() {
    let index = SearchIndex::new(&bundled_catalog().stocks);
    for query in ["zhaoshangyinhang", "zsyh"] {
        assert!(
            index.search(query, 20).iter().any(|s| s.id == "sh600036"),
            "{query}"
        );
    }
    assert!(
        index
            .search("chongqingyinhang", 20)
            .iter()
            .any(|s| s.name == "重庆银行")
    );
}

#[test]
fn first_run_mode_save_does_not_persist_temporary_cli_network_settings() {
    let (_dir, mut app) = app();
    app.config.refresh.quotes_secs = 30;
    app.config.network.proxy_mode = ProxyMode::Custom;
    app.config.network.proxy_url = "http://temporary:secret@localhost:7890".into();
    app.modal = Some(Modal::Welcome(1));
    app.key(key(KeyCode::Enter));
    let saved = app.paths.load_config().unwrap().0;
    assert_eq!(saved.default_mode, Mode::Minimal);
    assert_eq!(saved.refresh.quotes_secs, 5);
    assert_eq!(saved.network.proxy_mode, ProxyMode::System);
    assert_eq!(app.config.refresh.quotes_secs, 30);
    assert!(
        !std::fs::read_to_string(&app.paths.config)
            .unwrap()
            .contains("secret")
    );
}

#[test]
fn failed_modal_group_save_is_reported_and_can_be_retried() {
    let (_dir, mut app) = app();
    let state_path = app.paths.data.join("demo-state.json");
    std::fs::create_dir(&state_path).unwrap();
    app.modal = Some(Modal::GroupName {
        id: None,
        input: "需重试分组".into(),
    });
    app.key(key(KeyCode::Enter));
    assert!(app.state_dirty);
    assert!(app.message.contains("保存失败"));
    std::fs::remove_dir(&state_path).unwrap();
    assert!(app.flush_state());
    assert!(!app.state_dirty);
    let saved = UserState::load(&state_path).unwrap();
    assert!(saved.groups.iter().any(|g| g.name == "需重试分组"));
}

#[test]
fn failed_add_to_group_save_is_retried_without_losing_the_stock() {
    let (_dir, mut app) = app();
    let state_path = app.paths.data.join("demo-state.json");
    std::fs::create_dir(&state_path).unwrap();
    let stock = app
        .catalog
        .iter()
        .find(|s| s.id == "sz000001")
        .unwrap()
        .clone();
    app.modal = Some(Modal::AddTo { stock, index: 1 });
    app.key(key(KeyCode::Enter));
    assert!(app.state_dirty);
    assert!(app.message.contains("保存失败"));
    std::fs::remove_dir(&state_path).unwrap();
    assert!(app.flush_state());
    let saved = UserState::load(&state_path).unwrap();
    assert!(saved.groups[0].symbols.contains(&"sz000001".into()));
}
