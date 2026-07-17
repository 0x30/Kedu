# 当前项目状态

最后同步：2026-07-17

## 已完成

- 原生 SwiftUI 菜单栏应用，无 Dock 图标。
- 唯一主窗口，关闭后仍持续采集。
- CPU、内存、磁盘、网络按 PID 采集并按应用聚合。
- 进程 CPU 与活动监视器口径一致。
- 默认 5 秒采样、30 分钟内存保留，可配置。
- 自绘堆叠面积图、自适应纵轴、低对比度 X/Y 轴。
- 历史时刻点击和应用/PID 抽屉。
- PID 启动命令、cwd、可执行路径按需查看。
- 菜单栏四类趋势图。
- 工具箱网格和“遗留进程”清理工具。
- CSV 导出。
- 自定义应用图标。
- Release ZIP、SHA-256、GitHub Actions 自动更新说明。

## 当前验证基线

- 测试：16 项，6 个 Suite。
- 密集图表测试：360 个采样点、每点 120 个应用，渲染模型目标小于 1 秒。
- 打包产物：`dist/KeduMonitor-macOS.zip`。
- 仓库：`git@github.com:0x30/Kedu.git`。
- 分支：`main`。

## 已知限制

- 未做 Apple notarization；自签或 ad-hoc 构建首次打开可能需要清除 quarantine。
- 网络依赖 `/usr/bin/nettop`，系统格式变化可能影响解析。
- 无权限读取的系统进程可能缺少路径、参数或 cwd。
- 历史 PID 已退出后，历史指标仍在，但无法再读取启动命令。
- `systemCPUPercent` 是进程 CPU 合计除以逻辑核心数的近似整机负载，不包含无法读取的进程或内核全部调度细节。
- 内存使用 physical footprint 合计，不等同于内存压力。
- 工具箱停止操作只发送 `SIGTERM`，不会强制杀死拒绝退出的进程。

## 近期可选方向

- 将 `ContentView.swift` 拆分为 Dashboard、ApplicationDrawer、Toolbox 三个文件，降低单文件复杂度。
- 为工具箱建立独立工具协议/模型，减少新增工具时修改主视图。
- 增加应用过滤、固定系列或隐藏系列交互。
- 增加网络/磁盘采集失败的诊断详情。
- 若需要公开分发，接入 Developer ID、notarization 和 stapling。
- 增加 UI 自动化或快照测试；目前主要依赖构建、逻辑测试和人工运行验证。

## Git 状态说明

本地开发可能领先 `origin/main`。用 `git status -sb` 查看实时状态；推送 `main` 会触发 Release 工作流，推送前先确认 `.github-secrets/` 仍被忽略。
