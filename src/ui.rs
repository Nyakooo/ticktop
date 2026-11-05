use crate::{
    app::{App, Confirm, Modal, field_value},
    config::Mode,
    model::{Period, Quote},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Cell, Chart, Clear, Dataset, GraphType, List, ListItem, ListState,
        Paragraph, Row, Table, TableState, Wrap,
    },
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) fg: Color,
    pub(crate) dim: Color,
    pub(crate) line: Color,
    pub(crate) accent: Color,
    pub(crate) up: Color,
    pub(crate) down: Color,
    pub(crate) bg: Color,
    pub(crate) selected: Color,
}
impl Palette {
    fn new(mode: Mode) -> Self {
        let minimal = mode == Mode::Minimal;
        Self {
            fg: Color::Rgb(202, 211, 212),
            dim: Color::Rgb(130, 145, 149),
            line: Color::Rgb(49, 65, 70),
            accent: if minimal {
                Color::Rgb(174, 183, 187)
            } else {
                Color::Rgb(151, 207, 200)
            },
            up: if minimal {
                Color::Rgb(174, 183, 187)
            } else {
                Color::Rgb(231, 137, 145)
            },
            down: if minimal {
                Color::Rgb(174, 183, 187)
            } else {
                Color::Rgb(113, 193, 157)
            },
            bg: Color::Rgb(17, 22, 25),
            selected: Color::Rgb(31, 46, 49),
        }
    }
    pub(crate) fn change(self, v: f64) -> Color {
        if v > 0.0 {
            self.up
        } else if v < 0.0 {
            self.down
        } else {
            self.fg
        }
    }
}
pub(crate) fn text(s: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(color))
}
fn block(title: impl Into<String>, p: Palette) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(p.line))
        .title(Line::from(text(title, p.accent)))
}
fn source_status(q: &Quote) -> String {
    if q.age_secs() > 120 {
        format!("快照 · {}", q.source_time.format("%m-%d %H:%M"))
    } else {
        "近期报价 · 非保证实时".into()
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let p = Palette::new(app.user.mode);
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.fg)),
        area,
    );
    if area.width < 46 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("窗口过小，请扩大至至少 46 列 × 16 行。\nq 退出 / m 简约")
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let outer = outer_layout(area, app);
    header(frame, app, outer[0], p);
    crate::overseas_ui::nav(frame, app, outer[5], p);
    let overseas = matches!(
        app.page(),
        crate::overseas::Page::Indices | crate::overseas::Page::Us
    );
    if app.page() == crate::overseas::Page::Indices {
        crate::overseas_ui::index_tabs(frame, app, outer[1], p);
    } else if overseas {
        let label = if app.page() == crate::overseas::Page::Indices {
            "全球指数 · 亚洲 / 美国".to_string()
        } else {
            let state = &app.user.overseas;
            let group = state
                .groups
                .iter()
                .find(|g| g.id == state.group)
                .map(|g| g.name.as_str())
                .unwrap_or("全部订阅");
            format!("{group}   {} · Tab 切时段", state.session.label())
        };
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(p.dim)),
            outer[1],
        );
    } else if app.user.heatmap {
        cloud_tabs(frame, app, outer[1], p);
    } else {
        groups(frame, app, outer[1], p);
    }
    if overseas {
        crate::overseas_ui::draw(frame, app, outer[2], p);
    } else if app.user.heatmap {
        heatmap(frame, app, outer[2], p)
    } else if app.user.mode == Mode::Minimal {
        minimal(frame, app, outer[2], p)
    } else {
        full(frame, app, outer[2], p)
    }
    let status = if !app.message.is_empty() {
        app.message.clone()
    } else if app.paused {
        if app.user.mode == Mode::Minimal {
            "hold".into()
        } else {
            "显示已暂停 · 后台继续更新，Space 恢复最新数据".into()
        }
    } else if overseas && app.user.mode == Mode::Minimal {
        if app.overseas_error.is_some() {
            "E04 / cached".into()
        } else if app.night_error.is_some() {
            "E05 / cached".into()
        } else if app.demo {
            "sample".into()
        } else {
            format!(
                "q={}s n={}s",
                app.config
                    .refresh
                    .quotes_secs
                    .max(app.config.refresh.chart_secs)
                    .max(30),
                app.config.refresh.quotes_secs.max(60)
            )
        }
    } else if overseas {
        app.overseas_error
            .as_ref()
            .or(app.night_error.as_ref())
            .cloned()
            .unwrap_or_else(|| {
                if app.demo {
                    "演示数据".into()
                } else {
                    "Yahoo 公开行情 · 时间统一北京时间 · 非保证实时 · 缺失数据保留空白".into()
                }
            })
    } else if app.user.heatmap {
        if app.user.mode == Mode::Minimal {
            if app.sector_error.is_some() {
                "E03 / cached".into()
            } else {
                format!("s={}s", app.config.refresh.sectors_secs)
            }
        } else if let Some(error) = &app.sector_error {
            error.clone()
        } else {
            "全市场板块 · 新浪分类 · 平均涨跌幅 · 数据源不提供行情时间戳".into()
        }
    } else if app.user.mode == Mode::Minimal {
        if app.quote_error.is_some() {
            "E01 / cached".into()
        } else if app.chart_error.is_some() {
            "E02 / cached".into()
        } else {
            format!(
                "q={}s c={}s",
                app.config.refresh.quotes_secs, app.config.refresh.chart_secs
            )
        }
    } else if let Some(e) = &app.quote_error {
        e.clone()
    } else if let Some(e) = &app.chart_error {
        e.clone()
    } else if let Some(e) = &app.catalog_error {
        e.clone()
    } else {
        format!(
            "{} · {} 只自选 · / 搜索添加",
            if app.demo {
                "演示数据"
            } else {
                "免费公开行情 · 数据以源时间为准"
            },
            app.user.watchlist.len()
        )
    };
    frame.render_widget(
        Paragraph::new(status)
            .style(Style::default().fg(p.dim))
            .wrap(Wrap { trim: true }),
        outer[3],
    );
    let keys = if app.page() == crate::overseas::Page::Indices {
        if app.user.overseas.index_detail {
            "m 简约  h 云图  Esc 总览  j/k 选择  ←→ 周期  Shift+←→ 游标  ? 帮助  q 退出"
        } else {
            "m 简约  h 云图  j/k 选择  Enter 走势  a 添加  d 移除  t 置顶  Alt+↑↓ 排序  e 别名  ?  q"
        }
    } else if overseas {
        "m 简约  h 云图  j/k 选择  ←→ 周期  Tab 时段  t 置顶  Alt+↑↓ 排序  a 订阅  e 别名  g 分组  ? 帮助  q 退出"
    } else if app.user.heatmap {
        if app.user.mode == Mode::Minimal {
            "m mode  h back     Tab type  n/p page  s sort  mouse  i index  i index  , cfg  ?  q"
        } else {
            "m 简约  h 返回前页   Tab 分类  s 排序  PgUp/PgDn 或 n/p 翻页  鼠标点击/滚轮  i 常驻  , 设置  ? 帮助  q 退出"
        }
    } else if app.user.mode == Mode::Minimal {
        if app.key_releases {
            "m mode  v hold:name  t pin  e alias  g groups  / find  ?  q"
        } else {
            "m mode  v toggle:name  t pin  e alias  g groups  / find  ?  q"
        }
    } else {
        "m 简约  h 云图  t 置顶  Alt+↑↓ 排序  e 别名  g 分组  / 搜索  ←→ 周期  ? 帮助  q 退出"
    };
    frame.render_widget(
        Paragraph::new(keys).style(Style::default().fg(p.dim)),
        outer[4],
    );
    if let Some(modal) = &app.modal {
        modal_view(frame, app, modal, p);
    }
}
fn header(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let quiet = app.user.mode == Mode::Minimal;
    if matches!(
        app.page(),
        crate::overseas::Page::Indices | crate::overseas::Page::Us
    ) {
        f.render_widget(
            Paragraph::new(if quiet && app.page() == crate::overseas::Page::Indices {
                "metrics --index".to_string()
            } else {
                format!(
                    "ticktop_   {} · 只看盘{}",
                    app.page().label(),
                    if app.demo { " · 演示数据" } else { "" }
                )
            })
            .style(Style::default().fg(p.accent)),
            r,
        );
        return;
    }
    let spans = vec![
        text(if quiet { "metrics --watch" } else { "ticktop_" }, p.accent),
        text(
            if quiet {
                "    market / local"
            } else {
                "   行情 · 只看盘"
            },
            p.dim,
        ),
        text(
            if app.demo {
                if quiet {
                    "   sample"
                } else {
                    "   [演示数据]"
                }
            } else {
                if quiet {
                    "   live"
                } else {
                    "   [公开行情]"
                }
            },
            p.dim,
        ),
        text(if app.paused { "   ||" } else { "" }, p.dim),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), r);
    let summary = match (app.quotes.get("sh000002"), app.quotes.get("sz399107")) {
        (Some(sh), Some(sz)) if sh.source_time.date_naive() == sz.source_time.date_naive() => {
            match (sh.amount_wan, sz.amount_wan) {
                (Some(a), Some(b)) => format!(
                    "两市A股成交额 {:.2}亿{} · {} {}{}",
                    (a + b) / 10000.,
                    turnover_comparison(app, sh, sz),
                    sh.source_time.format("%m-%d"),
                    sh.source_time.min(sz.source_time).format("%H:%M:%S"),
                    if sh.age_secs() > 120 || sz.age_secs() > 120 {
                        " · 快照"
                    } else {
                        ""
                    }
                ),
                _ => "两市A股成交额 --".into(),
            }
        }
        _ => "两市A股成交额 --".into(),
    };
    f.render_widget(
        Paragraph::new(summary).style(Style::default().fg(p.dim)),
        Rect::new(r.x, r.y + 1, r.width, 1),
    );
    index_bar(f, app, r, p);
}
fn turnover_comparison(app: &App, sh: &Quote, sz: &Quote) -> String {
    let Some(c) = &app.turnover else {
        return String::new();
    };
    if c.date != sh.source_time.format("%Y%m%d").to_string()
        || c.date != sz.source_time.format("%Y%m%d").to_string()
    {
        return String::new();
    }
    let Ok(time) =
        chrono::NaiveDateTime::parse_from_str(&format!("{} {}", c.date, c.minute), "%Y%m%d %H:%M")
    else {
        return String::new();
    };
    let lag = (sh.source_time.min(sz.source_time).naive_local() - time).num_seconds();
    let closed = c.minute == "15:00" && lag >= 0;
    if !closed
        && (!(0..=180).contains(&lag)
            || (!app.paused && (chrono::Utc::now() - c.fetched_at).num_seconds() > 180))
    {
        return String::new();
    }
    let delta = c.delta_yi();
    // Below the displayed 0.01亿 precision, show unchanged rather than a signed zero.
    let label = if delta.abs() < 0.005 {
        "持平".into()
    } else {
        format!(
            "{} {:.2}亿",
            if delta > 0. { "放量" } else { "缩量" },
            delta.abs()
        )
    };
    if closed {
        format!(" · 较前日收盘{label}（15:00）")
    } else {
        format!(" · 较昨同时{label}（{}）", c.minute)
    }
}
fn index_columns(app: &App, width: u16) -> usize {
    (width
        / if app.user.mode == Mode::Minimal {
            31
        } else {
            42
        })
    .max(1) as usize
}
fn index_rows(app: &App, width: u16) -> u16 {
    app.config
        .pinned_indices
        .len()
        .div_ceil(index_columns(app, width)) as u16
}
fn index_cells(app: &App, r: Rect) -> Vec<Rect> {
    let columns = index_columns(app, r.width);
    let width = r.width / columns as u16;
    app.config
        .pinned_indices
        .iter()
        .enumerate()
        .take(r.height.saturating_sub(2) as usize * columns)
        .map(|(i, _)| {
            Rect::new(
                r.x + (i % columns) as u16 * width,
                r.y + 2 + (i / columns) as u16,
                width,
                1,
            )
        })
        .collect()
}
fn index_bar(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let cells = index_cells(app, r);
    for (i, (id, cell)) in app
        .config
        .pinned_indices
        .iter()
        .zip(cells.iter().copied())
        .enumerate()
    {
        if cells.len() < app.config.pinned_indices.len() && i + 1 == cells.len() {
            f.render_widget(
                Paragraph::new(format!(
                    "+{} 项 · 放大窗口 / i 设置",
                    app.config.pinned_indices.len() - i
                ))
                .style(Style::default().fg(p.dim)),
                cell,
            );
            continue;
        }
        let name = if app.user.mode == Mode::Minimal {
            app.minimal_label(id)
        } else {
            crate::instruments::INDICES
                .iter()
                .find(|(code, _)| code == id)
                .map_or(id.as_str(), |(_, name)| *name)
        };
        let mut spans = vec![text(format!("{name} "), p.dim)];
        if let Some(q) = app.quotes.get(id) {
            spans.push(text(
                format!(
                    "{} {:+.2}%",
                    crate::instruments::price(id, q.price),
                    q.percent()
                ),
                p.change(q.percent()),
            ));
            if app.user.mode == Mode::Full {
                spans.push(text(
                    format!(" {}", q.source_time.format("%m-%d %H:%M")),
                    p.dim,
                ));
            }
            if q.age_secs() > 120 {
                spans.push(text("*", p.dim));
            }
        } else {
            spans.push(text("-- --", p.dim));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), cell);
    }
}
fn groups(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let quiet = app.user.mode == Mode::Minimal;
    let mut list = vec![(0, "全部自选".to_string())];
    list.extend(app.user.groups.iter().map(|g| (g.id, g.name.clone())));
    let spans = list
        .into_iter()
        .flat_map(|(id, name)| {
            let selected = id == app.user.group;
            let label = if quiet {
                format!(" g{id:02} ")
            } else {
                format!(" {name} ")
            };
            [
                Span::styled(
                    label,
                    Style::default()
                        .fg(if selected { p.accent } else { p.dim })
                        .bg(if selected { p.selected } else { p.bg }),
                ),
                text(" ", p.dim),
            ]
        })
        .collect::<Vec<_>>();
    // Keep the selected group visible even when there are more groups than columns.
    let width: usize = spans.iter().map(|s| s.content.width()).sum();
    let line = if width > r.width as usize {
        Line::from(vec![
            text("[ ", p.dim),
            text(
                if quiet {
                    format!("g{:02}", app.user.group)
                } else {
                    app.user.group_name().into()
                },
                p.accent,
            ),
            text(" ]", p.dim),
        ])
    } else {
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(line), r);
}
fn full(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let panes = if r.width >= 98 {
        Layout::horizontal([Constraint::Length(36), Constraint::Min(35)])
            .spacing(2)
            .split(r)
    } else {
        Layout::vertical([
            Constraint::Length((r.height / 3).clamp(5, 10)),
            Constraint::Min(7),
        ])
        .spacing(1)
        .split(r)
    };
    let ids = app.visible();
    let rows = ids
        .iter()
        .map(|id| {
            let instrument = app.catalog.iter().find(|s| &s.id == id);
            let name = instrument
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.clone());
            let q = app.quotes.get(id);
            let price = q
                .map(|q| crate::instruments::price(&q.id, q.price))
                .unwrap_or_else(|| "--".into());
            let pct = q
                .map(|q| format!("{:+.2}%", q.percent()))
                .unwrap_or_else(|| "--".into());
            Row::new(vec![
                Cell::from(vec![
                    Line::from(name),
                    Line::from(text(
                        format!(
                            "{}{}",
                            if app.user.is_pinned(id) { "↑ " } else { "" },
                            id.get(2..).unwrap_or(id)
                        ),
                        p.dim,
                    )),
                ]),
                Cell::from(price)
                    .style(Style::default().fg(q.map(|q| p.change(q.percent())).unwrap_or(p.dim))),
                Cell::from(pct)
                    .style(Style::default().fg(q.map(|q| p.change(q.percent())).unwrap_or(p.dim))),
            ])
            .height(2)
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Min(10),
            Constraint::Length(9),
            Constraint::Length(9),
        ],
    )
    .header(Row::new(["自选 / 代码", "最新价", "较昨收"]).style(Style::default().fg(p.dim)))
    .row_highlight_style(Style::default().bg(p.selected))
    .highlight_symbol("› ");
    let mut state = TableState::default().with_selected(
        ids.iter()
            .position(|id| Some(id) == app.user.selected.as_ref()),
    );
    f.render_stateful_widget(table, panes[0], &mut state);
    if ids.is_empty() {
        f.render_widget(
            Paragraph::new("当前分组为空\na 搜索并添加股票").style(Style::default().fg(p.dim)),
            panes[0],
        );
    }
    detail(f, app, panes[1], p);
}
fn detail(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let Some(id) = &app.user.selected else { return };
    let stock = app.selected_stock();
    let q = app.quotes.get(id);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(r);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            text(stock.map(|s| s.name.as_str()).unwrap_or(id), p.fg),
            text(
                format!(
                    "  {} · {}",
                    id.get(2..).unwrap_or(id),
                    crate::instruments::kind(id)
                ),
                p.dim,
            ),
        ])),
        rows[0],
    );
    let mut quote = vec![
        text(
            q.map(|q| crate::instruments::price(&q.id, q.price))
                .unwrap_or_else(|| "--".into()),
            p.fg,
        ),
        text(format!(" {}   ", crate::instruments::unit(id)), p.dim),
    ];
    if let Some(q) = q {
        quote.push(text(
            format!(
                "{:+.2}%  {}{}",
                q.percent(),
                if q.change().0 >= 0 { "+" } else { "" },
                crate::instruments::price(id, q.change())
            ),
            p.change(q.percent()),
        ));
    }
    let mut metrics = q
        .map(quote_metrics)
        .unwrap_or_else(|| "量比 --  换手率 --".into());
    if crate::instruments::is_index(id) {
        metrics.push_str(" · 筹码峰：指数不适用");
    } else if crate::instruments::is_etf(id) {
        metrics.push_str(" · 筹码峰：ETF 暂不支持");
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(quote),
            Line::styled(metrics, Style::default().fg(p.dim)),
        ]),
        rows[1],
    );
    let tabs = Period::ALL
        .into_iter()
        .flat_map(|period| {
            [
                Span::styled(
                    format!(" {} ", period.label()),
                    Style::default()
                        .fg(if period == app.user.period {
                            p.accent
                        } else {
                            p.dim
                        })
                        .bg(if period == app.user.period {
                            p.selected
                        } else {
                            p.bg
                        }),
                ),
                text(" ", p.dim),
            ]
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(Line::from(tabs)), rows[2]);
    if crate::instruments::is_index(id) || crate::instruments::is_etf(id) {
        price_chart(f, app, rows[3], p);
    } else if rows[3].width >= 66 {
        let panes = Layout::horizontal([Constraint::Min(30), Constraint::Length(31)])
            .spacing(2)
            .split(rows[3]);
        price_chart(f, app, panes[0], p);
        chip_panel(f, app, panes[1], p);
    } else {
        let panes = Layout::vertical([Constraint::Min(2), Constraint::Length(4)]).split(rows[3]);
        price_chart(f, app, panes[0], p);
        chip_panel(f, app, panes[1], p);
    }
    let readout = if let Some(s) = app.series() {
        if let Some(i) = app.chart_cursor {
            let pt = &s.points[i.min(s.points.len().saturating_sub(1))];
            format!(
                "{}   {} {}   Shift+←→ 游标",
                pt.label,
                crate::instruments::price(&s.id, pt.price),
                crate::instruments::unit(&s.id)
            )
        } else {
            format!(
                "{} · {} · {}点 · Shift+←→ 查看",
                s.date,
                if app.user.period == Period::Intraday {
                    "分时价格"
                } else {
                    "OHLC · 不复权"
                },
                s.points.len()
            )
        }
    } else {
        "走势加载中；网络异常可按 , 修改代理".into()
    };
    f.render_widget(
        Paragraph::new(readout).style(Style::default().fg(p.dim)),
        rows[4],
    );
    let source = if let Some(q) = q {
        format!(
            "{} · 源时间 {} · {}",
            if app.demo { "模拟" } else { "腾讯" },
            q.source_time.format("%m-%d %H:%M:%S"),
            if app.demo {
                "演示".into()
            } else {
                source_status(q)
            }
        )
    } else {
        "等待报价；不会使用模拟数据替代真实行情".into()
    };
    f.render_widget(
        Paragraph::new(source).style(Style::default().fg(p.dim)),
        rows[5],
    );
}
fn chip_panel(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let d = app.user.selected.as_ref().and_then(|id| app.chips.get(id));
    let Some(d) = d else {
        f.render_widget(
            Paragraph::new(vec![
                Line::from("筹码峰 · 估算"),
                Line::from(if app.chip_error.is_some() {
                    "数据暂不可用，稍后重试"
                } else {
                    "日线数据加载中…"
                }),
            ])
            .style(Style::default().fg(p.dim)),
            r,
        );
        return;
    };
    let mut lines = vec![Line::from(text("筹码峰 · 估算 / 独立价格轴", p.accent))];
    let count = r.height.saturating_sub(6) as usize;
    if count >= 3 {
        let mut rows = vec![(0., 0.); count];
        for (i, &(price, weight)) in d.bins.iter().enumerate() {
            let index = (i * count / d.bins.len()).min(count - 1);
            rows[index].0 = price;
            rows[index].1 += weight;
        }
        let max = rows.iter().map(|b| b.1).fold(0., f64::max).max(1e-12);
        let width = r.width.saturating_sub(11) as usize;
        for (price, weight) in rows.into_iter().rev() {
            let len = (weight / max * width as f64).round() as usize;
            lines.push(Line::from(vec![
                text(format!("{price:>8.2} "), p.dim),
                text(
                    "─".repeat(len),
                    if price <= d.close { p.up } else { p.down },
                ),
            ]));
        }
    }
    lines.push(Line::from(text(
        format!("估算均价 {:.2}", d.average()),
        p.fg,
    )));
    lines.push(Line::from(text(
        format!("90% {:.2}–{:.2}", d.quantile(0.05), d.quantile(0.95)),
        p.fg,
    )));
    lines.push(Line::from(text(
        format!("日线获利比例 {:.1}%", d.profit()),
        p.dim,
    )));
    lines.push(Line::from(text(
        format!("截至 {} · {}日", d.date, d.days),
        p.dim,
    )));
    lines.push(Line::from(text(
        if app.chip_error.is_some() {
            "更新失败 · 保留缓存".into()
        } else {
            format!("前复权 · 初始残留 {:.1}%", d.seed_remaining * 100.)
        },
        p.dim,
    )));
    f.render_widget(Paragraph::new(lines), r);
}
// Use instrument tick units and evenly spaced ticks: Ratatui positions axis labels uniformly.
// Rounding arbitrary floats only at display time can produce misleading/repeated labels.
pub(crate) fn price_scale(
    min: crate::model::Price,
    max: crate::model::Price,
    height: u16,
    decimals: usize,
) -> Vec<i64> {
    let units = if decimals == 3 { 10 } else { 100 };
    let low = min.0.div_euclid(units);
    let high = (max.0 + units - 1).div_euclid(units);
    let padding = ((high - low) * 12 / 100).max(high / 1000).max(1);
    let low = (low - padding).max(0);
    let high = high + padding;
    let intervals = (height.saturating_sub(3) / 2).clamp(2, 12) as i64;
    let rough = ((high - low) + intervals - 1) / intervals;
    let mut magnitude = 1;
    while magnitude * 10 < rough {
        magnitude *= 10;
    }
    let step = [1, 2, 5, 10]
        .into_iter()
        .map(|n| n * magnitude)
        .find(|n| *n >= rough)
        .unwrap();
    let start = low.div_euclid(step) * step;
    let end = ((high + step - 1) / step) * step;
    (start..=end).step_by(step as usize).collect()
}

fn quote_metrics(q: &Quote) -> String {
    format!(
        "量比 {}  换手率 {}",
        q.volume_ratio
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "--".into()),
        q.turnover_percent
            .map(|v| format!("{v:.2}%"))
            .unwrap_or_else(|| "--".into())
    )
}
fn volume_text(volume: Option<f64>, id: &str) -> String {
    let unit = Period::volume_unit(id);
    volume
        .map(|v| {
            if v >= 10000. {
                format!("{:.2}万{unit}", v / 10000.)
            } else {
                format!("{v:.0}{unit}")
            }
        })
        .unwrap_or_else(|| "--".into())
}
/// Trading minutes, excluding the lunch break. Pre-open points sit at the opening edge.
pub fn intraday_position(label: &str, hong_kong: bool) -> Option<f64> {
    let (h, m) = label.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    if h > 23 || m > 59 {
        return None;
    }
    let time = h * 60 + m;
    let morning_end = if hong_kong { 720 } else { 690 };
    let close = if hong_kong { 960 } else { 900 };
    Some(if time <= morning_end {
        time.saturating_sub(570)
    } else {
        morning_end - 570 + time.min(close).saturating_sub(780)
    } as f64)
}
fn price_chart(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let Some(full_series) = app.series().filter(|s| !s.points.is_empty()) else {
        f.render_widget(
            Paragraph::new(if app.chart_error.is_some() {
                "暂无走势 · 数据请求失败"
            } else {
                "正在获取走势…"
            })
            .style(Style::default().fg(p.dim)),
            r,
        );
        return;
    };
    if r.width < 18 || r.height < 4 {
        return;
    }
    let mut viewport;
    let mut start = 0;
    let s = if full_series.period != Period::Intraday {
        let count = (r.width.saturating_sub(14) as usize / 2)
            .max(1)
            .min(full_series.points.len());
        let last = app
            .chart_cursor
            .unwrap_or(full_series.points.len() - 1)
            .min(full_series.points.len() - 1);
        let end = if last < full_series.points.len().saturating_sub(count) {
            (last + count).min(full_series.points.len())
        } else {
            full_series.points.len()
        };
        start = end - count;
        viewport = full_series.clone();
        viewport.points = full_series.points[start..end].to_vec();
        &viewport
    } else {
        full_series
    };
    let points: Vec<_> = s
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                if s.period == Period::Intraday {
                    intraday_position(&p.label, s.id.starts_with("hk")).unwrap_or(0.)
                } else {
                    i as f64
                },
                p.price.as_f64(),
            )
        })
        .collect();
    let min = s
        .points
        .iter()
        .map(|pt| pt.ohlc.as_ref().map_or(pt.price, |c| c.low))
        .min()
        .unwrap();
    let max = s
        .points
        .iter()
        .map(|pt| pt.ohlc.as_ref().map_or(pt.price, |c| c.high))
        .max()
        .unwrap();
    let decimals = crate::instruments::decimals(&s.id);
    let scale = if decimals == 3 { 1000. } else { 100. };
    let ticks = price_scale(min, max, r.height.saturating_sub(1), decimals);
    let bounds = [
        ticks[0] as f64 / scale,
        ticks[ticks.len() - 1] as f64 / scale,
    ];
    let selected = app
        .chart_cursor
        .map(|i| i.saturating_sub(start))
        .unwrap_or(points.len() - 1)
        .min(points.len() - 1);
    let selected_point = &s.points[selected];
    let mut caption = format!(
        "{} {} {}  {}",
        if app.chart_cursor.is_some() {
            "查看"
        } else {
            "末价"
        },
        crate::instruments::price(&s.id, selected_point.price),
        crate::instruments::unit(&s.id),
        selected_point.label
    );
    if let Some(c) = &selected_point.ohlc {
        caption = format!(
            "{} 开{} 高{} 低{} 收{}",
            selected_point.label,
            crate::instruments::price(&s.id, c.open),
            crate::instruments::price(&s.id, c.high),
            crate::instruments::price(&s.id, c.low),
            crate::instruments::price(&s.id, selected_point.price)
        );
    }
    // Leave enough room for both a readable price plot and the volume bars.
    let volume_height = if r.width >= 52 && r.height >= 14 {
        (r.height / 3).clamp(6, 8)
    } else {
        0
    };
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(volume_height),
    ])
    .split(r);
    f.render_widget(
        Paragraph::new(caption).style(Style::default().fg(p.fg)),
        areas[0],
    );
    let r = areas[1];
    let xmin = if s.period == Period::Intraday {
        0.
    } else {
        -0.5
    };
    let xmax = if s.period == Period::Intraday {
        if s.id.starts_with("hk") { 330. } else { 240. }
    } else {
        (points.len() as f64 - 0.5).max(0.5)
    };
    let color = app
        .user
        .selected
        .as_ref()
        .and_then(|id| app.quotes.get(id))
        .map(|q| p.change(q.percent()))
        .unwrap_or(p.accent);
    // Bodies and wicks use true OHLC; close-only cached points never masquerade as candles.
    let candles: Vec<_> = s
        .points
        .iter()
        .enumerate()
        .filter_map(|(i, pt)| {
            pt.ohlc.as_ref().map(|c| {
                let x = i as f64;
                let body_low = c.open.min(pt.price).as_f64();
                let body_high = c.open.max(pt.price).as_f64();
                (
                    vec![(x, c.low.as_f64()), (x, c.high.as_f64())],
                    vec![(x, body_low), (x, body_high)],
                    p.change((pt.price.0 - c.open.0) as f64),
                )
            })
        })
        .collect();
    let mut datasets = Vec::new();
    if s.period == Period::Intraday {
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(&points),
        );
    } else {
        for (wick, body, color) in &candles {
            datasets.extend(crate::candles::datasets(wick, body, *color));
        }
    }
    let price_guide = [(xmin, points[selected].1), (xmax, points[selected].1)];
    let selected_dot = [points[selected]];
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(p.line))
            .data(&price_guide),
    );
    let guide;
    if let Some(i) = app.chart_cursor {
        let pt = points[i.saturating_sub(start).min(points.len() - 1)];
        guide = vec![(pt.0, bounds[0]), (pt.0, bounds[1])];

        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(p.line))
                .data(&guide),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .style(Style::default().fg(p.accent))
            .data(&selected_dot),
    );
    let labels = if s.period == Period::Intraday {
        if s.id.starts_with("hk") {
            vec![Span::raw("09:30"), Span::raw("13:15"), Span::raw("16:00")]
        } else {
            vec![
                Span::raw("09:30"),
                Span::raw("11:30/13:00"),
                Span::raw("15:00"),
            ]
        }
    } else if points.len() > 1 {
        vec![
            Span::raw(s.points[0].label.clone()),
            Span::raw(s.points[points.len() / 2].label.clone()),
            Span::raw(s.points[points.len() - 1].label.clone()),
        ]
    } else {
        vec![Span::raw(s.points[0].label.clone())]
    };
    let ylabels = ticks
        .into_iter()
        .map(|cents| {
            Span::styled(
                format!("{:>9}", format!("{:.*}", decimals, cents as f64 / scale)),
                Style::default().fg(p.dim),
            )
        })
        .collect::<Vec<_>>();
    let volume_labels = labels.clone();
    let labels = if volume_height > 0 {
        labels
            .into_iter()
            .map(|label| label.style(Style::default().fg(p.bg)))
            .collect()
    } else {
        labels
    };
    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .bounds([xmin, xmax])
                .labels(labels)
                .style(Style::default().fg(p.line)),
        )
        .y_axis(
            Axis::default()
                .bounds(bounds)
                .labels(ylabels)
                .style(Style::default().fg(p.line)),
        )
        .hidden_legend_constraints((Constraint::Length(0), Constraint::Length(0)));
    f.render_widget(chart, r);
    if volume_height > 0 {
        volume_chart(
            f,
            app,
            s,
            &points,
            [xmin, xmax],
            volume_labels,
            selected,
            areas[2],
            p,
        );
    }
}

// Dense minute samples sharing a terminal column are summed, never drawn on top of each other.
#[allow(clippy::too_many_arguments)]
fn volume_chart(
    f: &mut Frame,
    app: &App,
    s: &crate::model::Series,
    points: &[(f64, f64)],
    bounds: [f64; 2],
    labels: Vec<Span<'static>>,
    selected: usize,
    r: Rect,
    p: Palette,
) {
    let pt = &s.points[selected];
    let mut caption = format!(
        "成交量 {}（{}） · {} {}",
        if s.period == Period::Intraday {
            "分时"
        } else {
            s.period.label()
        },
        Period::volume_unit(&s.id),
        pt.label,
        volume_text(pt.volume, &s.id)
    );
    if s.period == Period::Intraday {
        let total = app.quotes.get(&s.id).and_then(|q| q.volume_lots);
        caption = format!(
            "成交量 分时（{}） {} · 今日{}",
            Period::volume_unit(&s.id),
            volume_text(pt.volume, &s.id),
            volume_text(total, &s.id)
        );
    }
    let areas = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).split(r);
    f.render_widget(
        Paragraph::new(caption).style(Style::default().fg(p.dim)),
        areas[0],
    );
    let cols = areas[1].width.saturating_sub(14).max(1) as usize;
    let mut bins = vec![(0., None, 0.); cols];
    for (i, pt) in s.points.iter().enumerate() {
        let bin = (((points[i].0 - bounds[0]) / (bounds[1] - bounds[0]) * cols as f64) as usize)
            .min(cols - 1);
        let b = &mut bins[bin];
        if let Some(v) = pt.volume {
            b.0 += v;
            b.1 = Some(v);
        }
        b.2 = pt.ohlc.as_ref().map_or_else(
            || {
                if i > 0 {
                    pt.price.0 - s.points[i - 1].price.0
                } else {
                    0
                }
            },
            |c| pt.price.0 - c.open.0,
        ) as f64;
    }
    let max = bins
        .iter()
        .filter(|b| b.1.is_some())
        .map(|b| b.0)
        .fold(0., f64::max)
        .max(1.);
    let bars: Vec<_> = bins
        .iter()
        .enumerate()
        .filter(|(_, b)| b.1.is_some())
        .map(|(i, b)| {
            let x = bounds[0] + (i as f64 + 0.5) / cols as f64 * (bounds[1] - bounds[0]);
            ([(x, 0.), (x, b.0)], p.change(b.2))
        })
        .collect();
    let mut datasets: Vec<_> = bars
        .iter()
        .map(|(bar, color)| {
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(*color))
                .data(bar)
        })
        .collect();
    let guide = [(points[selected].0, 0.), (points[selected].0, max)];
    if app.chart_cursor.is_some() {
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(p.dim))
                .data(&guide),
        );
    }
    if bars.is_empty() {
        f.render_widget(
            Paragraph::new("成交量暂无数据").style(Style::default().fg(p.dim)),
            areas[1],
        );
        return;
    }
    f.render_widget(
        Chart::new(datasets)
            .x_axis(
                Axis::default()
                    .bounds(bounds)
                    .labels(labels)
                    .style(Style::default().fg(p.line)),
            )
            .y_axis(
                Axis::default()
                    .bounds([0., max])
                    .labels([
                        format!("{:>9}", "0"),
                        format!(
                            "{:>9}",
                            if max >= 1_000_000. {
                                format!("{:.1}e6", max / 1_000_000.)
                            } else {
                                format!("{max:.0}")
                            }
                        ),
                    ])
                    .style(Style::default().fg(p.line)),
            )
            .hidden_legend_constraints((Constraint::Length(0), Constraint::Length(0))),
        areas[1],
    );
}
pub(crate) fn compact_name_width(longest: usize, available: u16, reserved: u16) -> u16 {
    (longest.clamp(8, 26) as u16).min(available.saturating_sub(reserved).max(1))
}
pub(crate) fn compact_table_area(area: Rect, widths: &[Constraint]) -> Rect {
    let Some(width) = widths.iter().try_fold(0u16, |sum, c| match c {
        Constraint::Length(n) => sum.checked_add(*n),
        _ => None,
    }) else {
        return area;
    };
    Rect::new(
        area.x,
        area.y,
        area.width
            .min(width.saturating_add(widths.len().saturating_sub(1) as u16)),
        area.height,
    )
}
fn minimal(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let areas = Layout::vertical([Constraint::Min(4), Constraint::Length(5)]).split(r);
    let ids = app.visible();
    let show_age = r.width >= 72;
    let rows = ids
        .iter()
        .map(|id| {
            let q = app.quotes.get(id);
            let mut cells = vec![
                format!(
                    "{}{}",
                    if app.user.is_pinned(id) { "^" } else { "" },
                    app.minimal_label(id)
                ),
                q.map(|q| crate::instruments::price(&q.id, q.price))
                    .unwrap_or_else(|| "--".into()),
                q.map(|q| format!("{:+.2}", q.percent()))
                    .unwrap_or_else(|| "--".into()),
            ];
            if show_age {
                cells.push(
                    q.map(|q| q.age_secs().to_string())
                        .unwrap_or_else(|| "--".into()),
                );
            }
            Row::new(cells)
        })
        .collect::<Vec<_>>();
    let longest = ids
        .iter()
        .map(|id| app.minimal_label(id).width() + usize::from(app.user.is_pinned(id)))
        .max()
        .unwrap_or(8);
    let name_width = compact_name_width(longest, r.width, if show_age { 32 } else { 23 });
    let mut widths = vec![
        Constraint::Length(name_width),
        Constraint::Length(12),
        Constraint::Length(9),
    ];
    let mut headers = vec!["item", "value", "delta%"];
    if show_age {
        widths.push(Constraint::Length(8));
        headers.push("age(s)");
    }
    let table_area = compact_table_area(areas[0], &widths);
    let table = Table::new(rows, widths)
        .header(Row::new(headers).style(Style::default().fg(p.dim)))
        .row_highlight_style(Style::default().bg(p.selected));
    let mut state = TableState::default().with_selected(
        ids.iter()
            .position(|id| Some(id) == app.user.selected.as_ref()),
    );
    f.render_stateful_widget(table, table_area, &mut state);
    let code = app.user.selected.clone().unwrap_or_default();
    let label = app.minimal_label(&code);
    let mut samples = vec![format!("{label} / {}", app.user.period.label())];
    if let Some(q) = app.quotes.get(&code) {
        samples.push(quote_metrics(q));
    }
    if let Some(s) = app.series()
        && let Some(pt) = s
            .points
            .get(app.chart_cursor.unwrap_or(s.points.len().saturating_sub(1)))
    {
        let (time_label, price_label, volume_label) = match s.period {
            Period::Intraday => ("时间", "价格", "分钟成交量"),
            Period::Day => ("日期", "收盘价", "日成交量"),
            Period::Week => ("日期", "收盘价", "周成交量"),
            Period::Month => ("日期", "收盘价", "月成交量"),
        };
        samples.push(format!(
            "\n{time_label} {}  {price_label} {} {}  {volume_label} {}",
            pt.label,
            crate::instruments::price(&s.id, pt.price),
            crate::instruments::unit(&s.id),
            volume_text(pt.volume, &s.id)
        ));
    }

    if let Some(d) = app.user.selected.as_ref().and_then(|id| app.chips.get(id)) {
        samples.push(format!(
            "\n筹码估算  均价 {:.2}  90% {:.2}–{:.2}  日线获利 {:.1}%  截至 {}{}",
            d.average(),
            d.quantile(0.05),
            d.quantile(0.95),
            d.profit(),
            d.date,
            if app.chip_error.is_some() {
                " / 更新失败，保留缓存"
            } else {
                ""
            }
        ));
    } else {
        samples.push(
            if app.user.selected.as_ref().is_some_and(|id| {
                crate::instruments::is_index(id) || crate::instruments::is_etf(id)
            }) {
                "\nchip n/a".into()
            } else if app.chip_error.is_some() {
                "\n筹码估算 暂不可用".into()
            } else {
                "\n筹码估算 加载中…".into()
            },
        );
    }
    f.render_widget(
        Paragraph::new(samples.join("   "))
            .style(Style::default().fg(p.dim))
            .wrap(Wrap { trim: false }),
        areas[1],
    );
}
fn cloud_tabs(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let mut spans = [
        crate::sectors::Category::Industry,
        crate::sectors::Category::Concept,
    ]
    .into_iter()
    .flat_map(|category| {
        let selected = category == app.user.sector_category;
        let label = format!(" {}板块 ", category.label());
        [
            Span::styled(
                label,
                Style::default()
                    .fg(if selected { p.accent } else { p.dim })
                    .bg(if selected { p.selected } else { p.bg }),
            ),
            text(" ", p.dim),
        ]
    })
    .collect::<Vec<_>>();
    spans.push(text(
        format!("[s {}]", app.user.sector_sort.label()),
        p.accent,
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), r);
}
fn heatmap(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let quiet = app.user.mode == Mode::Minimal;
    let Some(snapshot) = app.sector_snapshot().filter(|s| !s.sectors.is_empty()) else {
        f.render_widget(
            Paragraph::new(if quiet {
                "-- / loading"
            } else if app.sector_error.is_some() {
                "暂无全市场板块数据。请检查网络/代理；不会用自选股票替代。"
            } else {
                "正在加载全市场板块…"
            })
            .style(Style::default().fg(p.dim))
            .wrap(Wrap { trim: false }),
            r,
        );
        return;
    };
    let sectors = &snapshot.sectors;
    let selected = app.sector_index.min(sectors.len() - 1);
    let (columns, page_size) = App::cloud_dimensions(r.width, r.height, app.user.mode);
    let start = selected / page_size * page_size;
    let end = sectors.len().min(start + page_size);
    let areas = cloud_areas(r);
    let gainers = sectors
        .iter()
        .filter(|s| s.change_pct.is_some_and(|v| v > 0.))
        .count();
    let losers = sectors
        .iter()
        .filter(|s| s.change_pct.is_some_and(|v| v < 0.))
        .count();
    let summary = if quiet {
        format!(
            "[ prev ][ next ] matrix {}/{} / {} / {} rows / {}-{}",
            start / page_size + 1,
            sectors.len().div_ceil(page_size),
            snapshot.category.label(),
            sectors.len(),
            start + 1,
            end
        )
    } else {
        format!(
            "[ 上页 ][ 下页 ] 第 {}/{} 页 · 大盘云图 · 全部{}板块 {} · 上涨 {} / 下跌 {}",
            start / page_size + 1,
            sectors.len().div_ceil(page_size),
            snapshot.category.label(),
            sectors.len(),
            gainers,
            losers
        )
    };
    f.render_widget(
        Paragraph::new(summary).style(Style::default().fg(p.dim)),
        areas[0],
    );
    let visible = &sectors[start..end];
    let cells = cloud_tiles(areas[1], visible, app.user.mode, columns);
    for (offset, (sector, cell)) in visible.iter().zip(cells).enumerate() {
        let index = start + offset;
        let active = index == selected;
        let percent = sector
            .change_pct
            .map(|v| format!("{v:+.2}%"))
            .unwrap_or_else(|| "--".into());
        if quiet {
            let name = fit_label(&sector.name, cell.width.saturating_sub(14) as usize);
            f.render_widget(
                Paragraph::new(format!(
                    "{}{:03} {} {:>7}",
                    if active { ">" } else { " " },
                    index + 1,
                    name,
                    percent
                ))
                .style(Style::default().fg(p.fg).bg(if active {
                    p.selected
                } else {
                    p.bg
                })),
                cell,
            );
        } else {
            render_cloud_tile(f, sector, cell, active, p);
        }
    }
    let sector = &sectors[selected];
    let retrieved = snapshot
        .fetched_at
        .with_timezone(&chrono::FixedOffset::east_opt(28800).unwrap())
        .format("%m-%d %H:%M:%S");
    let stale = (chrono::Utc::now() - snapshot.fetched_at).num_seconds()
        > (app.config.refresh.sectors_secs * 2) as i64;
    let detail = if quiet {
        format!(
            "{:03}  {}  {}  {}  {}s",
            selected + 1,
            sector.name,
            sector
                .change_pct
                .map(|v| format!("{v:+.2}"))
                .unwrap_or_else(|| "--".into()),
            sector.stock_count,
            (chrono::Utc::now() - snapshot.fetched_at)
                .num_seconds()
                .max(0)
        )
    } else {
        format!(
            "{}  {}  平均涨跌 {}  成分股 {}",
            sector.name,
            sector.code,
            sector
                .change_pct
                .map(|v| format!("{v:+.2}%"))
                .unwrap_or_else(|| "--".into()),
            sector.stock_count
        )
    };
    let source = if quiet {
        format!(
            "{} / {}",
            if app.demo { "sample" } else { "sina" },
            if stale { "cached" } else { "snapshot" }
        )
    } else {
        format!(
            "{} · 获取 {}{} · 本页面积=成分股数，非市值",
            if app.demo {
                "演示数据"
            } else {
                "新浪板块"
            },
            retrieved,
            if stale { " · 缓存" } else { "" }
        )
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(detail), Line::from(source)])
            .style(Style::default().fg(p.dim)),
        areas[2],
    );
}
fn centered(r: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    Rect::new(r.x + (r.width - w) / 2, r.y + (r.height - h) / 2, w, h)
}
fn modal_view(f: &mut Frame, app: &App, modal: &Modal, p: Palette) {
    let r = centered(
        f.area(),
        78,
        match modal {
            Modal::Help { .. } => 32,
            Modal::Search { .. } | Modal::UsSearch { .. } => 24,
            Modal::Settings { .. } => 24,
            _ => 16,
        },
    );
    f.render_widget(Clear, r);
    f.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.fg)),
        r,
    );
    let title = match modal {
        Modal::IndexSearch { .. } => "添加指数 · 中文 / 代码 / 拼音",
        Modal::UsSearch { .. } => "订阅美股 · 代码 / 名称 / 拼音",
        Modal::OverseasInput { purpose, .. } => purpose.label(),
        Modal::Welcome(_) => "首次启动",
        Modal::Help { .. } => "快捷键 / 编号映射",
        Modal::Search { adding: true, .. } => "添加自选 · 中文 / 代码 / 拼音",
        Modal::Search { .. } => "搜索 · 中文 / 代码 / 拼音",
        Modal::AddTo { .. } => "添加到分组",
        Modal::Groups { .. } => "自选分组",
        Modal::GroupName { id: Some(_), .. } => "重命名分组",
        Modal::GroupName { .. } => "新建分组",
        Modal::Alias { .. } => "简约模式别名",
        Modal::Confirm(_) => "确认",
        Modal::Settings { .. } => "行情与网络设置",
    };
    let b = block(format!(" {title} · Esc 返回 "), p);
    let inner = b.inner(r);
    f.render_widget(b, r);
    match modal {
        Modal::IndexSearch {
            query,
            results,
            index,
        }
        | Modal::UsSearch {
            query,
            results,
            index,
            ..
        } => {
            let us = matches!(modal, Modal::UsSearch { .. });
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(if us { 3 } else { 2 }),
            ])
            .split(inner);
            draw_input(f, rows[0], query, p);
            let items = results
                .iter()
                .map(|stock| {
                    if us {
                        return ListItem::new(format!(
                            "{}  {}  {}",
                            stock.id,
                            stock.name,
                            if app.user.overseas.subscriptions.contains(&stock.id) {
                                "已订阅"
                            } else {
                                "+"
                            }
                        ));
                    }
                    ListItem::new(format!(
                        "{}  {}  {}  {}",
                        stock.name,
                        stock.id,
                        crate::index_board::region(&stock.id),
                        if app.user.overseas.index_watchlist.contains(&stock.id) {
                            "已关注"
                        } else {
                            "+"
                        }
                    ))
                })
                .collect::<Vec<_>>();
            f.render_stateful_widget(
                List::new(items).highlight_style(Style::default().bg(p.selected)),
                rows[1],
                &mut ListState::default().with_selected((!results.is_empty()).then_some(*index)),
            );
            let hint = if let Modal::UsSearch { status, .. } = modal {
                format!(
                    "↑↓ 选择 · Enter 订阅到当前分组 · Esc 取消\n代码 / 中英文 / 拼音；中文仅本地目录\n{status}"
                )
            } else {
                "↑↓ 选择 · Enter 加入关注 · Esc 取消\n内置目录提供代码与数据源，移除关注不会删除目录。".into()
            };
            f.render_widget(
                Paragraph::new(hint).style(Style::default().fg(p.dim)),
                rows[2],
            );
        }
        Modal::OverseasInput { input, .. } => {
            f.render_widget(Paragraph::new(format!("{input}▏\n\nEnter 保存 · Esc 取消 · Ctrl+u 清空\n美股：a 添加，g 选择/创建分组，G 加入分组，r 重命名\n常见名称：苹果、英伟达、特斯拉；其他股票输入代码")).wrap(Wrap{trim:false}),inner);
        }
        Modal::Welcome(i) => {
            f.render_widget(
                Paragraph::new(vec![
                    Line::raw("选择默认显示模式，之后按 m 随时切换。"),
                    Line::raw(""),
                    Line::from(text(
                        if *i == 0 {
                            "› 完整：行情列表、分时走势、热力图"
                        } else {
                            "  完整：行情列表、分时走势、热力图"
                        },
                        if *i == 0 { p.accent } else { p.dim },
                    )),
                    Line::from(text(
                        if *i == 1 {
                            "› 简约：单色数字与固定编号"
                        } else {
                            "  简约：单色数字与固定编号"
                        },
                        if *i == 1 { p.accent } else { p.dim },
                    )),
                    Line::raw(""),
                    Line::raw("↑↓ 选择 · Enter 保存 · 数据源免费，无需 Key"),
                    Line::raw("按 / 搜索股票，按 a 添加到自选分组。"),
                ])
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        Modal::Help { offset } => {
            let body = vec![
                "↑↓ / PgUp/PgDn 滚动帮助；Esc 返回",
                "F1 自选 / F2 指数 / F3 美股 / F4 云图（可鼠标点击）",
                "m 完整/简约 · h 云图/返回前页 · [ ] 当前市场分组",
                "美股 Tab 切时段 · a / / 模糊搜索订阅 · g 分组选择/创建 · G 加入分组",
                "美股 t 置顶 · Alt+↑↓ 排序 · e 别名 · r 重命名分组 · d 移除（确认）",
                "指数 Tab 我的关注/全部 · Enter 走势 · Esc 总览",
                "指数 a 添加（中文/代码/拼音）· d 取消关注 · t 置顶 · Alt+↑↓ 排序",
                "j/↓ 下一只 · k/↑ 上一只 · ←→ 周期 · Shift+←→ 游标",
                "滚轮切标的／云图翻页；页签上滚轮或 Shift+滚轮切换 F1–F4",
                "/ 搜索  Enter 查看  a 添加  d 移除（确认）",
                "g 分组管理（n 新建 / r 重命名 / d 删除 / Alt+↑↓ 排序）",
                "t 置顶/取消 · Alt+↑↓ 当前分组自选排序 · e 设置 alias",
                "r 修改当前分组名称；全部自选不可重命名",
                "简约模式 v 临时显示原名；支持释放事件时按住查看，否则再按还原",
                ", 设置  i 常驻指数  Space 暂停显示  Esc 返回  q 退出",
                "指数单位为点，ETF 保留三位小数；常驻栏 * 表示源报价超过120秒。",
                "搜索输入中文/代码/拼音/首字母；Ctrl+a 加入分组",
                "简约：item=自定义别名，未设置时为代码（^置顶）",
                "value=价格；delta%=较昨收；e 编辑别名，留空恢复编号",
                "age(s)=源报价年龄；E01 报价 / E02 走势 / E03 板块失败",
                "云图：Tab 分类，方向键选择，PgUp/PgDn 或 n/p 翻页；鼠标点击/滚轮",
                "云图：s 切换涨幅/跌幅/规模排序；规模=成分股数。",
                "云图按新浪平均涨跌幅着色，板块可重叠，不可累加。",
                "暂停只冻结显示，后台继续接收；恢复显示最新数据。",
                "红涨绿跌；免费源时效不保证，休市仍显示最后快照。",
            ];
            let mut lines: Vec<Line> = body.into_iter().map(Line::raw).collect();
            lines.push(Line::raw(""));
            if app.user.heatmap {
                lines.push(Line::from(text("板块序号映射（按当前排序）", p.accent)));
                if let Some(snapshot) = app.sector_snapshot() {
                    for (i, sector) in snapshot.sectors.iter().enumerate() {
                        lines.push(Line::raw(format!(
                            "{:03}  {}  {}",
                            i + 1,
                            sector.code,
                            sector.name
                        )));
                    }
                }
            }
            lines.push(Line::from(text("自选映射（当前分组优先）", p.accent)));
            let mut ids = app.visible();
            ids.extend(
                app.user
                    .watchlist
                    .iter()
                    .filter(|id| !app.visible().contains(id))
                    .cloned(),
            );
            for id in &ids {
                let name = app
                    .catalog
                    .iter()
                    .find(|s| &s.id == id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("");
                lines.push(Line::raw(format!(
                    "{}  {}  {}",
                    app.user.alias(id),
                    id,
                    name
                )));
            }
            lines.push(Line::from(text(
                app.user
                    .groups
                    .iter()
                    .map(|g| format!("g{:02} {}", g.id, g.name))
                    .collect::<Vec<_>>()
                    .join(" · "),
                p.dim,
            )));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll(((*offset).min(u16::MAX as usize) as u16, 0)),
                inner,
            );
        }
        Modal::Search {
            query,
            results,
            index,
            adding,
        } => {
            let parts = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(2),
                Constraint::Length(1),
            ])
            .split(inner);
            draw_input(f, parts[0], query, p);
            let items = results
                .iter()
                .map(|s| {
                    ListItem::new(format!(
                        "{}  {}  {}  {} / {}",
                        app.user.alias(&s.id),
                        s.code(),
                        s.name,
                        s.exchange(),
                        crate::instruments::kind(&s.id)
                    ))
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default().with_selected(if results.is_empty() {
                None
            } else {
                Some(*index)
            });
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(p.selected))
                    .highlight_symbol("› "),
                parts[1],
                &mut state,
            );
            f.render_widget(
                Paragraph::new(if *adding {
                    "↑↓ 选择 · Enter 选择目标分组"
                } else {
                    "↑↓ 选择 · Enter 预览 · Ctrl+a 加自选"
                })
                .style(Style::default().fg(p.dim)),
                parts[2],
            );
        }
        Modal::AddTo { stock, index } => {
            let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(2)]).split(inner);
            f.render_widget(
                Paragraph::new(format!("{} {}", stock.code(), stock.name)),
                parts[0],
            );
            let mut items = vec![ListItem::new("全部自选")];
            items.extend(
                app.user
                    .groups
                    .iter()
                    .map(|g| ListItem::new(g.name.clone())),
            );
            let mut state = ListState::default().with_selected(Some(*index));
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(p.selected))
                    .highlight_symbol("› "),
                parts[1],
                &mut state,
            );
        }
        Modal::Groups { index } => {
            let parts = Layout::vertical([Constraint::Min(2), Constraint::Length(2)]).split(inner);
            let items = app
                .user
                .groups
                .iter()
                .map(|g| {
                    ListItem::new(format!("g{:02}  {}  / {}只", g.id, g.name, g.symbols.len()))
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default().with_selected(if items.is_empty() {
                None
            } else {
                Some(*index)
            });
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(p.selected))
                    .highlight_symbol("› "),
                parts[0],
                &mut state,
            );
            f.render_widget(
                Paragraph::new(
                    "n 新建 / r 重命名 / d 删除 / Alt+↑↓ 排序 / Enter 切换\n删除分组保留股票；全部自选不可删除",
                )
                .style(Style::default().fg(p.dim)),
                parts[1],
            );
        }
        Modal::GroupName { input, .. } => {
            draw_input(f, inner, input, p);
        }
        Modal::Alias { id, input } => {
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Min(1),
            ])
            .split(inner);
            f.render_widget(
                Paragraph::new(format!("{id} · 最多12显示列；简约视图以别名替代代码")),
                rows[0],
            );
            draw_input(f, rows[1], input, p);
            f.render_widget(
                Paragraph::new(
                    "Enter 保存 · 留空恢复编号 · Ctrl+u 清空
Esc 取消；别名不能与其他自选别名或编号重复",
                )
                .style(Style::default().fg(p.dim))
                .wrap(Wrap { trim: true }),
                rows[2],
            );
        }
        Modal::Confirm(action) => {
            let msg = match action {
                Confirm::OverseasRemove(id) => {
                    format!("从当前美股分组移除 {id}？\n全部订阅中移除也会从其他分组取消订阅。")
                }
                Confirm::Remove(id) => format!(
                    "从 {} 移除 {}？\n{}",
                    app.user.group_name(),
                    app.catalog
                        .iter()
                        .find(|s| &s.id == id)
                        .map(|s| s.name.as_str())
                        .unwrap_or(id),
                    if app.user.group == 0 {
                        "也会从其他分组移除该股票。"
                    } else {
                        "其他分组和全部自选不受影响。"
                    }
                ),
                Confirm::DeleteGroup(id) => format!(
                    "删除分组“{}”？\n其中股票仍保留在全部自选。",
                    app.user
                        .groups
                        .iter()
                        .find(|g| g.id == *id)
                        .map(|g| g.name.as_str())
                        .unwrap_or("")
                ),
            };
            f.render_widget(
                Paragraph::new(format!("{msg}\n\ny / Enter 确认 · n / Esc 取消"))
                    .wrap(Wrap { trim: false }),
                inner,
            );
        }
        Modal::Settings {
            draft,
            field,
            input,
        } => {
            let labels = [
                "报价间隔(s)",
                "走势间隔(s)",
                "代理策略",
                "代理地址",
                "请求超时(s)",
                "板块间隔(s)",
                "常驻指数",
            ];
            let mut lines = vec![
                Line::from(text(
                    "Tab / Shift+Tab 切字段 · Enter 保存 · Esc 取消",
                    p.dim,
                )),
                Line::raw(""),
            ];
            for (i, label) in labels.iter().enumerate() {
                let raw = if *field == i {
                    input.clone()
                } else {
                    field_value(draft, i)
                };
                let value = if i == 3 { redact_proxy(&raw) } else { raw };
                lines.push(Line::from(text(
                    format!(
                        "{} {:<12} {}",
                        if i == *field { "›" } else { " " },
                        label,
                        value
                    ),
                    if i == *field { p.accent } else { p.fg },
                )));
            }
            lines.extend([
                Line::raw(""),
                Line::raw("常驻：中文名称/代码，逗号分隔；清空或输入 无 关闭。最多6项。"),
                Line::raw("上证指数,深证成指,恒生指数,科创50,沪深300,创业板指"),
                Line::raw("另支持：上证50、中证500、中证1000。i 直接设置常驻。"),
                Line::raw("代理策略 ←→ 切换：system / custom / direct"),
                Line::raw("间隔 2–3600 秒，超时 1–120 秒；Ctrl+u 清空当前字段。"),
                Line::raw("支持 http(s) / socks5 / socks5h，不自动绕过代理。"),
                Line::raw("代理凭据隐藏显示，配置文件仅当前用户可读。"),
            ]);
            if !app.message.is_empty() {
                lines.push(Line::from(text(app.message.clone(), p.up)));
            }
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }
}
fn draw_input(f: &mut Frame, r: Rect, input: &str, p: Palette) {
    let available = r.width.saturating_sub(3) as usize;
    let mut visible = String::new();
    for c in input.chars().rev() {
        let next = format!("{c}{visible}");
        if next.width() > available {
            break;
        }
        visible = next;
    }
    let width = visible.width();
    f.render_widget(
        Paragraph::new(format!("/ {visible}")).style(Style::default().fg(p.fg)),
        r,
    );
    if r.height > 0 && r.width > 2 {
        f.set_cursor_position((r.x + 2 + width as u16, r.y));
    }
}
pub fn redact_proxy(raw: &str) -> String {
    if raw.contains('@') {
        if let Ok(url) = reqwest::Url::parse(raw) {
            format!(
                "{}://***@{}{}",
                url.scheme(),
                url.host_str().unwrap_or(""),
                url.port().map(|p| format!(":{p}")).unwrap_or_default()
            )
        } else {
            "[含凭据，已隐藏]".into()
        }
    } else {
        raw.into()
    }
}

fn outer_layout(area: Rect, app: &App) -> std::rc::Rc<[Rect]> {
    let overseas = matches!(
        app.page(),
        crate::overseas::Page::Indices | crate::overseas::Page::Us
    );
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if overseas {
                1
            } else {
                index_rows(app, area.width.saturating_sub(2))
                    .saturating_add(2)
                    .max(2)
                    .min(area.height.saturating_sub(13).max(2))
            }),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(area);
    let offset = if app.user.mode == Mode::Minimal {
        16
    } else {
        11
    };
    let nav = Rect::new(
        areas[0].x + offset,
        areas[0].y,
        areas[0].width.saturating_sub(offset),
        1,
    );
    vec![areas[0], areas[1], areas[2], areas[3], areas[4], nav].into()
}

pub fn content_area(app: &App, width: u16, height: u16) -> Rect {
    outer_layout(Rect::new(0, 0, width, height), app)[2]
}

/// Hit testing uses the same tile allocations as the renderer.
pub fn mouse(app: &mut App, event: crossterm::event::MouseEvent, width: u16, height: u16) {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
    use ratatui::layout::Position;
    if app.modal.is_some() || width < 46 || height < 16 {
        return;
    }
    app.set_viewport(width, height);
    let outer = outer_layout(Rect::new(0, 0, width, height), app);
    let point = Position::new(event.column, event.row);
    if event.modifiers.contains(KeyModifiers::SHIFT)
        || crate::overseas_ui::nav_hit(outer[5], point).is_some()
    {
        let step = match event.kind {
            MouseEventKind::ScrollDown => Some(1),
            MouseEventKind::ScrollUp => Some(3),
            _ => None,
        };
        if let Some(step) = step {
            use crate::overseas::Page;
            let current = match app.page() {
                Page::Watch => 0,
                Page::Indices => 1,
                Page::Us => 2,
                Page::Heatmap => 3,
            };
            app.key(KeyEvent::new(
                KeyCode::F((current + step) % 4 + 1),
                KeyModifiers::NONE,
            ));
            return;
        }
    }
    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) && outer[5].contains(point) {
        if let Some(index) = crate::overseas_ui::nav_hit(outer[5], point) {
            app.key(KeyEvent::new(
                KeyCode::F(index as u8 + 1),
                KeyModifiers::NONE,
            ));
        }
        return;
    }
    // Scroll only selects entries in the active market (clouds keep paging).
    // Reuse keyboard handling so selection, charts and persistence stay in sync.
    if outer[2].contains(point) {
        let code = match event.kind {
            MouseEventKind::ScrollDown => Some(if app.user.heatmap {
                KeyCode::PageDown
            } else {
                KeyCode::Down
            }),
            MouseEventKind::ScrollUp => Some(if app.user.heatmap {
                KeyCode::PageUp
            } else {
                KeyCode::Up
            }),
            _ => None,
        };
        if let Some(code) = code {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
    }
    if matches!(
        app.page(),
        crate::overseas::Page::Indices | crate::overseas::Page::Us
    ) {
        if outer[2].contains(point) {
            crate::overseas_ui::mouse(app, outer[2], point, event.kind);
        } else if outer[1].contains(point)
            && app.page() == crate::overseas::Page::Indices
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        {
            let x = point.x - outer[1].x;
            if x < 36 && app.user.overseas.index_all != (x >= 18) {
                app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
            } else if x >= 38 && app.user.overseas.index_detail {
                app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            }
        } else if outer[1].contains(point)
            && app.page() == crate::overseas::Page::Us
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        {
            app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        } else if outer[4].contains(point)
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        {
            app.key(KeyEvent::new(
                KeyCode::Char(if point.x - outer[4].x < 8 { 'm' } else { 'h' }),
                KeyModifiers::NONE,
            ));
        }
        return;
    }
    let key = match event.kind {
        MouseEventKind::Moved if app.user.heatmap => {
            select_cloud_at(app, outer[2], point);
            None
        }
        MouseEventKind::ScrollDown if app.user.heatmap && outer[2].contains(point) => {
            Some(KeyCode::PageDown)
        }
        MouseEventKind::ScrollUp if app.user.heatmap && outer[2].contains(point) => {
            Some(KeyCode::PageUp)
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(index) = index_cells(app, outer[0])
                .iter()
                .position(|r| r.contains(point))
            {
                let cells = index_cells(app, outer[0]);
                if cells.len() < app.config.pinned_indices.len() && index + 1 == cells.len() {
                    app.key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
                } else {
                    let id = app.config.pinned_indices[index].clone();
                    app.preview_index(&id);
                }
                return;
            }
            if outer[4].contains(point) {
                if event.column < outer[4].x + 8 {
                    Some(KeyCode::Char('m'))
                } else if event.column < outer[4].x + 20 {
                    Some(KeyCode::Char('h'))
                } else {
                    None
                }
            } else if app.user.heatmap && outer[1].contains(point) {
                let relative = event.column - outer[1].x;
                let tab_width = 11;
                if relative < tab_width * 2 {
                    let category = if relative < tab_width {
                        crate::sectors::Category::Industry
                    } else {
                        crate::sectors::Category::Concept
                    };
                    (category != app.user.sector_category).then_some(KeyCode::Tab)
                } else if relative < tab_width * 2 + 10 {
                    Some(KeyCode::Char('s'))
                } else {
                    None
                }
            } else if app.user.heatmap && outer[2].contains(point) {
                let r = outer[2];
                if event.row == r.y {
                    if event.column < r.x + 8 {
                        Some(KeyCode::PageUp)
                    } else if event.column < r.x + 16 {
                        Some(KeyCode::PageDown)
                    } else {
                        None
                    }
                } else {
                    select_cloud_at(app, r, point);
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(code) = key {
        app.key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

fn select_cloud_at(app: &mut App, r: Rect, point: ratatui::layout::Position) {
    let Some(snapshot) = app.sector_snapshot() else {
        return;
    };
    let count = snapshot.sectors.len();
    if count == 0 {
        return;
    }
    let (columns, page_size) = App::cloud_dimensions(r.width, r.height, app.user.mode);
    let start = app.sector_index.min(count - 1) / page_size * page_size;
    let end = count.min(start + page_size);
    let cells = cloud_tiles(
        cloud_areas(r)[1],
        &snapshot.sectors[start..end],
        app.user.mode,
        columns,
    );
    if let Some(offset) = cells.iter().position(|cell| cell.contains(point)) {
        let index = start + offset;
        if app.sector_index != index {
            app.sector_index = index;
            app.dirty = true;
            app.message.clear();
        }
    }
}

fn render_cloud_tile(
    f: &mut Frame,
    sector: &crate::sectors::Sector,
    cell: Rect,
    active: bool,
    p: Palette,
) {
    let pct = sector.change_pct.unwrap_or(0.);
    let intensity = (pct.abs() / 5.).min(1.);
    let background = if pct > 0. {
        Color::Rgb((53. + intensity * 52.) as u8, 33, 39)
    } else if pct < 0. {
        Color::Rgb(25, (48. + intensity * 38.) as u8, 43)
    } else {
        p.selected
    };
    f.render_widget(Block::default().style(Style::default().bg(p.bg)), cell);
    let tile = Rect::new(
        cell.x,
        cell.y,
        cell.width.saturating_sub(u16::from(cell.width >= 3)),
        cell.height.saturating_sub(u16::from(cell.height >= 3)),
    );
    let name = fit_label(&sector.name, tile.width.saturating_sub(1) as usize)
        .trim_end()
        .to_owned();
    let mut lines = vec![Line::styled(
        format!("{}{name}", if active { "▎" } else { " " }),
        Style::default()
            .fg(if active { p.accent } else { p.fg })
            .add_modifier(Modifier::BOLD),
    )];
    let percent = sector
        .change_pct
        .map(|v| format!("{v:+.2}%"))
        .unwrap_or_else(|| "--".into());
    if tile.height >= 2 && tile.width as usize > percent.width() {
        lines.push(Line::raw(format!(" {percent}")));
    }
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(background).fg(p.fg)),
        tile,
    );
}

fn cloud_areas(r: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(2),
        Constraint::Length(2),
    ])
    .split(r)
}

/// Ordered weighted rows share geometry between rendering and mouse input.
fn cloud_tiles(
    r: Rect,
    sectors: &[crate::sectors::Sector],
    mode: Mode,
    columns: usize,
) -> Vec<Rect> {
    if mode == Mode::Minimal {
        let rows =
            Layout::vertical(vec![Constraint::Length(1); sectors.len().div_ceil(columns)]).split(r);
        return rows
            .iter()
            .flat_map(|row| {
                Layout::horizontal(vec![Constraint::Ratio(1, columns as u32); columns])
                    .spacing(1)
                    .split(*row)
                    .to_vec()
            })
            .take(sectors.len())
            .collect();
    }
    if sectors.is_empty() {
        return Vec::new();
    }
    let weights: Vec<u64> = sectors
        .iter()
        .map(|s| u64::from(s.stock_count.max(1)))
        .collect();
    let row_weights: Vec<u64> = weights
        .chunks(columns)
        .map(|row| row.iter().sum())
        .collect();
    let rows = Layout::vertical(
        proportional_lengths(&row_weights, r.height)
            .into_iter()
            .map(Constraint::Length)
            .collect::<Vec<_>>(),
    )
    .split(r);
    let mut result = Vec::with_capacity(sectors.len());
    for (row, weights) in weights.chunks(columns).enumerate() {
        result.extend(
            Layout::horizontal(
                proportional_lengths(weights, r.width)
                    .into_iter()
                    .map(Constraint::Length)
                    .collect::<Vec<_>>(),
            )
            .split(rows[row])
            .iter()
            .copied(),
        );
    }
    result
}

fn proportional_lengths(weights: &[u64], size: u16) -> Vec<u16> {
    let total: u64 = weights.iter().sum();
    let mut previous = 0;
    let mut cumulative = 0;
    weights
        .iter()
        .map(|weight| {
            cumulative += weight;
            let boundary = ((cumulative * u64::from(size) + total / 2) / total) as u16;
            let length = boundary - previous;
            previous = boundary;
            length
        })
        .collect()
}

// Measure display columns, so Chinese names cannot crowd out the numeric value.
fn fit_label(name: &str, width: usize) -> String {
    if name.width() <= width {
        return format!("{}{}", name, " ".repeat(width - name.width()));
    }
    if width == 0 {
        return String::new();
    }
    let mut label = String::new();
    for ch in name.chars() {
        if label.width() + unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) > width - 1 {
            break;
        }
        label.push(ch);
    }
    label.push('…');
    label.push_str(&" ".repeat(width - label.width()));
    label
}

#[cfg(test)]
mod cloud_tests {
    use super::*;
    #[test]
    fn treemap_uses_constituent_weights_and_covers_area_without_overlap() {
        let mut sectors = crate::sectors::demo(crate::sectors::Category::Industry).sectors;
        sectors.truncate(2);
        sectors[0].stock_count = 30;
        sectors[1].stock_count = 10;
        let area = Rect::new(1, 7, 80, 10);
        let tiles = cloud_tiles(area, &sectors, Mode::Full, 5);
        assert_eq!(tiles[0].area(), 600);
        assert_eq!(tiles[1].area(), 200);
        assert_eq!(tiles[0].intersection(tiles[1]).area(), 0);
        assert_eq!(tiles.iter().map(|r| r.area()).sum::<u32>(), area.area());
    }
}

#[cfg(test)]
mod price_axis_tests {
    use super::price_scale;
    use crate::model::Price;
    #[test]
    fn price_ticks_are_exact_distinct_even_and_cover_samples() {
        for (low, high) in [
            (10000, 10100),
            (268_5000, 268_5000),
            (1_0000, 5000_0000),
            (274_1234, 284_5678),
        ] {
            let ticks = price_scale(Price(low), Price(high), 30, 2);
            assert!(ticks[0] * 100 <= low);
            assert!(ticks[ticks.len() - 1] * 100 >= high);
            let step = ticks[1] - ticks[0];
            assert!(step > 0);
            assert!(ticks.windows(2).all(|w| w[1] - w[0] == step));
        }
        assert!(price_scale(Price(275100), Price(313100), 40, 2).len() > 3);
        assert!(price_scale(Price(275100), Price(313100), 6, 2).len() <= 5);
    }
}
