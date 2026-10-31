//! Whole-market sector snapshots, independent of the user's watchlist.
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use encoding_rs::GBK;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Industry,
    Concept,
}
impl Category {
    pub fn key(self) -> &'static str {
        match self {
            Self::Industry => "industry",
            Self::Concept => "class",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Industry => "行业",
            Self::Concept => "概念",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Industry => Self::Concept,
            Self::Concept => Self::Industry,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Gainers,
    Losers,
    Size,
}
impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Gainers => Self::Losers,
            Self::Losers => Self::Size,
            Self::Size => Self::Gainers,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Gainers => "涨幅↓",
            Self::Losers => "跌幅↓",
            Self::Size => "规模↓",
        }
    }
    pub fn apply(self, sectors: &mut [Sector]) {
        sectors.sort_by(|a, b| {
            let order = match self {
                Self::Size => b.stock_count.cmp(&a.stock_count),
                _ => match (a.change_pct, b.change_pct) {
                    (Some(a), Some(b)) => {
                        if self == Self::Gainers {
                            b.total_cmp(&a)
                        } else {
                            a.total_cmp(&b)
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                },
            };
            order.then_with(|| a.code.cmp(&b.code))
        });
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sector {
    pub code: String,
    pub name: String,
    /// Provider-reported average percentage change; not a synthesized stock index.
    pub change_pct: Option<f64>,
    pub stock_count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub category: Category,
    pub sectors: Vec<Sector>,
    /// The upstream response has no quote timestamp. Never label this as source time.
    pub fetched_at: DateTime<Utc>,
}
impl Snapshot {
    pub fn selected(&self, index: usize) -> Option<&Sector> {
        self.sectors.get(index)
    }
}
pub async fn fetch(http: &reqwest::Client, category: Category) -> Result<Snapshot> {
    let bytes = http
        .get("https://vip.stock.finance.sina.com.cn/q/view/newFLJK.php")
        .query(&[("param", category.key())])
        .header("Referer", "https://finance.sina.com.cn/")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let (text, _, invalid) = GBK.decode(&bytes);
    if invalid {
        bail!("invalid sector encoding");
    }
    parse(category, &text)
}
pub fn parse(category: Category, text: &str) -> Result<Snapshot> {
    let start = text.find('{').context("missing sector object")?;
    let end = text.rfind('}').context("missing sector end")?;
    if end < start {
        bail!("invalid sector object");
    }
    let rows: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&text[start..=end])?;
    if rows.is_empty() {
        bail!("empty sector snapshot");
    }
    let prefix = match category {
        Category::Industry => "hangye_",
        Category::Concept => "gn_",
    };
    let mut sectors = Vec::with_capacity(rows.len());
    for (code, value) in rows {
        if !code.starts_with(prefix) {
            bail!("wrong sector classification");
        }
        let fields: Vec<_> = value
            .as_str()
            .context("invalid sector row")?
            .split(',')
            .collect();
        if fields.len() < 6 || fields[0] != code || fields[1].trim().is_empty() {
            bail!("incomplete sector row");
        }
        let change_pct = match fields[5].trim() {
            "" | "-" | "--" => None,
            raw => {
                let pct: f64 = raw.parse()?;
                if !pct.is_finite() {
                    bail!("invalid sector change");
                }
                Some(pct)
            }
        };
        sectors.push(Sector {
            code,
            name: fields[1].trim().into(),
            change_pct,
            stock_count: fields[2].parse()?,
        });
    }
    // Stable ordering across updates, never just a gainers-only top-N list.
    sectors.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(Snapshot {
        category,
        sectors,
        fetched_at: Utc::now(),
    })
}
pub fn demo(category: Category) -> Snapshot {
    let names = match category {
        Category::Industry => vec![
            "农业",
            "林业",
            "煤炭开采",
            "石油开采",
            "有色金属",
            "食品制造",
            "纺织业",
            "服装制造",
            "造纸业",
            "石油加工",
            "化学原料",
            "化学制品",
            "医药制造",
            "化学纤维",
            "橡胶塑料",
            "非金属矿物",
            "钢铁冶炼",
            "金属制品",
            "通用设备",
            "专用设备",
            "汽车制造",
            "铁路设备",
            "电气机械",
            "电子设备",
            "仪器仪表",
            "电力供应",
            "燃气供应",
            "水务",
            "房屋建筑",
            "土木工程",
            "建筑安装",
            "批发业",
            "零售业",
            "铁路运输",
            "道路运输",
            "水上运输",
            "航空运输",
            "仓储业",
            "邮政业",
            "住宿业",
            "餐饮业",
            "电信服务",
            "互联网",
            "软件服务",
            "银行",
            "证券",
            "保险",
            "房地产",
            "研究开发",
            "环保治理",
            "医疗卫生",
            "文化传媒",
            "娱乐业",
            "综合",
        ],
        Category::Concept => vec![
            "人工智能",
            "半导体",
            "机器人",
            "新能源车",
            "光伏",
            "储能",
            "低空经济",
            "算力",
            "消费电子",
            "创新药",
            "数字经济",
            "国产软件",
            "工业互联网",
            "智能电网",
            "军工",
            "卫星导航",
            "稀土永磁",
            "黄金",
            "国企改革",
            "乡村振兴",
            "跨境电商",
            "数据中心",
            "智能驾驶",
            "芯片设计",
        ],
    };
    Snapshot {
        category,
        fetched_at: Utc::now(),
        sectors: names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Sector {
                code: format!(
                    "{}demo{i:03}",
                    if category == Category::Industry {
                        "hangye_"
                    } else {
                        "gn_"
                    }
                ),
                name: name.into(),
                change_pct: Some(((i * 37 % 103) as f64 - 50.) / 12.),
                stock_count: 10 + (i * 13 % 120) as u32,
            })
            .collect(),
    }
}
