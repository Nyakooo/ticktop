# Ticktop v0.1 specification

Agreed scope: a free, personal A-share market-watching terminal application; no
orders, accounts, holdings, indicators, or foreign markets in this release.

- Rust, Ratatui, Tokio; Linux/macOS release binaries; no runtime Python dependency.
- Full mode: watchlist, clear price chart (intraday/week/month), group heatmap.
- Minimal mode: monochrome fixed numeric aliases, prices, changes, numeric chart
  samples and heatmap matrix; reveal stock/group names only during explicit search,
  group management or help. Switching mode preserves selection, group and period.
- Local persistent watchlist and named groups; add/remove/rename/delete; a stock
  can appear in multiple groups; All is the union; confirmation before removals.
- Chinese/code/pinyin/initials/partial/fuzzy local search, exact/prefix ranked first.
- Default red up / green down; quote timestamps, stale/disconnected/paused states.
- Free public A-share quotes and history; no invented data on live errors. Explicit
  demo mode for offline use. Full-market sector heatmap is deferred; first release
  uses actual quotes for the selected watchlist group, equal-sized tiles.
- Config: system HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY; explicit HTTP/HTTPS/
  SOCKS5/SOCKS5H proxy or direct; never silently fall back from proxy to direct.
  Hide proxy credentials in errors/UI/logs. Live-editable request intervals:
  quotes 5s, chart 15s defaults; timeouts, no overlapping jobs, retry backoff.
- Keyboard: m mode, h quotes/heatmap, [ ] groups, up/down stocks, left/right period,
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
