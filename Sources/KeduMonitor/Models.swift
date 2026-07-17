import Foundation

struct ApplicationIdentity: Identifiable, Hashable, Codable, Sendable {
    let id: String
    let name: String
    let bundlePath: String?
}

struct ProcessMetrics: Identifiable, Codable, Sendable {
    let pid: Int32
    let name: String
    let cpuPercent: Double
    let memoryBytes: UInt64
    let diskReadBytesPerSecond: Double
    let diskWriteBytesPerSecond: Double
    let networkDownloadBytesPerSecond: Double
    let networkUploadBytesPerSecond: Double

    var id: Int32 { pid }
}

struct ApplicationMetrics: Identifiable, Codable, Sendable {
    let identity: ApplicationIdentity
    let processes: [ProcessMetrics]
    let cpuPercent: Double
    let memoryBytes: UInt64
    let diskReadBytesPerSecond: Double
    let diskWriteBytesPerSecond: Double
    let networkDownloadBytesPerSecond: Double
    let networkUploadBytesPerSecond: Double

    init(
        identity: ApplicationIdentity,
        processIDs: [Int32],
        processes: [ProcessMetrics]? = nil,
        cpuPercent: Double,
        memoryBytes: UInt64,
        diskReadBytesPerSecond: Double,
        diskWriteBytesPerSecond: Double,
        networkDownloadBytesPerSecond: Double = 0,
        networkUploadBytesPerSecond: Double = 0
    ) {
        self.identity = identity
        self.processes = processes ?? processIDs.map { pid in
            ProcessMetrics(
                pid: pid,
                name: "PID \(pid)",
                cpuPercent: 0,
                memoryBytes: 0,
                diskReadBytesPerSecond: 0,
                diskWriteBytesPerSecond: 0,
                networkDownloadBytesPerSecond: 0,
                networkUploadBytesPerSecond: 0
            )
        }
        self.cpuPercent = cpuPercent
        self.memoryBytes = memoryBytes
        self.diskReadBytesPerSecond = diskReadBytesPerSecond
        self.diskWriteBytesPerSecond = diskWriteBytesPerSecond
        self.networkDownloadBytesPerSecond = networkDownloadBytesPerSecond
        self.networkUploadBytesPerSecond = networkUploadBytesPerSecond
    }

    var id: String { identity.id }
    var processIDs: [Int32] { processes.map(\.pid) }
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

    func merging(networkRates: ApplicationNetworkRates) -> MetricSnapshot {
        MetricSnapshot(
            timestamp: timestamp,
            applications: applications.map { application in
                let rates = networkRates.totals[application.identity] ?? .zero
                return ApplicationMetrics(
                    identity: application.identity,
                    processIDs: application.processIDs,
                    processes: application.processes.map { process in
                        let rates = networkRates.byPID[process.pid] ?? .zero
                        return ProcessMetrics(
                            pid: process.pid,
                            name: process.name,
                            cpuPercent: process.cpuPercent,
                            memoryBytes: process.memoryBytes,
                            diskReadBytesPerSecond: process.diskReadBytesPerSecond,
                            diskWriteBytesPerSecond: process.diskWriteBytesPerSecond,
                            networkDownloadBytesPerSecond: rates.downloadBytesPerSecond,
                            networkUploadBytesPerSecond: rates.uploadBytesPerSecond
                        )
                    },
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

struct ApplicationNetworkRates: Sendable {
    var totals: [ApplicationIdentity: NetworkRates]
    var byPID: [Int32: NetworkRates]
}
