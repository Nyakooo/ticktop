use crate::{
    config::{Config, Mode, Paths, ProxyMode},
    market::{Kind, MarketEvent, Request},
    model::*,
    search::SearchIndex,
    storage::{Cache, Catalog, UserState},
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

pub enum Confirm {
    Remove(String),
    DeleteGroup(u64),
}
pub enum Modal {
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
        for id in &self.config.pinned_indices {
            if !symbols.contains(id) {
                symbols.push(id.clone());
            }
        }
        Request {
            config: self.config.clone(),
            symbols,
            focus: self.user.selected.clone(),
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
    fn save_state_change(&mut self) -> bool {
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
    fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if !self.paused {
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
    pub fn key(&mut self, key: KeyEvent) {
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
        if self.user.heatmap && self.cloud_key(key.code) {
            return;
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
            KeyCode::Left | KeyCode::Right => {
                self.user.period = self.user.period.next(key.code == KeyCode::Left);
                self.load_chart();
                self.request_dirty = true;
            }
            // Shift+arrows inspect the chart without changing its period.
            KeyCode::Home => self.chart_cursor = Some(0),
            KeyCode::End => {
                self.chart_cursor = self.series().map(|s| s.points.len().saturating_sub(1))
            }
            KeyCode::Char('j') | KeyCode::Char('k') => {
                if let Some(s) = self.series() {
                    let n = s.points.len();
                    let i = self.chart_cursor.unwrap_or(n.saturating_sub(1));
                    self.chart_cursor = Some(if key.code == KeyCode::Char('j') {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(n.saturating_sub(1))
                    });
                }
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
