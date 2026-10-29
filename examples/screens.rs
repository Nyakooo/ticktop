//! Produce actual TUI screen captures with the same renderer used by the application.
use ratatui::{Terminal, backend::TestBackend};
use ticktop::{
    app::App,
    config::{Config, Mode, Paths},
    market::{self, MarketEvent},
    ui,
};
fn main() -> anyhow::Result<()> {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/screens".into()),
    );
    std::fs::create_dir_all(&out)?;
    let data = out.join("data");
    let mut app = App::new(Config::default(), Paths::resolve(Some(&data))?, false, true)?;
    app.on_market(MarketEvent::Quotes(market::demo_quotes(
        &app.user.watchlist,
    )));
    app.on_market(MarketEvent::Chart(market::demo_history(
        app.user.selected.as_deref().unwrap(),
        app.user.period,
    )));
    for (name, mode, heat) in [
        ("full", Mode::Full, false),
        ("heatmap", Mode::Full, true),
        ("minimal", Mode::Minimal, false),
        ("matrix", Mode::Minimal, true),
    ] {
        app.user.mode = mode;
        app.user.heatmap = heat;
        let mut terminal = Terminal::new(TestBackend::new(130, 36))?;
        terminal.draw(|f| ui::draw(f, &app))?;
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        let mut cells = Vec::new();
        for y in 0..36 {
            let mut x = 0;
            while x < 130 {
                let cell = &buffer[(x, y)];
                let symbol = cell.symbol();
                text.push_str(symbol);
                cells.push(serde_json::json!({"x":x,"y":y,"s":symbol,"fg":format!("{:?}",cell.fg),"bg":format!("{:?}",cell.bg)}));
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            text.push('\n');
        }
        std::fs::write(out.join(format!("{name}.txt")), text)?;
        std::fs::write(
            out.join(format!("{name}.json")),
            serde_json::to_vec(&cells)?,
        )?;
    }
    Ok(())
}
