import Testing
@testable import KeduMonitor

@Suite("NetworkCollector")
struct NetworkCollectorTests {
    @Test("streams process counters from nettop", .timeLimit(.minutes(1)))
    func streamsNettopCounters() async throws {
        let collector = NetworkCollector()
        let counters = try await collector.captureProcessCounters()
        await collector.stop()
        #expect(!counters.isEmpty)
    }

    @Test("parses a nettop per-process row")
    func parsesProcessRow() {
        let row = NetworkCollector.parseProcessRow("Google Chrome H.34675,176921561,312224,")
        #expect(row?.pid == 34675)
        #expect(row?.counters == .init(bytesIn: 176_921_561, bytesOut: 312_224))
    }

    @Test("ignores nettop headers")
    func ignoresHeader() {
        #expect(NetworkCollector.parseProcessRow(",bytes_in,bytes_out,") == nil)
    }

    @Test("calculates rates from cumulative counters")
    func calculatesRates() {
        let rates = NetworkCollector.rates(
            current: .init(bytesIn: 4_000, bytesOut: 900),
            previous: .init(bytesIn: 1_000, bytesOut: 400),
            elapsed: 2
        )
        #expect(rates.downloadBytesPerSecond == 1_500)
        #expect(rates.uploadBytesPerSecond == 250)
    }
}
