# v0.1 validation

Validated locally on Linux x86_64, 2026-10-29. macOS and ARM builds are defined in
release CI but have not been run on this workstation. No release has been published.

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

Final scope exclusions: no trading, technical indicators, foreign markets, or
full-market sector indices. Heatmaps represent the user's actual watchlist group.
The curl installer and manual updater need published, checksummed release assets.
