import Foundation
import Testing
@testable import KeduMonitor

@Suite("MonitorStore")
struct MonitorStoreTests {
    @Test("history compaction keeps application totals and process samples")
    func compactsHistory() {
        let snapshot = makeSnapshot(index: 0, includesProcess: true)
        let compacted = snapshot.compactedForHistory()

        #expect(compacted.applications[0].processIDs == [42])
        #expect(compacted.applications[0].processes.map(\.pid) == [42])
        #expect(compacted.totalCPUPercent == snapshot.totalCPUPercent)
        #expect(compacted.totalMemoryBytes == snapshot.totalMemoryBytes)
    }

    @Test("long histories are reduced into stable time buckets")
    func downsamplesLongHistory() {
        let snapshots = (0..<2_160).map { makeSnapshot(index: $0, includesProcess: false) }
        let result = MonitorStore.downsample(
            snapshots,
            maximumCount: 600,
            bucketDuration: 18
        )

        #expect(result.count <= 600)
        #expect(result.last?.timestamp == snapshots.last?.timestamp)
        #expect(result.map(\.timestamp).allSatisfy { timestamp in
            snapshots.contains { $0.timestamp == timestamp }
        })
    }

    private func makeSnapshot(index: Int, includesProcess: Bool) -> MetricSnapshot {
        let process = ProcessMetrics(
            pid: 42,
            name: "worker",
            cpuPercent: 2,
            memoryBytes: 1_024,
            diskReadBytesPerSecond: 10,
            diskWriteBytesPerSecond: 20,
            networkDownloadBytesPerSecond: 30,
            networkUploadBytesPerSecond: 40
        )
        let application = ApplicationMetrics(
            identity: ApplicationIdentity(id: "app", name: "App", bundlePath: nil),
            processIDs: [42],
            processes: includesProcess ? [process] : [],
            cpuPercent: 2,
            memoryBytes: 1_024,
            diskReadBytesPerSecond: 10,
            diskWriteBytesPerSecond: 20,
            networkDownloadBytesPerSecond: 30,
            networkUploadBytesPerSecond: 40
        )
        return MetricSnapshot(
            timestamp: Date(timeIntervalSinceReferenceDate: Double(index * 5)),
            applications: [application]
        )
    }
}
