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
    let outer = outer_layout(area);
    header(frame, app, outer[0], p);
    if app.user.heatmap {
        cloud_tabs(frame, app, outer[1], p);
    } else {
        groups(frame, app, outer[1], p);
    }
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
    let keys = if app.user.heatmap {
        if app.user.mode == Mode::Minimal {
            "m mode  h table     Tab type  n/p page  s sort  mouse  , cfg  ?  q"
        } else {
            "m 简约  h 自选行情   Tab 分类  s 排序  PgUp/PgDn 或 n/p 翻页  鼠标点击/滚轮  , 设置  ? 帮助  q 退出"
        }
    } else if app.user.mode == Mode::Minimal {
        "m mode  h view  [] group  / find  a add  g groups  , cfg  ?  q"
    } else {
        "m 简约  h 大盘云图  [] 分组  ↑↓ 选股  ←→ 周期  / 搜索  a 添加  g 分组  , 设置  ? 帮助  q 退出"
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
    if rows[3].width >= 66 {
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
                    "▇".repeat(len),
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
// Use cent-based, evenly spaced ticks: Ratatui positions axis labels uniformly.
// Rounding arbitrary floats only at display time can produce misleading/repeated labels.
fn price_scale(min: crate::model::Price, max: crate::model::Price, height: u16) -> Vec<i64> {
    let low = min.0.div_euclid(100);
    let high = (max.0 + 99).div_euclid(100);
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
    let min = s.points.iter().map(|p| p.price).min().unwrap();
    let max = s.points.iter().map(|p| p.price).max().unwrap();
    let ticks = price_scale(min, max, r.height.saturating_sub(1));
    let bounds = [ticks[0] as f64 / 100., ticks[ticks.len() - 1] as f64 / 100.];
    let selected = app
        .chart_cursor
        .unwrap_or(points.len() - 1)
        .min(points.len() - 1);
    let selected_point = &s.points[selected];
    let caption = format!(
        "{} {} CNY  {}",
        if app.chart_cursor.is_some() {
            "查看"
        } else {
            "末价"
        },
        selected_point.price,
        selected_point.label
    );
    let areas = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).split(r);
    f.render_widget(
        Paragraph::new(caption).style(Style::default().fg(p.fg)),
        areas[0],
    );
    let r = areas[1];
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
    let price_guide = [(0., points[selected].1), (xmax, points[selected].1)];
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
        let pt = points[i.min(points.len() - 1)];
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
    let labels = if points.len() > 1 {
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
                format!("{}.{:02}", cents / 100, cents % 100),
                Style::default().fg(p.dim),
            )
        })
        .collect::<Vec<_>>();
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
    let areas = Layout::vertical([Constraint::Min(4), Constraint::Length(5)]).split(r);
    let ids = app.visible();
    let rows = ids
        .iter()
        .map(|id| {
            let q = app.quotes.get(id);
            Row::new([
                id.clone(),
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
            Constraint::Length(9),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Min(8),
        ],
    )
    .header(Row::new(["code", "value", "delta%", "age(s)"]).style(Style::default().fg(p.dim)))
    .row_highlight_style(Style::default().bg(p.selected));
    let mut state = TableState::default().with_selected(
        ids.iter()
            .position(|id| Some(id) == app.user.selected.as_ref()),
    );
    f.render_stateful_widget(table, areas[0], &mut state);
    let code = app.user.selected.clone().unwrap_or_default();
    let mut samples = vec![format!("{code} / {}", app.user.period.key())];
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
        samples.push(if app.chip_error.is_some() {
            "\n筹码估算 暂不可用".into()
        } else {
            "\n筹码估算 加载中…".into()
        });
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
                "m 完整/简约    h 行情/大盘云图    [ ] 自选分组",
                "↑↓ 选择股票   ←→ 日内/1周/1月   j/k 走势游标",
                "/ 搜索  Enter 查看  a 添加  d 移除（确认）",
                "g 分组管理（n 新建 / r 重命名 / d 删除）",
                ", 设置  Space 暂停显示  Esc 返回  q 退出",
                "搜索输入中文/代码/拼音/首字母；Ctrl+a 加入分组",
                "简约：code=股票代码；value=价格；delta%=较昨收",
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
                "板块间隔(s)",
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

fn outer_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(area)
}

/// Hit testing uses the same tile allocations as the renderer.
pub fn mouse(app: &mut App, event: crossterm::event::MouseEvent, width: u16, height: u16) {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
    use ratatui::layout::Position;
    if app.modal.is_some() || width < 46 || height < 16 {
        return;
    }
    app.set_viewport(width, height);
    let outer = outer_layout(Rect::new(0, 0, width, height));
    let point = Position::new(event.column, event.row);
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
            let ticks = price_scale(Price(low), Price(high), 30);
            assert!(ticks[0] * 100 <= low);
            assert!(ticks[ticks.len() - 1] * 100 >= high);
            let step = ticks[1] - ticks[0];
            assert!(step > 0);
            assert!(ticks.windows(2).all(|w| w[1] - w[0] == step));
        }
        assert!(price_scale(Price(275100), Price(313100), 40).len() > 3);
        assert!(price_scale(Price(275100), Price(313100), 6).len() <= 5);
    }
}
