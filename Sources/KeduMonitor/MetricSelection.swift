import Foundation

enum MetricCategory: String, CaseIterable, Identifiable, Sendable {
    case cpu
    case memory
    case disk
    case network

    var id: Self { self }

    var title: String {
        switch self {
        case .cpu: "CPU"
        case .memory: "内存"
        case .disk: "磁盘"
        case .network: "网络"
        }
    }
}

enum TransferDirection: String, CaseIterable, Identifiable, Sendable {
    case incoming
    case outgoing

    var id: Self { self }
}

enum MetricKind: Sendable {
    case cpu
    case memory
    case diskRead
    case diskWrite
    case networkDownload
    case networkUpload

    init(category: MetricCategory, direction: TransferDirection) {
        switch category {
        case .cpu:
            self = .cpu
        case .memory:
            self = .memory
        case .disk:
            self = direction == .incoming ? .diskRead : .diskWrite
        case .network:
            self = direction == .incoming ? .networkDownload : .networkUpload
        }
    }

    var title: String {
        switch self {
        case .cpu: "CPU 使用率"
        case .memory: "内存占用"
        case .diskRead: "磁盘读取"
        case .diskWrite: "磁盘写入"
        case .networkDownload: "网络下载"
        case .networkUpload: "网络上传"
        }
    }

    var shortUnit: String {
        switch self {
        case .cpu: "%"
        case .memory: "GB"
        case .diskRead, .diskWrite, .networkDownload, .networkUpload: "MB/s"
        }
    }

    func value(for application: ApplicationMetrics) -> Double {
        switch self {
        case .cpu:
            application.cpuPercent
        case .memory:
            Double(application.memoryBytes) / 1_073_741_824
        case .diskRead:
            application.diskReadBytesPerSecond / 1_048_576
        case .diskWrite:
            application.diskWriteBytesPerSecond / 1_048_576
        case .networkDownload:
            application.networkDownloadBytesPerSecond / 1_048_576
        case .networkUpload:
            application.networkUploadBytesPerSecond / 1_048_576
        }
    }

    func total(in snapshot: MetricSnapshot) -> Double {
        snapshot.applications.reduce(0) { $0 + value(for: $1) }
    }

    func formatted(_ value: Double) -> String {
        switch self {
        case .cpu, .memory, .diskRead, .diskWrite, .networkDownload:
            String(format: "%.1f%@", value, shortUnit)
        case .networkUpload:
            String(format: value < 1 ? "%.2f%@" : "%.1f%@", value, shortUnit)
        }
    }

    func axisLabel(_ value: Double) -> String {
        switch self {
        case .cpu:
            String(format: "%.0f", value)
        case .memory:
            String(format: "%.0f", value)
        case .diskRead, .diskWrite, .networkDownload, .networkUpload:
            value < 1 ? String(format: "%.1f", value) : String(format: "%.0f", value)
        }
    }
}
