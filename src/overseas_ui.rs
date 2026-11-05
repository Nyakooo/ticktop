use crate::{
    app::App,
    config::Mode,
    model::{Period, Price},
    overseas::{self, Page, Session},
    ui::{Palette, text},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::Style,
    symbols::Marker,
    text::Line,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Row, Table, TableState, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(crate) fn nav(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    f.render_widget(ratatui::widgets::Clear, r);
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(p.bg).fg(p.fg)),
        r,
    );
    for (i, page) in [Page::Watch, Page::Indices, Page::Us, Page::Heatmap]
        .into_iter()
        .enumerate()
    {
        let cell = nav_cell(r, i);
        let label = if app.user.mode == Mode::Minimal {
            if cell.width < 8 {
                ["own", "idx", "us", "map"][i]
            } else {
                ["watch", "index", "us", "map"][i]
            }
        } else {
            page.label()
        };
        f.render_widget(
            Paragraph::new(format!("F{} {}", i + 1, label)).style(
                Style::default()
                    .fg(if page == app.page() { p.accent } else { p.dim })
                    .bg(if page == app.page() { p.selected } else { p.bg }),
            ),
            cell,
        );
    }
}
fn nav_cell(r: Rect, i: usize) -> Rect {
    let step = (r.width / 4).min(11);
    Rect::new(r.x + i as u16 * step, r.y, step.saturating_sub(1), 1)
}
pub(crate) fn nav_hit(r: Rect, point: Position) -> Option<usize> {
    (0..4).find(|&i| nav_cell(r, i).contains(point))
}
fn panes(app: &App, r: Rect) -> (Rect, Rect) {
    let quiet = app.user.mode == Mode::Minimal;
    if quiet || r.width < 90 {
        let count = if app.page() == Page::Indices {
            app.user.overseas.index_rows().len()
        } else {
            app.user.overseas.visible().len()
        };
        let height = (count as u16 + 1)
            .min(r.height.saturating_sub(7))
            .min(if quiet { 16 } else { 8 })
            .max(1);
        let a = Layout::vertical([Constraint::Length(height), Constraint::Min(4)])
            .spacing(1)
            .split(r);
        let mut list = a[0];
        if quiet {
            let ids = app.user.overseas.visible();
            let longest = ids
                .iter()
                .map(|id| app.overseas_label(id).width())
                .max()
                .unwrap_or(8);
            list.width = list
                .width
                .min(crate::ui::compact_name_width(longest, r.width, 28) + 28);
        }
        (list, a[1])
    } else {
        let a = Layout::horizontal([Constraint::Length(42), Constraint::Min(30)])
            .spacing(2)
            .split(r);
        (a[0], a[1])
    }
}
pub(crate) fn draw(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let index = app.page() == Page::Indices;
    if index && !app.user.overseas.index_detail {
        index_overview(f, app, r, p);
        return;
    }
    let (list, detail) = if index {
        (Rect::default(), r)
    } else {
        panes(app, r)
    };
    let ids = if index {
        app.user.overseas.index_rows()
    } else {
        app.user.overseas.visible()
    };
    let rows = ids
        .iter()
        .map(|id| {
            let q = app.overseas_quote(id);
            let pct = q.and_then(|q| q.percent());
            let mut cells = vec![
                if !index && app.user.mode == Mode::Full && overseas::name(id) != id {
                    format!("{id} {}", overseas::name(id))
                } else {
                    app.overseas_label(id).to_string()
                },
                q.map(|q| format!("{:.2}", q.price))
                    .unwrap_or_else(|| "--".into()),
                pct.map(|v| {
                    format!(
                        "{v:+.2}{}",
                        if app.user.mode == Mode::Minimal {
                            ""
                        } else {
                            "%"
                        }
                    )
                })
                .unwrap_or_else(|| "--".into()),
            ];
            if !index {
                cells.push(
                    q.map(|q| q.session.label().to_string())
                        .unwrap_or_else(|| "--".into()),
                );
            }
            Row::new(cells).style(Style::default().fg(pct.map(|v| p.change(v)).unwrap_or(p.fg)))
        })
        .collect::<Vec<_>>();
    let selected = ids
        .iter()
        .position(|id| Some(id.as_str()) == app.user.overseas.selected());
    let name_width = crate::ui::compact_name_width(
        ids.iter()
            .map(|id| app.overseas_label(id).width())
            .max()
            .unwrap_or(8),
        list.width,
        28,
    );
    let mut widths = vec![
        if app.user.mode == Mode::Minimal {
            Constraint::Length(name_width)
        } else {
            Constraint::Min(9)
        },
        Constraint::Length(11),
        Constraint::Length(9),
    ];
    let mut headers = vec![
        if index { "指数" } else { "美股" },
        if index { "点位" } else { "USD" },
        "涨跌幅",
    ];
    if !index {
        widths.push(Constraint::Length(5));
        headers.push("阶段");
    }
    let table = Table::new(rows, widths)
        .header(Row::new(headers).style(Style::default().fg(p.dim)))
        .row_highlight_style(Style::default().bg(p.selected));
    f.render_stateful_widget(
        table,
        list,
        &mut TableState::default().with_selected(selected),
    );
    let Some(id) = app.user.overseas.selected() else {
        f.render_widget(
            Paragraph::new("暂无订阅；按 a 添加美股，或 [ ] 切换分组")
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(p.dim)),
            detail,
        );
        return;
    };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(detail);
    let quote = app.overseas_quote(id);
    let label = if app.user.mode == Mode::Full {
        format!("{} {id}", app.overseas_label(id))
    } else {
        app.overseas_label(id).to_string()
    };
    let headline = quote
        .map(|q| {
            format!(
                "{label}  {:.2} {}  {}",
                q.price,
                if index { "点" } else { "USD" },
                q.percent()
                    .map(|v| format!(
                        "{v:+.2}{}",
                        if app.user.mode == Mode::Minimal {
                            ""
                        } else {
                            "%"
                        }
                    ))
                    .unwrap_or_else(|| "--".into())
            )
        })
        .unwrap_or_else(|| format!("{label}  --"));
    f.render_widget(
        Paragraph::new(headline).style(Style::default().fg(p.accent)),
        rows[0],
    );
    let context = if let Some(q) = quote {
        let age = (chrono::Utc::now().timestamp() - q.time).max(0);
        format!(
            "{} · {} · 源时间 {} 北京时间\n{} · 报价距今 {}s",
            q.source,
            q.session.label(),
            overseas::source_time(q.time),
            if age > 180 {
                "历史快照；非当前报价"
            } else {
                "公开报价；非保证实时"
            },
            age
        )
    } else {
        "当前阶段暂无报价；不会以其他阶段价格替代".into()
    };
    f.render_widget(
        Paragraph::new(context).style(Style::default().fg(p.dim)),
        rows[1],
    );
    if app.user.mode == Mode::Minimal {
        let mut lines = vec![Line::from(format!(
            "{} / {}",
            app.user.overseas.period().label(),
            app.user.overseas.session.label()
        ))];
        if let Some(s) = app.overseas_series() {
            let bars = s.bars(if index {
                Session::All
            } else {
                app.user.overseas.session
            });
            if let Some(bar) = bars.get(app.overseas_cursor.unwrap_or(bars.len().saturating_sub(1)))
            {
                lines.push(Line::from(format!(
                    "{}  价 {:.2}",
                    overseas::source_time(bar.time),
                    bar.close
                )));
                if let Some(v) = bar.volume {
                    lines.push(Line::from(format!(
                        "成交量 {v:.0}{}",
                        if index { "（源口径）" } else { "股" }
                    )));
                }
            }
        }
        f.render_widget(
            Paragraph::new(lines).style(Style::default().fg(p.dim)),
            Rect::new(
                rows[2].x,
                rows[2].y,
                rows[2].width,
                rows[2].height + rows[3].height,
            ),
        );
    } else {
        let tabs = Period::ALL
            .iter()
            .flat_map(|period| {
                [
                    text(
                        format!(" {} ", period.label()),
                        if *period == app.user.overseas.period() {
                            p.accent
                        } else {
                            p.dim
                        },
                    ),
                    text(" ", p.dim),
                ]
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(Line::from(tabs)), rows[2]);
        chart(f, app, rows[3], p, index);
    }
    let note = if app.user.overseas.session == Session::Overnight && !index {
        "夜盘仅展示可获取报价，暂无历史分钟曲线；来源 Blue Ocean ATS"
    } else {
        "Shift+←→ 游标 · Home/End 首末 · 缺失数据不补线"
    };
    let readout = app
        .overseas_series()
        .and_then(|s| {
            let bars = s.bars(if index {
                Session::All
            } else {
                app.user.overseas.session
            });
            bars.get(app.overseas_cursor.unwrap_or(bars.len().saturating_sub(1)))
                .map(|b| {
                    format!(
                        "{}  {} {:.2}",
                        overseas::source_time(b.time),
                        if s.period == Period::Intraday {
                            "价格"
                        } else {
                            "收盘"
                        },
                        b.close
                    )
                })
        })
        .unwrap_or_default();
    f.render_widget(
        Paragraph::new(format!("{readout}\n{note}")).style(Style::default().fg(p.dim)),
        rows[4],
    );
}
fn chart(f: &mut Frame, app: &App, r: Rect, p: Palette, index: bool) {
    if r.height < 3 || r.width < 20 {
        return;
    }
    let Some(snapshot) = app.overseas_series() else {
        f.render_widget(
            Paragraph::new("走势加载中；失败时保留缓存，, 设置代理")
                .style(Style::default().fg(p.dim)),
            r,
        );
        return;
    };
    let period = snapshot.period;
    let session = if index || period != Period::Intraday {
        Session::All
    } else {
        app.user.overseas.session
    };
    let bars = snapshot.bars(session);
    if bars.is_empty()
        || (!index && period == Period::Intraday && app.user.overseas.session == Session::Overnight)
    {
        f.render_widget(
            Paragraph::new("当前阶段暂无历史走势\n不会用盘后走势代替夜盘")
                .style(Style::default().fg(p.dim)),
            r,
        );
        return;
    }
    let selected = app
        .overseas_cursor
        .unwrap_or(bars.len() - 1)
        .min(bars.len() - 1);
    let capacity = (r.width.saturating_sub(12) / 2).max(2) as usize;
    let start = if period == Period::Intraday {
        0
    } else {
        bars.len().saturating_sub(capacity).min(selected)
    };
    let end = if period == Period::Intraday {
        bars.len()
    } else {
        (start + capacity).min(bars.len())
    };
    let view = &bars[start..end];
    let min = view
        .iter()
        .map(|b| b.low.unwrap_or(b.close))
        .fold(f64::INFINITY, f64::min);
    let max = view
        .iter()
        .map(|b| b.high.unwrap_or(b.close))
        .fold(f64::NEG_INFINITY, f64::max);
    let volume_area =
        if !index && r.width >= 52 && r.height >= 14 && view.iter().any(|b| b.volume.is_some()) {
            let panes = Layout::vertical([Constraint::Min(7), Constraint::Length(5)]).split(r);
            Some((panes[0], panes[1]))
        } else {
            None
        };
    let price_area = volume_area.map_or(r, |a| a.0);
    let ticks = crate::ui::price_scale(
        Price((min * 10000.) as i64),
        Price((max * 10000.) as i64),
        price_area.height,
        2,
    );
    let bounds = [ticks[0] as f64 / 100., ticks[ticks.len() - 1] as f64 / 100.];
    let first = view[0].time;
    let last = view[view.len() - 1].time;
    let mut xmin = first as f64;
    let mut xmax = (last.max(first + 60)) as f64;
    if period == Period::Intraday
        && let Some(regular) = snapshot
            .intervals
            .iter()
            .filter(|w| w.session == Session::Regular)
            .min_by_key(|w| w.start.abs_diff(first))
    {
        let day = snapshot.intervals.iter().filter(|w| {
            (w.session == Session::Regular && w.start == regular.start)
                || (w.session == Session::Pre && w.end == regular.start)
                || (w.session == Session::Post && w.start == regular.end)
        });
        let windows = day
            .filter(|w| session == Session::All || w.session == session)
            .collect::<Vec<_>>();
        if let Some(v) = windows.iter().map(|w| w.start).min() {
            xmin = v as f64;
        }
        if let Some(v) = windows.iter().map(|w| w.end).max() {
            xmax = v as f64;
        }
    }
    let mut segments: Vec<Vec<(f64, f64)>> = vec![];
    if period == Period::Intraday {
        let mut previous = None;
        for b in view {
            if previous.is_none_or(|(t, s)| b.time - t > 120 || b.session != s) {
                segments.push(vec![]);
            }
            segments.last_mut().unwrap().push((b.time as f64, b.close));
            previous = Some((b.time, b.session));
        }
    }
    let candles = view
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            Some((
                vec![(i as f64, b.low?), (i as f64, b.high?)],
                vec![(i as f64, b.open?), (i as f64, b.close)],
                p.change(b.close - b.open?),
            ))
        })
        .collect::<Vec<_>>();
    // Calendar gaps are compacted only for daily/weekly/monthly candles.
    if period != Period::Intraday {
        xmin = -0.5;
        xmax = (view.len() as f64 - 0.5).max(0.5);
    }
    let mut datasets = segments
        .iter()
        .map(|s| {
            Dataset::default()
                .data(s)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(p.accent))
        })
        .collect::<Vec<_>>();
    if period != Period::Intraday {
        for (wick, body, c) in &candles {
            datasets.extend(crate::candles::datasets(wick, body, *c));
        }
    }
    let cursor_x = if period == Period::Intraday {
        bars[selected].time as f64
    } else {
        (selected - start) as f64
    };
    let guide = [(cursor_x, bounds[0]), (cursor_x, bounds[1])];
    if app.overseas_cursor.is_some() {
        datasets.push(
            Dataset::default()
                .data(&guide)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(p.line)),
        );
    }
    let label = |t: i64| {
        let text = overseas::source_time(t);
        if period == Period::Intraday {
            text.get(6..11).unwrap_or("--").to_string()
        } else {
            chrono::DateTime::from_timestamp(t, 0)
                .map(|t| {
                    t.with_timezone(&chrono::FixedOffset::east_opt(28800).unwrap())
                        .format("%y-%m-%d")
                        .to_string()
                })
                .unwrap_or_else(|| "--".into())
        }
    };
    f.render_widget(
        Chart::new(datasets)
            .x_axis(
                Axis::default()
                    .title("北京时间")
                    .bounds([xmin, xmax])
                    .labels(vec![
                        label(if period == Period::Intraday {
                            xmin as i64
                        } else {
                            first
                        }),
                        label(if period == Period::Intraday {
                            ((xmin + xmax) / 2.) as i64
                        } else {
                            view[view.len() / 2].time
                        }),
                        label(if period == Period::Intraday {
                            xmax as i64
                        } else {
                            last
                        }),
                    ])
                    .style(Style::default().fg(p.line)),
            )
            .y_axis(
                Axis::default()
                    .title(if index { "点" } else { "USD" })
                    .bounds(bounds)
                    .labels(
                        ticks
                            .iter()
                            .map(|v| format!("{:.2}", *v as f64 / 100.))
                            .collect::<Vec<_>>(),
                    )
                    .style(Style::default().fg(p.line)),
            )
            .style(Style::default().fg(p.fg)),
        price_area,
    );
    if let Some((_, area)) = volume_area {
        let mut bins = std::collections::BTreeMap::<u16, (f64, f64, ratatui::style::Color)>::new();
        let columns = area.width.saturating_sub(12).max(1);
        for (i, b) in view.iter().enumerate() {
            if let Some(volume) = b.volume {
                let x = if period == Period::Intraday {
                    b.time as f64
                } else {
                    i as f64
                };
                let column =
                    ((x - xmin) / (xmax - xmin) * columns as f64).clamp(0., columns as f64) as u16;
                let entry = bins.entry(column).or_insert((
                    x,
                    0.,
                    p.change(b.close - b.open.unwrap_or(b.close)),
                ));
                entry.1 += volume;
            }
        }
        let max = bins.values().map(|v| v.1).fold(0., f64::max).max(1.);
        let lines = bins
            .values()
            .map(|&(x, v, c)| (vec![(x, 0.), (x, v)], c))
            .collect::<Vec<_>>();
        let datasets = lines
            .iter()
            .map(|(line, c)| {
                Dataset::default()
                    .data(line)
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(*c))
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Chart::new(datasets)
                .x_axis(
                    Axis::default()
                        .bounds([xmin, xmax])
                        .style(Style::default().fg(p.line)),
                )
                .y_axis(
                    Axis::default()
                        .bounds([0., max])
                        .title("量（股）")
                        .labels(vec!["0".to_string(), format!("{max:.0}")])
                        .style(Style::default().fg(p.line)),
                ),
            area,
        );
    }
}
pub fn mouse(
    app: &mut App,
    r: Rect,
    point: Position,
    event: crossterm::event::MouseEventKind,
) -> bool {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
    if app.page() == Page::Indices && !app.user.overseas.index_detail {
        match event {
            MouseEventKind::Down(MouseButton::Left) if point.y == r.bottom().saturating_sub(1) => {
                app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                return true;
            }
            MouseEventKind::Down(MouseButton::Left)
                if point.y > r.y && point.y < r.bottom().saturating_sub(1) =>
            {
                let rows = app.user.overseas.index_rows();
                let selected = rows
                    .iter()
                    .position(|id| id == &app.user.overseas.index_selected)
                    .unwrap_or(0);
                let count = r.height.saturating_sub(2) as usize;
                let offset = selected.saturating_sub(count.saturating_sub(1));
                app.overseas_select(offset + (point.y - r.y - 1) as usize);
                return true;
            }
            MouseEventKind::ScrollDown => {
                app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                return true;
            }
            MouseEventKind::ScrollUp => {
                app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                return true;
            }
            _ => return false,
        }
    }
    let (list, detail) = if app.page() == Page::Indices {
        (Rect::default(), r)
    } else {
        panes(app, r)
    };

    if list.contains(point) {
        match event {
            MouseEventKind::Down(MouseButton::Left) if point.y > list.y => {
                let ids = if app.page() == Page::Indices {
                    app.user.overseas.index_rows()
                } else {
                    app.user.overseas.visible()
                };
                let selected = ids
                    .iter()
                    .position(|id| Some(id.as_str()) == app.user.overseas.selected())
                    .unwrap_or(0);
                let available = list.height.saturating_sub(1) as usize;
                let offset = selected.saturating_sub(available.saturating_sub(1));
                app.overseas_select(offset + (point.y - list.y - 1) as usize);
                return true;
            }
            MouseEventKind::ScrollDown => {
                app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                return true;
            }
            MouseEventKind::ScrollUp => {
                app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                return true;
            }
            _ => {}
        }
    }
    if detail.contains(point)
        && point.y == detail.y + 3
        && app.user.mode == Mode::Full
        && matches!(event, MouseEventKind::Down(MouseButton::Left))
    {
        let i = ((point.x - detail.x) / 7).min(3) as usize;
        let period = Period::ALL[i];
        if app.page() == Page::Indices {
            app.user.overseas.index_period = period;
        } else {
            app.user.overseas.us_period = period;
        }
        app.overseas_cursor = None;
        app.request_dirty = true;
        app.dirty = true;
        app.state_dirty = true;
        return true;
    }
    false
}

pub(crate) fn index_tabs(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    for (i, label) in [
        format!(
            " {} {} ",
            if app.user.mode == Mode::Minimal {
                "watch"
            } else {
                "我的关注"
            },
            app.user.overseas.index_watchlist.len()
        ),
        format!(
            " {} {} ",
            if app.user.mode == Mode::Minimal {
                "all"
            } else {
                "全部指数"
            },
            crate::index_board::catalog().len()
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let active = app.user.overseas.index_all == (i == 1);
        f.render_widget(
            Paragraph::new(label).style(
                Style::default()
                    .fg(if active { p.accent } else { p.dim })
                    .bg(if active { p.selected } else { p.bg }),
            ),
            Rect::new(
                r.x + i as u16 * 18,
                r.y,
                18.min(r.width.saturating_sub(i as u16 * 18)),
                1,
            ),
        );
    }
    if r.width >= 50 {
        f.render_widget(
            Paragraph::new(if app.user.overseas.index_detail {
                "Esc 返回总览"
            } else {
                "Tab 切换"
            })
            .style(Style::default().fg(p.dim)),
            Rect::new(r.x + 38, r.y, r.width - 38, 1),
        );
    }
}
fn index_overview(f: &mut Frame, app: &App, r: Rect, p: Palette) {
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(r);
    let ids = app.user.overseas.index_rows();
    let region = r.width >= 72;
    let code = r.width >= 112 && app.user.mode == Mode::Full;
    let quiet = app.user.mode == Mode::Minimal;
    let region_width = if quiet { 3 } else { 6 };
    let price_width = if quiet { 10 } else { 11 };
    let pct_width = if quiet { 8 } else { 9 };
    let time_width = if r.width >= 72 {
        if quiet { 14 } else { 17 }
    } else {
        5
    };
    let mut widths = vec![];
    let mut headers = vec![];
    if region {
        widths.push(Constraint::Length(region_width));
        headers.push(if app.user.mode == Mode::Minimal {
            "area"
        } else {
            "地区"
        });
    }
    let longest = ids
        .iter()
        .map(|id| app.overseas_label(id).width() + 2)
        .max()
        .unwrap_or(8);
    let reserved = price_width + pct_width + time_width + if region { region_width + 4 } else { 3 };
    widths.push(if quiet {
        Constraint::Length(crate::ui::compact_name_width(longest, r.width, reserved))
    } else if code {
        Constraint::Length(22)
    } else {
        Constraint::Min(12)
    });
    headers.push(if app.user.mode == Mode::Full {
        "指数"
    } else {
        "item"
    });
    if code {
        widths.push(Constraint::Length(11));
        headers.push("代码");
    }
    widths.extend([
        Constraint::Length(price_width),
        Constraint::Length(pct_width),
        Constraint::Length(time_width),
    ]);
    headers.extend(if app.user.mode == Mode::Minimal {
        ["value", "delta%", "time"]
    } else {
        ["点位", "涨跌幅", "源时间"]
    });
    let rows = ids
        .iter()
        .map(|id| {
            let quote = app.overseas_quote(id);
            let pct = quote.and_then(|q| q.percent());
            let mut cells = vec![];
            if region {
                let region = crate::index_board::region(id);
                cells.push(ratatui::widgets::Cell::from(
                    if app.user.mode == Mode::Minimal {
                        match region {
                            "内地" => "CN",
                            "香港" => "HK",
                            "日本" => "JP",
                            "韩国" => "KR",
                            _ => "US",
                        }
                    } else {
                        region
                    },
                ));
            }
            cells.push(ratatui::widgets::Cell::from(format!(
                "{}{}",
                if app.user.overseas.index_pinned.contains(id) {
                    "^ "
                } else {
                    "  "
                },
                app.overseas_label(id)
            )));
            if code {
                cells.push(ratatui::widgets::Cell::from(id.clone()));
            }
            cells.push(ratatui::widgets::Cell::from(
                quote
                    .map(|q| format!("{:.2}", q.price))
                    .unwrap_or_else(|| "--".into()),
            ));
            cells.push(
                ratatui::widgets::Cell::from(
                    pct.map(|v| {
                        format!(
                            "{v:+.2}{}",
                            if app.user.mode == Mode::Minimal {
                                ""
                            } else {
                                "%"
                            }
                        )
                    })
                    .unwrap_or_else(|| "--".into()),
                )
                .style(Style::default().fg(pct.map(|v| p.change(v)).unwrap_or(p.dim))),
            );
            cells.push(
                ratatui::widgets::Cell::from(
                    quote
                        .map(|q| {
                            let time = overseas::source_time(q.time);
                            if time_width == 5 {
                                time.get(6..11).unwrap_or("--").into()
                            } else {
                                time
                            }
                        })
                        .unwrap_or_else(|| "--".into()),
                )
                .style(Style::default().fg(p.dim)),
            );
            Row::new(cells)
        })
        .collect::<Vec<_>>();
    let selected = ids
        .iter()
        .position(|id| id == &app.user.overseas.index_selected);
    let table_area = if quiet {
        crate::ui::compact_table_area(areas[0], &widths)
    } else {
        areas[0]
    };
    f.render_stateful_widget(
        Table::new(rows, widths)
            .header(Row::new(headers).style(Style::default().fg(p.dim)))
            .row_highlight_style(Style::default().bg(p.selected)),
        table_area,
        &mut TableState::default().with_selected(selected),
    );
    if ids.is_empty() {
        f.render_widget(
            Paragraph::new("暂无关注，按 a 从全部指数添加。").style(Style::default().fg(p.dim)),
            Rect::new(r.x, r.y + 1, r.width, 1),
        );
    }
    let label = if let Some(id) = app.user.overseas.selected() {
        let source = app
            .overseas_quote(id)
            .map(|q| format!("{} · {} 北京时间", q.source, overseas::source_time(q.time)))
            .unwrap_or_else(|| "暂无报价；不以模拟数据替代".into());
        format!("{} · {source} · Enter 走势", app.overseas_label(id))
    } else {
        "a 添加 · Tab 查看全部指数".into()
    };
    f.render_widget(
        Paragraph::new(label).style(Style::default().fg(p.dim)),
        areas[1],
    );
}
