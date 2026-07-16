import Testing
@testable import KeduMonitor

@Suite("MetricSelection")
struct MetricSelectionTests {
    @Test("converts byte rates to megabytes")
    func convertsByteRates() {
        let metrics = ApplicationMetrics(
            identity: .init(id: "test", name: "Test", bundlePath: nil),
            processIDs: [1],
            cpuPercent: 4,
            memoryBytes: 2_147_483_648,
            diskReadBytesPerSecond: 1_048_576,
            diskWriteBytesPerSecond: 0,
            networkDownloadBytesPerSecond: 2_097_152,
            networkUploadBytesPerSecond: 524_288
        )
        #expect(MetricKind.memory.value(for: metrics) == 2)
        #expect(MetricKind.diskRead.value(for: metrics) == 1)
        #expect(MetricKind.networkDownload.value(for: metrics) == 2)
        #expect(MetricKind.networkUpload.value(for: metrics) == 0.5)
    }
}
