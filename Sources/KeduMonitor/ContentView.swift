import AppKit
import SwiftUI

struct ContentView: View {
    @Environment(MonitorStore.self) private var store
    @State private var category = MetricCategory.cpu
    @State private var direction = TransferDirection.incoming
    @State private var showsApplications = false
    @State private var showsSettings = false

    private var metric: MetricKind {
        MetricKind(category: category, direction: direction)
    }

    var body: some View {
        ZStack(alignment: .trailing) {
            VStack(spacing: 0) {
                SummaryStrip(snapshot: store.latestSnapshot, selection: $category)
                Divider()
                chartContent
            }

            if showsApplications {
                Color.black.opacity(0.025)
                    .contentShape(Rectangle())
                    .onTapGesture { showsApplications = false }
                    .transition(.opacity)

                ApplicationDrawer(
                    snapshot: store.latestSnapshot,
                    metric: metric,
                    onClose: { showsApplications = false }
                )
                .frame(width: 282)
                .transition(.move(edge: .trailing))
            }
        }
        .background(Color(nsColor: .underPageBackgroundColor).opacity(0.32))
        .animation(.easeOut(duration: 0.18), value: showsApplications)
        .task { store.start() }
        .onDisappear { store.stop() }
    }

    private var chartContent: some View {
        VStack(spacing: 10) {
            HStack(alignment: .center, spacing: 8) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(metric.title)
                        .font(.system(size: 15, weight: .semibold))
                    Text("最近 \(retentionLabel) · 每 \(intervalLabel)采样")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if category == .disk || category == .network {
                    Picker("方向", selection: $direction) {
                        Image(systemName: "arrow.down").tag(TransferDirection.incoming)
                        Image(systemName: "arrow.up").tag(TransferDirection.outgoing)
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .frame(width: 72)
                    .help(category == .network ? "下载 / 上传" : "读取 / 写入")
                }

                Spacer()

                Text(metric.formatted(store.latestSnapshot.map(metric.total(in:)) ?? 0))
                    .font(.system(.title3, design: .monospaced, weight: .semibold))

                if store.errorMessage != nil {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                        .help(store.errorMessage ?? "")
                }

                iconButton(
                    store.isPaused ? "play.fill" : "pause.fill",
                    help: store.isPaused ? "继续采样" : "暂停采样",
                    action: store.togglePause
                )

                iconButton(
                    showsApplications ? "sidebar.right" : "sidebar.right",
                    help: "应用",
                    action: { showsApplications.toggle() }
                )

                Button {
                    showsSettings.toggle()
                } label: {
                    Image(systemName: "slider.horizontal.3")
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.borderless)
                .help("采样设置")
                .popover(isPresented: $showsSettings, arrowEdge: .bottom) {
                    SamplingSettingsView()
                        .environment(store)
                }
            }

            StackedMetricChart(
                snapshots: store.displaySnapshots(),
                metric: metric
            )
            .frame(maxHeight: .infinity)
        }
        .padding(.horizontal, 14)
        .padding(.top, 13)
        .padding(.bottom, 10)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func iconButton(
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

    private var intervalLabel: String {
        store.samplingInterval < 60
            ? "\(Int(store.samplingInterval)) 秒"
            : "\(Int(store.samplingInterval / 60)) 分钟"
    }

    private var retentionLabel: String {
        store.retentionDuration < 3600
            ? "\(Int(store.retentionDuration / 60)) 分钟"
            : "\(Int(store.retentionDuration / 3600)) 小时"
    }
}

private struct SamplingSettingsView: View {
    @Environment(MonitorStore.self) private var store
    @State private var exportError: String?

    var body: some View {
        @Bindable var store = store

        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("采样设置")
                    .font(.headline)
                Spacer()
                Label("仅本次会话", systemImage: "circle.fill")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .symbolRenderingMode(.palette)
                    .foregroundStyle(.green, .secondary)
            }

            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 10) {
                GridRow {
                    Text("采样间隔")
                        .foregroundStyle(.secondary)
                    Picker("采样间隔", selection: $store.samplingInterval) {
                        Text("5 秒").tag(TimeInterval(5))
                        Text("10 秒").tag(TimeInterval(10))
                        Text("15 秒").tag(TimeInterval(15))
                        Text("30 秒").tag(TimeInterval(30))
                        Text("1 分钟").tag(TimeInterval(60))
                    }
                    .labelsHidden()
                    .frame(width: 126)
                }
                GridRow {
                    Text("保留时长")
                        .foregroundStyle(.secondary)
                    Picker("保留时长", selection: $store.retentionDuration) {
                        Text("15 分钟").tag(TimeInterval(15 * 60))
                        Text("30 分钟").tag(TimeInterval(30 * 60))
                        Text("1 小时").tag(TimeInterval(60 * 60))
                        Text("3 小时").tag(TimeInterval(3 * 60 * 60))
                    }
                    .labelsHidden()
                    .frame(width: 126)
                }
            }

            Divider()

            HStack {
                Label(ByteCountFormatter.string(fromByteCount: Int64(store.estimatedStorageBytes), countStyle: .file), systemImage: "memorychip")
                Spacer()
                Text("仅存内存")
            }
            .font(.caption)
            .foregroundStyle(.secondary)

            HStack {
                Button("导出数据", systemImage: "square.and.arrow.down") {
                    exportSession()
                }
                .disabled(store.snapshots.isEmpty)

                Spacer()
                Button("清空数据", systemImage: "trash", role: .destructive) {
                    store.clear()
                }
                .disabled(store.snapshots.isEmpty)
            }

            if let exportError {
                Text(exportError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .padding(16)
        .frame(width: 292)
    }

    private func exportSession() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.commaSeparatedText]
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = "刻度-\(Date.now.formatted(.iso8601.year().month().day().dateSeparator(.dash)))-会话.csv"
        panel.message = "导出当前内存中的监控数据"
        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }
        do {
            try SessionExporter.csvData(for: store.snapshots).write(to: url, options: .atomic)
            exportError = nil
        } catch {
            exportError = "导出失败：\(error.localizedDescription)"
        }
    }
}

private struct SummaryStrip: View {
    let snapshot: MetricSnapshot?
    @Binding var selection: MetricCategory

    var body: some View {
        HStack(spacing: 0) {
            item(.cpu, icon: "cpu", value: String(format: "%.1f", snapshot?.totalCPUPercent ?? 0), unit: "%")
            Divider()
            item(
                .memory,
                icon: "memorychip",
                value: String(format: "%.1f", Double(snapshot?.totalMemoryBytes ?? 0) / 1_073_741_824),
                unit: "GB"
            )
            Divider()
            item(
                .disk,
                icon: "internaldrive",
                value: String(format: "%.1f", diskTotal),
                unit: "MB/s"
            )
            Divider()
            item(
                .network,
                icon: "network",
                value: String(format: "%.1f", networkTotal),
                unit: "MB/s"
            )
        }
        .frame(height: 62)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var diskTotal: Double {
        Double((snapshot?.totalDiskReadBytesPerSecond ?? 0) + (snapshot?.totalDiskWriteBytesPerSecond ?? 0)) / 1_048_576
    }

    private var networkTotal: Double {
        ((snapshot?.totalNetworkDownloadBytesPerSecond ?? 0) + (snapshot?.totalNetworkUploadBytesPerSecond ?? 0)) / 1_048_576
    }

    private func item(
        _ category: MetricCategory,
        icon: String,
        value: String,
        unit: String
    ) -> some View {
        Button {
            selection = category
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                Label(category.title, systemImage: icon)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(selection == category ? Color.teal : .secondary)
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    Text(value)
                        .font(.system(size: 20, weight: .semibold, design: .monospaced))
                    Text(unit)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(selection == category ? Color.primary : Color.secondary)
        .overlay(alignment: .leading) {
            if selection == category {
                Capsule()
                    .fill(Color.teal)
                    .frame(width: 2, height: 30)
            }
        }
    }
}

private struct ApplicationDrawer: View {
    let snapshot: MetricSnapshot?
    let metric: MetricKind
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("应用")
                        .font(.headline)
                    Text("按当前\(metric.title)排序")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(action: onClose) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .help("关闭应用列表")
            }
            .padding(.horizontal, 13)
            .frame(height: 56)

            Divider()

            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(sortedApplications) { application in
                        ApplicationRow(
                            application: application,
                            metric: metric,
                            total: total
                        )
                        Divider().padding(.leading, 56)
                    }
                }
            }
        }
        .background(.regularMaterial)
        .overlay(alignment: .leading) {
            Rectangle().fill(Color.primary.opacity(0.1)).frame(width: 1)
        }
        .shadow(color: .black.opacity(0.12), radius: 18, x: -8)
    }

    private var sortedApplications: [ApplicationMetrics] {
        (snapshot?.applications ?? []).sorted {
            metric.value(for: $0) > metric.value(for: $1)
        }
    }

    private var total: Double {
        snapshot.map(metric.total(in:)) ?? 0
    }
}

private struct ApplicationRow: View {
    let application: ApplicationMetrics
    let metric: MetricKind
    let total: Double

    var body: some View {
        HStack(spacing: 10) {
            ApplicationIconView(identity: application.identity, size: 32)
            VStack(alignment: .leading, spacing: 5) {
                Text(application.identity.name)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                GeometryReader { geometry in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.secondary.opacity(0.13))
                        Capsule()
                            .fill(ApplicationPalette.color(for: application.identity))
                            .frame(width: geometry.size.width * fraction)
                    }
                }
                .frame(height: 3)
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 3) {
                Text(metric.formatted(value))
                    .font(.caption.monospacedDigit())
                Text("\(Int((fraction * 100).rounded()))%")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 58)
    }

    private var value: Double {
        metric.value(for: application)
    }

    private var fraction: Double {
        total > 0 ? min(1, value / total) : 0
    }
}
