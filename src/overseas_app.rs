use crate::{
    app::{App, Confirm, Modal},
    config::Mode,
    overseas::{self, Page, Session},
};
use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[derive(Clone, Copy)]
pub enum Input {
    Add,
    Alias,
    Group,
    RenameGroup,
    AssignGroup,
}
impl Input {
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "订阅美股：代码 / 常见中文名称",
            Self::Alias => "设置海外别名（留空恢复代码）",
            Self::Group => "美股分组：输入名称选择或创建，留空为全部",
            Self::RenameGroup => "修改美股分组名称",
            Self::AssignGroup => "将当前美股加入分组：输入分组名称",
        }
    }
}
impl App {
    pub fn page(&self) -> Page {
        if self.user.heatmap {
            Page::Heatmap
        } else if self.user.overseas.page == Page::Heatmap {
            Page::Watch
        } else {
            self.user.overseas.page
        }
    }
    pub fn set_page(&mut self, page: Page) {
        let previous = self.page();
        if page == Page::Heatmap && previous != Page::Heatmap {
            self.user.overseas.return_page = previous;
        }
        self.user.overseas.page = page;
        self.user.heatmap = page == Page::Heatmap;
        self.user.overseas.ensure_selection();
        self.overseas_error = None;
        self.night_error = None;
        self.conceal_names();
        self.request_dirty = true;
        self.dirty = true;
        self.save_state_change();
    }
    pub fn overseas_snapshot(&self, id: &str) -> Option<&overseas::Snapshot> {
        self.overseas
            .values()
            .filter(|s| s.id == id)
            .max_by_key(|s| s.received_at)
    }
    pub fn overseas_series(&self) -> Option<&overseas::Snapshot> {
        let s = &self.user.overseas;
        self.overseas.get(&(s.selected()?.into(), s.period()))
    }
    fn overseas_quote_snapshot(&self, id: &str) -> Option<&overseas::Snapshot> {
        self.overseas
            .get(&(id.to_string(), crate::model::Period::Intraday))
            .or_else(|| self.overseas_snapshot(id))
    }

    pub fn overseas_quote_is_current(&self, id: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.demo
            || overseas::current_us_quote(
                self.overseas_quote_snapshot(id),
                self.nights.get(id),
                now,
            )
            .is_some()
    }

    pub fn overseas_quote(&self, id: &str) -> Option<&overseas::Quote> {
        let snapshot = self.overseas_quote_snapshot(id);
        if self.page() != Page::Us {
            return snapshot.and_then(|s| s.quote(Session::Regular));
        }
        if self.demo {
            return snapshot.and_then(|s| s.quote(Session::All));
        }
        overseas::display_us_quote(snapshot, self.nights.get(id), chrono::Utc::now())
    }

    pub fn active_us_session(&self) -> Session {
        if self.demo || self.page() != Page::Us {
            return self
                .user
                .overseas
                .selected()
                .and_then(|id| self.overseas_quote(id))
                .map_or(Session::All, |q| q.session);
        }
        let snapshot = self
            .user
            .overseas
            .selected()
            .and_then(|id| self.overseas_quote_snapshot(id));
        overseas::us_session_at(snapshot, chrono::Utc::now()).unwrap_or(Session::All)
    }

    pub fn overseas_chart_session(&self, period: crate::model::Period) -> Session {
        if self.page() == Page::Us && period == crate::model::Period::Intraday {
            self.active_us_session()
        } else {
            Session::All
        }
    }

    pub fn overseas_chart_bars<'a>(
        &self,
        snapshot: &'a overseas::Snapshot,
    ) -> Vec<&'a overseas::Bar> {
        let session = self.overseas_chart_session(snapshot.period);
        let mut bars = snapshot.bars(session);
        if self.page() == Page::Us && !self.demo && session != Session::All {
            let now = chrono::Utc::now();
            let date = now
                .with_timezone(&chrono_tz::America::New_York)
                .date_naive();
            bars.retain(|bar| {
                chrono::DateTime::from_timestamp(bar.time, 0).is_some_and(|time| {
                    time <= now
                        && time
                            .with_timezone(&chrono_tz::America::New_York)
                            .date_naive()
                            == date
                })
            });
        }
        bars
    }

    pub fn overseas_label<'a>(&'a self, id: &'a str) -> &'a str {
        if self.user.mode == Mode::Minimal && !self.reveal_names {
            self.user
                .overseas
                .aliases
                .get(id)
                .map_or(id, String::as_str)
        } else {
            self.user
                .overseas
                .us_names
                .get(id)
                .map(String::as_str)
                .unwrap_or_else(|| overseas::name(id))
        }
    }
    pub fn overseas_select(&mut self, index: usize) {
        if self.page() == Page::Indices {
            if let Some(id) = self.user.overseas.index_rows().get(index) {
                self.user.overseas.index_selected = id.clone();
            }
        } else if let Some(id) = self.user.overseas.visible().get(index) {
            self.user.overseas.us_selected = Some(id.clone());
        }
        self.overseas_cursor = None;
        self.overseas_error = None;
        self.night_error = None;
        self.request_dirty = true;
        self.dirty = true;
        self.save_state_change();
    }
    pub(crate) fn overseas_key(&mut self, key: KeyEvent) {
        if self.page() == Page::Indices && self.index_key(key) {
            return;
        }
        let index_page = self.page() == Page::Indices;
        let state = &mut self.user.overseas;
        match key.code {
            KeyCode::Up | KeyCode::Down
                if !index_page && key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if let Some(id) = state.us_selected.clone()
                    && let Err(e) = state.move_us_symbol(&id, key.code == KeyCode::Down)
                {
                    self.message = e.to_string();
                }
            }
            KeyCode::Char('t') if !index_page => {
                if let Some(id) = state.us_selected.clone()
                    && let Err(e) = state.toggle_us_pin(&id)
                {
                    self.message = e.to_string();
                }
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k') if key.modifiers.is_empty() => {
                let rows = if index_page {
                    state.index_rows()
                } else {
                    state.visible()
                };
                if !rows.is_empty() {
                    let i = rows
                        .iter()
                        .position(|s| Some(s.as_str()) == state.selected())
                        .unwrap_or(0);
                    let down = matches!(key.code, KeyCode::Down | KeyCode::Char('j'));
                    let i = (i + if down { 1 } else { rows.len() - 1 }) % rows.len();
                    self.overseas_select(i);
                    return;
                }
            }
            KeyCode::Left | KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(s) = self.overseas_series() {
                    let n = self.overseas_chart_bars(s).len();
                    if n > 0 {
                        let i = self.overseas_cursor.unwrap_or(n - 1).min(n - 1);
                        self.overseas_cursor = Some(if key.code == KeyCode::Left {
                            i.saturating_sub(1)
                        } else {
                            (i + 1).min(n - 1)
                        });
                    }
                }
            }
            KeyCode::Home => self.overseas_cursor = Some(0),
            KeyCode::End => self.overseas_cursor = None,
            KeyCode::Left | KeyCode::Right => {
                let p = if index_page {
                    &mut state.index_period
                } else {
                    &mut state.us_period
                };
                *p = p.next(key.code == KeyCode::Left);
                self.overseas_cursor = None;
                self.request_dirty = true;
            }
            KeyCode::Char('[' | ']') if !index_page => {
                let ids = std::iter::once(0)
                    .chain(state.groups.iter().map(|g| g.id))
                    .collect::<Vec<_>>();
                let i = ids.iter().position(|g| *g == state.group).unwrap_or(0);
                state.group = ids[(i + if key.code == KeyCode::Char('[') {
                    ids.len() - 1
                } else {
                    1
                }) % ids.len()];
                state.ensure_selection();
                self.request_dirty = true;
                self.overseas_cursor = None;
            }
            KeyCode::Char('/' | 'a') if !index_page => {
                self.us_search_generation = self.us_search_generation.wrapping_add(1);
                self.modal = Some(Modal::UsSearch {
                    query: String::new(),
                    results: crate::us_search::local(state, ""),
                    index: 0,
                    generation: self.us_search_generation,
                    status: "常见美股＋已订阅；输入后补充在线结果".into(),
                });
                self.request_dirty = true;
            }
            KeyCode::Char('e') => {
                if let Some(id) = state.selected() {
                    self.modal = Some(Modal::OverseasInput {
                        purpose: Input::Alias,
                        input: state.aliases.get(id).cloned().unwrap_or_default(),
                    });
                }
            }
            KeyCode::Char('g') if !index_page => {
                self.modal = Some(Modal::OverseasInput {
                    purpose: Input::Group,
                    input: String::new(),
                })
            }
            KeyCode::Char('G') if !index_page => {
                self.modal = Some(Modal::OverseasInput {
                    purpose: Input::AssignGroup,
                    input: String::new(),
                })
            }
            KeyCode::Char('r') if !index_page && state.group != 0 => {
                self.modal = Some(Modal::OverseasInput {
                    purpose: Input::RenameGroup,
                    input: state
                        .groups
                        .iter()
                        .find(|g| g.id == state.group)
                        .map(|g| g.name.clone())
                        .unwrap_or_default(),
                })
            }
            KeyCode::Char('d') if !index_page => {
                if let Some(id) = state.us_selected.clone() {
                    self.modal = Some(Modal::Confirm(Confirm::OverseasRemove(id)));
                }
            }
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.modal = Some(Modal::Help { offset: 0 }),
            KeyCode::Char(',') => {
                self.modal = Some(Modal::Settings {
                    draft: self.config.clone(),
                    field: 0,
                    input: self.config.refresh.quotes_secs.to_string(),
                })
            }
            _ => return,
        }
        self.save_state_change();
    }
    fn index_key(&mut self, key: KeyEvent) -> bool {
        let state = &mut self.user.overseas;
        match key.code {
            KeyCode::Up | KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                if let Err(e) = state.move_index(key.code == KeyCode::Down) {
                    self.message = e.to_string();
                }
            }
            KeyCode::Tab => {
                state.index_all = !state.index_all;
                state.index_detail = false;
                state.ensure_index_selection();
                self.overseas_cursor = None;
                self.request_dirty = true;
            }
            KeyCode::Enter if state.selected().is_some() => {
                state.index_detail = true;
                self.request_dirty = true;
            }
            KeyCode::Esc => {
                state.index_detail = false;
                self.overseas_cursor = None;
                self.request_dirty = true;
            }
            KeyCode::Home | KeyCode::End if !state.index_detail => {
                let rows = state.index_rows();
                if let Some(id) = if key.code == KeyCode::Home {
                    rows.first()
                } else {
                    rows.last()
                } {
                    state.index_selected = id.clone();
                }
            }
            KeyCode::Left | KeyCode::Right if !state.index_detail => {}
            KeyCode::Char('a' | '/') => {
                self.modal = Some(Modal::IndexSearch {
                    query: String::new(),
                    results: crate::index_board::catalog()
                        .into_iter()
                        .map(|(id, name)| crate::model::Instrument {
                            id: id.into(),
                            name: name.into(),
                        })
                        .collect(),
                    index: 0,
                });
            }
            KeyCode::Char('d') => {
                let id = state.index_selected.clone();
                state.remove_index(&id);
                self.overseas_cursor = None;
                self.request_dirty = true;
                self.message = "已取消关注，全部指数目录仍保留".into();
            }
            KeyCode::Char('t') => {
                if let Some(id) = state.selected().map(str::to_string)
                    && let Err(e) = state.pin_index(&id)
                {
                    self.message = e.to_string();
                }
                self.request_dirty = true;
            }
            _ => return false,
        }
        self.save_state_change();
        true
    }
    pub(crate) fn overseas_input(&mut self, purpose: Input, input: &str) -> Result<()> {
        let state = &mut self.user.overseas;
        match purpose {
            Input::Add => {
                let input = input.trim();
                let id = overseas::STOCKS
                    .iter()
                    .find(|(id, name)| id.eq_ignore_ascii_case(input) || *name == input)
                    .map(|(id, _)| id.to_string())
                    .unwrap_or_else(|| input.to_ascii_uppercase());
                if !overseas::valid_symbol(&id) {
                    bail!("请输入美股代码，如 AAPL / NVDA / BRK-B，或常见中文名称");
                }
                if !state.subscriptions.contains(&id) {
                    if state.subscriptions.len() >= 100 {
                        bail!("最多订阅100只美股");
                    }
                    state.subscriptions.push(id.clone());
                }
                if let Some(g) = state.groups.iter_mut().find(|g| g.id == state.group)
                    && !g.symbols.contains(&id)
                {
                    g.symbols.push(id.clone());
                }
                state.us_selected = Some(id);
                self.request_dirty = true;
            }
            Input::Alias => {
                let id = state.selected().context("请先选择标的")?.to_string();
                let alias = input.trim();
                if alias.is_empty() {
                    state.aliases.remove(&id);
                } else {
                    if !overseas::valid_label(alias) {
                        bail!("别名需为1–24个可见字符");
                    }
                    state.aliases.insert(id, alias.into());
                }
            }
            Input::Group => {
                state.group = if input.trim().is_empty() {
                    0
                } else {
                    state.group_named(input)?
                };
                state.ensure_selection();
                self.request_dirty = true;
            }
            Input::AssignGroup => {
                let id = state.us_selected.clone().context("请先选择美股")?;
                let group = state.group_named(input)?;
                if let Some(g) = state.groups.iter_mut().find(|g| g.id == group)
                    && !g.symbols.contains(&id)
                {
                    g.symbols.push(id);
                }
                self.message = "已加入美股分组".into();
            }
            Input::RenameGroup => {
                let name = input.trim();
                if !overseas::valid_label(name)
                    || state
                        .groups
                        .iter()
                        .any(|g| g.id != state.group && g.name == name)
                {
                    bail!("名称为空、过长或重复");
                }
                state
                    .groups
                    .iter_mut()
                    .find(|g| g.id == state.group)
                    .context("分组不存在")?
                    .name = name.into();
            }
        }
        self.overseas_cursor = None;
        self.save_state_change();
        Ok(())
    }
}
