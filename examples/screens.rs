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
    // Optional second argument renders an existing live cache without fetching data
    // or overwriting the user's mode and view preferences.
    let indices_preview = std::env::args().any(|s| s == "--indices");
    let live_data = std::env::args()
        .nth(2)
        .filter(|s| s != "--indices")
        .map(std::path::PathBuf::from);
    let demo = live_data.is_none();
    let data = live_data.unwrap_or_else(|| out.join("data"));
    let mut app = App::new(Config::default(), Paths::resolve(Some(&data))?, false, demo)?;
    if indices_preview {
        app.config.pinned_indices = ticktop::instruments::INDICES
            .iter()
            .take(5)
            .map(|(id, _)| (*id).to_string())
            .collect();
        app.user.add("sh510300", 0)?;
        app.user.selected = Some("sh510300".into());
    }
    if demo {
        app.on_market(MarketEvent::Quotes(market::demo_quotes(
            &app.request().symbols,
        )));
        app.on_market(MarketEvent::Chart(market::demo_history(
            app.user.selected.as_deref().unwrap(),
            app.user.period,
        )));
        if !ticktop::instruments::is_index(app.user.selected.as_deref().unwrap())
            && !ticktop::instruments::is_etf(app.user.selected.as_deref().unwrap())
        {
            app.on_market(MarketEvent::Chips(ticktop::chips::demo(
                app.user.selected.as_deref().unwrap(),
            )));
        }
        app.on_market(MarketEvent::Sectors(ticktop::sectors::demo(
            ticktop::sectors::Category::Industry,
        )));
    }
    let (width, height) = if demo { (130, 36) } else { (150, 44) };
    for (name, mode, heat) in [
        ("full", Mode::Full, false),
        ("heatmap", Mode::Full, true),
        ("minimal", Mode::Minimal, false),
        ("matrix", Mode::Minimal, true),
    ] {
        app.user.mode = mode;
        app.user.heatmap = heat;
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|f| ui::draw(f, &app))?;
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        let mut cells = Vec::new();
        for y in 0..height {
            let mut x = 0;
            while x < width {
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
