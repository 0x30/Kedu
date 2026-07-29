# 当前项目状态

最后同步：2026-07-29

## 已完成

- 从 SwiftUI 菜单栏应用完全重写为 Rust 2024 + Ratatui TUI。
- 单一 `kedu` 二进制，包含 CLI、隐藏 daemon 和 TUI 客户端。
- `kedu start/stop/restart/status` 用户级 launchd 服务管理。
- TOML 配置初始化、校验和路径查询。
- Unix Socket IPC，权限限制为当前用户 `0600`。
- daemon 内存环形历史，旧快照只保留应用汇总，最新快照保留 PID。
- SQLite 应用级历史持久化，服务重启后恢复并自动裁剪。
- macOS libproc CPU、physical footprint、磁盘累计计数采集。
- nettop 单帧网络采集和应用/PID 速率聚合。
- 最外层 `.app` 和父进程应用归属。
- Ratatui 半格彩色堆叠图、逐采样滚动视口、稳定应用颜色、历史位置游标和随时间同步的应用面板。
- ARM64/Intel Release 压缩包、SHA-256、GitHub Actions 和 Formula 模板。
- main 推送后由 CI 自动创建版本标签和 Release，无需手工推送标签。
- 本地 `homebrew-tap` 仓库已加入 Kedu Formula、校验值更新脚本和自动更新 PR 工作流。

## 验证基线

- `cargo check --all-targets` 通过。
- `cargo test --all-targets`：41 项通过。
- `cargo clippy --all-targets -- -D warnings` 通过。
- 图表测试使用 Ratatui `TestBackend` 渲染 360 个采样点。
- 采集测试包含真实 libproc 进程扫描。

## 当前限制

- 仅支持 macOS 14+。
- 网络依赖 `/usr/bin/nettop`，系统输出格式变化可能影响解析。
- 默认只保留配置窗口内历史，不是无限期归档。
- 历史快照不保留 PID 列表，PID 面板只展示当前时刻。
- 终端图表精度受字符单元尺寸限制。
- 鼠标功能取决于终端是否支持 mouse reporting。
- `display.color = auto` 会主动覆盖 `NO_COLOR`；可用 `none` 显式关闭颜色。
- Homebrew Formula 的校验值需要在首个正式 Release 后更新。

## 后续方向

- 增加多层时间桶，让 24 小时以上历史保持较低内存占用。
- 增加 daemon 配置热重载。
- 补充 Unix Socket 集成测试和 PTY 端到端测试。
- 为图表增加可见系列开关和时间缩放。
- 首个 Release 后运行 Tap 更新脚本，替换 Formula 校验值并推送 Tap 首次提交。
