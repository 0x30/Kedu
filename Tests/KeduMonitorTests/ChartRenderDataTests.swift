import Foundation
import Testing
@testable import KeduMonitor

@Suite("ChartRenderData")
struct ChartRenderDataTests {
    @Test("builds a dense 30 minute chart model within one second")
    @MainActor
    func buildsThirtyMinuteModel() {
        let identities = (0..<120).map { index in
            ApplicationIdentity(id: "app-\(index)", name: "App \(index)", bundlePath: nil)
        }
        let snapshots = (0..<360).map { sampleIndex in
            MetricSnapshot(
                timestamp: Date(timeIntervalSinceReferenceDate: Double(sampleIndex * 5)),
                applications: identities.enumerated().map { appIndex, identity in
                    ApplicationMetrics(
                        identity: identity,
                        processIDs: [],
                        processes: [],
                        cpuPercent: Double((sampleIndex + appIndex) % 20) / 10,
                        memoryBytes: UInt64(1_000_000 + appIndex * 10_000),
                        diskReadBytesPerSecond: Double(appIndex * 100),
                        diskWriteBytesPerSecond: Double(appIndex * 50)
                    )
                }
            )
        }

        let clock = ContinuousClock()
        let start = clock.now
        let renderData = ChartRenderData.make(from: snapshots, metric: .cpu)
        let elapsed = start.duration(to: clock.now)

        #expect(renderData.series.count == 8)
        #expect(renderData.series.allSatisfy { $0.values.count == 360 })
        #expect(renderData.dominanceColors.count == 360)
        #expect(elapsed < .seconds(1))
    }
}
