# v0.1 validation

Validated locally on Linux x86_64, 2026-10-29. macOS and ARM builds are defined in
release CI but have not been run on this workstation. No release has been published.

Repository pushes to master succeeded. Remote CI status could not be read with the
available GitHub API credentials (404), so only local checks are claimed here.
Anonymous download access to the repository was not available in this session; the
public curl flow additionally needs anonymously accessible script/release assets.

## Automated checks

- `cargo fmt --check`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked`: behavior and local-socket proxy tests; the timing-only
  benchmark is explicitly ignored in normal CI to avoid hardware-dependent failures.
- `python3 -m unittest discover -s tests -p 'test_*.py' -v`: eight offline installer
  cases; zsh/bash selection, ZDOTDIR, idempotent PATH, checksum preservation,
  symlink archive rejection and shell-special-character paths.
- `python3 scripts/pty_smoke.py`: the actual executable in a pseudo-terminal;
  Chinese search, adding a stock, creating a Chinese-named group, full/minimal
  switching, heatmap, pause/resume, resize, help, exit and alternate-screen restore.
- `cargo run --example screens`: actual renderer in TestBackend; four screen states
  can be captured as text and cell/color data in `target/screens/`.

## Live source checks

- `ticktop --check`: Tencent batch quotes for Shanghai and Shenzhen symbols returned
  successfully, with exact integer price parsing and source timestamps.
- Real TUI session fetched six watchlist quotes and all three selected-stock history
  periods; the SQLite cache contained intraday data, five daily points and 22 daily
  points. The process restored the terminal after SIGTERM.
- CNInfo stock-list request succeeded; 6,184 A-share catalog entries are bundled as
  a first-run/offline search index. Listing availability is determined by the source;
  a listed name can have unavailable quotes (e.g. suspension/delisting).
- These checks confirm connectivity here, not an upstream service guarantee.

## Performance observation

Release-mode search benchmark over the bundled 6,184 entries: about 66 ms to build
an index, about 9.3 ms per `ndsd` query on this workstation. Includes bounded
polyphonic pinyin expansion. Timings vary with hardware and workload.

Final release binary: about 7.9 MiB. A local minimal/demo-mode PTY sample reached
the first screen in about 154 ms, used about 14.7 MiB RSS, and averaged 0.40% of
one logical CPU over a five-second sample. This is a small demo watchlist and a
short observation, not a guarantee for all machines or large real-data workloads.

## Code review

Fixed review baseline: `a17daf1`; originating scope: `docs/SPEC.md`.

Standards axis: no repository-standard breaches. A duplicated persistence path in
modal handlers could skip retry after disk errors; consolidated through the shared
state-save/retry helpers, including add-to-group. Regression tests cover recovery.

Spec axis: fixed two findings. Malformed environment proxies could previously be
silently ignored by the HTTP dependency; environment proxies are now explicitly
validated and installed, including NO_PROXY. First-run mode saving previously
persisted temporary CLI network overrides; saved and runtime configuration are now
separate. Regression tests cover both paths, credential redaction and exclusions.

Original scope exclusions: no trading, technical indicators, foreign markets, or
full-market sector indices. The original watchlist heatmap has since been replaced
by the whole-market cloud described below.
The curl installer and manual updater need published, checksummed release assets.


## Local whole-market cloud revision (2026-10-31)

The heatmap now uses independent whole-market sector snapshots. Live requests
verified 84 industry sectors and 175 concept sectors from Sina; no top-N truncation.
These counts are observations, not hardcoded limits. Percentage changes are Sina's
reported sector averages, and the UI labels retrieval time because the response
contains no source timestamp. All sectors remain accessible via paging.

Regression checks cover watchlist independence (including an empty watchlist),
category switching, all-sector navigation, numeric-only minimal rendering, pause
and cached-snapshot recovery. Live source validation is opt-in and excluded from
normal offline CI. This revision remains local pending the user's approval to push.


The native desktop preview also exposed an unused legacy ALL_PROXY value alongside
valid HTTP_PROXY and HTTPS_PROXY settings. Resolution now validates only applicable
fallbacks; explicit protocol proxies retain precedence, and malformed applicable
fallbacks still block requests. The desktop environment's real quote check passed
without switching to direct mode. Regression tests cover both cases.

## Local paging, mouse and weighted cloud revision

Reproduced partial-last-page navigation with `cargo test --test behavior cloud_paging_returns -- --nocapture`: failed (selection 56 versus page start 44). Paging now targets page starts, preserves selection at boundaries, and reports first/last page. Added n/p aliases when terminals intercept PgUp/PgDn. Both modes are covered.

Cloud full mode now uses a binary treemap weighted by per-page constituent count, explicitly labelled as non-capitalization; concept constituents overlap. Minimal mode keeps a monochrome numeric matrix. A 3:1 weight test verifies 3:1 cell area, complete coverage and no overlap. Renderer and mouse hit tests share tile geometry. Mouse supports cloud selection, tabs, wheel paging, page buttons and footer mode/view switching; background input is ignored with a modal open.

Validation: 45 Rust tests passed (2 opt-in tests ignored), Clippy with warnings denied passed; native PTY smoke passed with SGR mouse clicks, wheel events and mouse-capture restoration assertions. Full and minimal captures inspected from actual Ratatui renderer. No push performed.

Cloud separation refinement: framed full-mode tiles with muted neutral borders and an accent border on the selected tile; very small tiles use a right separator to preserve label space. Tile rectangles, area weights and mouse hit testing are unchanged. Actual renderer output inspected; focused cloud mode regression passed.

Minimal mode identity refinement: stock tables and selected-series labels now show market-prefixed stock codes (e.g. sz300750). Sector matrices now show Chinese names beside ordinals and signed percentage changes, with display-column-aware truncation; the selected sector detail retains the full name. Sector categories are named in both modes. The matrix uses wider columns, shared by pagination and mouse hit testing. Supersedes the previous numeric-only display requirement. Existing multi-size rendering, paging and mouse tests updated and passed.

Cloud sorting revision: default descending percentage change; s or the clickable sort label cycles gainers, losers and constituent count descending. Missing changes sort last and ties use source codes. Sorting is shared across modes and categories, persisted in user state, and applied to restored caches and network updates. Refresh preserves the selected sector by code; changing sort returns to page one. Added coverage for ordering, missing values, ties, selection retention, cache restore and mouse sorting. Gray frames have been replaced with dark gutters and an accent selection marker.

Sorted full cloud now uses ordered weighted horizontal strips, read left-to-right then top-to-bottom, replacing recursive irregular subdivisions. Tile widths and strip heights encode constituent counts within the page; rounding is to terminal cells and dark gutters are decorative. Shared mouse geometry and the 3:1 area/coverage test passed after this layout update.

Reference-style cloud revision: ordered squarified treemap replaces horizontal strips, accounts for terminal cell aspect ratio, and retains the chosen sort as allocation order/rank. Full-mode density adapts to the viewport. Larger tiles center wrapped names, percentages and constituent counts; tiny tiles remain colored without misleading truncated percentages. Dark seams replace gray frames. Hover/click details share renderer geometry and repeated hovering over one tile avoids redraws. The scope is sector-level visualization using the existing real sector snapshots, with constituent-count weights, not market caps or synthetic constituent-stock data. Both minimal and full modes remain available. Geometry tests cover 0–175 weights at normal and very small terminal sizes, checking coverage, bounds and non-overlap. Real cached data can be captured with `cargo run --example screens -- target/treemap-preview target/cloud-preview`.

User rejected the dense squarified reference-style experiment. Restored ordered weighted rows with the previous full-mode page capacity (up to three rows), muted colors, dark gutters and single-line left-aligned names. Sorting, named minimal matrices, stock codes, hover/click details, pagination and preferences remain. Removed the experimental treemap module. This supersedes the preceding reference-style layout revision.

Initial v0.1.0 release preparation: branding uses Ticktop for display and ticktop for repository, crate and command names. Repository has been renamed to Nyakooo/ticktop; the user authorized public visibility and publication. Local release checks passed: 47 Rust tests (2 opt-in tests ignored), Clippy with warnings denied, format checking, abrupt-restart mode smoke, real PTY smoke, and 8 installer tests. A source scan found no private-key, GitHub-token or AWS-access-key patterns. README now links the four planned release archives and checksums; anonymous asset and installation verification remains a post-publication step.

Release review: Standards axis found one terminal-lifecycle cleanup issue; fixed with TerminalGuard, covering mouse-capture setup errors and panic unwinding, and reviewer confirmed closure. Spec axis found no functional release blocker; public asset availability and anonymous installation remain post-publication gates. Consolidated SPEC to the final accepted behavior, removing superseded numeric-only/equal-area/experimental treemap requirements. Follow-up Clippy and real PTY smoke passed; credential scan across all five local commits found no configured token/private-key patterns.


## 筹码峰增量验证（2026-10-31）

- 新增日 K 线字段解析、换手衰减/归一化、平价/零换手、无效数据、双模式多尺寸渲染、暂停恢复及缓存重启测试。
- 实际 TUI 渲染预览由 `examples/screens.rs` 生成，演示数据明确标识。
- 东方财富日线接口最初经 Python 成功返回 240 条日线；后续 Python 与 Rust 请求均被远端断连。opt-in `live_chips_daily_source` 未通过，不能据此宣称实时数据接入稳定。失败展示不可用或保留已有缓存，不以演示数据替代。


## ETF / 指数及常驻栏增量验证（2026-10-31）

- 内置新浪公开 ETF 名单快照 1,715 项，原始分页条数与 count 接口一致，按完整交易所代码去重。指数注册表包含 9 个常用指数。
- `cargo test --locked`：62 项通过，4 项需手动开启的测试忽略。覆盖代码重名、中文/拼音/代码搜索、ETF 三位小数、恒生时间格式、设置迁移/保存/关闭、窄窗口折叠、云图分页几何及鼠标定位。
- `cargo test --locked live_indices_and_etf_quotes_and_history -- --ignored --nocapture`：真实网络验证通过，7 个标的报价完整；恒生、沪深300ETF、上证指数的分时和日线均返回数据。
- `cargo clippy --locked --all-targets -- -D warnings`、`cargo fmt`、优化版构建通过。
- `python3 scripts/pty_smoke.py target/release/ticktop`：真实 PTY 验证中文/ETF 搜索、常驻指数设置保存、分组、完整/简约、云图、暂停、鼠标、缩放、帮助与终端恢复通过。
- `examples/screens --indices` 生成完整及简约模式的实际渲染预览；预览明确使用演示数据。


## Windows 原生支持验证（2026-11-01 / v0.3.0）

- Windows Server 2022 x64 runner 完成原生 MSVC 编译、Clippy 和 62 项 Rust 测试（4 项手动测试忽略）。CI 41289546653 的 Windows/Linux jobs 均成功。
- Windows PowerShell 5.1 安装器实测：固定版本安装、中文/空格/方括号路径、用户/当前会话 PATH 去重、NoPath、运行中替换 exe、备份清理、个人数据保留、校验失败/路径穿越/版本不符拒绝并保留原安装。
- pywinpty 3.0.2 的 Windows 终端实测：中文搜索并添加平安银行（按保存的代码确认）、ETF 预览、常驻设置、模式与云图、缩放、退出与重启恢复。测试依赖仅在 CI 使用，产品无 Python 依赖。
- Linux 格式检查、Clippy、62 项 Rust 测试、PTY 操作、模式重启及 9 项 Bash 安装器测试通过；Git Bash 分发入口使用本地运输桩检查，确认委托 PowerShell 且不改 Unix rc 文件。
- 发布矩阵新增 x86_64-pc-windows-msvc ZIP + SHA-256，使用静态 CRT；发布后 Windows job 对公开脚本安装、自更新和下载版 ConPTY 再次验证。

## v0.3.1 发布验证（2026-11-02）

- Rust 测试：65 通过，4 项联网检查按需忽略；Clippy、rustfmt、9 项安装器回归检查通过。
- 两市成交额同日汇总、完整/简约/云图常驻、量比字段及无效值处理、固定全天时间轴均通过回归验证。
- 完整与简约渲染结果断言连续显示“量比 1.56  换手率 1.23%  成交量 12.35万手”，避免只校验字段存在而漏掉顺序错误。
- 重新构建的本地 release 二进制报告 0.3.1，通过两种模式指标顺序的真实 PTY 检查，以及搜索、指数、分组、云图、模式持久化等交互验证。终端测试等待设置实际保存后再断言，避免编译高负载下的固定时延误判。

用户确认取消成交额预测后，删除常驻栏的“预计暂无”提示，仅保留实际成交额及源时间；回归测试同时断言完整/简约/云图中不出现“预计”。原 v0.3.1 构建取消，tag 与安装包基于调整后的提交重新生成。

## K 线与成交量本地验证（2026-11-02）

- 四周期切换、日 K 选择持久化、OHLC 解析及异常范围拒绝、累计分钟量差分（含零量、回退及缺失字段）、旧缓存兼容、K 线游标视窗和简约数字摘要均有回归验证。
- 常规测试 68 项通过；另实测恒生指数、沪深300 ETF、上证指数的四种周期数据，均返回有效行情。
- 成交量按行情源口径展示，价格与量图共享时间坐标；日/周/月 K 使用独立缓存键，不把原“1周 / 1月”日收盘价缓存当作周期 OHLC。
- 真实 PTY 流程增加分时到日 K 的切换、成交量面板与 Home/End 游标检查。测试通过调整窗口尺寸强制重绘后核对完整量图标题，避免增量终端输出省略未变文字导致误判。
- 实际渲染预览保存在 `target/volume-preview/`，使用演示数据，非实时行情截图。

## 两市同分钟成交额比较（2026-11-03）

- 免费接口实测返回沪市 A 股（sh000002）与深证 A 指（sz399107）最近 5 个交易日的分钟累计人民币成交额，具备上一交易日同时间数据。
- 实测 2026-11-03 与 2026-11-02 的 14:11：当前 13760.77 亿，前交易日 15375.03 亿，差额 -1614.27 亿。以上仅为验证时点快照。
- 测试覆盖上一交易日跨周末、排除尚未结束的最新分钟、收盘分钟、金额分精度、今昨日期错位、数据缺失/回退/异常、界面四模式显示、过期数据隐藏与暂停恢复。

## 自选管理与简约 alias（2026-11-03）

- 验证分组独立置顶与手动排序、置顶边界、重命名与分组排序、旧状态迁移、保存/重载、删除分组/股票时清理元数据。
- 别名验证覆盖重复、控制字符、不可见字符、显示宽度、保留编号冲突与新股票自动编号避让；取消输入不会覆盖现有别名。
- 简约表格保留代码与 alias；真实 PTY 流程验证 Ctrl+Down、置顶、alias 输入、分组重命名和实际保存的状态文件。模式重启检查通过。

## 默认配置文件自动读取与迁移（2026-11-03）

- 默认位置的 config.toml / state.json 缺失时创建；创建采用 create_new，已存在文件不会被初始默认值覆盖。
- 未知字段、格式错误、无效配置与编码异常均提供文件路径及备份/修复/删除后重建指引，不自动删除或覆盖原文件；解析错误不回显配置凭据。
- 通过复制两文件到另一套默认平台目录、不传 --data-dir 的真实终端测试，确认启动自动恢复 alias、置顶、模式等状态；删除文件后自动重建。
- 简约模式设置 alias 后列表、选中摘要及常驻指数以别名替代代码；未设置时保留代码。常规测试、Clippy 与终端模式持久化检查通过。
