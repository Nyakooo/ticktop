# Ticktop v0.1.0

Ticktop 是一款轻量的终端行情看板。当前版本接入 A 股，后续逐步扩展其他市场。仅用于看盘，不提供交易功能。

- 自选股票与自选分组，支持中文、代码、拼音和模糊搜索。
- 完整 / 简约两种模式，自动恢复上次选择；简约列表显示股票代码和板块名称。
- 全市场行业 / 概念板块云图，支持涨幅、跌幅和规模排序，以及键盘翻页、鼠标悬停和点击。
- 免费公开行情源，无需行情账号或 API Key；支持 HTTP / SOCKS 代理和自定义刷新间隔。

## 下载

在 Assets 中选择对应平台的 `.tar.gz` 文件，解压后运行 `./ticktop`。每份压缩包都附有 `.sha256` 校验文件。

| 系统 | 架构 |
| --- | --- |
| Linux | x86_64、ARM64 |
| macOS | Intel、Apple Silicon |

也可以一键安装：

```bash
curl -fsSL https://raw.githubusercontent.com/Nyakooo/ticktop/master/scripts/install.sh | bash
```

安装后按提示重新打开终端，运行 `ticktop`。按 `a` 添加自选，`m` 切模式，`h` 打开云图，`?` 查看帮助。

免费行情可能延迟；休市时显示最后可用数据。板块面积按成分股数量分配，并非市值占比。
