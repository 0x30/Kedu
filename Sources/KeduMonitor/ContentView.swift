import SwiftUI

struct ContentView: View {
    @Environment(MonitorStore.self) private var store
    @State private var category = MetricCategory.cpu
    @State private var direction = TransferDirection.incoming
    @State private var showsApplications = false

    private var metric: MetricKind {
        MetricKind(category: category, direction: direction)
    }

    var body: some View {
        ZStack(alignment: .trailing) {
            VStack(spacing: 0) {
                SummaryStrip(snapshot: store.latestSnapshot, selection: $category)
                Divider()
                toolbar
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

    private var toolbar: some View {
        HStack(spacing: 8) {
            Picker("指标", selection: $category) {
                ForEach(MetricCategory.allCases) { item in
                    Text(item.title).tag(item)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .frame(width: 286)

            if category == .disk || category == .network {
                Picker("方向", selection: $direction) {
                    Text(category == .network ? "下载" : "读取").tag(TransferDirection.incoming)
                    Text(category == .network ? "上传" : "写入").tag(TransferDirection.outgoing)
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(width: 116)
            }

            Spacer()

            if store.errorMessage != nil {
                Image(systemName: "exclamationmark.triangle")
                    .foregroundStyle(.orange)
                    .help(store.errorMessage ?? "")
            }

            Button {
                store.togglePause()
            } label: {
                Image(systemName: store.isPaused ? "play.fill" : "pause.fill")
                    .frame(width: 16)
            }
            .buttonStyle(.borderedProminent)
            .tint(.teal)
            .help(store.isPaused ? "继续采样" : "暂停采样")

            Button {
                showsApplications.toggle()
            } label: {
                Label("应用", systemImage: showsApplications ? "panel.right.fill" : "panel.right")
            }
            .buttonStyle(.bordered)
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var chartContent: some View {
        VStack(spacing: 10) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(metric.title)
                        .font(.system(size: 15, weight: .semibold))
                    Text("最近 \(retentionLabel) · 每 \(intervalLabel)采样")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 3) {
                    Text(metric.formatted(store.latestSnapshot.map(metric.total(in:)) ?? 0))
                        .font(.system(.title3, design: .monospaced, weight: .semibold))
                    Text(store.isPaused ? "已暂停" : "当前合计")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
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
        .frame(height: 64)
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
            VStack(alignment: .leading, spacing: 6) {
                Label(category.title, systemImage: icon)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    Text(value)
                        .font(.system(size: 21, weight: .semibold, design: .monospaced))
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
        .background(selection == category ? Color.teal.opacity(0.055) : .clear)
        .overlay(alignment: .bottom) {
            if selection == category {
                Rectangle().fill(Color.teal).frame(height: 2)
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
