# 架构与数据口径

## 总览

刻度是 SwiftPM 管理的原生 macOS SwiftUI 应用。应用启动时创建唯一的 `MonitorStore`，窗口关闭后 Store 和采集任务继续运行。`LSUIElement=true` 隐藏 Dock 图标，`MenuBarExtra` 是常驻入口。

```text
KeduMonitorApp
  └─ MonitorStore (@MainActor, @Observable)
       ├─ ProcessCollector actor ── libproc / sysctl
       ├─ NetworkCollector actor ── /usr/bin/nettop -P -L 1
       ├─ [MetricSnapshot] ─────── 内存环形时间窗口
       └─ ContentView / MenuBarExtra
            ├─ StackedMetricChart (Canvas)
            ├─ ApplicationDrawer
            ├─ ToolboxDrawer
            └─ SamplingSettingsView
```

## 采集链路

### CPU、内存和磁盘

`ProcessCollector` 使用 Darwin/libproc：

- `proc_listallpids`：枚举 PID。
- `proc_pid_rusage(RUSAGE_INFO_V4)`：累计 CPU 时间、physical footprint、累计磁盘读写字节。
- `proc_pidpath` / `proc_name`：可执行路径和进程名。
- `proc_pidinfo(PROC_PIDTBSDINFO)`：PPID、UID。
- `proc_pidinfo(PROC_PIDVNODEPATHINFO)`：当前工作目录。
- `sysctl(KERN_PROCARGS2)`：启动参数。

速率由相邻两次累计计数的差值除以实际经过时间计算。首次采样只能建立基线，因此 CPU 和磁盘速率为零；应用启动后 1 秒进行第二次采样，之后按配置间隔运行。

### 网络

`NetworkCollector` 每次执行：

```bash
/usr/bin/nettop -P -L 1 -n -x -J bytes_in,bytes_out
```

解析每个 PID 的累计收发字节，再用相邻采样差值计算下载/上传速率。网络采集失败不会中断其他指标；Store 会保留进程快照并设置 `errorMessage`。

## 应用聚合

聚合键是 `ApplicationIdentity`：

1. 从进程可执行路径提取最外层 `.app`。
2. Helper 自身位于嵌套 `.app` 时，仍使用路径中的第一个 `.app`，例如 Chrome Helper 归到 Google Chrome。
3. 无 `.app` 的子进程沿 PPID 向上查找父进程应用。
4. 仍无法归属时，以进程名聚合。

应用图标由 `NSWorkspace` 从 `bundlePath` 按需读取并缓存。

## 指标口径

### CPU

- `ProcessMetrics.cpuPercent` 与活动监视器的 `% CPU` 一致：一个逻辑核心满载为 `100%`，多线程进程可超过 `100%`。
- `ApplicationMetrics.cpuPercent` 是该应用全部 PID 的上述值之和。
- `MetricSnapshot.totalCPUPercent` 是全部可见进程 CPU 之和，可超过 `100%`。
- `MetricSnapshot.systemCPUPercent` 用逻辑核心数归一化并限制到 `0...100%`，用于顶部和菜单栏整机概览。

### 内存

使用 `ri_phys_footprint`，应用值为其 PID 之和。它是物理占用近似值，不等同于活动监视器的所有内存分类或系统内存压力。

### 磁盘和网络

内部存储单位为 bytes/s，UI 在 `MetricKind` 中转换为 MB/s。磁盘来自进程生命周期累计 I/O；网络来自 `nettop` 累计值。

## 历史数据

- 默认采样：5 秒。
- 默认保留：30 分钟。
- 数据只在内存中，`MonitorStore.snapshots` 按时间裁剪。
- `latestSnapshot` 保留当前完整快照。
- `displaySnapshots(maximumCount: 600)` 对超长窗口按固定时间桶下采样，保留每桶最后一个点。
- 历史快照保留应用和 PID 数值，支持点击图表查看历史进程详情。
- 启动命令和 cwd 不写入历史；展开 PID 时按需从当前内核读取。PID 已退出时无法读取详情。

## 图表

`StackedMetricChart` 使用 SwiftUI `Canvas`，不依赖 Swift Charts：

- 窗口累计占用最高的 7 个应用单独成系列，其余合并为“其他”。
- 自适应 Y 轴按当前窗口峰值增加约 6% 余量，并使用细粒度刻度。
- X 轴显示开始、中间、结束时间；Y 轴显示 5 个低对比度刻度。
- Tooltip 只列当前时刻前 5 个应用；应用抽屉列出全部应用。
- 图表渲染模型按 View 初始化计算，鼠标只在跨越采样点时更新选择状态。

## 遗留进程工具

“遗留进程”指当前用户进程的内核 cwd 仍有路径，但该路径已无法通过文件系统访问。常见原因是项目目录或 worktree 在进程运行期间被删除。

停止操作有以下保护：

- PID 必须大于 1。
- 不允许停止刻度自身。
- 目标 UID 必须等于当前用户 UID。
- 只发送 `SIGTERM`，不使用 `SIGKILL`。

## UI 结构

- `KeduMonitorApp.swift`：唯一主窗口、菜单栏浮层、迷你趋势图。
- `ContentView.swift`：主界面、指标栏、应用抽屉、设置和工具箱。
- `FrostedWindowBackground.swift`：`NSVisualEffectView` 磨砂窗口和 Escape 键监听。
- `StackedMetricChart.swift`：自绘图表、Tooltip、渲染模型。
