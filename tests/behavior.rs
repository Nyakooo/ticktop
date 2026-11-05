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
    let v = serde_json::json!({"code":0,"data":{"sz300750":{"day":[["2026-10-28","233.34","229.14","234.00","228.00","120000"],["2026-10-29","226.86","229.07","230.00","226.00","100000"]]}}});
    let s = market::parse_history("sz300750", Period::Day, &v).unwrap();
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
    cache.write("series:sz300750:kweek", &s).unwrap();
    assert_eq!(
        cache
            .series("sz300750", Period::Week)
            .unwrap()
            .unwrap()
            .points
            .len(),
        60
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
    for c in "sz000001".chars() {
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
fn every_mode_renders_at_multiple_terminal_sizes_with_minimal_stock_codes() {
    let (_dir, mut app) = app();
    app.on_market(MarketEvent::Sectors(ticktop::sectors::demo(
        ticktop::sectors::Category::Industry,
    )));
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
                    if !heatmap {
                        assert!(rendered.contains("300750"));
                    }
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
    std::fs::remove_file(&state_path).unwrap();
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
    std::fs::remove_file(&state_path).unwrap();
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

#[test]
fn mode_switch_is_saved_immediately_without_waiting_for_exit() {
    let (_dir, mut app) = app();
    app.key(key(KeyCode::Char('m')));
    let saved = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    assert_eq!(saved.mode, Mode::Minimal);
    assert_eq!(
        app.paths.load_config().unwrap().0.default_mode,
        Mode::Minimal
    );
    app.key(key(KeyCode::Char('m')));
    let saved = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    assert_eq!(saved.mode, Mode::Full);
}

#[test]
fn saved_mode_survives_restart_when_config_is_missing() {
    let (dir, mut app) = app();
    app.user.mode = Mode::Minimal;
    app.persist().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    let (config, first) = paths.load_config().unwrap();
    assert!(first);
    let restarted = App::new(config, paths, first, true).unwrap();
    assert_eq!(restarted.user.mode, Mode::Minimal);
    assert!(restarted.modal.is_none());
}

#[test]
fn new_state_uses_remembered_default_mode_without_welcome() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    let config = Config {
        default_mode: Mode::Minimal,
        ..Config::default()
    };
    paths.save_config(&config).unwrap();
    let app = App::new(config, paths, false, true).unwrap();
    assert_eq!(app.user.mode, Mode::Minimal);
}

#[test]
fn market_cloud_is_independent_of_watchlists_and_groups() {
    let (_dir, mut app) = app();
    let snapshot = ticktop::sectors::demo(ticktop::sectors::Category::Industry);
    let count = snapshot.sectors.len();
    app.on_market(MarketEvent::Sectors(snapshot));
    app.user.watchlist.clear();
    app.user.groups.clear();
    app.user.selected = None;
    app.key(key(KeyCode::Char('h')));
    assert_eq!(app.sector_snapshot().unwrap().sectors.len(), count);
    let full = screen(&app, 130, 36);
    assert!(full.contains("大盘云图"));
    assert!(full.contains("全部行业板块"));
    assert!(!full.contains("自选分组涨跌"));
    app.key(key(KeyCode::Char('m')));
    let minimal = screen(&app, 130, 36);
    assert!(minimal.contains("matrix"));
    assert!(minimal.contains("农业"));
    assert!(!minimal.contains("宁德时代"));
}

#[test]
fn all_sectors_are_reachable_and_stock_selection_is_preserved() {
    let (_dir, mut app) = app();
    app.on_market(MarketEvent::Sectors(ticktop::sectors::demo(
        ticktop::sectors::Category::Industry,
    )));
    let selected = app.user.selected.clone();
    let group = app.user.group;
    let period = app.user.period;
    app.key(key(KeyCode::Char('h')));
    app.set_viewport(80, 24);
    app.key(key(KeyCode::PageDown));
    assert_eq!(app.sector_index, app.cloud_page_size);
    app.key(key(KeyCode::End));
    assert_eq!(
        app.sector_index + 1,
        app.sector_snapshot().unwrap().sectors.len()
    );
    assert_eq!(app.user.selected, selected);
    assert_eq!(app.user.group, group);
    assert_eq!(app.user.period, period);
    app.key(key(KeyCode::Char('d')));
    assert!(app.modal.is_none());
    app.key(key(KeyCode::Tab));
    assert_eq!(
        app.user.sector_category,
        ticktop::sectors::Category::Concept
    );
    assert_eq!(app.sector_index, 0);
    assert!(app.request_dirty);
    assert_eq!(app.user.selected, selected);
}

#[test]
fn cloud_cache_and_pause_keep_actual_market_snapshots() {
    let (dir, mut app) = app();
    let initial = ticktop::sectors::demo(ticktop::sectors::Category::Industry);
    app.on_market(MarketEvent::Sectors(initial.clone()));
    app.key(key(KeyCode::Char(' ')));
    let mut latest = initial;
    latest.sectors[0].change_pct = Some(9.87);
    app.on_market(MarketEvent::Sectors(latest));
    assert_ne!(
        app.sector_snapshot().unwrap().sectors[0].change_pct,
        Some(9.87)
    );
    app.key(key(KeyCode::Char(' ')));
    assert_eq!(
        app.sector_snapshot().unwrap().sectors[0].change_pct,
        Some(9.87)
    );
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    let restarted = App::new(Config::default(), paths, false, true).unwrap();
    assert_eq!(
        restarted.sector_snapshot().unwrap().sectors[0].change_pct,
        Some(9.87)
    );
}

#[test]
fn sector_failure_never_substitutes_watchlist_data() {
    let (_dir, mut app) = app();
    app.user.heatmap = true;
    app.on_market(MarketEvent::Error {
        kind: market::Kind::Sectors,
        message: "failed".into(),
        retry_secs: 30,
    });
    assert!(app.sector_snapshot().is_none());
    assert!(screen(&app, 100, 30).contains("暂无全市场板块数据"));
}

#[test]
fn cloud_paging_returns_from_partial_last_page_in_both_modes() {
    for mode in [Mode::Full, Mode::Minimal] {
        let (_dir, mut app) = app();
        let mut snapshot = ticktop::sectors::demo(ticktop::sectors::Category::Industry);
        snapshot.sectors = (0..101)
            .map(|i| {
                let mut sector = snapshot.sectors[0].clone();
                sector.code = format!("test{i:03}");
                sector
            })
            .collect();
        app.on_market(MarketEvent::Sectors(snapshot));
        app.user.heatmap = true;
        app.user.mode = mode;
        app.set_viewport(80, 24);
        let page_size = app.cloud_page_size;
        app.key(key(KeyCode::Char('n')));
        assert_eq!(app.sector_index, page_size);
        app.key(key(KeyCode::Char('p')));
        assert_eq!(app.sector_index, 0);
        app.key(key(KeyCode::PageUp));
        assert!(!app.message.is_empty());
        app.key(key(KeyCode::End));
        let last_page = app.sector_index / page_size;
        app.key(key(KeyCode::PageDown));
        assert_eq!(app.sector_index, 100);
        app.key(key(KeyCode::PageUp));
        assert_eq!(app.sector_index / page_size, last_page - 1);
        app.key(key(KeyCode::Home));
        app.sector_index = page_size - 1;
        app.key(key(KeyCode::End));
        // Page navigation lands at page starts, even on a short final page.
        app.key(key(KeyCode::PageUp));
        assert_eq!(app.sector_index, (last_page - 1) * page_size);
    }
}

#[test]
fn mouse_controls_cloud_pages_categories_and_numeric_mode() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (_dir, mut app) = app();
    app.on_market(MarketEvent::Sectors(ticktop::sectors::demo(
        ticktop::sectors::Category::Industry,
    )));
    app.user.heatmap = true;
    let click = |x, y| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    ui::mouse(&mut app, click(10, 6), 80, 24);
    assert_eq!(app.sector_index, app.cloud_page_size);
    ui::mouse(&mut app, click(2, 6), 80, 24);
    assert_eq!(app.sector_index, 0);
    ui::mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
        80,
        24,
    );
    assert_eq!(app.sector_index, app.cloud_page_size);
    ui::mouse(&mut app, click(2, 22), 80, 24);
    assert_eq!(app.user.mode, Mode::Minimal);
    assert!(screen(&app, 80, 24).contains("matrix"));
    app.sector_index = 0;
    ui::mouse(&mut app, click(45, 8), 80, 24);
    assert_eq!(app.sector_index, 1);
    ui::mouse(&mut app, click(15, 4), 80, 24);
    assert_eq!(
        app.user.sector_category,
        ticktop::sectors::Category::Concept
    );
    ui::mouse(&mut app, click(25, 4), 80, 24);
    assert_eq!(app.user.sector_sort, ticktop::sectors::Sort::Losers);
    app.modal = Some(Modal::Help { offset: 0 });
    ui::mouse(&mut app, click(2, 22), 80, 24);
    assert_eq!(app.user.mode, Mode::Minimal);
}

#[test]
fn cloud_sort_orders_missing_values_last_and_persists_across_restart() {
    use ticktop::sectors::{Category, Sort};
    let (dir, mut app) = app();
    let mut snapshot = ticktop::sectors::demo(Category::Industry);
    snapshot.sectors.truncate(4);
    for (sector, (change, count)) in snapshot.sectors.iter_mut().zip([
        (Some(1.), 10),
        (Some(-2.), 40),
        (None, 30),
        (Some(1.), 20),
    ]) {
        sector.change_pct = change;
        sector.stock_count = count;
    }
    let code = snapshot.sectors[3].code.clone();
    app.on_market(MarketEvent::Sectors(snapshot.clone()));
    app.user.heatmap = true;
    assert_eq!(
        app.sector_snapshot().unwrap().sectors[0].code,
        snapshot.sectors[0].code
    );
    assert!(
        app.sector_snapshot()
            .unwrap()
            .sectors
            .last()
            .unwrap()
            .change_pct
            .is_none()
    );
    app.sector_index = 1;
    snapshot.sectors[1].change_pct = Some(5.);
    app.on_market(MarketEvent::Sectors(snapshot.clone()));
    assert_eq!(
        app.sector_snapshot().unwrap().sectors[app.sector_index].code,
        code
    );
    app.key(key(KeyCode::Char('s')));
    assert_eq!(app.user.sector_sort, Sort::Losers);
    assert_eq!(
        app.sector_snapshot().unwrap().sectors[0].change_pct,
        Some(1.)
    );
    assert!(
        app.sector_snapshot()
            .unwrap()
            .sectors
            .last()
            .unwrap()
            .change_pct
            .is_none()
    );
    app.key(key(KeyCode::Char('s')));
    assert_eq!(app.user.sector_sort, Sort::Size);
    assert_eq!(app.sector_snapshot().unwrap().sectors[0].stock_count, 40);
    app.persist().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    let restarted = App::new(Config::default(), paths, false, true).unwrap();
    assert_eq!(restarted.user.sector_sort, Sort::Size);
    assert_eq!(
        restarted.sector_snapshot().unwrap().sectors[0].stock_count,
        40
    );
}

#[test]
fn hovering_the_treemap_updates_details_without_repeated_redraws() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let (_dir, mut app) = app();
    app.on_market(MarketEvent::Sectors(ticktop::sectors::demo(
        ticktop::sectors::Category::Industry,
    )));
    app.user.heatmap = true;
    app.user.mode = Mode::Full;
    let event = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 70,
        row: 17,
        modifiers: KeyModifiers::NONE,
    };
    app.dirty = false;
    ui::mouse(&mut app, event, 80, 24);
    assert_ne!(app.sector_index, 0);
    assert!(app.dirty);
    let sector = &app.sector_snapshot().unwrap().sectors[app.sector_index];
    assert!(screen(&app, 80, 24).contains(&sector.name));
    app.dirty = false;
    ui::mouse(&mut app, event, 80, 24);
    assert!(!app.dirty);
}

#[test]
fn chips_render_both_modes_and_freeze_while_paused() {
    let (_dir, mut app) = app();
    let id = app.user.selected.clone().unwrap();
    app.user.heatmap = false;
    let d = ticktop::chips::demo(&id);
    app.on_market(MarketEvent::Chips(d.clone()));
    for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16), (30, 10)] {
        app.user.mode = Mode::Full;
        let full = screen(&app, w, h);
        if w >= 100 {
            assert!(full.contains("筹码峰"));
            assert!(full.contains("90%"));
        }
        app.user.mode = Mode::Minimal;
        let minimal = screen(&app, w, h);
        if w >= 80 {
            assert!(minimal.contains("筹码估算"));
            assert!(minimal.contains("90%"));
        }
    }
    app.key(key(KeyCode::Char(' ')));
    let mut changed = d.clone();
    changed.close += 1.;
    app.on_market(MarketEvent::Chips(changed.clone()));
    assert_eq!(app.chips[&id].close, d.close);
    app.key(key(KeyCode::Char(' ')));
    assert_eq!(app.chips[&id].close, changed.close);
    // Cache is isolated per symbol and survives a restart.
    let reopened = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert_eq!(reopened.chips[&id].close, changed.close);
}

#[tokio::test]
#[ignore = "requires public network; opt-in upstream contract check"]
async fn live_chips_daily_source() {
    let http = market::client(&Config::default().network).unwrap();
    let d = ticktop::chips::fetch(&http, "sz300750").await.unwrap();
    assert_eq!(d.id, "sz300750");
    assert!(d.days >= 100 && d.days <= 240);
    assert!((d.bins.iter().map(|b| b.1).sum::<f64>() - 1.).abs() < 1e-8);
    assert!(d.quantile(0.05) <= d.average() && d.average() <= d.quantile(0.95));
    println!(
        "{} {} {} days; mean {:.2}; 90% {:.2}–{:.2}; seed {:.2}%",
        d.id,
        d.date,
        d.days,
        d.average(),
        d.quantile(0.05),
        d.quantile(0.95),
        d.seed_remaining * 100.
    );
}

#[test]
fn chart_price_readout_tracks_sample_not_live_quote() {
    let (_dir, mut app) = app();
    app.user.mode = Mode::Full;
    app.user.heatmap = false;
    let id = app.user.selected.clone().unwrap();
    let mut series = market::demo_history(&id, app.user.period);
    let last = series.points.len() - 1;
    series.points[last].price = Price(1234500);
    app.on_market(MarketEvent::Chart(series.clone()));
    assert!(screen(&app, 160, 42).contains("末价 123.45 CNY"));
    app.chart_cursor = Some(0);
    assert!(screen(&app, 160, 42).contains(&format!("查看 {} CNY", series.points[0].price)));
}

#[test]
fn resident_indices_can_be_configured_disabled_and_opened_without_changing_groups() {
    let (_dir, mut app) = app();
    app.key(key(KeyCode::Char('i')));
    assert!(matches!(app.modal, Some(Modal::Settings { field: 6, .. })));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "上证指数,深证成指,恒生指数,科创50,沪深300".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    assert!(app.modal.is_none());
    assert_eq!(app.config.pinned_indices.len(), 5);
    let saved = app.paths.load_config().unwrap().0;
    assert_eq!(saved.pinned_indices, app.config.pinned_indices);
    let symbols = app.request().symbols;
    assert!(
        app.config
            .pinned_indices
            .iter()
            .all(|s| symbols.contains(s))
    );
    assert_eq!(symbols.iter().filter(|s| *s == "sh000001").count(), 1);
    app.on_market(MarketEvent::Quotes(market::demo_quotes(&symbols)));
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for heat in [false, true] {
            app.user.heatmap = heat;
            let rendered = screen(&app, 160, 42);
            assert!(rendered.contains(if mode == Mode::Full {
                "上证指数"
            } else {
                "sh000001"
            }));
        }
    }
    let previous = app.user.watchlist.clone();
    app.preview_index("hkHSI");
    assert_eq!(app.user.selected.as_deref(), Some("hkHSI"));
    assert!(!app.user.heatmap);
    assert_eq!(app.user.watchlist, previous);
    app.key(key(KeyCode::Char('i')));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.key(key(KeyCode::Enter));
    assert!(app.config.pinned_indices.is_empty());
    assert!(!app.request().symbols.contains(&"sh000001".into()));
}
#[test]
fn etf_detail_and_minimal_mode_keep_third_decimal_and_no_chip_model() {
    let (_dir, mut app) = app();
    app.user.add("sh510300", 0).unwrap();
    app.user.selected = Some("sh510300".into());
    app.user.heatmap = false;
    let quotes = market::parse_quotes(include_str!("fixtures/indices-etfs.txt")).unwrap();
    app.on_market(MarketEvent::Quotes(quotes));
    app.on_market(MarketEvent::Chart(market::demo_history(
        "sh510300",
        app.user.period,
    )));
    app.user.mode = Mode::Full;
    let full = screen(&app, 160, 42);
    assert!(full.contains("4.065"));
    assert!(full.contains("筹码峰：ETF 暂不支持"));
    assert!(!full.contains("筹码峰 · 估算"));
    assert!(!full.contains("筹码估算 加载中"));
    app.user.mode = Mode::Minimal;
    assert!(screen(&app, 160, 42).contains("4.065"));
}

#[test]
fn multi_index_headers_keep_cloud_geometry_and_mouse_targets_in_sync() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (_dir, mut app) = app();
    app.config.pinned_indices = ticktop::instruments::INDICES
        .iter()
        .take(6)
        .map(|(id, _)| (*id).to_string())
        .collect();
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for (w, h) in [(160, 42), (100, 30), (46, 16)] {
            let _ = screen(&app, w, h);
            app.set_viewport(w, h);
            let r = ui::content_area(&app, w, h);
            assert_eq!(
                (app.cloud_columns, app.cloud_page_size),
                App::cloud_dimensions(r.width, r.height, mode)
            );
        }
    }
    app.user.mode = Mode::Full;
    let small = screen(&app, 46, 16);
    assert!(small.contains("+6 项"));
    ui::mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        },
        46,
        16,
    );
    assert!(matches!(app.modal, Some(Modal::Settings { field: 6, .. })));
    app.key(key(KeyCode::Esc));
    let before = app.user.watchlist.clone();
    ui::mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        },
        160,
        42,
    );
    assert_eq!(app.user.selected.as_deref(), Some("sh000001"));
    assert_eq!(app.user.watchlist, before);
}

#[test]
fn intraday_axis_is_fixed_and_ignores_lunch_duration() {
    use ticktop::ui::intraday_position;
    assert_eq!(intraday_position("09:50", false), Some(20.));
    assert_eq!(intraday_position("11:30", false), Some(120.));
    assert_eq!(intraday_position("13:00", false), Some(120.));
    assert_eq!(intraday_position("15:00", false), Some(240.));
    assert_eq!(intraday_position("09:15", false), Some(0.));
    assert_eq!(intraday_position("16:00", true), Some(330.));
    assert_eq!(intraday_position("13:15", true), Some(165.));
    assert_eq!(intraday_position("09:99", false), None);
}
#[test]
fn mainland_volume_amount_and_turnover_are_optional_and_unit_preserving() {
    let mut f = vec![""; 50];
    f[1] = "平安银行";
    f[3] = "11.69";
    f[4] = "11.64";
    f[5] = "11.60";
    f[6] = "361600";
    f[30] = "20261102095215";
    f[33] = "11.73";
    f[34] = "11.57";
    f[37] = "42163";
    f[38] = "0.19";
    f[49] = "1.99";
    let parse = |f: &[&str]| {
        market::parse_quotes(&format!("v_sz000001=\"{}\";", f.join("~")))
            .unwrap()
            .remove(0)
    };
    let q = parse(&f);
    assert_eq!(q.volume_lots, Some(361600.));
    assert_eq!(q.amount_wan, Some(42163.));
    assert_eq!(q.turnover_percent, Some(0.19));
    assert_eq!(q.volume_ratio, Some(1.99));
    f[49] = "NaN";
    f[38] = "NaN";
    f[37] = "-1";
    f[6] = "";
    let q = parse(&f);
    assert!(q.volume_ratio.is_none());
    assert!(q.volume_lots.is_none() && q.amount_wan.is_none() && q.turnover_percent.is_none());
}

#[test]
fn market_amount_stays_visible_across_modes_and_views() {
    let (_dir, mut app) = app();
    let mut quotes = market::demo_quotes(&app.request().symbols);
    for q in &mut quotes {
        q.amount_wan = Some(1000000.);
    }
    app.on_market(MarketEvent::Quotes(quotes));
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for heat in [false, true] {
            app.user.heatmap = heat;
            let rendered = screen(&app, 160, 42);
            assert!(rendered.contains("两市A股成交额 200.00亿"));
            assert!(!rendered.contains("预计"));
        }
    }
    app.user.heatmap = false;
    app.user.mode = Mode::Full;
    let id = app.user.selected.clone().unwrap();
    let mut series = market::demo_history(&id, Period::Intraday);
    series.points.truncate(21);
    app.on_market(MarketEvent::Chart(series));
    let rendered = screen(&app, 160, 42);
    assert!(rendered.contains("15:00"));
    assert!(rendered.contains("成交量 分时"));
    assert!(rendered.contains("换手率 1.23%"));
    assert!(rendered.contains("量比 1.56  换手率 1.23%"));
    app.user.mode = Mode::Minimal;
    assert!(screen(&app, 160, 42).contains("量比 1.56  换手率 1.23%"));
}

#[test]
fn minute_volumes_are_differences_not_cumulative_and_resets_are_missing() {
    let v = serde_json::json!({"code":0,"data":{"sz300750":{"data":{"date":"20261102","data":["0930 20.00 100","0931 20.10 160","0932 20.05 160","0933 20.00 10","0934 20.00 30","0935 20.00 NaN","0936 20.00 90"]}}}});
    let s = market::parse_history("sz300750", Period::Intraday, &v).unwrap();
    assert_eq!(
        s.points.iter().map(|p| p.volume).collect::<Vec<_>>(),
        vec![Some(100.), Some(60.), Some(0.), None, Some(20.), None, None]
    );
    assert!(s.points.iter().all(|p| p.ohlc.is_none()));
}
#[test]
fn candle_periods_parse_ohlc_and_own_volume_without_crossing_caches() {
    for period in [Period::Day, Period::Week, Period::Month] {
        let mut v = serde_json::json!({"code":0,"data":{"sz300750":{}}});
        v["data"]["sz300750"][period.source_key()] =
            serde_json::json!([["2026-11-02", "20.00", "21.00", "22.00", "19.00", "120000"]]);
        let s = market::parse_history("sz300750", period, &v).unwrap();
        let c = s.points[0].ohlc.as_ref().unwrap();
        assert_eq!(
            (c.open, c.high, c.low, s.points[0].price),
            (Price(200000), Price(220000), Price(190000), Price(210000))
        );
        assert_eq!(s.points[0].volume, Some(120000.));
        v["data"]["sz300750"][period.source_key()][0][3] = serde_json::json!("20.50");
        assert!(market::parse_history("sz300750", period, &v).is_err());
    }
    let mut p = Period::Intraday;
    for expected in [Period::Day, Period::Week, Period::Month, Period::Intraday] {
        p = p.next(false);
        assert_eq!(p, expected);
    }
    assert_eq!(p.next(true), Period::Month);
    assert_ne!(Period::Intraday.key(), Period::Day.key());
    let old: ticktop::model::Point =
        serde_json::from_str(r#"{"label":"09:30","price":200000}"#).unwrap();
    assert!(old.volume.is_none() && old.ohlc.is_none());
}
#[test]
fn daily_candles_and_volume_render_and_cursor_pans_to_older_dates() {
    let (_dir, mut app) = app();
    app.user.period = Period::Day;
    let id = app.user.selected.clone().unwrap();
    let mut series = market::demo_history(&id, Period::Day);
    series.points[0].label = "2020-01-02".into();
    app.on_market(MarketEvent::Chart(series));
    app.on_market(MarketEvent::Quotes(market::demo_quotes(
        &app.request().symbols,
    )));
    for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16)] {
        let _ = screen(&app, w, h);
    }
    let rendered = screen(&app, 160, 42);
    assert!(rendered.contains("日 K") && rendered.contains("周 K") && rendered.contains("月 K"));
    assert!(rendered.contains("成交量 日 K"));
    for (w, h) in [(160, 24), (80, 24), (46, 60)] {
        let small = screen(&app, w, h);
        assert!(
            !small.contains("成交量 日 K"),
            "volume panel must hide at {w}x{h}"
        );
        assert!(small.contains("日 K"));
    }
    assert!(screen(&app, 160, 42).contains("成交量 日 K"));
    assert!(!rendered.contains("2020-01-02"));
    app.key(key(KeyCode::Home));
    assert!(screen(&app, 160, 42).contains("2020-01-02"));
    app.user.mode = Mode::Minimal;
    let rendered = screen(&app, 160, 42);
    assert!(rendered.contains("日期 2020-01-02") && rendered.contains("日成交量"));
    assert!(rendered.contains(" / 日 K"));
    assert!(rendered.contains("收盘价"));
    assert!(!rendered.contains(" / kday"));
    app.persist().unwrap();
    let state = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    assert_eq!(state.period, Period::Day);
}

#[test]
fn turnover_comparison_is_aligned_optional_and_respects_pause() {
    let (_dir, mut app) = app();
    let quotes = market::demo_quotes(&app.request().symbols);
    let time = quotes
        .iter()
        .find(|q| q.id == "sh000002")
        .unwrap()
        .source_time;
    app.on_market(MarketEvent::Quotes(quotes));
    let comparison = ticktop::turnover::Comparison {
        date: time.format("%Y%m%d").to_string(),
        previous_date: "20261030".into(),
        minute: time.format("%H:%M").to_string(),
        current_fen: 12_000_000_000_000,
        previous_fen: 10_000_000_000_000,
        fetched_at: chrono::Utc::now(),
    };
    app.on_market(MarketEvent::Turnover(Some(comparison.clone())));
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for heat in [false, true] {
            app.user.heatmap = heat;
            assert!(screen(&app, 160, 42).contains("较昨同时放量 200.00亿"));
        }
    }
    app.key(key(KeyCode::Char(' ')));
    app.on_market(MarketEvent::Turnover(None));
    assert!(app.turnover.is_some());
    app.key(key(KeyCode::Char(' ')));
    assert!(app.turnover.is_none());
    let mut stale = comparison;
    stale.date = "20200101".into();
    app.on_market(MarketEvent::Turnover(Some(stale)));
    let rendered = screen(&app, 160, 42);
    assert!(!rendered.contains("较昨同时"));
    assert!(rendered.contains("两市A股成交额"));
    app.on_market(MarketEvent::Error {
        kind: market::Kind::Turnover,
        message: "unavailable".into(),
        retry_secs: 60,
    });
    assert!(app.turnover.is_none() && app.quote_error.is_none());
}

#[test]
fn pinning_manual_order_and_aliases_persist_without_cross_group_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut state = UserState::default();
    let ids = state.watchlist.clone();
    let mut numeric = state.clone();
    numeric.set_alias(&ids[0], "007").unwrap();
    numeric.add("sh600000", 0).unwrap();
    assert_eq!(numeric.alias("sh600000"), "008");
    numeric.save(&path).unwrap();
    let other = state.groups[0].symbols.clone();
    state.move_symbol(&ids[1], false).unwrap();
    assert_eq!(state.visible()[0], ids[1]);
    assert_eq!(state.groups[0].symbols, other);
    state.toggle_pin(&ids[2]).unwrap();
    assert_eq!(state.visible()[0], ids[2]);
    assert!(state.move_symbol(&ids[2], true).is_err());
    state.toggle_pin(&ids[1]).unwrap();
    state.move_symbol(&ids[2], false).unwrap();
    assert_eq!(state.visible()[0], ids[2]);
    state.set_alias(&ids[0], "cpu0").unwrap();
    assert!(state.set_alias(&ids[1], "CPU0").is_err());
    assert!(state.set_alias(&ids[1], "001").is_err());
    assert!(state.set_alias(&ids[1], "\u{1b}oops").is_err());
    assert!(state.set_alias(&ids[1], "abcdefghijklmn").is_err());
    assert!(state.set_alias(&ids[1], "\u{200b}").is_err());
    state.group = 1;
    assert!(!state.is_pinned(&ids[2]));
    state.toggle_pin(&ids[0]).unwrap();
    state.set_alias(&ids[1], "core").unwrap();
    state.rename_group(1, "我的观察").unwrap();
    state.save(&path).unwrap();
    let mut loaded = UserState::load(&path).unwrap();
    assert_eq!(loaded.alias(&ids[0]), "cpu0");
    assert!(loaded.is_pinned(&ids[0]));
    assert_eq!(loaded.group_name(), "我的观察");
    loaded.remove(&ids[0]);
    assert!(!loaded.visible().contains(&ids[0]));
    loaded.group = 0;
    assert!(loaded.watchlist.contains(&ids[0]));
    assert_eq!(loaded.alias(&ids[0]), "cpu0");
    loaded.remove(&ids[0]);
    assert!(!loaded.custom_aliases.contains_key(&ids[0]));
    loaded.delete_group(1);
    loaded.save(&path).unwrap();
    UserState::load(&path).unwrap();
    let mut legacy = serde_json::to_value(UserState::default()).unwrap();
    legacy.as_object_mut().unwrap().remove("pinned");
    legacy.as_object_mut().unwrap().remove("custom_aliases");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let legacy = UserState::load(&path).unwrap();
    assert!(legacy.pinned.is_empty() && legacy.custom_aliases.is_empty());
}
#[test]
fn watchlist_shortcuts_rename_and_minimal_alias_replace_code() {
    let (_dir, mut app) = app();
    let ids = app.user.watchlist.clone();
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    assert_eq!(app.visible()[1], ids[0]);
    app.key(key(KeyCode::Char('t')));
    assert_eq!(app.visible()[0], ids[0]);
    assert_eq!(app.user.selected.as_deref(), Some(ids[0].as_str()));
    app.key(key(KeyCode::Char('e')));
    for c in "worker-1".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    app.user.mode = Mode::Minimal;
    let text = screen(&app, 80, 24);
    assert!(text.contains("^worker-1") && text.contains("worker-1 /"));
    assert!(!text.contains(&ids[0]));
    assert!(text.contains(&ids[1]));
    app.key(key(KeyCode::Char('e')));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.key(key(KeyCode::Esc));
    assert_eq!(app.user.alias(&ids[0]), "worker-1");
    app.user.group = 1;
    app.key(key(KeyCode::Char('r')));
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "观察清单".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    assert_eq!(app.user.group_name(), "观察清单");
    let first = app.user.groups[0].id;
    app.key(key(KeyCode::Char('g')));
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    app.key(key(KeyCode::Esc));
    assert_eq!(app.user.groups[1].id, first);
    let saved = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    assert_eq!(saved.alias(&ids[0]), "worker-1");
    assert_eq!(saved.groups[1].name, "观察清单");
}

#[test]
fn default_settings_are_created_and_replaced_files_load_without_import() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let pa = Paths::resolve(Some(&a.path().to_path_buf())).unwrap();
    let pb = Paths::resolve(Some(&b.path().to_path_buf())).unwrap();
    let (mut config, first) = pa.load_config().unwrap();
    assert!(first && pa.config.is_file());
    assert!(!pa.load_config().unwrap().1);
    config.default_mode = Mode::Minimal;
    config.refresh.quotes_secs = 17;
    pa.save_config(&config).unwrap();
    let state_path = pa.data.join("state.json");
    let mut state = UserState::load(&state_path).unwrap();
    assert!(state_path.is_file());
    let id = state.watchlist[0].clone();
    state.set_alias(&id, "cpu0").unwrap();
    state.toggle_pin(&id).unwrap();
    state.rename_group(1, "随身观察").unwrap();
    state.mode = Mode::Minimal;
    state.save(&state_path).unwrap();
    std::fs::copy(&pa.config, &pb.config).unwrap();
    std::fs::copy(&state_path, pb.data.join("state.json")).unwrap();
    let (loaded, first) = pb.load_config().unwrap();
    assert!(!first && loaded.refresh.quotes_secs == 17);
    let app = App::new(loaded, pb, false, false).unwrap();
    assert_eq!(app.user.mode, Mode::Minimal);
    assert_eq!(app.user.alias(&id), "cpu0");
    assert!(app.user.is_pinned(&id));
    assert_eq!(app.user.groups[0].name, "随身观察");
    assert!(screen(&app, 100, 30).contains("cpu0"));
}
#[test]
fn unknown_or_invalid_settings_explain_recovery_without_modifying_files() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(&dir.path().to_path_buf())).unwrap();
    for raw in [
        b"unknown_setting = true".as_slice(),
        b"[refresh]\nquotes_secs = 0",
        b"\xff\xfe",
    ] {
        std::fs::write(&paths.config, raw).unwrap();
        let error = format!("{:#}", paths.load_config().unwrap_err());
        assert!(
            error.contains(&paths.config.display().to_string())
                && error.contains("删除")
                && error.contains("未修改")
        );
        assert_eq!(std::fs::read(&paths.config).unwrap(), raw);
    }
    let state_path = paths.data.join("state.json");
    for raw in [
        r#"{"unknown_setting":true}"#,
        r#"{"mode":"nonsense"}"#,
        "not json",
    ] {
        std::fs::write(&state_path, raw).unwrap();
        let error = format!("{:#}", UserState::load(&state_path).unwrap_err());
        assert!(error.contains(&state_path.display().to_string()) && error.contains("删除"));
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), raw);
    }
    std::fs::remove_file(&paths.config).unwrap();
    std::fs::remove_file(&state_path).unwrap();
    paths.load_config().unwrap();
    UserState::load(&state_path).unwrap();
    assert!(paths.config.is_file() && state_path.is_file());
    assert!(!ticktop::storage::create_settings(&state_path, b"replacement").unwrap());
    assert!(UserState::load(&state_path).is_ok());
}

#[test]
fn stock_chips_remain_visible_in_all_chart_periods_and_narrow_windows() {
    let (_dir, mut app) = app();
    let id = app.user.selected.clone().unwrap();
    app.on_market(MarketEvent::Chips(ticktop::chips::demo(&id)));
    for period in Period::ALL {
        app.user.period = period;
        app.on_market(MarketEvent::Chart(market::demo_history(&id, period)));
        for (w, h) in [(160, 42), (130, 36), (100, 30), (80, 24)] {
            let rendered = screen(&app, w, h);
            assert!(
                rendered.contains("筹码峰"),
                "chips hidden in {period:?} at {w}x{h}"
            );
            assert!(!rendered.contains("▇"), "chip bars must remain thin lines");
        }
    }
}

#[test]
fn index_chip_status_is_explicit_and_switching_to_stock_keeps_its_chips() {
    let (_dir, mut app) = app();
    app.on_market(MarketEvent::Chips(ticktop::chips::demo("sz300750")));
    app.user.add("sh000001", 0).unwrap();
    app.user.selected = Some("sh000001".into());
    app.on_market(MarketEvent::Chart(market::demo_history(
        "sh000001",
        Period::Intraday,
    )));
    let index = screen(&app, 250, 70);
    assert!(index.contains("筹码峰：指数不适用"));
    assert!(!index.contains("筹码峰 · 估算"));
    app.user.selected = Some("sz300750".into());
    app.on_market(MarketEvent::Chart(market::demo_history(
        "sz300750",
        Period::Intraday,
    )));
    let stock = screen(&app, 250, 70);
    assert!(stock.contains("筹码峰 · 估算"));
    assert!(!stock.contains("指数不适用"));
}

#[test]
fn minimal_name_peek_restores_alias_without_changing_saved_state() {
    use crossterm::event::KeyEventKind;
    let (_dir, mut app) = app();
    app.modal = None;
    app.user.mode = Mode::Minimal;
    app.key_releases = true;
    let id = app.user.selected.clone().unwrap();
    let name = app
        .catalog
        .iter()
        .find(|s| s.id == id)
        .unwrap()
        .name
        .clone();
    app.user.set_alias(&id, "demo").unwrap();
    let before = serde_json::to_string(&app.user).unwrap();
    app.key(key(KeyCode::Char('v')));
    assert_eq!(app.minimal_label(&id), name);
    assert!(screen(&app, 120, 35).contains(&name));
    app.key(KeyEvent::new_with_kind(
        KeyCode::Char('v'),
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert!(app.reveal_names);
    app.key(KeyEvent::new_with_kind(
        KeyCode::Char('v'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(app.minimal_label(&id), "demo");
    assert_eq!(serde_json::to_string(&app.user).unwrap(), before);
    app.key(key(KeyCode::Char('v')));
    app.conceal_names();
    assert!(!app.reveal_names);
}

#[test]
fn minimal_name_peek_fallback_and_text_input_remain_usable() {
    use crossterm::event::KeyEventKind;
    let (_dir, mut app) = app();
    app.modal = None;
    app.user.mode = Mode::Minimal;
    app.key(key(KeyCode::Char('v')));
    assert!(app.reveal_names);
    app.key(KeyEvent::new_with_kind(
        KeyCode::Char('v'),
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert!(app.reveal_names);
    app.key(key(KeyCode::Char('v')));
    assert!(!app.reveal_names);
    app.key(key(KeyCode::Char('/')));
    app.key(key(KeyCode::Char('v')));
    assert!(matches!(&app.modal, Some(Modal::Search { query, .. }) if query == "v"));
    assert!(!app.reveal_names);
}

#[test]
fn jk_match_stock_selection_arrows_in_both_modes() {
    for mode in [Mode::Full, Mode::Minimal] {
        let (_dir, mut app) = app();
        app.modal = None;
        app.user.mode = mode;
        app.user.heatmap = false;
        app.user.add("sh600519", 0).unwrap();
        app.user.add("sz300750", 0).unwrap();
        let initial = app.user.selected.clone();
        let period = app.user.period;
        app.key(key(KeyCode::Down));
        let next = app.user.selected.clone();
        app.key(key(KeyCode::Up));
        assert_eq!(app.user.selected, initial);
        app.key(key(KeyCode::Char('j')));
        assert_eq!(app.user.selected, next);
        app.key(key(KeyCode::Char('k')));
        assert_eq!(app.user.selected, initial);
        assert_eq!(app.user.period, period);
        app.key(key(KeyCode::Char('/')));
        app.key(key(KeyCode::Char('j')));
        app.key(key(KeyCode::Char('k')));
        assert!(matches!(&app.modal, Some(Modal::Search { query, .. }) if query == "jk"));
    }
}

#[test]
fn shifted_arrows_inspect_chart_without_switching_stock_or_period() {
    let (_dir, mut app) = app();
    app.modal = None;
    let selected = app.user.selected.clone();
    let period = app.user.period;
    app.on_market(MarketEvent::Chart(market::demo_history(
        selected.as_ref().unwrap(),
        period,
    )));
    let last = app.series().unwrap().points.len() - 1;
    assert!(last > 0);
    app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    assert_eq!(app.chart_cursor, Some(last - 1));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert_eq!(app.chart_cursor, Some(last));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert_eq!(app.chart_cursor, Some(last));
    app.key(key(KeyCode::Home));
    app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    assert_eq!(app.chart_cursor, Some(0));
    app.key(key(KeyCode::End));
    assert_eq!(app.chart_cursor, Some(last));
    assert_eq!(app.user.selected, selected);
    assert_eq!(app.user.period, period);
    app.key(key(KeyCode::Right));
    assert_ne!(app.user.period, period);
    assert_eq!(app.user.selected, selected);
}

#[test]
fn closing_turnover_survives_stale_fetch_failure_and_restart() {
    use chrono::{TimeZone, Utc};
    let (_dir, mut app) = app();
    let zone = chrono::FixedOffset::east_opt(28800).unwrap();
    let mut quotes = market::demo_quotes(&["sh000002".into(), "sz399107".into()]);
    for q in &mut quotes {
        q.source_time = zone.with_ymd_and_hms(2026, 11, 3, 15, 10, 0).unwrap();
    }
    app.on_market(MarketEvent::Quotes(quotes));
    app.on_market(MarketEvent::Turnover(Some(ticktop::turnover::Comparison {
        date: "20261103".into(),
        previous_date: "20261102".into(),
        minute: "15:00".into(),
        current_fen: 15_000_000_000_000,
        previous_fen: 16_000_000_000_000,
        fetched_at: Utc::now() - chrono::Duration::hours(4),
    })));
    assert!(screen(&app, 250, 50).contains("缩量"));
    app.on_market(MarketEvent::Error {
        kind: market::Kind::Turnover,
        message: "offline".into(),
        retry_secs: 60,
    });
    assert!(screen(&app, 250, 50).contains("缩量"));
    let reopened = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert!(screen(&reopened, 250, 50).contains("缩量"));
    let mut next_day = market::demo_quotes(&["sh000002".into(), "sz399107".into()]);
    for q in &mut next_day {
        q.source_time = zone.with_ymd_and_hms(2026, 11, 4, 9, 35, 0).unwrap();
    }
    app.on_market(MarketEvent::Quotes(next_day));
    assert!(!screen(&app, 250, 50).contains("较前日收盘"));
}

#[test]
fn market_pages_keep_selections_periods_and_h_return_independent() {
    use ticktop::overseas::{Page, Session};
    let (_dir, mut app) = app();
    app.modal = None;
    let stock = app.user.selected.clone();
    let period = app.user.period;
    app.key(key(KeyCode::F(2)));
    assert_eq!(app.page(), Page::Indices);
    app.key(key(KeyCode::Char('j')));
    assert_eq!(app.user.overseas.index_selected, "^KS11");
    app.key(key(KeyCode::Enter));
    app.key(key(KeyCode::Right));
    assert_eq!(app.user.overseas.index_period, Period::Day);
    app.key(key(KeyCode::Char('h')));
    assert_eq!(app.page(), Page::Heatmap);
    app.key(key(KeyCode::Char('h')));
    assert_eq!(app.page(), Page::Indices);
    app.key(key(KeyCode::F(3)));
    app.key(key(KeyCode::Char('j')));
    assert_eq!(app.user.overseas.us_selected.as_deref(), Some("NVDA"));
    app.key(key(KeyCode::Tab));
    assert_eq!(app.user.overseas.session, Session::Pre);
    app.key(key(KeyCode::F(1)));
    assert_eq!(app.user.selected, stock);
    assert_eq!(app.user.period, period);
    app.key(key(KeyCode::F(2)));
    assert_eq!(app.user.overseas.index_selected, "^KS11");
    assert_eq!(app.user.overseas.index_period, Period::Day);
    app.persist().unwrap();
    let restarted = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert_eq!(restarted.page(), Page::Indices);
    assert_eq!(restarted.user.overseas.us_selected.as_deref(), Some("NVDA"));
    assert_eq!(
        restarted.request().overseas.unwrap().focus.as_deref(),
        Some("^KS11")
    );
    assert!(restarted.request().focus.is_none());
}

#[test]
fn overseas_subscription_group_alias_pause_and_minimal_render() {
    use ticktop::overseas::{self, Page, Session};
    let (_dir, mut app) = app();
    app.modal = None;
    app.set_page(Page::Us);
    fn input(app: &mut App, command: char, value: &str) {
        app.key(key(KeyCode::Char(command)));
        for ch in value.chars() {
            app.key(key(KeyCode::Char(ch)));
        }
        app.key(key(KeyCode::Enter));
    }
    input(&mut app, 'a', "微软");
    assert!(app.user.overseas.subscriptions.contains(&"MSFT".into()));
    input(&mut app, 'G', "长期关注");
    input(&mut app, 'g', "长期关注");
    assert_eq!(app.user.overseas.visible(), ["MSFT"]);
    input(&mut app, 'e', "office");
    app.user.mode = Mode::Minimal;
    app.on_market(MarketEvent::Overseas(
        vec![overseas::demo("MSFT", Period::Intraday)],
        vec![],
    ));
    let rendered = screen(&app, 120, 35);
    assert!(rendered.contains("office"));
    assert!(!rendered.contains("MSFT"));
    app.key(key(KeyCode::Char('v')));
    assert!(screen(&app, 120, 35).contains("微软"));
    app.key(key(KeyCode::Char('v')));
    app.key(key(KeyCode::Char(' ')));
    let old = app.overseas_quote("MSFT").unwrap().price;
    let mut update = overseas::demo("MSFT", Period::Intraday);
    update.regular.price = 123.;
    app.on_market(MarketEvent::Overseas(vec![update], vec![]));
    assert_eq!(app.overseas_quote("MSFT").unwrap().price, old);
    app.key(key(KeyCode::Char(' ')));
    assert_eq!(app.overseas_quote("MSFT").unwrap().price, 123.);
    app.user.overseas.session = Session::Overnight;
    assert!(app.overseas_quote("MSFT").is_none());
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16)] {
            let s = screen(&app, w, h);
            assert!(s.contains("F2"), "{w}x{h} {mode:?}\n{s}");
        }
    }
    app.key(key(KeyCode::Char('d')));
    app.key(key(KeyCode::Char('y')));
    assert!(app.user.overseas.visible().is_empty());
    assert!(app.user.overseas.subscriptions.contains(&"MSFT".into()));
}

#[test]
fn foreign_cache_and_navigation_mouse_work_in_both_modes() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ticktop::overseas::{self, Page};
    let (_dir, mut app) = app();
    app.modal = None;
    app.set_page(Page::Indices);
    for period in Period::ALL {
        app.on_market(MarketEvent::Overseas(
            vec![overseas::demo("^N225", period)],
            vec![],
        ));
    }
    app.persist().unwrap();
    let mut app = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert!(app.overseas_series().is_some());
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16)] {
            for period in Period::ALL {
                app.user.overseas.index_period = period;
                let s = screen(&app, w, h);
                assert!(s.contains("F2"), "{w}x{h} {mode:?}\n{s}");
            }
        }
    }
    app.user.mode = Mode::Full;
    // Foreign pages have a one-line header at y=1; navigation shares that line.
    ui::mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 90,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        160,
        42,
    );
    assert_eq!(app.page(), Page::Indices); // Blank title space does not select a tab.
    ui::mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 35,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        160,
        42,
    );
    assert_eq!(app.page(), Page::Us);
}

#[test]
fn index_overview_follow_search_detail_sort_alias_and_empty_state_persist() {
    use ticktop::overseas::Page;
    let (_dir, mut app) = app();
    app.modal = None;
    app.set_page(Page::Indices);
    let watchlist = app.user.watchlist.clone();
    assert!(app.user.overseas.index_all);
    assert!(!app.user.overseas.index_detail);
    assert_eq!(app.request().overseas.unwrap().ids.len(), 13);
    assert!(app.request().overseas.unwrap().focus.is_none());
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for (w, h) in [(160, 42), (100, 30), (80, 24), (46, 16)] {
            let s = screen(&app, w, h);
            assert!(s.contains("F2"));
            assert!(!s.contains("Shift+"));
        }
    }
    app.user.mode = Mode::Full;
    app.key(key(KeyCode::Char('a')));
    for c in "沪深300".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    assert!(!app.user.overseas.index_all);
    assert!(
        app.user
            .overseas
            .index_watchlist
            .contains(&"sh000300".into())
    );
    assert_eq!(app.user.overseas.index_selected, "sh000300");
    app.key(key(KeyCode::Char('t')));
    assert_eq!(app.user.overseas.index_rows()[0], "sh000300");
    app.key(key(KeyCode::Enter));
    assert!(app.user.overseas.index_detail);
    assert_eq!(
        app.request().overseas.unwrap().focus.as_deref(),
        Some("sh000300")
    );
    app.key(key(KeyCode::Esc));
    assert!(!app.user.overseas.index_detail);
    assert!(app.request().overseas.unwrap().focus.is_none());
    app.key(key(KeyCode::Char('t')));
    let previous = app.user.overseas.index_rows();
    app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
    assert_ne!(app.user.overseas.index_rows(), previous);
    app.key(key(KeyCode::Char('e')));
    for c in "work".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    app.key(key(KeyCode::Enter));
    app.user.mode = Mode::Minimal;
    let s = screen(&app, 160, 42);
    assert!(s.contains("work"));
    assert!(!s.contains("sh000300"));
    app.persist().unwrap();
    let restored = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert_eq!(
        restored.user.overseas.index_rows(),
        app.user.overseas.index_rows()
    );
    assert_eq!(restored.user.overseas.aliases["sh000300"], "work");
    assert_eq!(restored.user.watchlist, watchlist);
    for id in app.user.overseas.index_watchlist.clone() {
        app.user.overseas.index_selected = id;
        app.key(key(KeyCode::Char('d')));
    }
    assert!(app.user.overseas.index_rows().is_empty());
    assert!(app.user.overseas.selected().is_none());
    assert!(screen(&app, 100, 30).contains("暂无关注"));
    app.key(key(KeyCode::Tab));
    assert_eq!(app.user.overseas.index_rows().len(), 13);
    assert!(app.user.overseas.selected().is_some());
}

#[test]
fn minimal_market_columns_stay_together_when_terminal_is_widened() {
    use ticktop::overseas::{self, Page};
    let (_dir, mut app) = app();
    app.modal = None;
    app.user.mode = Mode::Minimal;
    app.on_market(MarketEvent::Quotes(market::demo_quotes(
        &app.request().symbols,
    )));
    app.on_market(MarketEvent::Overseas(
        vec![
            overseas::demo("^N225", Period::Intraday),
            overseas::demo("AAPL", Period::Intraday),
        ],
        vec![],
    ));
    for (page, id, price) in [
        (Page::Watch, "sz300750", "268.50"),
        (Page::Indices, "^N225", "62188.63"),
        (Page::Us, "AAPL", "225.40"),
    ] {
        app.set_page(page);
        let mut expected = None;
        for width in [100, 160, 250] {
            let rendered = screen(&app, width, 36);
            let row = rendered
                .lines()
                .find(|line| line.contains(id) && line.contains(price))
                .unwrap();
            let position = row.find(price).unwrap();
            assert!(position < 35, "{page:?} at {width}: {row}");
            assert_eq!(
                *expected.get_or_insert(position),
                position,
                "{page:?} columns stretch at {width}"
            );
            let y = rendered
                .lines()
                .position(|line| line.contains(id) && line.contains(price))
                .unwrap() as u16;
            let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
            terminal.draw(|f| ui::draw(f, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(
                buffer[(width - 2, y)].bg,
                buffer[(width - 1, 35)].bg,
                "{page:?} highlight stretches into blank area"
            );
        }
        // Very small windows and holding v for real Chinese names still render safely.
        app.reveal_names = true;
        let rendered = screen(&app, 46, 16);
        assert!(rendered.contains("F2"));
        app.reveal_names = false;
    }
}

#[test]
fn candle_wicks_share_body_column_at_different_terminal_widths() {
    let (_dir, mut app) = app();
    app.user.period = Period::Day;
    let id = app.user.selected.clone().unwrap();
    let mut series = market::demo_history(&id, Period::Day);
    series.points.truncate(1);
    series.points[0].price = Price::parse("22").unwrap();
    series.points[0].ohlc = Some(ticktop::model::Ohlc {
        open: Price::parse("20").unwrap(),
        high: Price::parse("25").unwrap(),
        low: Price::parse("17").unwrap(),
    });
    app.on_market(MarketEvent::Chart(series));
    for width in [80, 100, 159, 160, 201] {
        let mut terminal = Terminal::new(TestBackend::new(width, 42)).unwrap();
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let body_columns: Vec<_> = (0..width)
            .filter(|&x| (0..42).any(|y| matches!(buffer[(x, y)].symbol(), "█" | "▀" | "▄")))
            .collect();
        assert!(!body_columns.is_empty());
        for x in body_columns {
            assert!(
                (0..42).any(|y| buffer[(x, y)].symbol() == "│"),
                "candle body at column {x} lacks a centered wick at width {width}"
            );
        }
    }
}

#[test]
fn candle_body_and_wick_have_no_half_cell_breaks() {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::Color,
        widgets::{Axis, Chart, Widget},
    };
    // Isolate the same dataset layering used by both market charts. A half-block
    // endpoint overwrites the wick in that cell, leaving a visible half-cell gap.
    for height in [9, 12, 19, 28] {
        for (open, close) in [(20., 22.), (22., 20.), (20.3, 21.7), (21., 21.)] {
            let wick = [(0., 17.), (0., 25.)];
            let body = [(0., open), (0., close)];
            let area = Rect::new(0, 0, 21, height);
            let mut buf = Buffer::empty(area);
            Chart::new(ticktop::candles::datasets(&wick, &body, Color::Green).to_vec())
                .x_axis(Axis::default().bounds([-0.5, 0.5]))
                .y_axis(Axis::default().bounds([16., 26.]))
                .render(area, &mut buf);
            let occupied: Vec<_> = (0..height)
                .flat_map(|y| (0..21).map(move |x| (x, y)))
                .filter(|&(x, y)| matches!(buf[(x, y)].symbol(), "│" | "█"))
                .collect();
            let x = occupied.first().expect("candle rendered").0;
            assert!(occupied.iter().all(|&(col, _)| col == x));
            let top = occupied.iter().map(|&(_, y)| y).min().unwrap();
            let bottom = occupied.iter().map(|&(_, y)| y).max().unwrap();
            for y in top..=bottom {
                assert!(
                    matches!(buf[(x, y)].symbol(), "│" | "█"),
                    "gap inside candle at ({x}, {y})"
                );
            }
            assert!(
                !buf.content.iter().any(|c| matches!(c.symbol(), "▀" | "▄")),
                "half-cell body endpoint erases part of the underlying wick at height {height}"
            );
        }
    }
}

#[test]
fn mouse_wheel_selects_current_market_without_switching_pages() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    use ticktop::overseas::Page;
    let (_dir, mut app) = app();
    app.modal = None;
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        for page in [Page::Watch, Page::Indices, Page::Us] {
            app.set_page(page);
            let selected = |a: &App| match page {
                Page::Watch => a.user.selected.clone(),
                _ => a.user.overseas.selected().map(str::to_owned),
            };
            let before = selected(&app);
            app.key(key(KeyCode::Down));
            let expected = selected(&app);
            assert_ne!(before, expected);
            app.key(key(KeyCode::Up));
            for (kind, want) in [
                (MouseEventKind::ScrollDown, expected),
                (MouseEventKind::ScrollUp, before),
            ] {
                ui::mouse(
                    &mut app,
                    MouseEvent {
                        kind,
                        column: 100,
                        row: 20,
                        modifiers: KeyModifiers::NONE,
                    },
                    160,
                    42,
                );
                assert_eq!(selected(&app), want, "{page:?} {mode:?}");
                assert_eq!(app.page(), page);
            }
        }
    }
}

#[test]
fn shifted_mouse_wheel_cycles_market_pages_without_selecting_entries() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    use ticktop::overseas::Page;
    let (_dir, mut app) = app();
    app.modal = None;
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        app.set_page(Page::Watch);
        let stock = app.user.selected.clone();
        let index = app.user.overseas.index_selected.clone();
        let us = app.user.overseas.us_selected.clone();
        for (kind, pages) in [
            (
                MouseEventKind::ScrollDown,
                [Page::Indices, Page::Us, Page::Heatmap, Page::Watch],
            ),
            (
                MouseEventKind::ScrollUp,
                [Page::Heatmap, Page::Us, Page::Indices, Page::Watch],
            ),
        ] {
            for page in pages {
                ui::mouse(
                    &mut app,
                    MouseEvent {
                        kind,
                        column: 100,
                        row: 20,
                        modifiers: KeyModifiers::SHIFT,
                    },
                    160,
                    42,
                );
                assert_eq!(app.page(), page);
                assert_eq!(app.user.selected, stock);
                assert_eq!(app.user.overseas.index_selected, index);
                assert_eq!(app.user.overseas.us_selected, us);
            }
        }
        app.key(key(KeyCode::Char('?')));
        assert!(app.modal.is_some());
        ui::mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 100,
                row: 20,
                modifiers: KeyModifiers::SHIFT,
            },
            160,
            42,
        );
        assert_eq!(app.page(), Page::Watch);
        app.key(key(KeyCode::Esc));
    }
}

#[test]
fn tab_mouse_wheel_cycles_pages_and_ignores_blank_header() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    use ticktop::overseas::Page;
    let (_dir, mut app) = app();
    app.modal = None;
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        app.set_page(Page::Watch);
        let stock = app.user.selected.clone();
        let index = app.user.overseas.index_selected.clone();
        let us = app.user.overseas.us_selected.clone();
        for (kind, pages) in [
            (
                MouseEventKind::ScrollDown,
                [Page::Indices, Page::Us, Page::Heatmap, Page::Watch],
            ),
            (
                MouseEventKind::ScrollUp,
                [Page::Heatmap, Page::Us, Page::Indices, Page::Watch],
            ),
        ] {
            for page in pages {
                ui::mouse(
                    &mut app,
                    MouseEvent {
                        kind,
                        column: if mode == Mode::Minimal { 18 } else { 13 },
                        row: 1,
                        modifiers: KeyModifiers::NONE,
                    },
                    160,
                    42,
                );
                assert_eq!(app.page(), page);
                assert_eq!(app.user.selected, stock);
                assert_eq!(app.user.overseas.index_selected, index);
                assert_eq!(app.user.overseas.us_selected, us);
            }
        }
        ui::mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 100,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            160,
            42,
        );
        assert_eq!(app.page(), Page::Watch);
        app.key(key(KeyCode::Char('?')));
        assert!(app.modal.is_some());
        ui::mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: if mode == Mode::Minimal { 18 } else { 13 },
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            160,
            42,
        );
        assert_eq!(app.page(), Page::Watch);
        app.key(key(KeyCode::Esc));
    }
}

#[test]
fn us_pin_and_alt_order_shortcuts_render_persist_and_clean_up_removals() {
    use ticktop::overseas::Page;
    for mode in [Mode::Full, Mode::Minimal] {
        let (_dir, mut app) = app();
        app.modal = None;
        app.user.mode = mode;
        app.set_page(Page::Us);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!(app.user.overseas.visible(), ["NVDA", "AAPL", "TSLA"]);
        assert_eq!(app.user.overseas.us_selected.as_deref(), Some("AAPL"));
        app.key(key(KeyCode::Char('t')));
        assert_eq!(app.user.overseas.visible(), ["AAPL", "NVDA", "TSLA"]);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        assert!(app.message.contains("分别排序"));
        assert_eq!(app.user.overseas.us_selected.as_deref(), Some("AAPL"));
        assert!(screen(&app, 160, 42).contains("^ AAPL"));
        let group = app.user.overseas.group_named("美股观察").unwrap();
        app.user.overseas.groups[0].symbols = vec!["NVDA".into(), "AAPL".into(), "TSLA".into()];
        app.user.overseas.group = group;
        assert!(!app.user.overseas.us_is_pinned("AAPL"));
        app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert_eq!(app.user.overseas.visible(), ["AAPL", "NVDA", "TSLA"]);
        app.key(key(KeyCode::Char('t')));
        let loaded = UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
        assert_eq!(loaded.overseas.visible(), app.user.overseas.visible());
        assert!(loaded.overseas.us_is_pinned("AAPL"));
        let restarted = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
        assert_eq!(
            restarted.user.overseas.visible(),
            app.user.overseas.visible()
        );
        assert!(restarted.user.overseas.us_is_pinned("AAPL"));
        app.key(key(KeyCode::Char('d')));
        app.key(key(KeyCode::Char('y')));
        assert!(!app.user.overseas.us_is_pinned("AAPL"));
        assert!(app.user.overseas.subscriptions.contains(&"AAPL".into()));
        app.user.overseas.group = 0;
        assert!(app.user.overseas.us_is_pinned("AAPL"));
        app.user.overseas.us_selected = Some("AAPL".into());
        app.key(key(KeyCode::Char('d')));
        app.key(key(KeyCode::Char('y')));
        assert!(!app.user.overseas.subscriptions.contains(&"AAPL".into()));
        assert!(
            app.user
                .overseas
                .us_pinned
                .values()
                .all(|ids| !ids.contains(&"AAPL".into()))
        );
        app.user.overseas.validate().unwrap();
        UserState::load(&app.paths.data.join("demo-state.json")).unwrap();
    }
}

#[test]
fn us_search_adds_to_current_group_ignores_stale_responses_and_keeps_names_on_restart() {
    use ticktop::overseas::Page;
    let (_dir, mut app) = app();
    app.modal = None;
    app.set_page(Page::Us);
    let group = app.user.overseas.group_named("新观察").unwrap();
    app.user.overseas.group = group;
    for mode in [Mode::Full, Mode::Minimal] {
        app.user.mode = mode;
        app.key(key(KeyCode::Char('a')));
        for c in "weiruan".chars() {
            app.key(key(KeyCode::Char(c)));
        }
        assert!(
            matches!(&app.modal, Some(Modal::UsSearch { results, .. }) if results[0].id == "MSFT")
        );
        for (w, h) in [(160, 42), (80, 24), (46, 16)] {
            assert!(screen(&app, w, h).contains("订阅美股"));
        }
        app.key(key(KeyCode::Enter));
        assert!(app.user.overseas.visible().contains(&"MSFT".into()));
        assert_eq!(
            app.user
                .overseas
                .subscriptions
                .iter()
                .filter(|id| *id == "MSFT")
                .count(),
            1
        );
    }
    app.key(key(KeyCode::Char('/')));
    for c in "international".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    let (generation, query) = app.request().us_search.unwrap();
    let rows = vec![ticktop::model::Instrument {
        id: "IBM".into(),
        name: "International Business Machines".into(),
    }];
    app.on_market(MarketEvent::UsSearch {
        generation: generation - 1,
        query: query.clone(),
        rows: rows.clone(),
        error: None,
    });
    assert!(
        matches!(&app.modal, Some(Modal::UsSearch { results, .. }) if !results.iter().any(|s| s.id == "IBM"))
    );
    app.on_market(MarketEvent::UsSearch {
        generation,
        query: query.clone(),
        rows: rows.clone(),
        error: None,
    });
    assert!(matches!(&app.modal, Some(Modal::UsSearch { results, .. }) if results[0].id == "IBM"));
    app.key(key(KeyCode::Enter));
    assert_eq!(app.user.overseas.us_selected.as_deref(), Some("IBM"));
    let restarted = App::new(Config::default(), app.paths.clone(), false, true).unwrap();
    assert_eq!(
        restarted.user.overseas.us_names["IBM"],
        "International Business Machines"
    );
    app.key(key(KeyCode::Char('a')));
    for c in "ZZZZ".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    let (generation, query) = app.request().us_search.unwrap();
    app.on_market(MarketEvent::UsSearch {
        generation,
        query,
        rows: vec![],
        error: Some("在线搜索暂不可用".into()),
    });
    assert!(
        matches!(&app.modal, Some(Modal::UsSearch { results, status, .. }) if results[0].id == "ZZZZ" && status.contains("暂不可用"))
    );
    app.key(key(KeyCode::Enter));
    assert!(app.user.overseas.subscriptions.contains(&"ZZZZ".into()));
    assert!(!app.user.overseas.us_names.contains_key("ZZZZ"));
    app.key(key(KeyCode::Char('a')));
    app.key(key(KeyCode::Esc));
    assert!(app.request().us_search.is_none());
    app.on_market(MarketEvent::UsSearch {
        generation,
        query: "ZZZZ".into(),
        rows,
        error: None,
    });
    assert!(app.modal.is_none());
    app.user.overseas.validate().unwrap();
}
