# Kedu AI 开发指南

## 先读

开始修改前依次阅读：

1. `README.md`：用户功能和安装。
2. `docs/STATUS.md`：当前已完成、限制和待选方向。
3. `docs/ARCHITECTURE.md`：采集、聚合、CPU 口径和历史模型。
4. `docs/DEVELOPMENT.md`：命令、测试、提交和发布约束。

## 项目一句话

刻度是 macOS 14+ 的纯原生 SwiftUI 菜单栏资源监控器，用 libproc/sysctl/nettop 采集进程指标，数据只保存在内存中。

## 快速项目地图

- `KeduMonitorApp.swift`：App 生命周期、唯一窗口、菜单栏浮层。
- `MonitorStore.swift`：采集循环、保留窗口、下采样、UI 状态入口。
- `ProcessCollector.swift`：CPU/内存/磁盘、进程归属、参数/cwd、遗留进程停止。
- `NetworkCollector.swift`：nettop 单帧采集和网络速率。
- `Models.swift`：快照、应用、PID、网络和工具模型。
- `MetricSelection.swift`：指标选择、单位和值提取。
- `StackedMetricChart.swift`：Canvas 堆叠图和 Tooltip。
- `ContentView.swift`：主布局、抽屉、设置、工具箱。当前偏大，新增复杂功能优先拆文件。
- `FrostedWindowBackground.swift`：窗口磨砂和 Escape 监听。
- `SessionExporter.swift`：CSV。
- `ApplicationIconView.swift`：应用图标缓存和颜色。

## 不要破坏的行为

- 应用启动立即采集；窗口关闭后继续采集。
- 只有一个主窗口，菜单栏点击不能创建多个窗口。
- 无 Dock 图标。
- 默认每 5 秒采样，保留 30 分钟，默认不落盘。
- 进程 `% CPU`：一个逻辑核心为 100%，必须匹配活动监视器。
- 顶部整机 CPU：进程 CPU 合计除以逻辑核心数，限制 0...100%。
- Chrome/Electron Helper 归属最外层应用。
- 历史图最多绘制 600 点，超长历史按固定时间桶下采样。
- Tooltip 鼠标在同一采样点内移动时不更新状态。
- Escape 关闭当前抽屉或设置。
- 进程停止只允许当前用户、PID > 1、非自身，并发送 SIGTERM。

## 性能规则

- 不要在 SwiftUI `body` 或 Canvas 每个路径点重复扫描全部快照。
- 不要在每个采样周期读取 `KERN_PROCARGS2` 或 cwd；只在用户展开详情或打开工具时读取。
- 图表系列保持前 7 个应用 + “其他”，不要为全部应用创建独立图元。
- 新增历史字段前评估 5 秒 × 3 小时的内存量。
- 网络采集必须在 actor 内串行，避免多个 nettop 同时运行。

## 安全规则

- 不允许提交 `.github-secrets/`、P12、密码或私钥。
- 不扩大进程终止权限；工具箱默认只操作当前用户进程。
- 不把启动命令、路径等诊断数据自动上传或持久化。
- 修改命令显示时保持 shell quoting，避免复制出的命令语义变化。

## 验证清单

每次代码修改至少执行：

```bash
swift build
swift test
git diff --check
```

UI、Info.plist、图标、打包或 Release 修改还执行：

```bash
./scripts/package-release.sh
cd dist
shasum -a 256 -c KeduMonitor-macOS.zip.sha256
codesign --verify --deep --strict --verbose=2 "刻度.app"
```

运行新包：

```bash
pkill -x KeduMonitor || true
open "dist/刻度.app"
```

## 提交和推送

- 用户要求每个独立功能单独提交。
- 提交前检查 `git status`、`git diff`、`git log --oneline -10`。
- 只 stage 本功能文件，不混入本地 Secret 或生成目录。
- 推送 `main` 会自动创建 GitHub Release；推送前确认测试和打包通过。

## 文档同步规则

这些变化必须同步文档：

- 新增/删除用户功能：更新 `README.md` 和 `docs/STATUS.md`。
- 修改采集来源、指标公式、聚合方式：更新 `docs/ARCHITECTURE.md` 和测试。
- 修改环境、命令、构建、CI 或 Secrets：更新 `docs/DEVELOPMENT.md`。
- 修改关键不变量或开发流程：更新本 `AGENTS.md`。
