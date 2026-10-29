use anyhow::{Context, Result, bail};
use clap::Parser;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "免费的 A 股终端看盘工具 / A quiet A-share market terminal"
)]
pub struct Cli {
    #[arg(long, conflicts_with = "full")]
    pub minimal: bool,
    #[arg(long)]
    pub full: bool,
    #[arg(long)]
    pub demo: bool,
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    #[arg(long, conflicts_with = "no_proxy")]
    pub proxy: Option<String>,
    #[arg(long)]
    pub no_proxy: bool,
    /// Quote interval, e.g. 5s or 1m (minimum 2s)
    #[arg(long, value_parser=parse_interval)]
    pub interval: Option<u64>,
    /// Print one JSON quote snapshot and exit; useful for diagnosing connectivity
    #[arg(long)]
    pub check: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Download and verify the latest published binary via the installer
    Update,
}
pub fn parse_interval(s: &str) -> std::result::Result<u64, String> {
    let (raw, mult) = if let Some(n) = s.strip_suffix('s') {
        (n, 1)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60)
    } else {
        (s, 1)
    };
    let n = raw
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(mult))
        .ok_or("间隔格式：5s / 1m")?;
    if !(2..=3600).contains(&n) {
        return Err("请求间隔必须为 2–3600 秒".into());
    }
    Ok(n)
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Full,
    Minimal,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    #[default]
    System,
    Custom,
    Direct,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Network {
    pub proxy_mode: ProxyMode,
    pub proxy_url: String,
    pub timeout_secs: u64,
}
impl Default for Network {
    fn default() -> Self {
        Self {
            proxy_mode: ProxyMode::System,
            proxy_url: String::new(),
            timeout_secs: 10,
        }
    }
}
impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("proxy_mode", &self.proxy_mode)
            .field("proxy_url", &"[redacted]")
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Refresh {
    pub quotes_secs: u64,
    pub chart_secs: u64,
}
impl Default for Refresh {
    fn default() -> Self {
        Self {
            quotes_secs: 5,
            chart_secs: 15,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub default_mode: Mode,
    pub network: Network,
    pub refresh: Refresh,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !(2..=3600).contains(&self.refresh.quotes_secs)
            || !(2..=3600).contains(&self.refresh.chart_secs)
        {
            bail!("请求间隔必须为 2–3600 秒");
        }
        if !(1..=120).contains(&self.network.timeout_secs) {
            bail!("超时必须为 1–120 秒");
        }
        if self.network.proxy_mode == ProxyMode::Custom {
            let url = reqwest::Url::parse(&self.network.proxy_url)
                .map_err(|_| anyhow::anyhow!("代理地址无效"))?;
            if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
                || url.host_str().is_none()
            {
                bail!("代理仅支持 http / https / socks5 / socks5h");
            }
        }
        Ok(())
    }
    pub fn apply_cli(&mut self, cli: &Cli) {
        if let Some(p) = &cli.proxy {
            self.network.proxy_mode = ProxyMode::Custom;
            self.network.proxy_url = p.clone();
        }
        if cli.no_proxy {
            self.network.proxy_mode = ProxyMode::Direct;
        }
        if let Some(i) = cli.interval {
            self.refresh.quotes_secs = i;
        }
    }
}
#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
}
impl Paths {
    pub fn resolve(portable: Option<&PathBuf>) -> Result<Self> {
        let p = if let Some(dir) = portable {
            Self {
                config: dir.join("config.toml"),
                data: dir.clone(),
            }
        } else {
            let dirs = ProjectDirs::from("", "", "ticktop")
                .context("无法确定用户目录，请使用 --data-dir")?;
            Self {
                config: dirs.config_dir().join("config.toml"),
                data: dirs.data_local_dir().to_path_buf(),
            }
        };
        std::fs::create_dir_all(&p.data)?;
        if let Some(parent) = p.config.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(p)
    }
    pub fn load_config(&self) -> Result<(Config, bool)> {
        if !self.config.exists() {
            return Ok((Config::default(), true));
        }
        let s = std::fs::read_to_string(&self.config)?;
        let c: Config = toml::from_str(&s).map_err(|_| {
            anyhow::anyhow!(
                "config.toml 格式错误，请检查 TOML 语法（具体内容已隐藏，避免泄露凭据）"
            )
        })?;
        c.validate()?;
        Ok((c, false))
    }
    pub fn save_config(&self, c: &Config) -> Result<()> {
        c.validate()?;
        crate::storage::atomic_write(&self.config, toml::to_string_pretty(c)?.as_bytes())
    }
}
