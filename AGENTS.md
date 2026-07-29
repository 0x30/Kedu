# Kedu AI 开发指南

## 协作

1. 中文回答用户。
2. 大型任务拆分多个子会话，主会话保持精炼上下文。

## 开始修改前

依次阅读：

1. `README.md`
2. `docs/STATUS.md`
3. `docs/ARCHITECTURE.md`
4. `docs/DEVELOPMENT.md`

## 项目一句话

刻度是 macOS 14+ 的 Rust/Ratatui 应用级资源监控器，由 launchd daemon 常驻采集，TUI 通过 Unix Socket 查看有界内存历史。

## 项目地图

- `src/main.rs`：Clap 命令和运行模式。
- `src/collector/`：libproc、nettop、应用归属和指标模型。
- `src/config.rs`：TOML 配置。
- `src/history.rs`：有界内存历史。
- `src/storage.rs`：SQLite 持久化历史。
- `src/ipc.rs`：JSON Lines Unix Socket 协议。
- `src/daemon.rs`：采集循环和客户端广播。
- `src/launchd.rs`：LaunchAgent 生命周期。
- `src/tui.rs`：Ratatui 堆叠图、应用/PID 和输入处理。
- `Formula/kedu.rb`：Homebrew Formula 模板。

## 不要破坏的行为

- `kedu start` 后终端关闭仍继续采集。
- `kedu` 退出不能停止 daemon。
- `kedu stop` 只停止刻度自身并移除 LaunchAgent。
- 默认每 5 秒采样、保留 30 分钟，应用级历史写入当前用户 SQLite。
- 进程 CPU 一个逻辑核心为 `100%`。
- 顶部 CPU 按逻辑核心数归一化到 `0...100%`。
- Chrome/Electron Helper 归属最外层 `.app`。
- 网络采集串行运行。
- 历史同时受保留时间和最大样本数限制。
- daemon 重启后恢复 SQLite 历史；关闭存储时保持纯内存模式。
- 图表保持前 N 个应用 + “其他”。
- `display.color = auto` 必须显示彩色系列，即使环境设置了 `NO_COLOR`。
- Socket 权限为 `0600`。
- 工具不读取启动命令/cwd，不终止其他进程。

## 验证

每次代码修改至少运行：

```bash
cargo fmt --all --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
git diff --check
```

发布修改还运行：

```bash
TARGET=$(rustc -vV | awk '/^host:/{print $2}')
./scripts/package-release.sh "$TARGET" 0.1.0
shasum -a 256 -c "dist/kedu-0.1.0-$TARGET.tar.gz.sha256"
brew style Formula/kedu.rb
```

## 安全

- 不提交 `.github-secrets/`、证书、密码或私钥。
- 不增加终止其他进程的能力。
- 只持久化应用级监控历史，不保存命令、cwd 或其他诊断信息。
- 不放宽 Socket 文件权限。
- launchd plist 必须使用明确的可执行文件路径，不依赖 shell 展开。
- Release 由 main 推送自动创建版本标签；不要手工创建发布标签。

## 文档同步

- 用户功能或 CLI：更新 `README.md` 和 `docs/STATUS.md`。
- 指标口径、采集、聚合、IPC：更新 `docs/ARCHITECTURE.md` 和测试。
- 构建、CI、Release、Homebrew：更新 `docs/DEVELOPMENT.md`。
- 关键不变量或流程：更新本文件。
