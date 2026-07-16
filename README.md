# 刻度

轻量的 macOS 应用级资源监控器。监控数据默认只保存在内存中，退出应用即清除。

## 开发

```bash
swift build
swift run KeduMonitor
```

生成可直接双击运行的应用：

```bash
./scripts/build-app.sh
open "dist/刻度.app"
```

要求 macOS 14 或更高版本，以及 Xcode 16 或更高版本。
