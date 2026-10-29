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
struct Palette {
    fg: Color,
    dim: Color,
    line: Color,
    accent: Color,
    up: Color,
    down: Color,
    bg: Color,
    selected: Color,
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
    fn change(self, v: f64) -> Color {
        if v > 0.0 {
            self.up
        } else if v < 0.0 {
            self.down
        } else {
            self.fg
        }
    }
}
fn text(s: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(color))
}
fn block(title: impl Into<String>, p: Palette) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(p.line))
        .title(Line::from(text(title, p.accent)))
}
fn age(q: &Quote) -> String {
    let secs = q.age_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
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
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(area);
    header(frame, app, outer[0], p);
    groups(frame, app, outer[1], p);
    if app.user.heatmap {
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
    let keys = if app.user.mode == Mode::Minimal {
        "m mode  h view  [] group  / find  a add  g groups  , cfg  ?  q"
    } else {
        "m 简约  h 热力图  [] 分组  ↑↓ 选股  ←→ 周期  / 搜索  a 添加  g 分组  , 设置  ? 帮助  q 退出"
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
    let spans = vec![
        text(if quiet { "metrics --watch" } else { "ticktop_" }, p.accent),
        text(
            if quiet {
                "    A / local"
            } else {
                "   A股 · 只看盘"
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
                .map(|q| q.price.to_string())
                .unwrap_or_else(|| "--".into());
            let pct = q
                .map(|q| format!("{:+.2}%", q.percent()))
                .unwrap_or_else(|| "--".into());
            Row::new(vec![
                Cell::from(vec![
                    Line::from(name),
                    Line::from(text(id.get(2..).unwrap_or(id).to_string(), p.dim)),
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
            text(format!("  {}", id.get(2..).unwrap_or(id)), p.dim),
        ])),
        rows[0],
    );
    let mut quote = vec![
        text(
            q.map(|q| q.price.to_string())
                .unwrap_or_else(|| "--".into()),
            p.fg,
        ),
        text(" CNY   ", p.dim),
    ];
    if let Some(q) = q {
        quote.push(text(
            format!(
                "{:+.2}%  {}{}",
                q.percent(),
                if q.change().0 >= 0 { "+" } else { "" },
                q.change()
            ),
            p.change(q.percent()),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(quote)), rows[1]);
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
    price_chart(f, app, rows[3], p);
    let readout = if let Some(s) = app.series() {
        if let Some(i) = app.chart_cursor {
            let pt = &s.points[i.min(s.points.len().saturating_sub(1))];
            format!("{}   {} CNY   j/k 游标", pt.label, pt.price)
        } else {
            format!(
                "{} · {} · {}点 · j/k 查看",
                s.date,
                if app.user.period == Period::Intraday {
                    "分时价格"
                } else {
                    "日收盘价 · 不复权"
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
fn price_chart(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let Some(s) = app.series().filter(|s| !s.points.is_empty()) else {
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
    let points: Vec<_> = s
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.price.as_f64()))
        .collect();
    let min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let padding = ((max - min) * 0.12).max(max.abs() * 0.001).max(0.01);
    let bounds = [min - padding, max + padding];
    let xmax = (points.len().saturating_sub(1) as f64).max(1.);
    let color = app
        .user
        .selected
        .as_ref()
        .and_then(|id| app.quotes.get(id))
        .map(|q| p.change(q.percent()))
        .unwrap_or(p.accent);
    let mut datasets = vec![
        Dataset::default()
            .name("")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color))
            .data(&points),
    ];
    let guide;
    let dot;
    if let Some(i) = app.chart_cursor {
        let pt = points[i.min(points.len() - 1)];
        guide = vec![(pt.0, bounds[0]), (pt.0, bounds[1])];
        dot = vec![pt];
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(p.line))
                .data(&guide),
        );
        datasets.push(
            Dataset::default()
                .marker(Marker::Dot)
                .style(Style::default().fg(p.accent))
                .data(&dot),
        );
    }
    let labels = if points.len() > 1 {
        vec![
            Span::raw(s.points[0].label.clone()),
            Span::raw(s.points[points.len() / 2].label.clone()),
            Span::raw(s.points[points.len() - 1].label.clone()),
        ]
    } else {
        vec![Span::raw(s.points[0].label.clone())]
    };
    let ylabels =
        [bounds[0], (bounds[0] + bounds[1]) / 2., bounds[1]].map(|n| Span::raw(format!("{n:.2}")));
    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .bounds([0., xmax])
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
}
fn minimal(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let areas = Layout::vertical([Constraint::Min(4), Constraint::Length(3)]).split(r);
    let ids = app.visible();
    let rows = ids
        .iter()
        .map(|id| {
            let q = app.quotes.get(id);
            Row::new([
                app.user.alias(id),
                q.map(|q| q.price.to_string())
                    .unwrap_or_else(|| "--".into()),
                q.map(|q| format!("{:+.2}", q.percent()))
                    .unwrap_or_else(|| "--".into()),
                q.map(|q| q.age_secs().to_string())
                    .unwrap_or_else(|| "--".into()),
            ])
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Min(8),
        ],
    )
    .header(Row::new(["id", "value", "delta%", "age(s)"]).style(Style::default().fg(p.dim)))
    .row_highlight_style(Style::default().bg(p.selected));
    let mut state = TableState::default().with_selected(
        ids.iter()
            .position(|id| Some(id) == app.user.selected.as_ref()),
    );
    f.render_stateful_widget(table, areas[0], &mut state);
    let alias = app
        .user
        .selected
        .as_ref()
        .map(|id| app.user.alias(id))
        .unwrap_or_default();
    let mut samples = vec![format!("{alias} / {}", app.user.period.key())];
    if let Some(s) = app.series() {
        let n = s.points.len();
        if n > 0 {
            let count = ((r.width as usize).saturating_sub(14) / 12)
                .clamp(1, 8)
                .min(n);
            for i in 0..count {
                let idx = if count == 1 {
                    n - 1
                } else {
                    i * (n - 1) / (count - 1)
                };
                samples.push(s.points[idx].price.to_string());
            }
        }
    }
    f.render_widget(
        Paragraph::new(samples.join("   "))
            .style(Style::default().fg(p.dim))
            .wrap(Wrap { trim: false }),
        areas[1],
    );
}
fn heatmap(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let ids = app.visible();
    if ids.is_empty() {
        f.render_widget(
            Paragraph::new(if app.user.mode == Mode::Minimal {
                "0 rows"
            } else {
                "当前分组暂无股票，按 a 添加"
            }),
            r,
        );
        return;
    }
    if app.user.mode == Mode::Minimal {
        let rows = ids
            .iter()
            .map(|id| {
                let q = app.quotes.get(id);
                Row::new([
                    app.user.alias(id),
                    q.map(|q| format!("{:+.2}", q.percent()))
                        .unwrap_or_else(|| "--".into()),
                    q.map(|q| q.price.to_string())
                        .unwrap_or_else(|| "--".into()),
                    q.map(|q| q.age_secs().to_string())
                        .unwrap_or_else(|| "--".into()),
                ])
            })
            .collect::<Vec<_>>();
        let mut state = TableState::default().with_selected(
            ids.iter()
                .position(|id| Some(id) == app.user.selected.as_ref()),
        );
        f.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(7),
                    Constraint::Length(12),
                    Constraint::Length(16),
                    Constraint::Min(8),
                ],
            )
            .header(Row::new(["id", "delta%", "value", "age(s)"]).style(Style::default().fg(p.dim)))
            .row_highlight_style(Style::default().bg(p.selected)),
            r,
            &mut state,
        );
        return;
    }
    let rows_area = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(r);
    let columns = (r.width / 23).clamp(1, 4) as usize;
    let max_rows = (rows_area[0].height / 5).max(1) as usize;
    let page_size = columns * max_rows;
    let selected = ids
        .iter()
        .position(|id| Some(id) == app.user.selected.as_ref())
        .unwrap_or(0);
    let start = selected / page_size * page_size;
    let page = &ids[start..ids.len().min(start + page_size)];
    let row_count = page.len().div_ceil(columns);
    let bands = Layout::vertical(vec![Constraint::Ratio(1, row_count as u32); row_count])
        .split(rows_area[0]);
    for (i, chunk) in page.chunks(columns).enumerate() {
        let cells = Layout::horizontal(vec![Constraint::Ratio(1, columns as u32); columns])
            .spacing(1)
            .split(bands[i]);
        for (j, id) in chunk.iter().enumerate() {
            let q = app.quotes.get(id);
            let pct = q.map(|q| q.percent());
            let intensity = (pct.unwrap_or(0.).abs() / 5.).min(1.);
            let bg = if pct.unwrap_or(0.) > 0. {
                Color::Rgb((53. + intensity * 52.) as u8, 33, 39)
            } else if pct.unwrap_or(0.) < 0. {
                Color::Rgb(25, (48. + intensity * 38.) as u8, 43)
            } else {
                p.selected
            };
            let name = app
                .catalog
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.clone());
            let label = vec![
                Line::from(name),
                Line::from(Span::styled(
                    pct.map(|n| format!("{n:+.2}%"))
                        .unwrap_or_else(|| "--".into()),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(text(
                    format!(
                        "{}  {}",
                        id.get(2..).unwrap_or(id),
                        q.map(age).unwrap_or_default()
                    ),
                    p.dim,
                )),
            ];
            let b = Block::default()
                .borders(if Some(id) == app.user.selected.as_ref() {
                    Borders::ALL
                } else {
                    Borders::NONE
                })
                .border_style(Style::default().fg(p.accent))
                .padding(ratatui::widgets::Padding::uniform(1))
                .style(Style::default().bg(bg));
            f.render_widget(Paragraph::new(label).block(b), cells[j]);
        }
    }
    f.render_widget(
        Paragraph::new(format!(
            "自选分组涨跌 · 等面积 / 非板块指数 · 较昨收 · {}–{} / {}",
            start + 1,
            start + page.len(),
            ids.len()
        ))
        .style(Style::default().fg(p.dim)),
        rows_area[1],
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
            Modal::Search { .. } => 24,
            Modal::Settings { .. } => 20,
            _ => 16,
        },
    );
    f.render_widget(Clear, r);
    f.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.fg)),
        r,
    );
    let title = match modal {
        Modal::Welcome(_) => "首次启动",
        Modal::Help { .. } => "快捷键 / 编号映射",
        Modal::Search { adding: true, .. } => "添加自选 · 中文 / 代码 / 拼音",
        Modal::Search { .. } => "搜索 · 中文 / 代码 / 拼音",
        Modal::AddTo { .. } => "添加到分组",
        Modal::Groups { .. } => "自选分组",
        Modal::GroupName { id: Some(_), .. } => "重命名分组",
        Modal::GroupName { .. } => "新建分组",
        Modal::Confirm(_) => "确认",
        Modal::Settings { .. } => "网络与刷新设置",
    };
    let b = block(format!(" {title} · Esc 返回 "), p);
    let inner = b.inner(r);
    f.render_widget(b, r);
    match modal {
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
                "m 完整/简约    h 行情/热力图    [ ] 切换分组",
                "↑↓ 选择股票   ←→ 日内/1周/1月   j/k 走势游标",
                "/ 搜索  Enter 查看  a 添加  d 移除（确认）",
                "g 分组管理（n 新建 / r 重命名 / d 删除）",
                ", 设置  Space 暂停显示  Esc 返回  q 退出",
                "搜索输入中文/代码/拼音/首字母；Ctrl+a 加入分组",
                "简约：id=固定编号；value=价格；delta%=较昨收",
                "age(s)=源报价距现在秒数；E01 报价失败 / E02 走势失败",
                "暂停只冻结显示，后台继续接收；恢复显示最新数据。",
                "红涨绿跌；免费源时效不保证，休市仍显示最后快照。",
            ];
            let mut lines: Vec<Line> = body.into_iter().map(Line::raw).collect();
            lines.push(Line::raw(""));
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
                        "{}  {}  {}  {}",
                        app.user.alias(&s.id),
                        s.code(),
                        s.name,
                        s.exchange()
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
                    "n 新建 / r 重命名 / d 删除 / Enter 切换\n删除分组保留股票；全部自选不可删除",
                )
                .style(Style::default().fg(p.dim)),
                parts[1],
            );
        }
        Modal::GroupName { input, .. } => {
            draw_input(f, inner, input, p);
        }
        Modal::Confirm(action) => {
            let msg = match action {
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
