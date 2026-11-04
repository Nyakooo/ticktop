use crate::{
    config::{Config, Mode, Paths, ProxyMode},
    market::{Kind, MarketEvent, Request},
    model::*,
    search::SearchIndex,
    storage::{Cache, Catalog, UserState},
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::HashMap;

pub enum Confirm {
    OverseasRemove(String),
    Remove(String),
    DeleteGroup(u64),
}
pub enum Modal {
    IndexSearch {
        query: String,
        results: Vec<Instrument>,
        index: usize,
    },
    OverseasInput {
        purpose: crate::overseas_app::Input,
        input: String,
    },
    Welcome(usize),
    Help {
        offset: usize,
    },
    Search {
        query: String,
        results: Vec<Instrument>,
        index: usize,
        adding: bool,
    },
    AddTo {
        stock: Instrument,
        index: usize,
    },
    Groups {
        index: usize,
    },
    GroupName {
        id: Option<u64>,
        input: String,
    },
    Alias {
        id: String,
        input: String,
    },
    Confirm(Confirm),
    Settings {
        draft: Config,
        field: usize,
        input: String,
    },
}
pub struct App {
    pub config: Config,
    saved_config: Config,
    pub paths: Paths,
    pub user: UserState,
    pub cache: Cache,
    pub catalog: Vec<Instrument>,
    pub catalog_as_of: String,
    pub search: SearchIndex,
    pub quotes: HashMap<String, Quote>,
    pub charts: HashMap<(String, Period), Series>,
    pub modal: Option<Modal>,
    pub message: String,
    pub quote_error: Option<String>,
    pub turnover: Option<crate::turnover::Comparison>,
    pending_turnover: Option<Option<crate::turnover::Comparison>>,
    pub overseas: HashMap<(String, Period), crate::overseas::Snapshot>,
    pub nights: HashMap<String, crate::overseas::Quote>,
    pub overseas_error: Option<String>,
    pub night_error: Option<String>,
    pub overseas_cursor: Option<usize>,
    pending_overseas: Vec<crate::overseas::Snapshot>,
    pending_nights: Vec<(String, crate::overseas::Quote)>,
    pub chart_error: Option<String>,
    pub chip_error: Option<String>,
    pub chips: HashMap<String, crate::chips::Distribution>,
    pending_chips: HashMap<String, crate::chips::Distribution>,
    pub catalog_error: Option<String>,
    pub sectors: HashMap<crate::sectors::Category, crate::sectors::Snapshot>,
    pub sector_error: Option<String>,
    pub sector_index: usize,
    pub cloud_columns: usize,
    pub cloud_page_size: usize,
    pub demo: bool,
    pub paused: bool,
    pub quit: bool,
    pub dirty: bool,
    pub request_dirty: bool,
    pub state_dirty: bool,
    pub chart_cursor: Option<usize>,
    pub reveal_names: bool,
    pub key_releases: bool,
    pending_quotes: Option<Vec<Quote>>,
    pending_charts: Vec<Series>,
    pending_sectors: HashMap<crate::sectors::Category, crate::sectors::Snapshot>,
}
impl App {
    pub fn new(config: Config, paths: Paths, first: bool, demo: bool) -> Result<Self> {
        let cache = Cache::open(&paths.data.join(if demo {
            "demo-cache.sqlite3"
        } else {
            "cache.sqlite3"
        }))?;
        let catalog = cache
            .read::<Catalog>("catalog")?
            .unwrap_or_else(crate::storage::bundled_catalog);
        let catalog_as_of = catalog.as_of;
        let mut catalog = catalog.stocks;
        crate::instruments::merge(&mut catalog);
        let search = SearchIndex::new(&catalog);
        let state_path = paths.data.join(if demo {
            "demo-state.json"
        } else {
            "state.json"
        });
        let has_saved_state = state_path.exists();
        let mut user = UserState::load(&state_path)?;
        if !has_saved_state {
            user.mode = config.default_mode;
            user.save(&state_path)?;
        }
        let welcome = first && !has_saved_state;
        let welcome_selection = usize::from(user.mode == Mode::Minimal);
        let quotes = cache
            .quotes()?
            .into_iter()
            .map(|q| (q.id.clone(), q))
            .collect();
        let mut sectors = HashMap::new();
        for category in [
            crate::sectors::Category::Industry,
            crate::sectors::Category::Concept,
        ] {
            if let Some(mut snapshot) =
                cache.read::<crate::sectors::Snapshot>(&format!("sectors:{}", category.key()))?
            {
                user.sector_sort.apply(&mut snapshot.sectors);
                sectors.insert(category, snapshot);
            }
        }
        if user.heatmap {
            user.overseas.page = crate::overseas::Page::Heatmap;
        }
        let cached_overseas = cache
            .read::<Vec<crate::overseas::Snapshot>>("overseas:v1")?
            .unwrap_or_default();
        let nights = cache
            .read::<HashMap<String, crate::overseas::Quote>>("overnight:v1")?
            .unwrap_or_default();
        let turnover = cache.read::<crate::turnover::Comparison>("turnover:v1")?;
        let mut app = Self {
            saved_config: config.clone(),
            config,
            paths,
            user,
            cache,
            catalog,
            catalog_as_of,
            search,
            quotes,
            charts: HashMap::new(),
            modal: if welcome {
                Some(Modal::Welcome(welcome_selection))
            } else {
                None
            },
            message: String::new(),
            quote_error: None,
            overseas: cached_overseas
                .into_iter()
                .map(|s| ((s.id.clone(), s.period), s))
                .collect(),
            nights,
            overseas_error: None,
            night_error: None,
            overseas_cursor: None,
            pending_overseas: vec![],
            pending_nights: vec![],
            turnover,
            pending_turnover: None,
            chart_error: None,
            chip_error: None,
            chips: HashMap::new(),
            pending_chips: HashMap::new(),
            catalog_error: None,
            sectors,
            sector_error: None,
            sector_index: 0,
            cloud_columns: 8,
            cloud_page_size: 88,
            demo,
            paused: false,
            quit: false,
            dirty: true,
            request_dirty: true,
            state_dirty: false,
            chart_cursor: None,
            reveal_names: false,
            key_releases: false,
            pending_quotes: None,
            pending_charts: Vec::new(),
            pending_sectors: HashMap::new(),
        };
        app.ensure_selection();
        app.load_chart();
        Ok(app)
    }
    pub fn preview_index(&mut self, id: &str) {
        if !self.config.pinned_indices.iter().any(|p| p == id) {
            return;
        }
        self.set_page(crate::overseas::Page::Watch);
        self.user.selected = Some(id.into());
        self.user.heatmap = false;
        self.load_chart();
        self.request_dirty = true;
        self.dirty = true;
        self.save_state_change();
    }
    pub fn request(&self) -> Request {
        let mut symbols = self.user.watchlist.clone();
        if let Some(id) = &self.user.selected
            && !symbols.contains(id)
        {
            symbols.push(id.clone());
        }
        for id in self
            .config
            .pinned_indices
            .iter()
            .map(String::as_str)
            .chain(["sh000002", "sz399107"])
        {
            if !symbols.iter().any(|s| s == id) {
                symbols.push(id.to_string());
            }
        }
        let page = self.page();
        let overseas = match page {
            crate::overseas::Page::Indices => Some(crate::overseas::Job {
                ids: self.user.overseas.index_rows(),
                focus: self
                    .user
                    .overseas
                    .index_detail
                    .then(|| self.user.overseas.selected().map(str::to_string))
                    .flatten(),
                period: if self.user.overseas.index_detail {
                    self.user.overseas.index_period
                } else {
                    Period::Intraday
                },
                us: false,
            }),
            crate::overseas::Page::Us => Some(crate::overseas::Job {
                ids: self.user.overseas.visible(),
                focus: self.user.overseas.us_selected.clone(),
                period: self.user.overseas.us_period,
                us: true,
            }),
            _ => None,
        };
        Request {
            overseas,
            config: self.config.clone(),
            symbols,
            focus: (page == crate::overseas::Page::Watch)
                .then(|| self.user.selected.clone())
                .flatten(),
            period: self.user.period,
            demo: self.demo,
            catalog_as_of: self.catalog_as_of.clone(),
            heatmap_enabled: self.user.heatmap,
            sector_category: self.user.sector_category,
        }
    }
    pub fn persist(&self) -> Result<()> {
        self.user.save(&self.paths.data.join(if self.demo {
            "demo-state.json"
        } else {
            "state.json"
        }))
    }
    pub fn save_default_mode(&mut self) -> Result<()> {
        let mut saved = self.saved_config.clone();
        saved.default_mode = self.user.mode;
        self.paths.save_config(&saved)?;
        self.saved_config = saved;
        self.config.default_mode = self.user.mode;
        Ok(())
    }
    /// Persist the display choice immediately, without saving temporary network overrides.
    pub fn remember_mode(&mut self) {
        let state_saved = self.save_state_change();
        if self.save_default_mode().is_err() && state_saved {
            self.message = "显示模式已记录，但默认配置保存失败".into();
        }
    }
    pub fn flush_state(&mut self) -> bool {
        match self.persist() {
            Ok(()) => {
                self.state_dirty = false;
                true
            }
            Err(_) => {
                self.state_dirty = true;
                self.message = "自选保存失败，将自动重试".into();
                self.dirty = true;
                false
            }
        }
    }
    pub(crate) fn save_state_change(&mut self) -> bool {
        self.state_dirty = true;
        self.flush_state()
    }
    pub fn sector_snapshot(&self) -> Option<&crate::sectors::Snapshot> {
        self.sectors.get(&self.user.sector_category)
    }
    pub fn cloud_dimensions(width: u16, height: u16, mode: Mode) -> (usize, usize) {
        let minimal = mode == Mode::Minimal;
        let columns = (width / if minimal { 32 } else { 16 }).clamp(1, 12) as usize;
        let rows = (height.saturating_sub(4) / if minimal { 1 } else { 2 }).max(1) as usize;
        let rows = if minimal { rows } else { rows.min(3) };
        (columns, columns * rows)
    }
    pub fn set_viewport(&mut self, width: u16, height: u16) {
        let area = crate::ui::content_area(self, width, height);
        (self.cloud_columns, self.cloud_page_size) =
            Self::cloud_dimensions(area.width, area.height, self.user.mode);
    }
    fn apply_sectors(&mut self, mut snapshot: crate::sectors::Snapshot) {
        self.user.sector_sort.apply(&mut snapshot.sectors);
        if snapshot.category == self.user.sector_category {
            let previous = self
                .sector_snapshot()
                .and_then(|s| s.selected(self.sector_index))
                .map(|s| s.code.clone());
            self.sector_index = previous
                .and_then(|id| snapshot.sectors.iter().position(|s| s.code == id))
                .unwrap_or(0);
        }
        self.sectors.insert(snapshot.category, snapshot);
    }
    fn cloud_key(&mut self, key: KeyCode) -> bool {
        let count = self.sector_snapshot().map(|s| s.sectors.len()).unwrap_or(0);
        match key {
            KeyCode::Char('s') => {
                self.user.sector_sort = self.user.sector_sort.next();
                for snapshot in self.sectors.values_mut() {
                    self.user.sector_sort.apply(&mut snapshot.sectors);
                }
                self.sector_index = 0;
                self.state_dirty = true;
                self.flush_state();
            }
            KeyCode::Tab => {
                self.user.sector_category = self.user.sector_category.next();
                self.sector_index = 0;
                self.sector_error = None;
                self.state_dirty = true;
                self.request_dirty = true;
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Char('n' | 'p')
            | KeyCode::Home
            | KeyCode::End => {
                if count == 0 {
                    return true;
                }
                self.sector_index = self.sector_index.min(count - 1);
                self.sector_index = match key {
                    KeyCode::Left => self.sector_index.saturating_sub(1),
                    KeyCode::Right => (self.sector_index + 1).min(count - 1),
                    KeyCode::Up => self.sector_index.saturating_sub(self.cloud_columns),
                    KeyCode::Down => (self.sector_index + self.cloud_columns).min(count - 1),
                    KeyCode::PageUp | KeyCode::Char('p') => {
                        let page = self.sector_index / self.cloud_page_size;
                        if page == 0 {
                            self.message = if self.user.mode == Mode::Minimal {
                                "1 / first"
                            } else {
                                "已是第一页；Tab 切换行业/概念"
                            }
                            .into();
                            self.sector_index
                        } else {
                            (page - 1) * self.cloud_page_size
                        }
                    }
                    KeyCode::PageDown | KeyCode::Char('n') => {
                        let page = self.sector_index / self.cloud_page_size;
                        if (page + 1) * self.cloud_page_size >= count {
                            self.message = if self.user.mode == Mode::Minimal {
                                "end / last"
                            } else {
                                "已是最后一页；Tab 切换行业/概念"
                            }
                            .into();
                            self.sector_index
                        } else {
                            (page + 1) * self.cloud_page_size
                        }
                    }
                    KeyCode::Home => 0,
                    _ => count - 1,
                };
            }
            KeyCode::Char('[' | ']' | 'a' | 'd' | 'g' | '/' | 'j' | 'k') => {
                self.message = if self.user.mode == Mode::Minimal {
                    "h / table".into()
                } else {
                    "按 h 返回自选行情后管理股票；Tab 切换行业/概念".into()
                };
            }
            _ => return false,
        }
        true
    }
    pub fn selected_stock(&self) -> Option<&Instrument> {
        self.user
            .selected
            .as_ref()
            .and_then(|id| self.catalog.iter().find(|s| &s.id == id))
    }
    pub fn series(&self) -> Option<&Series> {
        self.user
            .selected
            .as_ref()
            .and_then(|id| self.charts.get(&(id.clone(), self.user.period)))
    }
    pub fn visible(&self) -> Vec<String> {
        self.user.visible()
    }
    pub fn ensure_selection(&mut self) {
        if !self.user.groups.iter().any(|g| g.id == self.user.group) {
            self.user.group = 0;
        }
        let rows = self.visible();
        if self.user.selected.is_none()
            || !self
                .user
                .selected
                .as_ref()
                .is_some_and(|s| rows.contains(s))
        {
            self.user.selected = rows.first().cloned();
        }
    }
    fn load_chart(&mut self) {
        if let Some(id) = &self.user.selected {
            if !self.chips.contains_key(id)
                && let Ok(Some(d)) = self
                    .cache
                    .read::<crate::chips::Distribution>(&format!("chips:v1:{id}"))
            {
                self.chips.insert(id.clone(), d);
            }
            let key = (id.clone(), self.user.period);
            if !self.charts.contains_key(&key) {
                match self.cache.series(id, self.user.period) {
                    Ok(Some(s)) => {
                        self.charts.insert(key, s);
                    }
                    Ok(None) => {}
                    Err(_) => self.message = "走势缓存读取失败，将重新请求".into(),
                }
            }
        }
        self.chart_cursor = None;
        self.chart_error = None;
        self.chip_error = None;
    }
    pub fn on_market(&mut self, event: MarketEvent) {
        match event {
            MarketEvent::Overseas(snapshots, missing) => {
                self.overseas_error = if missing.is_empty() {
                    None
                } else {
                    Some(format!(
                        "{} 暂无新数据；保留源时间与旧值",
                        missing.join(", ")
                    ))
                };
                let mut merged = self.overseas.clone();
                for mut s in snapshots {
                    if s.bars.is_empty()
                        && let Some(old) = merged.get(&(s.id.clone(), s.period))
                    {
                        s.bars = old.bars.clone();
                        s.intervals = old.intervals.clone();
                    }
                    if merged
                        .get(&(s.id.clone(), s.period))
                        .is_none_or(|old| s.received_at >= old.received_at)
                    {
                        merged.insert((s.id.clone(), s.period), s.clone());
                    }
                    if self.paused {
                        self.pending_overseas
                            .retain(|old| old.id != s.id || old.period != s.period);
                        self.pending_overseas.push(s);
                    }
                }
                if self
                    .cache
                    .write("overseas:v1", &merged.values().collect::<Vec<_>>())
                    .is_err()
                {
                    self.message = "海外缓存写入失败".into();
                }
                if !self.paused {
                    self.overseas = merged;
                }
            }
            MarketEvent::Overnight(id, quote) => {
                self.night_error = quote
                    .is_none()
                    .then(|| "夜盘暂无新报价；不会使用盘后价替代".into());
                if let Some(q) = quote {
                    let mut all = self.nights.clone();
                    if all.get(&id).is_none_or(|old| q.time >= old.time) {
                        all.insert(id.clone(), q.clone());
                    }
                    if self.cache.write("overnight:v1", &all).is_err() {
                        self.message = "夜盘缓存写入失败".into();
                    }
                    if self.paused {
                        self.pending_nights.retain(|(s, _)| s != &id);
                        self.pending_nights.push((id, q));
                    } else {
                        self.nights = all;
                    }
                }
            }
            MarketEvent::Quotes(quotes) => {
                let requested = self.request().symbols;
                let returned: std::collections::HashSet<_> =
                    quotes.iter().map(|q| q.id.clone()).collect();
                let missing = requested.iter().filter(|s| !returned.contains(*s)).count();
                self.quote_error = if missing > 0 {
                    Some(format!("{missing} 个标的暂无新报价，保留旧值"))
                } else {
                    None
                };
                let mut merged = self
                    .pending_quotes
                    .as_ref()
                    .map(|quotes| quotes.iter().map(|q| (q.id.clone(), q.clone())).collect())
                    .unwrap_or_else(|| self.quotes.clone());
                for q in &quotes {
                    if merged
                        .get(&q.id)
                        .is_none_or(|old| q.source_time >= old.source_time)
                    {
                        merged.insert(q.id.clone(), q.clone());
                    }
                }
                let cached: Vec<_> = merged.values().cloned().collect();
                if self.cache.write("quotes", &cached).is_err() {
                    self.message = "行情缓存写入失败".into();
                }
                if self.paused {
                    self.pending_quotes = Some(cached);
                } else {
                    self.quotes = merged;
                }
            }
            MarketEvent::Turnover(comparison) => {
                let comparison = comparison.or_else(|| {
                    self.turnover
                        .as_ref()
                        .filter(|c| c.minute == "15:00")
                        .cloned()
                });
                if let Some(c) = &comparison
                    && self.cache.write("turnover:v1", c).is_err()
                {
                    self.message = "成交额比较缓存写入失败".into();
                }
                if self.paused {
                    self.pending_turnover = Some(comparison);
                } else {
                    self.turnover = comparison;
                }
            }
            MarketEvent::Chips(d) => {
                self.chip_error = None;
                if self.cache.write(&format!("chips:v1:{}", d.id), &d).is_err() {
                    self.message = "筹码缓存写入失败".into();
                }
                if self.paused {
                    self.pending_chips.insert(d.id.clone(), d);
                } else {
                    self.chips.insert(d.id.clone(), d);
                }
            }
            MarketEvent::Chart(s) => {
                if self.user.selected.as_ref() == Some(&s.id) && self.user.period == s.period {
                    self.chart_error = None;
                }
                if self
                    .cache
                    .write(&format!("series:{}:{}", s.id, s.period.key()), &s)
                    .is_err()
                {
                    self.message = "走势缓存写入失败".into();
                }
                if self.paused {
                    self.pending_charts
                        .retain(|p| p.id != s.id || p.period != s.period);
                    self.pending_charts.push(s);
                } else {
                    self.charts.insert((s.id.clone(), s.period), s);
                }
            }
            MarketEvent::Sectors(snapshot) => {
                if snapshot.category == self.user.sector_category {
                    self.sector_error = None;
                }
                if self
                    .cache
                    .write(&format!("sectors:{}", snapshot.category.key()), &snapshot)
                    .is_err()
                {
                    self.message = "板块缓存保存失败".into();
                }
                if self.paused {
                    self.pending_sectors.insert(snapshot.category, snapshot);
                } else {
                    self.apply_sectors(snapshot);
                }
            }
            MarketEvent::Catalog(catalog) => {
                if self.cache.write("catalog", &catalog).is_err() {
                    self.message = "搜索索引缓存写入失败".into();
                }
                self.catalog_as_of = catalog.as_of;
                self.catalog = catalog.stocks;
                crate::instruments::merge(&mut self.catalog);
                self.search = SearchIndex::new(&self.catalog);
                self.catalog_error = None;
            }
            MarketEvent::Error {
                kind,
                message,
                retry_secs,
            } => {
                let msg = format!("{message} · {retry_secs}s 后重试");
                match kind {
                    Kind::Quotes => self.quote_error = Some(msg),
                    Kind::Overseas => self.overseas_error = Some(msg),
                    Kind::Overnight => self.night_error = Some(msg),
                    Kind::Turnover => {
                        if self.turnover.as_ref().is_some_and(|c| c.minute == "15:00") {
                            return;
                        }
                        if self.paused {
                            self.pending_turnover = Some(None);
                        } else {
                            self.turnover = None;
                        }
                    }
                    Kind::Chart => self.chart_error = Some(msg),
                    Kind::Chips => self.chip_error = Some(msg),
                    Kind::Sectors => self.sector_error = Some(msg),
                    Kind::Catalog => {
                        self.catalog_error = Some("名单更新失败，继续使用本地索引".into())
                    }
                }
            }
        }
        if !self.paused {
            self.dirty = true;
        }
    }
    pub(crate) fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if !self.paused {
            for s in self.pending_overseas.drain(..) {
                self.overseas.insert((s.id.clone(), s.period), s);
            }
            self.nights.extend(self.pending_nights.drain(..));
            if let Some(comparison) = self.pending_turnover.take() {
                self.turnover = comparison;
            }
            if let Some(quotes) = self.pending_quotes.take() {
                self.quotes = quotes.into_iter().map(|q| (q.id.clone(), q)).collect();
            }
            self.chips.extend(self.pending_chips.drain());
            for s in self.pending_charts.drain(..) {
                self.charts.insert((s.id.clone(), s.period), s);
            }
            for (_, snapshot) in std::mem::take(&mut self.pending_sectors) {
                self.apply_sectors(snapshot);
            }
        }
    }
    fn open_search(&mut self, adding: bool) {
        self.modal = Some(Modal::Search {
            query: String::new(),
            results: Vec::new(),
            index: 0,
            adding,
        });
    }
    fn input_char(input: &mut String, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            input.clear();
            return true;
        }
        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if input.chars().count() < 160 {
                    input.push(c);
                }
                true
            }
            KeyCode::Backspace => {
                input.pop();
                true
            }
            _ => false,
        }
    }
    pub fn minimal_label<'a>(&'a self, id: &'a str) -> &'a str {
        if self.reveal_names {
            self.catalog
                .iter()
                .find(|stock| stock.id == id)
                .map_or(id, |stock| stock.name.as_str())
        } else {
            self.user.custom_aliases.get(id).map_or(id, String::as_str)
        }
    }
    pub fn conceal_names(&mut self) {
        if self.reveal_names {
            self.reveal_names = false;
            self.dirty = true;
        }
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('v') && key.kind == KeyEventKind::Release {
            if self.key_releases {
                self.conceal_names();
            }
            return;
        }
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.modal.is_none()
            && self.user.mode == Mode::Minimal
            && key.code == KeyCode::Char('v')
            && key.modifiers.is_empty()
        {
            if self.key_releases {
                self.reveal_names = true;
            } else if key.kind == KeyEventKind::Press {
                self.reveal_names = !self.reveal_names;
            }
            self.dirty = true;
            return;
        }
        self.conceal_names();
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        self.message.clear();
        self.dirty = true;
        if let Some(modal) = self.modal.take() {
            self.handle_modal(modal, key);
            return;
        }
        if let KeyCode::F(n @ 1..=4) = key.code {
            self.set_page(
                [
                    crate::overseas::Page::Watch,
                    crate::overseas::Page::Indices,
                    crate::overseas::Page::Us,
                    crate::overseas::Page::Heatmap,
                ][n as usize - 1],
            );
            return;
        }
        if key.code == KeyCode::Char('h') {
            let target = if self.page() == crate::overseas::Page::Heatmap {
                self.user.overseas.return_page
            } else {
                crate::overseas::Page::Heatmap
            };
            self.set_page(target);
            return;
        }
        if matches!(
            self.page(),
            crate::overseas::Page::Indices | crate::overseas::Page::Us
        ) && key.code != KeyCode::Char('m')
        {
            self.overseas_key(key);
            return;
        }
        if self.user.heatmap && self.cloud_key(key.code) {
            return;
        }
        let mut key = key;
        if !self.user.heatmap && key.modifiers.is_empty() {
            key.code = match key.code {
                KeyCode::Char('j') => KeyCode::Down,
                KeyCode::Char('k') => KeyCode::Up,
                code => code,
            };
        }
        let previous = (
            self.user.mode,
            self.user.heatmap,
            self.user.group,
            self.user.selected.clone(),
            self.user.period,
        );
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('m') => {
                self.user.mode = if self.user.mode == Mode::Full {
                    Mode::Minimal
                } else {
                    Mode::Full
                }
            }
            KeyCode::Char('h') => {
                self.user.heatmap = !self.user.heatmap;
                self.request_dirty = true;
            }
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('/') => self.open_search(false),
            KeyCode::Char('a') => self.open_search(true),
            KeyCode::Char('t') if !self.user.heatmap => {
                if let Some(id) = self.user.selected.clone() {
                    match self.user.toggle_pin(&id) {
                        Ok(()) => {
                            self.save_state_change();
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                }
            }
            KeyCode::Char('e') if !self.user.heatmap => {
                if let Some(id) = self.user.selected.clone()
                    && self.visible().contains(&id)
                {
                    let input = self
                        .user
                        .custom_aliases
                        .get(&id)
                        .cloned()
                        .unwrap_or_default();
                    self.modal = Some(Modal::Alias { id, input });
                }
            }
            KeyCode::Char('r') if !self.user.heatmap => {
                if self.user.group != 0 {
                    self.modal = Some(Modal::GroupName {
                        id: Some(self.user.group),
                        input: self.user.group_name().into(),
                    });
                } else {
                    self.message = "全部自选为系统分组；g 管理自定义分组".into();
                }
            }
            KeyCode::Up | KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(id) = self.user.selected.clone() {
                    match self.user.move_symbol(&id, key.code == KeyCode::Down) {
                        Ok(()) => {
                            self.save_state_change();
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                }
            }
            KeyCode::Char('g') => {
                self.modal = Some(Modal::Groups {
                    index: self
                        .user
                        .groups
                        .iter()
                        .position(|g| g.id == self.user.group)
                        .unwrap_or(0),
                })
            }
            KeyCode::Char('?') => self.modal = Some(Modal::Help { offset: 0 }),
            KeyCode::Char('i') => {
                self.modal = Some(Modal::Settings {
                    draft: self.config.clone(),
                    field: 6,
                    input: field_value(&self.config, 6),
                });
            }
            KeyCode::Char(',') => {
                self.modal = Some(Modal::Settings {
                    draft: self.config.clone(),
                    field: 0,
                    input: self.config.refresh.quotes_secs.to_string(),
                })
            }
            KeyCode::Char('d') => {
                if let Some(id) = &self.user.selected
                    && self.visible().contains(id)
                {
                    self.modal = Some(Modal::Confirm(Confirm::Remove(id.clone())));
                }
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                let mut ids = vec![0];
                ids.extend(self.user.groups.iter().map(|g| g.id));
                let i = ids
                    .iter()
                    .position(|id| *id == self.user.group)
                    .unwrap_or(0);
                self.user.group = ids[(i + if key.code == KeyCode::Char('[') {
                    ids.len() - 1
                } else {
                    1
                }) % ids.len()];
                self.ensure_selection();
                self.load_chart();
                self.request_dirty = true;
            }
            KeyCode::Up | KeyCode::Down => {
                let rows = self.visible();
                if !rows.is_empty() {
                    let i = rows
                        .iter()
                        .position(|id| Some(id) == self.user.selected.as_ref())
                        .unwrap_or(0);
                    self.user.selected = Some(
                        rows[(i + if key.code == KeyCode::Up {
                            rows.len() - 1
                        } else {
                            1
                        }) % rows.len()]
                        .clone(),
                    );
                    self.load_chart();
                    self.request_dirty = true;
                }
            }
            KeyCode::Left | KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(s) = self.series().filter(|s| !s.points.is_empty()) {
                    let last = s.points.len() - 1;
                    let i = self.chart_cursor.unwrap_or(last).min(last);
                    self.chart_cursor = Some(if key.code == KeyCode::Left {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(last)
                    });
                }
            }
            KeyCode::Left | KeyCode::Right => {
                self.user.period = self.user.period.next(key.code == KeyCode::Left);
                self.load_chart();
                self.request_dirty = true;
            }
            KeyCode::Home => self.chart_cursor = Some(0),
            KeyCode::End => {
                self.chart_cursor = self.series().map(|s| s.points.len().saturating_sub(1))
            }
            _ => {}
        }
        if previous
            != (
                self.user.mode,
                self.user.heatmap,
                self.user.group,
                self.user.selected.clone(),
                self.user.period,
            )
        {
            if previous.0 != self.user.mode {
                self.remember_mode();
            } else {
                self.state_dirty = true;
            }
        }
    }
    fn handle_modal(&mut self, modal: Modal, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            if matches!(modal, Modal::Welcome(_))
                && let Err(err) = self.save_default_mode()
            {
                self.message = err.to_string();
            }
            return;
        }
        let next = match modal {
            Modal::IndexSearch {
                mut query,
                mut results,
                mut index,
            } => {
                match key.code {
                    KeyCode::Up => index = index.saturating_sub(1),
                    KeyCode::Down => index = (index + 1).min(results.len().saturating_sub(1)),
                    KeyCode::Enter => {
                        if let Some(stock) = results.get(index) {
                            if let Err(error) = self.user.overseas.follow_index(&stock.id) {
                                self.message = error.to_string();
                            } else {
                                self.user.overseas.index_all = false;
                                self.user.overseas.index_detail = false;
                                self.request_dirty = true;
                                self.save_state_change();
                                return;
                            }
                        }
                    }
                    _ => {
                        if Self::input_char(&mut query, key) {
                            results = crate::index_board::search(&query);
                            if query.is_empty() {
                                results = crate::index_board::catalog()
                                    .into_iter()
                                    .map(|(id, name)| Instrument {
                                        id: id.into(),
                                        name: name.into(),
                                    })
                                    .collect();
                            }
                            index = 0;
                        }
                    }
                }
                Some(Modal::IndexSearch {
                    query,
                    results,
                    index,
                })
            }
            Modal::OverseasInput { purpose, mut input } => {
                if key.code == KeyCode::Enter {
                    if let Err(e) = self.overseas_input(purpose, &input) {
                        self.message = e.to_string();
                        Some(Modal::OverseasInput { purpose, input })
                    } else {
                        None
                    }
                } else {
                    Self::input_char(&mut input, key);
                    Some(Modal::OverseasInput { purpose, input })
                }
            }
            Modal::Help { mut offset } => {
                match key.code {
                    KeyCode::Char('?') => return,
                    KeyCode::Down => {
                        offset = (offset + 1).min(
                            self.user.watchlist.len()
                                + self.user.groups.len()
                                + self.sector_snapshot().map_or(0, |s| s.sectors.len())
                                + 16,
                        )
                    }
                    KeyCode::Up => offset = offset.saturating_sub(1),
                    KeyCode::PageDown => {
                        offset = (offset + 10).min(
                            self.user.watchlist.len()
                                + self.user.groups.len()
                                + self.sector_snapshot().map_or(0, |s| s.sectors.len())
                                + 16,
                        )
                    }
                    KeyCode::PageUp => offset = offset.saturating_sub(10),
                    _ => {}
                }
                Some(Modal::Help { offset })
            }
            Modal::Welcome(mut i) => {
                match key.code {
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => i = 1 - i,
                    KeyCode::Enter => {
                        self.user.mode = if i == 0 { Mode::Full } else { Mode::Minimal };
                        if let Err(e) = self.save_default_mode() {
                            self.message = e.to_string();
                        }
                        self.save_state_change();
                        return;
                    }
                    _ => {}
                }
                Some(Modal::Welcome(i))
            }
            Modal::Search {
                mut query,
                mut results,
                mut index,
                adding,
            } => {
                if key.code == KeyCode::Down && !results.is_empty() {
                    index = (index + 1) % results.len();
                } else if key.code == KeyCode::Up && !results.is_empty() {
                    index = (index + results.len() - 1) % results.len();
                } else if key.code == KeyCode::Enter
                    || (key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('a'))
                {
                    if let Some(stock) = results.get(index).cloned() {
                        if adding || key.modifiers.contains(KeyModifiers::CONTROL) {
                            self.modal = Some(Modal::AddTo { stock, index: 0 });
                            return;
                        }
                        self.user.selected = Some(stock.id.clone());
                        self.load_chart();
                        self.request_dirty = true;
                        self.message = if self.user.mode == Mode::Minimal {
                            "preview".into()
                        } else {
                            format!("预览 {} · a 添加自选", stock.name)
                        };
                        return;
                    }
                } else if Self::input_char(&mut query, key) {
                    results = self.search.search(&query, 30);
                    index = 0;
                }
                Some(Modal::Search {
                    query,
                    results,
                    index,
                    adding,
                })
            }
            Modal::AddTo { stock, mut index } => {
                let n = self.user.groups.len() + 1;
                match key.code {
                    KeyCode::Down => index = (index + 1) % n,
                    KeyCode::Up => index = (index + n - 1) % n,
                    KeyCode::Enter => {
                        let group = if index == 0 {
                            0
                        } else {
                            self.user.groups[index - 1].id
                        };
                        match self.user.add(&stock.id, group) {
                            Ok(()) => {
                                self.user.group = group;
                                self.user.selected = Some(stock.id);
                                self.request_dirty = true;
                                self.load_chart();
                                if self.save_state_change() {
                                    self.message = "已加入自选分组".into();
                                }
                                return;
                            }
                            Err(e) => self.message = e.to_string(),
                        }
                    }
                    _ => {}
                }
                Some(Modal::AddTo { stock, index })
            }
            Modal::Groups { mut index } => {
                let n = self.user.groups.len();
                match key.code {
                    KeyCode::Up | KeyCode::Down
                        if n > 0 && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        let next = if key.code == KeyCode::Down {
                            (index + 1).min(n - 1)
                        } else {
                            index.saturating_sub(1)
                        };
                        self.user.groups.swap(index, next);
                        index = next;
                        self.save_state_change();
                    }
                    KeyCode::Down if n > 0 => index = (index + 1) % n,
                    KeyCode::Up if n > 0 => index = (index + n - 1) % n,
                    KeyCode::Char('n') => {
                        return self.modal = Some(Modal::GroupName {
                            id: None,
                            input: String::new(),
                        });
                    }
                    KeyCode::Char('r') if n > 0 => {
                        return self.modal = Some(Modal::GroupName {
                            id: Some(self.user.groups[index].id),
                            input: self.user.groups[index].name.clone(),
                        });
                    }
                    KeyCode::Char('d') if n > 0 => {
                        return self.modal = Some(Modal::Confirm(Confirm::DeleteGroup(
                            self.user.groups[index].id,
                        )));
                    }
                    KeyCode::Enter if n > 0 => {
                        self.user.group = self.user.groups[index].id;
                        self.ensure_selection();
                        self.load_chart();
                        self.request_dirty = true;
                        self.save_state_change();
                        return;
                    }
                    _ => {}
                }
                Some(Modal::Groups { index })
            }
            Modal::Alias { id, mut input } => {
                if key.code == KeyCode::Enter {
                    match self.user.set_alias(&id, &input) {
                        Ok(()) => {
                            self.save_state_change();
                            return;
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                } else {
                    Self::input_char(&mut input, key);
                }
                Some(Modal::Alias { id, input })
            }
            Modal::GroupName { id, mut input } => {
                if key.code == KeyCode::Enter {
                    let result = if let Some(id) = id {
                        self.user.rename_group(id, &input).map(|_| id)
                    } else {
                        self.user.create_group(&input)
                    };
                    match result {
                        Ok(group) => {
                            self.user.group = group;
                            self.ensure_selection();
                            self.load_chart();
                            self.request_dirty = true;
                            self.save_state_change();
                            return;
                        }
                        Err(e) => self.message = e.to_string(),
                    }
                } else {
                    Self::input_char(&mut input, key);
                }
                Some(Modal::GroupName { id, input })
            }
            Modal::Confirm(action) => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) {
                    match action {
                        Confirm::OverseasRemove(id) => {
                            let state = &mut self.user.overseas;
                            if state.group == 0 {
                                state.subscriptions.retain(|s| s != &id);
                                state.aliases.remove(&id);
                                for g in &mut state.groups {
                                    g.symbols.retain(|s| s != &id);
                                }
                            } else if let Some(g) =
                                state.groups.iter_mut().find(|g| g.id == state.group)
                            {
                                g.symbols.retain(|s| s != &id);
                            }
                            state.ensure_selection();
                            self.overseas_cursor = None;
                            self.request_dirty = true;
                            self.save_state_change();
                            return;
                        }
                        Confirm::Remove(id) => self.user.remove(&id),
                        Confirm::DeleteGroup(id) => self.user.delete_group(id),
                    }
                    self.ensure_selection();
                    self.load_chart();
                    self.request_dirty = true;
                    self.save_state_change();
                    return;
                }
                if key.code == KeyCode::Char('n') {
                    return;
                }
                Some(Modal::Confirm(action))
            }
            Modal::Settings {
                mut draft,
                mut field,
                mut input,
            } => {
                if key.code == KeyCode::Tab
                    || key.code == KeyCode::BackTab
                    || key.code == KeyCode::Enter
                {
                    if let Err(e) = apply_field(&mut draft, field, &input) {
                        self.message = e.to_string();
                        Some(Modal::Settings {
                            draft,
                            field,
                            input,
                        })
                    } else if key.code == KeyCode::Enter {
                        match draft.validate() {
                            Ok(()) => match self.paths.save_config(&draft) {
                                Ok(()) => {
                                    self.saved_config = draft.clone();
                                    self.config = draft;
                                    self.request_dirty = true;
                                    self.message = "设置已保存并生效".into();
                                    None
                                }
                                Err(_) => {
                                    self.message = "设置保存失败，未更改当前配置".into();
                                    Some(Modal::Settings {
                                        draft,
                                        field,
                                        input,
                                    })
                                }
                            },
                            Err(e) => {
                                self.message = e.to_string();
                                Some(Modal::Settings {
                                    draft,
                                    field,
                                    input,
                                })
                            }
                        }
                    } else {
                        field = (field
                            + if key.code == KeyCode::BackTab
                                || key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                6
                            } else {
                                1
                            })
                            % 7;
                        input = field_value(&draft, field);
                        Some(Modal::Settings {
                            draft,
                            field,
                            input,
                        })
                    }
                } else if field == 2 && matches!(key.code, KeyCode::Left | KeyCode::Right) {
                    draft.network.proxy_mode = match draft.network.proxy_mode {
                        ProxyMode::System => ProxyMode::Custom,
                        ProxyMode::Custom => ProxyMode::Direct,
                        ProxyMode::Direct => ProxyMode::System,
                    };
                    input = field_value(&draft, field);
                    Some(Modal::Settings {
                        draft,
                        field,
                        input,
                    })
                } else {
                    Self::input_char(&mut input, key);
                    Some(Modal::Settings {
                        draft,
                        field,
                        input,
                    })
                }
            }
        };
        self.modal = next;
    }
}
pub fn field_value(c: &Config, field: usize) -> String {
    match field {
        0 => c.refresh.quotes_secs.to_string(),
        1 => c.refresh.chart_secs.to_string(),
        2 => match c.network.proxy_mode {
            ProxyMode::System => "system",
            ProxyMode::Custom => "custom",
            ProxyMode::Direct => "direct",
        }
        .into(),
        3 => c.network.proxy_url.clone(),
        4 => c.network.timeout_secs.to_string(),
        5 => c.refresh.sectors_secs.to_string(),
        _ => c.pinned_indices.join(","),
    }
}
fn apply_field(c: &mut Config, field: usize, input: &str) -> Result<()> {
    match field {
        0 => {
            c.refresh.quotes_secs =
                crate::config::parse_interval(input).map_err(anyhow::Error::msg)?
        }
        1 => {
            c.refresh.chart_secs =
                crate::config::parse_interval(input).map_err(anyhow::Error::msg)?
        }
        2 => {
            c.network.proxy_mode = match input.trim() {
                "system" => ProxyMode::System,
                "custom" => ProxyMode::Custom,
                "direct" => ProxyMode::Direct,
                _ => anyhow::bail!("代理策略：system / custom / direct"),
            }
        }
        3 => c.network.proxy_url = input.trim().into(),
        5 => {
            c.refresh.sectors_secs =
                crate::config::parse_interval(input).map_err(anyhow::Error::msg)?
        }
        6 => c.pinned_indices = crate::instruments::parse_pinned(input)?,
        _ => {
            let n = input
                .parse::<u64>()
                .map_err(|_| anyhow::anyhow!("超时必须为 1–120 秒"))?;
            if !(1..=120).contains(&n) {
                anyhow::bail!("超时必须为 1–120 秒");
            }
            c.network.timeout_secs = n;
        }
    }
    Ok(())
}
