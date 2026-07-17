# 开发与发布

## 环境

- macOS 14+
- Xcode 26 或兼容的 Swift 6.2 工具链
- Swift 6.2
- Swift Package Manager

项目没有第三方运行时依赖，也没有 Xcode 工程文件。

## 常用命令

```bash
# 编译 Debug
swift build

# 运行全部测试
swift test

# 直接运行 Debug 可执行文件
swift run KeduMonitor

# 生成 Release .app、ZIP 和 SHA-256
./scripts/package-release.sh

# 打开打包后的应用
open "dist/刻度.app"
```

重启当前本地构建：

```bash
pkill -x KeduMonitor || true
open "dist/刻度.app"
```

## 目录

```text
Assets/                      图标母版、iconset、icns
Sources/KeduMonitor/         应用源码
Tests/KeduMonitorTests/      Swift Testing 测试
docs/                        架构、开发、状态文档
scripts/build-app.sh         组装并签名 .app
scripts/package-release.sh   生成 ZIP 与 SHA-256
scripts/generate-signing-secrets.sh
.github/workflows/ci.yml     PR / 手动测试
.github/workflows/release.yml main 推送自动 Release
Info.plist                   Bundle 元数据模板
Package.swift                SwiftPM 清单
```

## 修改流程

本仓库约定每个独立功能单独提交：

1. 先阅读相关源文件和测试。
2. 保持改动范围最小，避免把 UI、采集和发布修改混入一个提交。
3. 使用 `apply_patch` 编辑代码。
4. 至少运行 `swift build && swift test`。
5. UI 或打包变更还需运行 `./scripts/package-release.sh`。
6. 验证 ZIP 和签名：

```bash
cd dist
shasum -a 256 -c KeduMonitor-macOS.zip.sha256
codesign --verify --deep --strict --verbose=2 "刻度.app"
```

7. 检查 `git diff --check` 和 `git status --short` 后提交。

## 测试覆盖

- `ProcessCollectorTests`：应用根路径、CPU 口径、参数解析、进程扫描、已删除 cwd 检测。
- `NetworkCollectorTests`：`nettop` 解析、速率差值、真实单帧采集。
- `MonitorStoreTests`：历史保留和下采样。
- `ChartRenderDataTests`：360 点 × 120 应用的密集图表模型。
- `MetricSelectionTests`：单位转换。
- `SessionExporterTests`：CSV 字段和转义。

真实系统测试会短暂启动 `sleep` 或 `nettop`，测试结束负责清理。

## 打包和版本

`scripts/build-app.sh [debug|release]`：

- 版本默认是 `0.1.<git commit count>`。
- Build 默认是 `dev.<short hash>`，工作区脏时追加 `+`。
- CI 可通过 `KEDU_VERSION`、`KEDU_BUILD` 注入版本。
- `KEDU_SIGN_ID` 为空时使用 ad-hoc 签名。

`scripts/package-release.sh` 输出：

```text
dist/刻度.app
dist/KeduMonitor-macOS.zip
dist/KeduMonitor-macOS.zip.sha256
```

## GitHub Actions

- PR 和手动触发：`.github/workflows/ci.yml` 运行 `swift test`。
- 推送 `main`：`.github/workflows/release.yml` 测试、签名、打包并创建 `build-<run_number>` Release。
- Release 更新说明自动汇总上一个 `build-*` 标签以来的提交。
- Release 上传 ZIP 和 SHA-256。

签名 Secrets：`KEDU_CERT_P12`、`KEDU_CERT_PWD`。本地生成值位于被忽略的 `.github-secrets/`，不得提交。

## 约束

- 保持菜单栏应用模式；不要移除 `LSUIElement`，除非明确决定恢复 Dock 图标。
- 进程 CPU 口径必须继续匹配活动监视器，不能再除以核心数写回进程值。
- 顶部整机 CPU 与进程 CPU 是不同口径，修改时同步更新文档和测试。
- 采集数据默认不落盘。新增持久化必须先确认隐私、清理和升级策略。
- 不要在每次采样读取完整启动参数或 cwd；这些信息按需读取，以控制开销。
- 不要让 Tooltip 的鼠标像素移动触发整图重算。
- 网络采集依赖系统 `nettop` 输出格式；修改解析器时保留样例测试和真实采集测试。
