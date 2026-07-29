# 架构与指标口径

## 总览

刻度由一个 Rust 二进制提供三种运行方式：

```text
launchd
  └─ kedu daemon
       ├─ ProcessCollector ── libproc
       ├─ NetworkCollector ── /usr/bin/nettop
       ├─ HistoryStore ────── 有界内存历史
       ├─ HistoryDatabase ─── SQLite 持久化
       └─ Unix Socket ─────── 0600
                    ↑
                kedu TUI
```

普通执行 `kedu` 启动 Ratatui 客户端。客户端退出不影响 daemon。`kedu start` 安装用户级 LaunchAgent，`kedu stop` bootout 服务并删除 plist。

## 模块

- `src/collector/`：macOS 进程和网络采集、应用归属、速率计算。
- `src/config.rs`：TOML 配置、默认值和校验。
- `src/history.rs`：按保留时长和最大采样数裁剪历史。
- `src/storage.rs`：SQLite 历史恢复、写入和裁剪。
- `src/ipc.rs`：JSON Lines IPC 协议和客户端订阅。
- `src/daemon.rs`：采集循环、Socket 服务和广播。
- `src/launchd.rs`：LaunchAgent 创建及 `launchctl` 管理。
- `src/tui.rs`：Ratatui 布局、堆叠图、键盘和鼠标。
- `src/main.rs`：Clap 命令入口。

## 采集

### CPU、内存和磁盘

`ProcessCollector` 直接调用 macOS libproc：

- `proc_listallpids` 枚举 PID。
- `proc_pid_rusage(RUSAGE_INFO_V4)` 获取累计 CPU 时间、physical footprint、累计磁盘读写。
- `proc_pidpath` 和 `proc_name` 获取路径和名称。
- `proc_pidinfo(PROC_PIDTBSDINFO)` 获取 PPID 和 UID。

CPU 和磁盘速率由相邻累计计数差值除以实际时间。PID 启动时间变化时视为 PID 复用并重新建立基线。首次采样只建立基线，daemon 在 1 秒后执行第二次采样，随后使用配置间隔。

### 网络

每次网络采样串行执行：

```bash
/usr/bin/nettop -P -L 1 -n -x -J bytes_in,bytes_out
```

累计收发字节按 PID 求差后聚合到应用。网络失败只记录日志，CPU、内存和磁盘仍正常发布。

## 应用归属

1. 从可执行路径提取第一个、即最外层 `.app`。
2. 无 `.app` 的进程沿 PPID 向上查找父进程应用。
3. 无法归属时按进程名称聚合。
4. 父进程遍历使用 visited 集合避免异常进程树循环。

## 指标口径

- 进程 CPU：一个逻辑核心满载为 `100%`，多线程进程可超过 `100%`。
- 应用 CPU：该应用全部 PID CPU 之和。
- 顶部 CPU：进程 CPU 合计除以逻辑核心数并限制在 `0...100%`。
- 内存：`ri_phys_footprint`，不是系统内存压力。
- 磁盘和网络：内部单位 bytes/s，TUI 自适应显示 KiB/s 或 MiB/s。

## 历史

daemon 保存两类状态：

- `latest`：完整应用与 PID 快照。
- `snapshots`：移除 PID 列表后的应用汇总历史。

历史同时受 `retention` 和 `maximum_samples` 限制。TUI 再按图表宽度下采样，每列保留对应时间桶最后一个点。

默认启用 SQLite 持久化，文件位于 `~/Library/Application Support/Kedu/history.sqlite3`。数据库使用 WAL 和 `synchronous=NORMAL`，每次采样写入应用级压缩快照，并按时间与样本数删除旧记录。daemon 启动时先恢复数据库历史，再继续采集。关闭持久化后仅使用内存历史。

## IPC

Socket 位于 `~/Library/Application Support/Kedu/kedu.sock`，权限为 `0600`。协议为换行分隔 JSON：

- 客户端发送 `subscribe`。
- 服务返回完整 `state`，包含历史和最新快照。
- 后续发送增量 `snapshot`。
- 客户端落后时重新发送完整状态。

## 堆叠图

图表先按窗口内累计值选择前 N 个应用，其余合并为“其他”。每个终端格拆成上下两个子像素：

- `█`：上下同色。
- `▀`：前景色表示上半格，背景色表示下半格。
- `▄`：只绘制下半格。

因此终端 20 行图表具有约 40 层纵向分辨率。鼠标坐标映射到采样索引，只有跨越采样点时才更新历史选择。键盘或鼠标选择历史后，图表绘制高对比竖向游标，右侧应用列表读取同一快照并在标题显示采样时间。历史快照只包含应用汇总，因此 PID 面板会明确提示历史不保存 PID；回到最新时刻后重新显示当前 PID。

颜色模式默认是 `auto`，会覆盖 shell 的 `NO_COLOR`，避免图形化监控退化为不可区分的单色堆叠。`truecolor` 使用 24 位 RGB，`ansi256` 使用索引色；需要单色时必须显式配置 `none`。

## 服务生命周期

LaunchAgent label 为 `io.github.0x30.kedu`，启用 `RunAtLoad` 和 `KeepAlive`。日志写入 `~/Library/Logs/Kedu/`。停止服务后删除 plist，确保下次登录不会自动恢复；SQLite 历史不会因停止服务而删除。
