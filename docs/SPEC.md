# Ticktop v0.1 specification

Agreed scope: a free, personal A-share market-watching terminal application; no
orders, accounts, holdings, indicators, or foreign markets in this release.

- Rust, Ratatui, Tokio; Linux/macOS release binaries; no runtime Python dependency.
- Full mode: watchlist, clear price chart (intraday/week/month), whole-market sector cloud (industry/concept).
- Minimal mode: monochrome fixed numeric aliases, prices, changes, numeric chart
  samples and heatmap matrix; reveal stock/group names only during explicit search,
  group management or help. Switching mode preserves selection, group and period.
- Local persistent watchlist and named groups; add/remove/rename/delete; a stock
  can appear in multiple groups; All is the union; confirmation before removals.
- Chinese/code/pinyin/initials/partial/fuzzy local search, exact/prefix ranked first.
- Default red up / green down; quote timestamps, stale/disconnected/paused states.
- Free public A-share quotes and history; no invented data on live errors. Explicit
  demo mode for offline use. Sector heatmaps use the complete upstream industry or
  concept classification, independently of watchlists, with equal-sized tiles and
  provider-reported average changes. Expose all sectors via paging, not a top-N.
  Sector responses lack quote timestamps: label retrieval time honestly.
- Config: system HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY; explicit HTTP/HTTPS/
  SOCKS5/SOCKS5H proxy or direct; never silently fall back from proxy to direct.
  Hide proxy credentials in errors/UI/logs. Live-editable request intervals:
  quotes 5s, chart 15s, sectors 30s defaults; timeouts, no overlapping jobs, retry
  backoff. Request sectors only while the cloud is open; keep last-good cache.
- Keyboard: m mode, h quotes/cloud, Tab industry/concept in cloud, arrows sectors,
  PgUp/PgDn sector pages, Home/End first/last; [ ] groups and arrows stocks/period
  on the quotes screen,
  / search, Enter inspect, a add, d confirm removal, g groups, Space pause display,
  comma settings, ? help/mapping, Esc close modal/search, q quit (outside inputs).
- Remember mode, group, selected stock and period. Portable --data-dir option.
- curl installer identifies OS/arch, downloads and verifies a release binary,
  installs without sudo, idempotently configures ~/.local/bin PATH in .zshrc
  (honoring ZDOTDIR) or .bashrc. Detect login/user shell, not the piped bash process;
  allow --shell override. Explain sourcing/reopening for the existing terminal.
- Manual download, explicit mode flags, manual update preserving user data.
- Versioned release workflow for Linux x86_64/aarch64 and macOS x86_64/aarch64.
  Do not claim a public installer works before release assets exist.
- Meaningful checks: price parsing, search ranking, state persistence/group actions,
  proxy/backoff scheduling, UI via TestBackend and PTY, offline installer tests.

Performance targets (not measurements): <1s local UI startup, <50ms local search,
responsive keys during network errors, 100–300 watchlist entries, low idle CPU.

Local cloud interaction revision: full mode uses a weighted binary treemap by constituent count within each page (not market capitalization); minimal mode keeps a numeric matrix. PgUp/PgDn and p/n navigate page starts with boundary feedback. Mouse supports cloud tile selection, category tabs, page controls, scroll paging and footer mode/view controls. Modals ignore background mouse input.

Minimal mode identity refinement: stock tables and selected-series labels now show market-prefixed stock codes (e.g. sz300750). Sector matrices now show Chinese names beside ordinals and signed percentage changes, with display-column-aware truncation; the selected sector detail retains the full name. Sector categories are named in both modes. The matrix uses wider columns, shared by pagination and mouse hit testing. Supersedes the previous numeric-only display requirement. Existing multi-size rendering, paging and mouse tests updated and passed.

Cloud sorting revision: default descending percentage change; s or the clickable sort label cycles gainers, losers and constituent count descending. Missing changes sort last and ties use source codes. Sorting is shared across modes and categories, persisted in user state, and applied to restored caches and network updates. Refresh preserves the selected sector by code; changing sort returns to page one. Added coverage for ordering, missing values, ties, selection retention, cache restore and mouse sorting. Gray frames have been replaced with dark gutters and an accent selection marker.

Sorted full cloud now uses ordered weighted horizontal strips, read left-to-right then top-to-bottom, replacing recursive irregular subdivisions. Tile widths and strip heights encode constituent counts within the page; rounding is to terminal cells and dark gutters are decorative. Shared mouse geometry and the 3:1 area/coverage test passed after this layout update.

Reference-style cloud revision: ordered squarified treemap replaces horizontal strips, accounts for terminal cell aspect ratio, and retains the chosen sort as allocation order/rank. Full-mode density adapts to the viewport. Larger tiles center wrapped names, percentages and constituent counts; tiny tiles remain colored without misleading truncated percentages. Dark seams replace gray frames. Hover/click details share renderer geometry and repeated hovering over one tile avoids redraws. The scope is sector-level visualization using the existing real sector snapshots, with constituent-count weights, not market caps or synthetic constituent-stock data. Both minimal and full modes remain available. Geometry tests cover 0–175 weights at normal and very small terminal sizes, checking coverage, bounds and non-overlap. Real cached data can be captured with `cargo run --example screens -- target/treemap-preview target/cloud-preview`.

User rejected the dense squarified reference-style experiment. Restored ordered weighted rows with the previous full-mode page capacity (up to three rows), muted colors, dark gutters and single-line left-aligned names. Sorting, named minimal matrices, stock codes, hover/click details, pagination and preferences remain. Removed the experimental treemap module. This supersedes the preceding reference-style layout revision.
