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
}
