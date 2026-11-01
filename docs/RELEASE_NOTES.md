# Ticktop v0.3.0

新增 Windows 原生支持。终端行情看板，只看盘，不交易。

## Windows 安装

支持 Windows 10/11 x64（Intel / AMD），无需 Rust、Git 或管理员权限。在 PowerShell 5.1+ 中执行：

```powershell
irm https://raw.githubusercontent.com/Nyakooo/ticktop/master/scripts/install.ps1 | iex
```

- 安装到 `%LOCALAPPDATA%\Ticktop\bin`，校验 ZIP 的 SHA-256 后安装，并自动加入用户 PATH；重复安装不会重复添加。
- Git Bash 支持原有 `curl ... | bash` 安装命令，自动选择 Windows 原生版；MinTTY 中用 `winpty ticktop`，Windows Terminal / PowerShell 直接运行 `ticktop`。
- Windows `ticktop update` 使用内置 PowerShell 安装器，不依赖 Bash。支持运行中替换 `.exe`，失败保留旧程序，升级保留个人设置与自选。
- Windows 安装包使用静态 MSVC CRT，无需另外安装 Visual C++ Redistributable。暂不提供 Windows ARM64 原生包。

## 下载与更新

| 平台 | 安装文件 |
| --- | --- |
| Windows x64 | `ticktop-x86_64-pc-windows-msvc.zip` |
| Linux x86_64 / ARM64 | 对应 `.tar.gz` |
| macOS Intel / Apple Silicon | 对应 `.tar.gz` |

每份安装包均有 `.sha256` 校验文件。Windows 解压后运行 `.\ticktop.exe`；Linux / macOS 运行 `./ticktop`。

Linux / macOS 一键安装：

```bash
curl -fsSL https://raw.githubusercontent.com/Nyakooo/ticktop/master/scripts/install.sh | bash
```

已安装用户运行 `ticktop update`。其他已打开的终端需重启以刷新 PATH。

保留 v0.2 的 ETF、指数、自选分组、常驻指数栏、云图与两种显示模式。`/` 搜索、`i` 设置常驻指数、`m` 切模式、`h` 看云图、`?` 帮助。

免费行情可能延迟，休市显示最后快照；筹码峰是估算值，其免费日线源可能不可用。指数不适用筹码分布，ETF 暂不提供筹码估算。
