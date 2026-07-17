import SwiftUI

@main
@MainActor
struct KeduMonitorApp: App {
    @State private var store: MonitorStore

    init() {
        let store = MonitorStore()
        store.start()
        _store = State(initialValue: store)
    }

    var body: some Scene {
        WindowGroup("刻度", id: "main") {
            ContentView()
                .environment(store)
                .frame(minWidth: 720, minHeight: 460)
        }
        .defaultSize(width: 900, height: 560)
        .windowResizability(.contentMinSize)

        MenuBarExtra("刻度", systemImage: "gauge.with.dots.needle.67percent") {
            MenuBarLauncher()
                .environment(store)
        }
        .menuBarExtraStyle(.window)
    }
}

private struct MenuBarLauncher: View {
    @Environment(\.openWindow) private var openWindow
    @Environment(MonitorStore.self) private var store
    @State private var showsSettings = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                MenuMetricValue(title: "CPU", value: cpuValue, color: .cyan)
                MenuMetricValue(title: "内存", value: memoryValue, color: .orange)
                Spacer(minLength: 0)
                Button {
                    showsSettings.toggle()
                } label: {
                    Image(systemName: "slider.horizontal.3")
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("设置")
                .popover(isPresented: $showsSettings, arrowEdge: .bottom) {
                    SamplingSettingsView()
                        .environment(store)
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 44)

            Divider()

            Button(action: openMainWindow) {
                VStack(spacing: 7) {
                    MenuTrendRow(
                        symbol: "cpu",
                        series: [MenuTrendSeries(values: recentSnapshots.map(\.totalCPUPercent), color: .cyan)],
                        maximum: cpuMaximum,
                        help: "CPU"
                    )
                    MenuTrendRow(
                        symbol: "memorychip",
                        series: [MenuTrendSeries(
                            values: recentSnapshots.map { Double($0.totalMemoryBytes) },
                            color: .orange
                        )],
                        maximum: Double(ProcessInfo.processInfo.physicalMemory),
                        help: "内存"
                    )
                    MenuTrendRow(
                        symbol: "internaldrive",
                        series: [
                            MenuTrendSeries(values: recentSnapshots.map(\.totalDiskReadBytesPerSecond), color: .cyan),
                            MenuTrendSeries(values: recentSnapshots.map(\.totalDiskWriteBytesPerSecond), color: .orange),
                        ],
                        maximum: diskMaximum,
                        help: "磁盘：青色读取，橙色写入",
                        showsDirections: true
                    )
                    MenuTrendRow(
                        symbol: "network",
                        series: [
                            MenuTrendSeries(values: recentSnapshots.map(\.totalNetworkDownloadBytesPerSecond), color: .cyan),
                            MenuTrendSeries(values: recentSnapshots.map(\.totalNetworkUploadBytesPerSecond), color: .orange),
                        ],
                        maximum: networkMaximum,
                        help: "网络：青色下载，橙色上传",
                        showsDirections: true
                    )
                }
                .padding(9)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("打开刻度")
        }
        .frame(width: 250)
    }

    private var recentSnapshots: [MetricSnapshot] {
        Array(store.displaySnapshots(maximumCount: 48).suffix(48))
    }

    private var cpuValue: String {
        String(format: "%.1f%%", store.latestSnapshot?.totalCPUPercent ?? 0)
    }

    private var memoryValue: String {
        let gigabytes = Double(store.latestSnapshot?.totalMemoryBytes ?? 0) / 1_073_741_824
        return String(format: "%.1fG", gigabytes)
    }

    private var cpuMaximum: Double {
        let observed = recentSnapshots.map(\.totalCPUPercent).max() ?? 0
        return max(5, min(100, ceil(observed / 5) * 5))
    }

    private var diskMaximum: Double {
        maximum(
            recentSnapshots.map(\.totalDiskReadBytesPerSecond),
            recentSnapshots.map(\.totalDiskWriteBytesPerSecond)
        )
    }

    private var networkMaximum: Double {
        maximum(
            recentSnapshots.map(\.totalNetworkDownloadBytesPerSecond),
            recentSnapshots.map(\.totalNetworkUploadBytesPerSecond)
        )
    }

    private func maximum(_ first: [Double], _ second: [Double]) -> Double {
        max(1, max(first.max() ?? 0, second.max() ?? 0) * 1.12)
    }

    private func openMainWindow() {
        openWindow(id: "main")
        NSApp.activate(ignoringOtherApps: true)
    }
}

private struct MenuMetricValue: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(color).frame(width: 5, height: 5)
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(.caption, design: .monospaced, weight: .semibold))
        }
        .fixedSize()
    }
}

private struct MenuTrendSeries {
    let values: [Double]
    let color: Color
}

private struct MenuTrendRow: View {
    let symbol: String
    let series: [MenuTrendSeries]
    let maximum: Double
    let help: String
    var showsDirections = false

    var body: some View {
        HStack(spacing: 7) {
            VStack(spacing: 2) {
                Image(systemName: symbol)
                    .font(.system(size: 10))
                if showsDirections {
                    HStack(spacing: 2) {
                        Image(systemName: "arrow.down")
                            .foregroundStyle(Color.cyan)
                        Image(systemName: "arrow.up")
                            .foregroundStyle(Color.orange)
                    }
                    .font(.system(size: 7, weight: .semibold))
                }
            }
            .foregroundStyle(.secondary)
            .frame(width: 24)

            Sparkline(series: series, maximum: maximum)
                .frame(height: 31)
        }
        .help(help)
    }
}

private struct Sparkline: View {
    let series: [MenuTrendSeries]
    let maximum: Double

    var body: some View {
        Canvas { context, size in
            guard maximum > 0 else {
                return
            }
            for item in series where item.values.count > 1 {
                let denominator = CGFloat(item.values.count - 1)
                var path = Path()
                for (index, value) in item.values.enumerated() {
                    let point = CGPoint(
                        x: size.width * CGFloat(index) / denominator,
                        y: size.height * (1 - CGFloat(min(1, max(0, value / maximum))))
                    )
                    index == 0 ? path.move(to: point) : path.addLine(to: point)
                }
                context.stroke(
                    path,
                    with: .color(item.color),
                    style: StrokeStyle(lineWidth: 1.25, lineCap: .round, lineJoin: .round)
                )
            }
        }
        .background {
            RoundedRectangle(cornerRadius: 3)
                .fill(Color.primary.opacity(0.035))
        }
    }
}
