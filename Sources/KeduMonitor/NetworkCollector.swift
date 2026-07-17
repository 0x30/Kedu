import Darwin
import Foundation

actor NetworkCollector {
    struct Counters: Equatable, Sendable {
        let bytesIn: UInt64
        let bytesOut: UInt64
    }

    struct ProcessRow: Equatable, Sendable {
        let pid: pid_t
        let counters: Counters
    }

    private var activeProcess: Process?
    private var previousFrame: [pid_t: Counters] = [:]
    private var previousFrameTime: ContinuousClock.Instant?
    private let clock = ContinuousClock()

    func stop() {
        if let activeProcess, activeProcess.isRunning {
            activeProcess.terminate()
        }
        activeProcess = nil
        previousFrame.removeAll(keepingCapacity: true)
        previousFrameTime = nil
    }

    func sampleRates(
        groupedBy identitiesByPID: [pid_t: ApplicationIdentity]
    ) throws -> ApplicationNetworkRates {
        let currentFrame = try captureProcessCounters()
        let now = clock.now
        defer {
            previousFrame = currentFrame
            previousFrameTime = now
        }
        guard let previousFrameTime else {
            return ApplicationNetworkRates(totals: [:], byPID: [:])
        }

        let elapsed = seconds(from: previousFrameTime, to: now)
        var grouped: [ApplicationIdentity: NetworkRates] = [:]
        var byPID: [Int32: NetworkRates] = [:]
        for (pid, current) in currentFrame {
            guard let previous = previousFrame[pid], let identity = identitiesByPID[pid] else {
                continue
            }
            let rates = Self.rates(current: current, previous: previous, elapsed: elapsed)
            byPID[pid] = rates
            var aggregate = grouped[identity, default: .zero]
            aggregate.downloadBytesPerSecond += rates.downloadBytesPerSecond
            aggregate.uploadBytesPerSecond += rates.uploadBytesPerSecond
            grouped[identity] = aggregate
        }
        return ApplicationNetworkRates(totals: grouped, byPID: byPID)
    }

    func captureProcessCounters() throws -> [pid_t: Counters] {
        let process = Process()
        let output = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/nettop")
        process.arguments = [
            "-P", "-L", "1", "-n", "-x",
            "-J", "bytes_in,bytes_out",
        ]
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        activeProcess = process
        try process.run()

        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        activeProcess = nil
        guard process.terminationStatus == 0,
              let text = String(data: data, encoding: .utf8) else {
            return [:]
        }
        return text.split(whereSeparator: \.isNewline).reduce(into: [:]) { frame, line in
            guard let row = Self.parseProcessRow(String(line)) else {
                return
            }
            frame[row.pid] = row.counters
        }
    }

    nonisolated static func parseProcessRow(_ line: String) -> ProcessRow? {
        guard !line.hasPrefix(","), !line.isEmpty else {
            return nil
        }
        let columns = line.split(separator: ",", omittingEmptySubsequences: false)
        guard columns.count >= 3,
              let separator = columns[0].lastIndex(of: "."),
              let pid = pid_t(columns[0][columns[0].index(after: separator)...]),
              let bytesIn = UInt64(columns[1]),
              let bytesOut = UInt64(columns[2]) else {
            return nil
        }
        return ProcessRow(pid: pid, counters: Counters(bytesIn: bytesIn, bytesOut: bytesOut))
    }

    nonisolated static func rates(
        current: Counters,
        previous: Counters,
        elapsed: Double
    ) -> NetworkRates {
        guard elapsed > 0 else {
            return .zero
        }
        return NetworkRates(
            downloadBytesPerSecond: Double(ProcessCollector.positiveDelta(current.bytesIn, previous.bytesIn)) / elapsed,
            uploadBytesPerSecond: Double(ProcessCollector.positiveDelta(current.bytesOut, previous.bytesOut)) / elapsed
        )
    }

    private func seconds(
        from start: ContinuousClock.Instant,
        to end: ContinuousClock.Instant
    ) -> Double {
        let components = start.duration(to: end).components
        return Double(components.seconds) + Double(components.attoseconds) / 1e18
    }
}
