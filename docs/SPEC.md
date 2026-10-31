# Ticktop v0.1.0 specification

Ticktop is a personal, read-only terminal market dashboard. This release supports
A-share data; other markets are future work. No orders, trading accounts,
holdings management or technical indicators are included.

- Rust, Ratatui and Tokio; Linux/macOS release binaries; no runtime Rust or Python
  dependency for downloaded binaries.
- Full mode: grouped watchlist, intraday/week/month price charts, and an independent
  whole-market industry/concept sector cloud. Red means up and green means down.
- Minimal mode: monochrome stock codes, prices, changes and numeric chart samples;
  sector matrices show ordinals, Chinese names and signed changes. Long names may
  be shortened in cells; selected-sector details show the full name.
- Mode switching preserves the selected stock, group and chart period. Persist
  mode, groups, selected stock, period, sector category and sorting. Explicit
  --full/--minimal choices are remembered. Demo/live state and caches are separate.
- Local watchlists and named groups; add/remove/rename/delete; one stock may appear
  in multiple groups; All is the union. Confirm removals.
- Local Chinese/code/pinyin/initials/partial/fuzzy search, exact/prefix ranked first.
- Sector clouds use the complete upstream classification, independent of watchlists.
  Full mode uses ordered weighted rows with muted colors, dark gutters, up to three
  rows per page and single-line names. Area represents constituent counts within
  the page, not market cap. Color uses provider-reported average percentage change.
  Classifications can overlap; show retrieval time because source time is absent.
- Default sector sorting is descending percentage change; s or the clickable sort
  label cycles gainers, losers and constituent counts descending. Missing changes
  sort last, ties use source codes. Refresh preserves the selected sector by code;
  changing sort returns to page one. All sectors are reachable through paging.
- Keys: m mode, h quotes/cloud, Tab sector category, s sector sort, arrows selection,
  PgUp/PgDn or p/n cloud pages, Home/End first/last; [ ] quote groups; arrows quote
  selection/chart period; / search, Enter inspect, a add, d confirm removal,
  g groups, Space pause display, comma settings, ? help, Esc close, q quit outside inputs.
- Mouse: cloud hover/click details, scroll/page buttons, category/sort tabs and footer
  mode/view controls. Ignore background mouse actions while a modal is open.
- Free public data sources with no API key. Keep last-good data on errors and report
  stale/disconnected states. Never replace live failures with invented demo data.
- System HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY, explicit HTTP/HTTPS/SOCKS5/
  SOCKS5H proxy, or direct connection. Never silently fall back from proxy to direct.
  Hide proxy credentials in UI/errors. Ignore unused ALL_PROXY when both explicit
  HTTP and HTTPS proxies exist. Defaults: quotes 5s, chart 15s, sectors 30s; settings
  are live-editable, requests do not overlap, errors back off. Request sectors only
  while the cloud is open. Pause freezes display while requests continue.
- Portable --data-dir, atomic configuration/state writes and local SQLite cache.
  Terminal raw mode, alternate screen and mouse reporting are restored on exit,
  initialization failure and panic unwinding.
- Display brand Ticktop; repository, Rust package and executable ticktop. README
  leads with a restrained poster, direct-download table, installer and basic keys.
  Detailed configuration and source-development instructions belong in GUIDE.md.
- Public v0.1.0 tag and Release with Linux x86_64/ARM64 and macOS Intel/Apple Silicon
  archives and SHA-256 files. Do not claim installation ready until assets exist.
- curl installer detects OS/arch, verifies checksums, installs without sudo, and
  idempotently adds ~/.local/bin to .zshrc (honoring ZDOTDIR) or .bashrc. Detect the
  user shell rather than piped bash; support --shell override. Explain reopening
  the terminal. Manual download and ticktop update preserve user data.
- Validation: price/search/state/group behavior, proxies/scheduling, multi-size
  TestBackend rendering, real PTY input/exit/resize/mouse, abrupt-restart mode
  persistence, offline installer tests and anonymous post-release downloads.

Performance targets (not measurements): <1s local UI startup, <50ms local search,
responsive keys during network errors, 100–300 watchlist entries and low idle CPU.
