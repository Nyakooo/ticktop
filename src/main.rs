use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind},
    execute,
};
use futures_util::StreamExt;
use std::{io::IsTerminal, time::Duration};
use ticktop::{
    app::App,
    config::{Cli, Command, Mode, Paths},
    market, ui,
};
use tokio::sync::{mpsc, watch};

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::Update)) {
        return update();
    }
    let paths = Paths::resolve(cli.data_dir.as_ref())?;
    let (saved_config, first) = paths.load_config()?;
    let mut config = saved_config.clone();
    config.apply_cli(&cli);
    config.validate()?;
    if cli.check {
        let ids = vec!["sh600519".to_string(), "sz300750".to_string()];
        let quotes = if cli.demo {
            market::demo_quotes(&ids)
        } else {
            let http = market::client(&config.network)?;
            market::quotes(&http, &ids)
                .await
                .map_err(|e| anyhow::anyhow!(market::safe_error(&e)))?
        };
        println!("{}", serde_json::to_string_pretty(&quotes)?);
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!("请在交互终端中运行，或用 --check 检查行情连接");
    }
    let mut app = App::new(
        saved_config,
        paths,
        first && !cli.minimal && !cli.full,
        cli.demo,
    )?;
    app.config = config;
    if cli.minimal {
        app.user.mode = Mode::Minimal;
    }
    if cli.full {
        app.user.mode = Mode::Full;
    }
    if cli.minimal || cli.full {
        app.remember_mode();
    }
    let (request_tx, request_rx) = watch::channel(app.request());
    let (event_tx, mut event_rx) = mpsc::channel(32);
    market::spawn_workers(request_rx, event_tx);
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let mut input = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let stop = terminate_signal();
    tokio::pin!(stop);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result:Result<()>=async {
        while !app.quit {
            if app.dirty { let size = terminal.size()?; app.set_viewport(size.width, size.height); terminal.draw(|f|ui::draw(f,&app))?; app.dirty=false; }
            if app.request_dirty{request_tx.send_replace(app.request());app.request_dirty=false;}
            tokio::select! {
                maybe=input.next()=>match maybe {Some(Ok(Event::Key(k))) if k.kind==KeyEventKind::Press||k.kind==KeyEventKind::Repeat=>app.key(k),Some(Ok(Event::Resize(..)))=>app.dirty=true,Some(Ok(Event::Mouse(mouse)))=>{let size=terminal.size()?;ui::mouse(&mut app,mouse,size.width,size.height);},Some(Err(e))=>return Err(e.into()),None=>break,_=>{}},
                event=event_rx.recv()=>if let Some(e)=event{app.on_market(e)},
                _ = tick.tick() => {
                    if app.state_dirty {
                        app.flush_state();
                    }
                    if !app.paused { app.dirty = true; }
                },
                _=tokio::signal::ctrl_c()=>break,
                _=&mut stop=>break,
            }
        }Ok(())
    }.await;
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    drop(request_tx);
    app.persist()?;
    result
}
#[cfg(unix)]
async fn terminate_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    if let Ok(mut s) = signal(SignalKind::terminate()) {
        s.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}
#[cfg(not(unix))]
async fn terminate_signal() {
    std::future::pending::<()>().await;
}
fn update() -> Result<()> {
    let exe = std::env::current_exe()?;
    let target = exe.parent().context("无法确定安装目录")?;
    // Embedded installer from the same release; it downloads checksummed assets, not remote shell code.
    let mut child = std::process::Command::new("bash")
        .args(["-s", "--", "--no-path"])
        .env("TICKTOP_INSTALL_DIR", target)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("更新需要 bash 和 curl")?;
    use std::io::Write;
    child
        .stdin
        .take()
        .context("installer stdin unavailable")?
        .write_all(include_bytes!("../scripts/install.sh"))?;
    if !child.wait()?.success() {
        anyhow::bail!("更新失败；原程序和个人数据已保留");
    }
    Ok(())
}
