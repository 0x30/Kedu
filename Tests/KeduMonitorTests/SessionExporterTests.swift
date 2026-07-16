import Foundation
import Testing
@testable import KeduMonitor

@Suite("SessionExporter")
struct SessionExporterTests {
    @Test("exports all metrics and escapes application names")
    func exportsCSV() throws {
        let identity = ApplicationIdentity(id: "demo", name: "Demo, \"App\"", bundlePath: "/Applications/Demo.app")
        let snapshot = MetricSnapshot(
            timestamp: Date(timeIntervalSince1970: 0),
            applications: [
                ApplicationMetrics(
                    identity: identity,
                    processIDs: [12, 34],
                    cpuPercent: 5.25,
                    memoryBytes: 1024,
                    diskReadBytesPerSecond: 200,
                    diskWriteBytesPerSecond: 300,
                    networkDownloadBytesPerSecond: 400,
                    networkUploadBytesPerSecond: 500
                ),
            ]
        )

        let data = SessionExporter.csvData(for: [snapshot])
        let text = try #require(String(data: data, encoding: .utf8))
        #expect(text.contains("\"Demo, \"\"App\"\"\""))
        #expect(text.contains("\"12;34\""))
        #expect(text.contains("\"network_upload_bytes_per_second\""))
        #expect(data.starts(with: [0xEF, 0xBB, 0xBF]))
    }
}
