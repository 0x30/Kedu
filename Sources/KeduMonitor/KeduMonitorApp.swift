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
            HStack(spacing: 0) {
                MenuMetricValue(
                    title: "CPU",
                    value: String(format: "%.1f%%", store.latestSnapshot?.totalCPUPercent ?? 0),
                    color: .cyan
                )
                Divider().frame(height: 30)
                MenuMetricValue(
                    title: "内存",
                    value: memoryValue,
                    color: .orange
                )
            }
            .padding(.horizontal, 12)
            .frame(height: 54)

            Divider()

            VStack(spacing: 10) {
                MenuTrendRow(
                    title: "CPU",
                    color: .cyan,
                    values: recentSnapshots.map(\.totalCPUPercent),
                    maximum: cpuMaximum
                )
                MenuTrendRow(
                    title: "内存",
                    color: .orange,
                    values: recentSnapshots.map { Double($0.totalMemoryBytes) },
                    maximum: Double(ProcessInfo.processInfo.physicalMemory)
                )
            }
            .padding(12)

            Divider()

            HStack(spacing: 16) {
                menuButton("macwindow", help: "显示窗口") {
                    openWindow(id: "main")
                    NSApp.activate(ignoringOtherApps: true)
                }
                menuButton(store.isPaused ? "play.fill" : "pause.fill", help: store.isPaused ? "继续采样" : "暂停采样") {
                    store.togglePause()
                }
                Button {
                    showsSettings.toggle()
                } label: {
                    Image(systemName: "slider.horizontal.3")
                }
                .buttonStyle(.borderless)
                .help("采样设置")
                .popover(isPresented: $showsSettings, arrowEdge: .bottom) {
                    SamplingSettingsView()
                        .environment(store)
                }
                Spacer()
                menuButton("power", help: "退出") {
                    NSApp.terminate(nil)
                }
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 14)
            .frame(height: 42)
        }
        .frame(width: 286)
    }

    private var recentSnapshots: [MetricSnapshot] {
        Array(store.displaySnapshots(maximumCount: 48).suffix(48))
    }

    private var memoryValue: String {
        let gigabytes = Double(store.latestSnapshot?.totalMemoryBytes ?? 0) / 1_073_741_824
        return String(format: "%.1f GB", gigabytes)
    }

    private var cpuMaximum: Double {
        let observed = recentSnapshots.map(\.totalCPUPercent).max() ?? 0
        return max(5, min(100, ceil(observed / 5) * 5))
    }

    private func menuButton(
        _ systemName: String,
        help: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .frame(width: 16, height: 16)
        }
        .buttonStyle(.borderless)
        .help(help)
    }
}

private struct MenuMetricValue: View {
    let title: String
    let value: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                Circle().fill(color).frame(width: 6, height: 6)
                Text(title)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(.callout, design: .monospaced, weight: .semibold))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
    }
}

private struct MenuTrendRow: View {
    let title: String
    let color: Color
    let values: [Double]
    let maximum: Double

    var body: some View {
        HStack(spacing: 9) {
            Text(title)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(width: 28, alignment: .trailing)
            Sparkline(values: values, maximum: maximum, color: color)
                .frame(height: 42)
        }
    }
}

private struct Sparkline: View {
    let values: [Double]
    let maximum: Double
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard values.count > 1, maximum > 0 else {
                return
            }
            let denominator = CGFloat(values.count - 1)
            var path = Path()
            for (index, value) in values.enumerated() {
                let point = CGPoint(
                    x: size.width * CGFloat(index) / denominator,
                    y: size.height * (1 - CGFloat(min(1, max(0, value / maximum))))
                )
                index == 0 ? path.move(to: point) : path.addLine(to: point)
            }
            context.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
        }
        .background {
            RoundedRectangle(cornerRadius: 4)
                .fill(Color.primary.opacity(0.035))
        }
    }
}
