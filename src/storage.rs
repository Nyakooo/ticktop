use crate::{
    config::Mode,
    model::{Instrument, Period, Quote, Series},
};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("missing parent directory")?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".ticktop-{}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: u64,
    pub name: String,
    pub symbols: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserState {
    pub watchlist: Vec<String>,
    pub groups: Vec<Group>,
    pub aliases: BTreeMap<String, u64>,
    pub next_alias: u64,
    pub mode: Mode,
    pub group: u64,
    pub selected: Option<String>,
    pub period: Period,
    pub heatmap: bool,
    pub sector_category: crate::sectors::Category,
    pub sector_sort: crate::sectors::Sort,
}
impl Default for UserState {
    fn default() -> Self {
        let watchlist: Vec<String> = [
            "sz300750", "sh600519", "sh601318", "sz002594", "sh688981", "sh600036",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let aliases = watchlist
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u64 + 1))
            .collect();
        Self {
            selected: watchlist.first().cloned(),
            watchlist: watchlist.clone(),
            groups: vec![
                Group {
                    id: 1,
                    name: "重点关注".into(),
                    symbols: watchlist[..3].to_vec(),
                },
                Group {
                    id: 2,
                    name: "科技".into(),
                    symbols: vec!["sz300750".into(), "sh688981".into()],
                },
            ],
            aliases,
            next_alias: 7,
            mode: Mode::Full,
            group: 0,
            period: Period::Intraday,
            heatmap: false,
            sector_category: crate::sectors::Category::default(),
            sector_sort: crate::sectors::Sort::default(),
        }
    }
}
impl UserState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s: Self = serde_json::from_slice(&std::fs::read(path)?)
            .context("state.json 损坏；请先备份后修复，程序不会覆盖它")?;
        s.validate()?;
        Ok(s)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
    fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        if self
            .watchlist
            .iter()
            .any(|s| !crate::model::valid_id(s) || !seen.insert(s))
        {
            bail!("自选列表含无效或重复代码");
        }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for group in &self.groups {
            if group.id == 0
                || !ids.insert(group.id)
                || group.name.trim().is_empty()
                || group.name == "全部自选"
                || !names.insert(&group.name)
            {
                bail!("分组数据无效");
            }
            if group.symbols.iter().any(|s| !self.watchlist.contains(s)) {
                bail!("分组包含不在自选列表的标的");
            }
        }
        let aliases: std::collections::HashSet<_> = self.aliases.values().collect();
        if aliases.len() != self.aliases.len()
            || self
                .aliases
                .values()
                .any(|v| *v == 0 || *v >= self.next_alias)
            || self.watchlist.iter().any(|s| !self.aliases.contains_key(s))
        {
            bail!("自选编号数据无效");
        }
        Ok(())
    }
    pub fn visible(&self) -> Vec<String> {
        if self.group == 0 {
            self.watchlist.clone()
        } else {
            self.groups
                .iter()
                .find(|g| g.id == self.group)
                .map(|g| g.symbols.clone())
                .unwrap_or_default()
        }
    }
    pub fn group_name(&self) -> &str {
        if self.group == 0 {
            "全部自选"
        } else {
            self.groups
                .iter()
                .find(|g| g.id == self.group)
                .map(|g| g.name.as_str())
                .unwrap_or("全部自选")
        }
    }
    pub fn add(&mut self, id: &str, group: u64) -> Result<()> {
        if !crate::model::valid_id(id) {
            bail!("无效股票代码");
        }
        if group != 0 && !self.groups.iter().any(|g| g.id == group) {
            bail!("分组不存在");
        }
        if !self.watchlist.iter().any(|s| s == id) {
            self.watchlist.push(id.into());
            if !self.aliases.contains_key(id) {
                self.aliases.insert(id.into(), self.next_alias);
                self.next_alias += 1;
            }
        }
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == group)
            && !g.symbols.iter().any(|s| s == id)
        {
            g.symbols.push(id.into());
        }
        Ok(())
    }
    pub fn remove(&mut self, id: &str) {
        if self.group == 0 {
            self.watchlist.retain(|s| s != id);
            for g in &mut self.groups {
                g.symbols.retain(|s| s != id);
            }
        } else if let Some(g) = self.groups.iter_mut().find(|g| g.id == self.group) {
            g.symbols.retain(|s| s != id);
        }
    }
    pub fn create_group(&mut self, name: &str) -> Result<u64> {
        let name = name.trim();
        self.check_name(name, None)?;
        let id = self.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        self.groups.push(Group {
            id,
            name: name.into(),
            symbols: Vec::new(),
        });
        Ok(id)
    }
    pub fn rename_group(&mut self, id: u64, name: &str) -> Result<()> {
        self.check_name(name.trim(), Some(id))?;
        self.groups
            .iter_mut()
            .find(|g| g.id == id)
            .context("分组不存在")?
            .name = name.trim().into();
        Ok(())
    }
    fn check_name(&self, name: &str, except: Option<u64>) -> Result<()> {
        if name.is_empty()
            || name.chars().count() > 24
            || name == "全部自选"
            || self
                .groups
                .iter()
                .any(|g| Some(g.id) != except && g.name == name)
        {
            bail!("组名需为 1–24 个字符，且不能重复或使用“全部自选”");
        }
        Ok(())
    }
    pub fn delete_group(&mut self, id: u64) {
        self.groups.retain(|g| g.id != id);
        if self.group == id {
            self.group = 0;
        }
    }
    pub fn alias(&self, id: &str) -> String {
        self.aliases
            .get(id)
            .map(|n| format!("{n:03}"))
            .unwrap_or_else(|| "---".into())
    }
}

pub struct Cache {
    conn: Connection,
}
impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS cache (key TEXT PRIMARY KEY, body TEXT NOT NULL);")?;
        Ok(Self { conn })
    }
    pub fn read<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut stmt = self.conn.prepare("SELECT body FROM cache WHERE key=?1")?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            let body: String = row.get(0)?;
            Ok(Some(serde_json::from_str(&body)?))
        } else {
            Ok(None)
        }
    }
    pub fn write<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO cache(key,body) VALUES(?1,?2)",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn quotes(&self) -> Result<Vec<Quote>> {
        Ok(self.read("quotes")?.unwrap_or_default())
    }
    pub fn series(&self, id: &str, period: Period) -> Result<Option<Series>> {
        self.read(&format!("series:{id}:{}", period.key()))
    }
}
#[derive(Deserialize, Serialize)]
pub struct Catalog {
    pub as_of: String,
    pub source: String,
    pub stocks: Vec<Instrument>,
}
pub fn bundled_catalog() -> Catalog {
    let mut catalog: Catalog = serde_json::from_str(include_str!("../assets/stocks.json"))
        .expect("validated bundled catalog");
    crate::instruments::merge(&mut catalog.stocks);
    catalog
}
