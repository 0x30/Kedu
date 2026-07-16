import Foundation

struct ApplicationIdentity: Identifiable, Hashable, Codable, Sendable {
    let id: String
    let name: String
    let bundlePath: String?
}

struct ApplicationMetrics: Identifiable, Codable, Sendable {
    let identity: ApplicationIdentity
    let processIDs: [Int32]
    let cpuPercent: Double
    let memoryBytes: UInt64
    let diskReadBytesPerSecond: Double
    let diskWriteBytesPerSecond: Double
    let networkDownloadBytesPerSecond: Double
    let networkUploadBytesPerSecond: Double

    init(
        identity: ApplicationIdentity,
        processIDs: [Int32],
        cpuPercent: Double,
        memoryBytes: UInt64,
        diskReadBytesPerSecond: Double,
        diskWriteBytesPerSecond: Double,
        networkDownloadBytesPerSecond: Double = 0,
        networkUploadBytesPerSecond: Double = 0
    ) {
        self.identity = identity
        self.processIDs = processIDs
        self.cpuPercent = cpuPercent
        self.memoryBytes = memoryBytes
        self.diskReadBytesPerSecond = diskReadBytesPerSecond
        self.diskWriteBytesPerSecond = diskWriteBytesPerSecond
        self.networkDownloadBytesPerSecond = networkDownloadBytesPerSecond
        self.networkUploadBytesPerSecond = networkUploadBytesPerSecond
    }

    var id: String { identity.id }
}

struct MetricSnapshot: Identifiable, Codable, Sendable {
    let id: UUID
    let timestamp: Date
    let applications: [ApplicationMetrics]

    init(timestamp: Date = .now, applications: [ApplicationMetrics]) {
        id = UUID()
        self.timestamp = timestamp
        self.applications = applications
    }

    var totalCPUPercent: Double {
        applications.reduce(0) { $0 + $1.cpuPercent }
    }

    var totalMemoryBytes: UInt64 {
        applications.reduce(0) { $0 + $1.memoryBytes }
    }

    var totalDiskReadBytesPerSecond: Double {
        applications.reduce(0) { $0 + $1.diskReadBytesPerSecond }
    }

    var totalDiskWriteBytesPerSecond: Double {
        applications.reduce(0) { $0 + $1.diskWriteBytesPerSecond }
    }

    var totalNetworkDownloadBytesPerSecond: Double {
        applications.reduce(0) { $0 + $1.networkDownloadBytesPerSecond }
    }

    var totalNetworkUploadBytesPerSecond: Double {
        applications.reduce(0) { $0 + $1.networkUploadBytesPerSecond }
    }

    func merging(networkRates: [ApplicationIdentity: NetworkRates]) -> MetricSnapshot {
        MetricSnapshot(
            timestamp: timestamp,
            applications: applications.map { application in
                let rates = networkRates[application.identity] ?? .zero
                return ApplicationMetrics(
                    identity: application.identity,
                    processIDs: application.processIDs,
                    cpuPercent: application.cpuPercent,
                    memoryBytes: application.memoryBytes,
                    diskReadBytesPerSecond: application.diskReadBytesPerSecond,
                    diskWriteBytesPerSecond: application.diskWriteBytesPerSecond,
                    networkDownloadBytesPerSecond: rates.downloadBytesPerSecond,
                    networkUploadBytesPerSecond: rates.uploadBytesPerSecond
                )
            }
        )
    }
}

struct NetworkRates: Codable, Sendable {
    static let zero = NetworkRates(downloadBytesPerSecond: 0, uploadBytesPerSecond: 0)

    var downloadBytesPerSecond: Double
    var uploadBytesPerSecond: Double
}
