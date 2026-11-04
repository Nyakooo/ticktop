//! Capture the actual renderer in isolated demo settings (no user preferences touched).
use ratatui::{Terminal, backend::TestBackend};
use ticktop::{
    app::App,
    config::{Config, Mode, Paths},
    market::MarketEvent,
    model::Period,
    overseas::{self, Page},
    ui,
};
fn main() -> anyhow::Result<()> {
    let out = std::path::PathBuf::from("target/overseas-preview");
    std::fs::create_dir_all(&out)?;
    let mut app = App::new(
        Config::default(),
        Paths::resolve(Some(&out.join("data")))?,
        false,
        true,
    )?;
    for id in ticktop::index_board::catalog()
        .iter()
        .map(|(id, _)| *id)
        .chain(["AAPL", "NVDA", "TSLA"])
    {
        for period in Period::ALL {
            app.on_market(MarketEvent::Overseas(
                vec![overseas::demo(id, period)],
                vec![],
            ));
        }
    }
    for (page, key) in [(Page::Indices, "indices"), (Page::Us, "us")] {
        app.set_page(page);
        for (mode, label) in [(Mode::Full, "full"), (Mode::Minimal, "minimal")] {
            app.user.mode = mode;
            for (width, height, size) in [(130, 36, "wide"), (80, 24, "narrow")] {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|f| ui::draw(f, &app))?;
                let b = terminal.backend().buffer();
                let mut cells = vec![];
                let mut text = String::new();
                for y in 0..height {
                    let mut x = 0;
                    while x < width {
                        let c = &b[(x, y)];
                        text.push_str(c.symbol());
                        cells.push(serde_json::json!({"x":x,"y":y,"s":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg)}));
                        x += unicode_width::UnicodeWidthStr::width(c.symbol()).max(1) as u16;
                    }
                    text.push('\n');
                }
                let name = format!("{key}-{label}-{size}");
                std::fs::write(out.join(format!("{name}.txt")), text)?;
                std::fs::write(
                    out.join(format!("{name}.json")),
                    serde_json::to_vec(&cells)?,
                )?;
            }
        }
    }
    Ok(())
}
