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
