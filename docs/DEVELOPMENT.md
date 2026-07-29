# 开发与发布

## 环境

- macOS 14+
- Rust 1.85+，edition 2024
- Xcode Command Line Tools
- Homebrew，仅用于 Formula 验证

## 常用命令

```bash
cargo fmt --all --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run -- --help
```

前台调试 daemon：

```bash
RUST_LOG=debug cargo run -- daemon
```

另一个终端运行：

```bash
cargo run
```

调试 launchd：

```bash
cargo run -- start
cargo run -- status
cargo run
cargo run -- stop
```

## 目录

```text
src/collector/             macOS 采集
src/config.rs              配置
src/history.rs             内存历史
src/storage.rs             SQLite 持久化
src/ipc.rs                 Unix Socket 协议
src/daemon.rs              常驻服务
src/launchd.rs             launchctl 管理
src/tui.rs                 Ratatui 界面
src/main.rs                CLI
Formula/kedu.rb            Homebrew Formula 模板
scripts/package-release.sh Release 打包
.github/workflows/         CI 和 Release
```

## 修改流程

1. 阅读 `README.md`、`docs/STATUS.md`、`docs/ARCHITECTURE.md` 和本文档。
2. 保持采集、daemon、TUI 和发布模块边界。
3. 修改后运行格式化、检查、测试和 Clippy。
4. 采集口径修改必须补测试并同步架构文档。
5. CLI、配置或用户功能修改必须同步 README 和状态文档。
6. 提交前运行 `git diff --check` 和 `git status --short`。

## 测试

- 采集：累计计数、CPU 口径、`.app` 根路径、父进程归属、真实进程扫描。
- 网络：nettop 行解析、累计速率和计数回退。
- 历史：PID 压缩、按时间和数量裁剪。
- 存储：跨重开恢复、SQLite 裁剪和清空。
- 配置：默认值往返和非法采样间隔。
- launchd：plist 内容、XML 转义和状态解析。
- TUI：Ratatui `TestBackend` 密集堆叠图渲染和下采样。

## Release

本地生成当前架构压缩包：

```bash
TARGET=$(rustc -vV | awk '/^host:/{print $2}')
./scripts/package-release.sh "$TARGET" 0.1.0
```

输出：

```text
dist/kedu-0.1.0-<target>.tar.gz
dist/kedu-0.1.0-<target>.tar.gz.sha256
```

推送 `v*` 标签触发 Release 工作流，构建 ARM64 和 Intel 两个产物。标签版本必须匹配 `Cargo.toml`。

首个 Release 后更新 `Formula/kedu.rb` 的版本和两个 SHA-256，再发布到 `0x30/homebrew-tap`。

本机 Tap 维护仓库：

```text
/Users/titfer/Documents/other/homebrew-tap
```

Release 资产上传完成后，在 Tap 仓库执行：

```bash
ruby scripts/update-kedu-formula.rb 0.1.0
brew style Formula/kedu.rb
git diff --check
```

Tap 的 `Update Kedu Formula` 工作流也可以根据版本自动更新双架构 SHA-256 并创建 PR。项目内 `Formula/kedu.rb` 是发布模板，正式安装 Formula 以 Tap 仓库为准。

## 关键约束

- 进程 CPU 保持一个逻辑核心 `100%`。
- 历史必须有界，不能随 daemon 运行时间无限增长。
- 网络采集保持串行，不能同时启动多个 nettop。
- Socket 权限必须保持 `0600`。
- TUI 退出不能停止 daemon。
- `kedu stop` 只能停止刻度服务，不能操作其他进程。
- 默认只在当前用户目录持久化应用级历史，不保存诊断信息。
- 持久化关闭时必须继续支持纯内存模式。
- 不提交 `.github-secrets/` 或任何签名材料。
