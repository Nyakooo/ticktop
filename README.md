# ticktop_

轻量的 A 股终端看盘工具。只看行情，不连接交易账户、不下单。

- **完整模式**：自选列表、Braille 点阵价格走势、分组热力图。
- **简约模式**：单色固定编号、纯数字报价、价格采样和涨跌矩阵。
- 中文、代码、拼音、首字母和容错搜索，全部本地执行。
- 自选分组持久保存；一只股票可以加入多个组。
- 免费公开数据源，无需行情账号或 API Key；支持代理、请求间隔设置。

## 运行

当前仓库提供源码。**首次可访问的 GitHub Release 发布前，一键下载命令无法安装。**
匿名 curl 安装还要求脚本及 Release 资产允许匿名下载；私有仓库需要另外安排可访问的分发地址，不能直接使用下面的匿名命令。

```bash
cargo run --release
# 明确标注的离线演示；演示状态和缓存独立，避免污染真实行情
cargo run --release -- --demo
# 简约模式
cargo run --release -- --minimal
# 仅打印一份真实报价并退出，便于排查网络
cargo run --release -- --check
```

构建需要 Rust 1.88+ 和 C 编译器；发布的程序不需要 Rust、Python 或外部数据库。
Linux/macOS，建议 UTF-8 终端、支持中文和 Braille 的等宽字体。最低 46 列 × 16 行，
推荐 120 列 × 35 行；窗口不足 98 列时完整视图改为上下布局。
中文使用终端自身的输入法，应用接收已提交的字符。

## 一键安装（发布 Release 后）

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/Nyakooo/terminal-stock-dashboard/master/scripts/install.sh | bash
```

- 自动识别 Linux/macOS 和 x86_64/ARM64，校验 SHA-256 后原子替换程序。
- 默认安装到 `~/.local/bin/ticktop`，无需 `sudo`。
- 根据 `$SHELL` 识别用户 Shell，不根据执行安装脚本的 bash 误判。
- zsh / oh-my-zsh 写入 `${ZDOTDIR:-$HOME}/.zshrc`；bash 写入 `~/.bashrc`。
- 使用独立标记块，不重复追加 PATH，不覆盖现有 Shell 配置。
- 脚本无法修改父终端的环境；安装后按提示 `source ~/.zshrc` / `source ~/.bashrc`
  或打开新终端。bash 的 login shell 若没有加载 `.bashrc`，需自行在 login 配置中引用。

```bash
# 显式选择 shell / 固定版本
curl -fsSL https://raw.githubusercontent.com/Nyakooo/terminal-stock-dashboard/master/scripts/install.sh \
  | bash -s -- --shell zsh --version v0.1.0

# 更新程序，保留全部个人配置
 ticktop update

# 指定存储目录（便携使用）
 ticktop --data-dir ./data
```

也可以从 Releases 下载对应压缩包及 `.sha256` 文件，校验、解压后 `./ticktop`。
指定二进制位置可设置 `TICKTOP_INSTALL_DIR`；`--no-path` 只安装、不修改 Shell 配置。
删除 `~/.local/bin/ticktop` 即可卸载程序；个人数据默认保留，可按下面路径自行清理。
Shell 配置中的 `# >>> ticktop >>>` 区块可以手动移除。

## 操作

| 按键 | 行为 |
| --- | --- |
| `m` | 完整 / 简约，保留当前分组、股票、周期和视图 |
| `h` | 行情 / 当前自选分组热力图 |
| `[` / `]` | 上一个 / 下一个分组 |
| `↑` / `↓` | 选股；长列表和热力图自动滚动 / 翻页 |
| `←` / `→` | 日内 / 1 周 / 1 月 |
| `j` / `k`，`Home` / `End` | 移动走势游标，查看对应价格 |
| `/` | 搜索；支持中文 / 代码 / 拼音 / 首字母 / 小幅拼写错误 |
| `Enter` | 搜索结果中预览股票；不会自动加入自选 |
| `a` | 搜索并添加到指定自选分组 |
| `Ctrl+a` | 搜索输入中将当前结果加入分组 |
| `d` | 从当前组移除股票，确认后执行；全部自选中移除会同时移出其他组 |
| `g` | 管理分组：`n` 新建、`r` 重命名、`d` 删除、`Enter` 切换 |
| `Space` | 暂停 / 恢复显示，后台请求继续；恢复时显示最新数据 |
| `,` | 网络和刷新设置，`Tab` 切字段，`Ctrl+u` 清空，`Enter` 保存立即生效 |
| `?` | 帮助、股票和分组编号对应关系 |
| `Esc` | 关闭弹层 / 搜索 |
| `q` / `Ctrl+c` | 退出（输入框里的 q 正常输入） |

简约模式主界面使用稳定编号，删除再添加同一股票不会换号。名称仅在主动打开搜索、
分组管理、确认弹层或帮助时显示。`age(s)` 是**源报价**距现在的秒数，不是请求间隔；
`E01` 表示报价请求异常，`E02` 表示走势异常，保留最后有效缓存。

热力图显示当前自选分组内各股票较昨收涨跌，红涨绿跌，等面积、不按市值加权。
**它不是全市场板块指数。** 全市场板块、技术指标、海外股票与 Crypto 留待后续。
1 周 / 1 月走势是最近 5 / 22 个交易日的不复权日收盘价，不是分钟线。
分时横轴按交易分钟排列，压缩午间休市，横轴标签保留实际交易时间。

## 配置

首次启动可选择默认模式。自选、分组、上次模式和选中股票会自动保存。
设置 `--minimal` / `--full` 可覆盖本次启动模式。

```toml
# config.toml
# 运行后的模式会保存在 state.json；default_mode 用于首次初始化。
default_mode = "full"

[network]
proxy_mode = "system" # system / custom / direct
proxy_url = ""
timeout_secs = 10

[refresh]
quotes_secs = 5
chart_secs = 15
```

```bash
ticktop --proxy http://127.0.0.1:7890
ticktop --proxy socks5h://127.0.0.1:1080
ticktop --no-proxy
ticktop --interval 10s
```

命令行覆盖配置文件，配置文件覆盖默认值。命令行覆盖不会自行写入配置；在设置页保存
时会保存当前设置。`system` 采用 HTTP_PROXY / HTTPS_PROXY / ALL_PROXY / NO_PROXY
及对应小写环境变量；`direct` 忽略代理环境变量；`custom` 仅走指定代理，失败不直连。
`socks5h` 将域名解析也交给代理。支持代理认证；界面、错误信息不输出凭据，配置文件
通过权限为 0600 的临时文件原子写入。不要把包含密码的配置提交进仓库。

间隔为 2–3600 秒，超时为 1–120 秒；同一类请求不重叠，失败指数退避，通常上限
300 秒（已设置更长间隔时不缩短间隔）。热力图复用报价，不产生额外行情请求。

默认路径：

| 文件 | Linux | macOS |
| --- | --- | --- |
| 配置 | `$XDG_CONFIG_HOME/ticktop/config.toml`，默认 `~/.config/ticktop/config.toml` | `~/Library/Application Support/ticktop/config.toml` |
| 数据 | `$XDG_DATA_HOME/ticktop/`，默认 `~/.local/share/ticktop/` | `~/Library/Application Support/ticktop/` |

数据目录保存 `state.json`、SQLite 缓存。`--data-dir DIR` 将配置和数据都放入 DIR。
演示使用独立的 `demo-state.json` / `demo-cache.sqlite3`。
损坏的自选文件会报错，程序不会偷偷覆盖它；请先备份再修复。

## 免费数据与边界

- 报价：[腾讯公开行情](https://qt.gtimg.cn/q=sh600519)，GBK 数据转换为 UTF-8。
- 分时 / 日历史：腾讯公开走势图接口。
- 股票名单：[巨潮资讯公开名单](https://www.cninfo.com.cn/new/data/szse_stock.json)。
  内置一份名单以保证首次离线可搜索，后台按日更新并缓存。
- 这些是公开站点接口，**不是有服务保证的授权实时行情产品**。可能限流、变更或缺失，
  本程序仅面向个人查看；数据使用仍受提供方条款约束，不提供再分发保证。
- 休市、停牌、未上市和退市证券可能返回旧快照或无报价；显示源时间，不假装实时。
- 请求失败保留旧值并提示；真实模式不会回退到随机演示数据。
- 价格用万分之一元整数解析和存储，仅图表与涨跌幅计算转换浮点数。

## 开发与验证

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --release bundled_catalog_search_is_fast_enough_for_typing -- --ignored --nocapture
python3 -m unittest discover -s tests -p 'test_*.py' -v
cargo run --release -- --check
```

测试覆盖价格/时间解析、搜索排序、分组独立性与稳定编号、持久化、暂停恢复、
代理脱敏、间隔与退避、多尺寸双模式绘制，以及无网络安装/校验/Shell 配置。
安装测试需 Python 3、bash、zsh；这些仅为开发测试依赖。

版本发布：更新 Cargo 版本和锁文件、通过 CI 后推送 `vX.Y.Z` 标签，Release workflow
构建四个平台，生成校验文件并上传。只有发布后，一键安装及 `ticktop update` 才可用。

完整范围：[docs/SPEC.md](docs/SPEC.md)。
