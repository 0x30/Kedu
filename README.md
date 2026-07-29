# 刻度

刻度是面向 macOS 14+ 的应用级终端资源监控器。后台服务持续采集 CPU、内存、磁盘和网络数据，`kedu` 随时连接服务并用彩色堆叠图展示历史。

历史默认保存在当前用户的本地 SQLite 数据库中。退出 TUI 或重启服务不会丢失保留窗口内的数据，旧数据仍按配置自动裁剪。

## 功能

- 使用 `launchd` 作为当前用户的常驻服务，异常退出后自动拉起。
- CPU、内存、磁盘和网络按 PID 采集并按应用聚合。
- Chrome、Electron 等 Helper 归属最外层 `.app`。
- 进程 CPU 口径与活动监视器一致：一个逻辑核心为 `100%`。
- Ratatui 彩色堆叠面积图，保持前 7 个应用 + “其他”。
- 支持键盘选择指标、历史时刻、应用和 PID。
- 支持鼠标悬浮历史、点击应用和滚轮选择。
- TOML 配置采样频率、保留时长、指标和显示参数。
- SQLite 持久化有界历史，可通过配置关闭。
- 无进程终止、命令读取或工作目录诊断功能，只做只读监控。

## 安装

首个 Release 发布后可通过项目 Tap 安装：

```bash
brew tap 0x30/tap
brew install kedu
```

当前源码构建：

```bash
cargo install --path .
```

## 使用

```bash
kedu start          # 启动常驻监控服务
kedu                # 打开 TUI
kedu status         # 查看服务状态
kedu restart        # 配置修改后重启
kedu stop           # 停止服务
kedu data path      # 查看历史数据库位置
kedu data clear     # 服务停止后清空历史
```

首次 `kedu start` 会创建：

```text
~/.config/kedu/config.toml
```

也可以手动管理：

```bash
kedu config init
kedu config check
kedu config path
```

## 默认配置

```toml
version = 1

[sampling]
interval = "5s"
retention = "30m"
maximum_samples = 21600

[storage]
enabled = true

[metrics]
cpu = true
memory = true
disk = true
network = true

[display]
top_applications = 7
maximum_chart_points = 600
mouse = true
color = "auto"

[daemon]
log_level = "warn"
```

配置修改后运行 `kedu restart`。采样间隔不能小于 1 秒。

历史数据库默认位于：

```text
~/Library/Application Support/Kedu/history.sqlite3
```

数据库只保存应用级指标历史，不保存启动命令、工作目录等诊断信息。设置 `storage.enabled = false` 可恢复为纯内存模式。

## TUI 操作

```text
Tab       切换 CPU / 内存 / 磁盘 / 网络
d         切换读取/写入或下载/上传
← →       选择历史采样点
↑ ↓       选择应用
鼠标移动   查看历史位置
鼠标点击   选择应用
滚轮       选择应用
q / Esc   退出 TUI，后台继续采集
```

## 开发

```bash
cargo fmt --all --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

更多信息：

- [架构与指标口径](docs/ARCHITECTURE.md)
- [开发与发布](docs/DEVELOPMENT.md)
- [当前状态](docs/STATUS.md)
