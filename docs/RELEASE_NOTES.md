# Ticktop v0.3.1

补充看盘信息，优化日内走势时间轴。

## 更新内容

- 顶部常驻两市 A 股成交额，完整、简约及云图模式均可见。汇总沪市 A 股与深市 A 股全市场口径（不含北交所、B 股），显示行情源时间；过期行情标注“快照”。暂无可靠免费全天预测数据时显示“预计暂无”。
- 个股详情增加量比、换手率、成交量，按此顺序排列。两种模式均支持，缺失字段显示 `--`。
- A 股日内分时图固定展示 09:30—15:00，午休压缩为 11:30/13:00；盘中未发生的时段留空。恒生指数固定展示至 16:00，周/月视图保持不变。
- 同步调整新增常驻行后的鼠标点击位置与窄窗口布局。

## 安装与更新

Linux / macOS / Git Bash：

```bash
curl -fsSL https://raw.githubusercontent.com/Nyakooo/ticktop/master/scripts/install.sh | bash
```

Windows 10/11 x64（PowerShell 5.1+）：

```powershell
irm https://raw.githubusercontent.com/Nyakooo/ticktop/master/scripts/install.ps1 | iex
```

已安装用户运行 `ticktop update`，退出旧程序后重新启动。更新保留自选、分组与个人设置。

提供 Windows x64、Linux x86_64 / ARM64、macOS Intel / Apple Silicon 安装包及各自的 SHA-256 校验文件。

免费行情可能延迟，休市显示最后快照；筹码峰为估算，其免费日线源可能不可用。
