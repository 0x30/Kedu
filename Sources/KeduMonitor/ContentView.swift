import AppKit
import SwiftUI

struct ContentView: View {
    @Environment(MonitorStore.self) private var store
    @State private var category = MetricCategory.cpu
    @State private var direction = TransferDirection.incoming
    @State private var showsApplications = false
    @State private var showsToolbox = false
    @State private var inspectedSnapshot: MetricSnapshot?
    @State private var showsSettings = false

    private var metric: MetricKind {
        MetricKind(category: category, direction: direction)
    }

    var body: some View {
        ZStack(alignment: .trailing) {
            VStack(spacing: 0) {
                SummaryStrip(
                    snapshot: store.latestSnapshot,
                    selection: $category,
                    showsApplications: $showsApplications,
                    showsToolbox: $showsToolbox,
                    showsSettings: $showsSettings,
                    onClearInspection: { inspectedSnapshot = nil }
                )
                Divider()
                chartContent
            }

            if showsApplications || showsToolbox {
                Color.black.opacity(0.025)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        showsApplications = false
                        showsToolbox = false
                    }
                    .transition(.opacity)

                if showsApplications {
                    ApplicationDrawer(
                        snapshot: inspectedSnapshot ?? store.latestSnapshot,
                        metric: metric,
                        isHistorical: inspectedSnapshot != nil,
                        onShowLive: { inspectedSnapshot = nil },
                        onClose: {
                            inspectedSnapshot = nil
                            showsApplications = false
                        }
                    )
                    .frame(width: 282)
                    .transition(.move(edge: .trailing))
                } else {
                    ToolboxDrawer(onClose: { showsToolbox = false })
                        .frame(width: 320)
                        .transition(.move(edge: .trailing))
                }
            }
        }
        .background {
            FrostedWindowBackground()
                .ignoresSafeArea()
        }
        .background {
            EscapeKeyMonitor {
                if showsApplications {
                    inspectedSnapshot = nil
                    showsApplications = false
                    return true
                }
                if showsToolbox {
                    showsToolbox = false
                    return true
                }
                if showsSettings {
                    showsSettings = false
                    return true
                }
                return false
            }
        }
        .animation(.easeOut(duration: 0.18), value: showsApplications)
    }

    private var chartContent: some View {
        ZStack(alignment: .topTrailing) {
            StackedMetricChart(
                snapshots: store.displaySnapshots(),
                metric: metric,
                onSelectSnapshot: { snapshot in
                    inspectedSnapshot = snapshot
                    showsApplications = true
                }
            )
            .frame(maxHeight: .infinity)

            HStack(spacing: 6) {
                if category == .disk || category == .network {
                    Picker("方向", selection: $direction) {
                        Image(systemName: "arrow.down").tag(TransferDirection.incoming)
                        Image(systemName: "arrow.up").tag(TransferDirection.outgoing)
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .frame(width: 68)
                    .help(category == .network ? "下载 / 上传" : "读取 / 写入")
                }
                if store.errorMessage != nil {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                        .help(store.errorMessage ?? "")
                }
            }
            .padding(5)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(.ultraThinMaterial)
    }
}

struct SamplingSettingsView: View {
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

            Divider()

            Button("退出刻度", systemImage: "power") {
                NSApp.terminate(nil)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)

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
    @Binding var showsApplications: Bool
    @Binding var showsToolbox: Bool
    @Binding var showsSettings: Bool
    let onClearInspection: () -> Void
    @Environment(MonitorStore.self) private var store

    var body: some View {
        HStack(spacing: 0) {
            item(.cpu, icon: "cpu", value: String(format: "%.1f", snapshot?.systemCPUPercent ?? 0), unit: "%")
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
            Divider()
            VStack(spacing: 0) {
                Button {
                    onClearInspection()
                    showsToolbox = false
                    showsApplications.toggle()
                } label: {
                    Image(systemName: "sidebar.right")
                        .frame(width: 17, height: 14)
                }
                .buttonStyle(.borderless)
                .help("应用")
                Button {
                    showsApplications = false
                    onClearInspection()
                    showsToolbox.toggle()
                } label: {
                    Image(systemName: "wrench.and.screwdriver")
                        .frame(width: 17, height: 14)
                }
                .buttonStyle(.borderless)
                .help("工具箱")
                Button {
                    showsSettings.toggle()
                } label: {
                    Image(systemName: "slider.horizontal.3")
                        .frame(width: 17, height: 14)
                }
                .buttonStyle(.borderless)
                .help("采样设置")
                .popover(isPresented: $showsSettings, arrowEdge: .bottom) {
                    SamplingSettingsView()
                        .environment(store)
                }
            }
            .frame(width: 28)
        }
        .padding(.horizontal, 9)
        .frame(height: 50)
        .background(.thinMaterial)
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
                    .font(.system(size: 18, weight: .semibold, design: .monospaced))
                    Text(unit)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 11)
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
    let isHistorical: Bool
    let onShowLive: () -> Void
    let onClose: () -> Void
    @State private var displayMode = ApplicationDisplayMode.value

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("应用")
                        .font(.headline)
                    Text(subtitle)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if isHistorical {
                    Button(action: onShowLive) {
                        Image(systemName: "dot.radiowaves.left.and.right")
                    }
                    .buttonStyle(.borderless)
                    .help("返回当前时刻")
                }
                Picker("显示方式", selection: $displayMode) {
                    Image(systemName: "number").tag(ApplicationDisplayMode.value)
                    Image(systemName: "percent").tag(ApplicationDisplayMode.share)
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(width: 62)
                .help("实际值 / 当前总量占比")
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
                            total: total,
                            displayMode: displayMode
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

    private var subtitle: String {
        if isHistorical, let timestamp = snapshot?.timestamp {
            return timestamp.formatted(.dateTime.hour().minute().second())
        }
        return displayMode == .value ? metric.title : "占当前总量"
    }
}

private struct ApplicationRow: View {
    let application: ApplicationMetrics
    let metric: MetricKind
    let total: Double
    let displayMode: ApplicationDisplayMode
    @State private var isExpanded = false

    var body: some View {
        VStack(spacing: 0) {
            Button {
                withAnimation(.easeOut(duration: 0.14)) {
                    isExpanded.toggle()
                }
            } label: {
                HStack(spacing: 10) {
                    ApplicationIconView(identity: application.identity, size: 30)
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 5) {
                            Text(application.identity.name)
                                .font(.callout.weight(.medium))
                                .lineLimit(1)
                            if application.processes.count > 1 {
                                Text("\(application.processes.count)")
                                    .font(.caption2.monospacedDigit())
                                    .foregroundStyle(.tertiary)
                            }
                        }
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
                    Text(displayValue(value))
                        .font(.caption.monospacedDigit())
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        .frame(width: 9)
                }
                .padding(.horizontal, 12)
                .frame(height: 56)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if isExpanded {
                VStack(spacing: 0) {
                    ForEach(sortedProcesses) { process in
                        ProcessDiagnosticRow(
                            process: process,
                            value: displayValue(metric.value(for: process))
                        )
                    }
                }
                .padding(.bottom, 5)
                .background(Color.primary.opacity(0.025))
            }
        }
    }

    private var value: Double {
        metric.value(for: application)
    }

    private var fraction: Double {
        total > 0 ? min(1, value / total) : 0
    }

    private var sortedProcesses: [ProcessMetrics] {
        application.processes.sorted {
            metric.value(for: $0) > metric.value(for: $1)
        }
    }

    private func displayValue(_ value: Double) -> String {
        switch displayMode {
        case .value:
            metric.formatted(value)
        case .share:
            total > 0 ? String(format: "%.0f%%", value / total * 100) : "0%"
        }
    }
}

private struct ProcessDiagnosticRow: View {
    @Environment(MonitorStore.self) private var store
    let process: ProcessMetrics
    let value: String
    @State private var isExpanded = false
    @State private var details: ProcessDetails?
    @State private var isLoading = false

    var body: some View {
        VStack(spacing: 0) {
            Button {
                withAnimation(.easeOut(duration: 0.14)) {
                    isExpanded.toggle()
                }
                if isExpanded, details == nil {
                    loadDetails()
                }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 7, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        .frame(width: 8)
                    Text("\(process.pid)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.tertiary)
                        .frame(width: 38, alignment: .trailing)
                    Text(process.name)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    Text(value)
                        .font(.caption2.monospacedDigit())
                }
                .padding(.leading, 9)
                .padding(.trailing, 32)
                .frame(height: 29)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if isExpanded {
                ProcessDetailsView(details: details, isLoading: isLoading)
                    .padding(.leading, 63)
                    .padding(.trailing, 14)
                    .padding(.bottom, 9)
            }
        }
    }

    private func loadDetails() {
        isLoading = true
        Task {
            details = await store.processDetails(for: process.pid)
            isLoading = false
        }
    }
}

private struct ProcessDetailsView: View {
    let details: ProcessDetails?
    let isLoading: Bool

    var body: some View {
        if isLoading {
            ProgressView()
                .controlSize(.mini)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else if let details {
            VStack(alignment: .leading, spacing: 7) {
                if let workingDirectory = details.workingDirectory {
                    detailRow(
                        icon: details.workingDirectoryExists ? "folder" : "folder.badge.questionmark",
                        value: details.workingDirectoryExists ? workingDirectory : "\(workingDirectory)（已删除）",
                        help: details.workingDirectoryExists ? "在 Finder 中显示" : "复制已删除的路径"
                    ) {
                        if details.workingDirectoryExists {
                            NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: workingDirectory)
                        } else {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(workingDirectory, forType: .string)
                        }
                    }
                }
                if let command = details.command {
                    detailRow(
                        icon: "terminal",
                        value: command,
                        help: "复制启动命令"
                    ) {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(command, forType: .string)
                    }
                }
                if let executablePath = details.executablePath,
                   executablePath != details.arguments.first {
                    detailRow(
                        icon: "shippingbox",
                        value: executablePath,
                        help: "在 Finder 中显示"
                    ) {
                        NSWorkspace.shared.selectFile(executablePath, inFileViewerRootedAtPath: "")
                    }
                }
            }
        } else {
            Text("无法读取进程详情")
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func detailRow(
        icon: String,
        value: String,
        help: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(alignment: .top, spacing: 6) {
                Image(systemName: icon)
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
                    .frame(width: 11)
                Text(value)
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                    .textSelection(.enabled)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

private struct ToolboxDrawer: View {
    @Environment(MonitorStore.self) private var store
    let onClose: () -> Void

    @State private var page = ToolboxPage.grid
    @State private var staleProcesses: [StaleProcess] = []
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var confirmsStopAll = false

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            switch page {
            case .grid:
                toolGrid
            case .staleProcesses:
                staleProcessList
            }
        }
        .background(.thickMaterial)
        .overlay(alignment: .leading) {
            Rectangle().fill(Color.primary.opacity(0.1)).frame(width: 1)
        }
        .shadow(color: .black.opacity(0.12), radius: 18, x: -8)
        .task { await refresh() }
        .alert("停止全部遗留进程？", isPresented: $confirmsStopAll) {
            Button("取消", role: .cancel) {}
            Button("停止全部", role: .destructive) {
                stopAll()
            }
        } message: {
            Text("将向列表中的 \(staleProcesses.count) 个进程发送 SIGTERM。")
        }
    }

    private var header: some View {
        HStack(spacing: 9) {
            if page != .grid {
                Button {
                    page = .grid
                } label: {
                    Image(systemName: "chevron.left")
                }
                .buttonStyle(.borderless)
                .help("返回工具箱")
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(page == .grid ? "工具箱" : "失效工作目录")
                    .font(.headline)
                Text(page == .grid ? "1 个工具" : "\(staleProcesses.count) 个进程")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if page == .staleProcesses {
                Button {
                    Task { await refresh() }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .help("重新扫描")
                Button {
                    confirmsStopAll = true
                } label: {
                    Image(systemName: "stop.circle")
                }
                .buttonStyle(.borderless)
                .disabled(staleProcesses.isEmpty)
                .help("停止全部")
            }
            Button(action: onClose) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .help("关闭工具箱")
        }
        .padding(.horizontal, 13)
        .frame(height: 54)
    }

    private var toolGrid: some View {
        ScrollView {
            LazyVGrid(
                columns: [GridItem(.flexible()), GridItem(.flexible())],
                spacing: 9
            ) {
                Button {
                    page = .staleProcesses
                } label: {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Image(systemName: "folder.badge.questionmark")
                                .font(.system(size: 18))
                                .foregroundStyle(.orange)
                            Spacer()
                            Text("\(staleProcesses.count)")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 3)
                        Text("遗留进程")
                            .font(.callout.weight(.semibold))
                        Text("工作目录已删除")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .padding(11)
                    .frame(maxWidth: .infinity, minHeight: 100, alignment: .leading)
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 7))
                    .overlay {
                        RoundedRectangle(cornerRadius: 7)
                            .stroke(Color.primary.opacity(0.08))
                    }
                }
                .buttonStyle(.plain)
            }
            .padding(12)
        }
    }

    @ViewBuilder
    private var staleProcessList: some View {
        if isLoading, staleProcesses.isEmpty {
            ProgressView()
                .controlSize(.small)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if staleProcesses.isEmpty {
            ContentUnavailableView(
                "未发现遗留进程",
                systemImage: "checkmark.circle",
                description: Text("没有进程持有已删除的工作目录")
            )
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    if let errorMessage {
                        Text(errorMessage)
                            .font(.caption)
                            .foregroundStyle(.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                    }
                    ForEach(staleProcesses) { process in
                        staleProcessRow(process)
                        Divider().padding(.leading, 40)
                    }
                }
            }
        }
    }

    private func staleProcessRow(_ process: StaleProcess) -> some View {
        HStack(alignment: .top, spacing: 9) {
            Image(systemName: "terminal")
                .foregroundStyle(.secondary)
                .frame(width: 20, height: 20)
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Text(process.name)
                        .font(.callout.weight(.medium))
                    Text("PID \(process.pid) · PPID \(process.parentPID)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
                Text(process.workingDirectory)
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundStyle(.orange)
                    .lineLimit(2)
                    .textSelection(.enabled)
                if let command = process.command {
                    Text(command)
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .textSelection(.enabled)
                }
            }
            Spacer(minLength: 4)
            Button {
                stop(process)
            } label: {
                Image(systemName: "stop.fill")
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.red)
            .help("停止进程")
        }
        .padding(.horizontal, 11)
        .padding(.vertical, 9)
    }

    private func refresh() async {
        isLoading = true
        staleProcesses = await store.staleWorkingDirectoryProcesses()
        isLoading = false
    }

    private func stop(_ process: StaleProcess) {
        Task {
            if let error = await store.terminateProcess(process.pid) {
                errorMessage = "无法停止 PID \(process.pid)：\(error)"
                return
            }
            errorMessage = nil
            try? await Task.sleep(for: .milliseconds(350))
            await refresh()
        }
    }

    private func stopAll() {
        Task {
            for process in staleProcesses {
                if let error = await store.terminateProcess(process.pid) {
                    errorMessage = "无法停止 PID \(process.pid)：\(error)"
                }
            }
            try? await Task.sleep(for: .milliseconds(350))
            await refresh()
        }
    }
}

private enum ToolboxPage {
    case grid
    case staleProcesses
}

private enum ApplicationDisplayMode: Hashable {
    case value
    case share
}
