import Darwin
import Foundation

actor ProcessCollector {
    private struct Counters: Sendable {
        let cpuNanoseconds: UInt64
        let memoryBytes: UInt64
        let diskReadBytes: UInt64
        let diskWriteBytes: UInt64
    }

    private struct ProcessRecord: Sendable {
        let pid: pid_t
        let parentPID: pid_t
        let name: String
        let executablePath: String?
        let counters: Counters
    }

    private struct Aggregate {
        var processes: [ProcessMetrics] = []
        var cpuPercent = 0.0
        var memoryBytes: UInt64 = 0
        var diskReadBytesPerSecond = 0.0
        var diskWriteBytesPerSecond = 0.0
    }

    private var previousCounters: [pid_t: Counters] = [:]
    private var previousSampleTime: ContinuousClock.Instant?
    private var latestIdentityByPID: [pid_t: ApplicationIdentity] = [:]
    private var identityCache: [String: ApplicationIdentity] = [:]
    private let clock = ContinuousClock()
    private let processorCount = max(1, ProcessInfo.processInfo.activeProcessorCount)

    func sample() -> MetricSnapshot {
        let now = clock.now
        let elapsed = previousSampleTime.map { max(0.001, seconds(from: $0, to: now)) }
        let records = processRecords()
        let recordsByPID = Dictionary(uniqueKeysWithValues: records.map { ($0.pid, $0) })
        var aggregates: [ApplicationIdentity: Aggregate] = [:]
        var identitiesByPID: [pid_t: ApplicationIdentity] = [:]

        for record in records {
            let identity = identity(for: record, recordsByPID: recordsByPID)
            identitiesByPID[record.pid] = identity

            var aggregate = aggregates[identity, default: Aggregate()]
            aggregate.memoryBytes &+= record.counters.memoryBytes

            var cpuPercent = 0.0
            var diskReadBytesPerSecond = 0.0
            var diskWriteBytesPerSecond = 0.0

            if let previous = previousCounters[record.pid], let elapsed {
                let cpuDelta = Self.positiveDelta(record.counters.cpuNanoseconds, previous.cpuNanoseconds)
                let readDelta = Self.positiveDelta(record.counters.diskReadBytes, previous.diskReadBytes)
                let writeDelta = Self.positiveDelta(record.counters.diskWriteBytes, previous.diskWriteBytes)
                cpuPercent = Double(cpuDelta) / 1_000_000_000 / elapsed
                    / Double(processorCount) * 100
                diskReadBytesPerSecond = Double(readDelta) / elapsed
                diskWriteBytesPerSecond = Double(writeDelta) / elapsed
            }

            aggregate.cpuPercent += cpuPercent
            aggregate.diskReadBytesPerSecond += diskReadBytesPerSecond
            aggregate.diskWriteBytesPerSecond += diskWriteBytesPerSecond
            aggregate.processes.append(ProcessMetrics(
                pid: record.pid,
                name: record.name,
                cpuPercent: cpuPercent,
                memoryBytes: record.counters.memoryBytes,
                diskReadBytesPerSecond: diskReadBytesPerSecond,
                diskWriteBytesPerSecond: diskWriteBytesPerSecond,
                networkDownloadBytesPerSecond: 0,
                networkUploadBytesPerSecond: 0
            ))

            aggregates[identity] = aggregate
        }

        previousCounters = Dictionary(uniqueKeysWithValues: records.map { ($0.pid, $0.counters) })
        previousSampleTime = now
        latestIdentityByPID = identitiesByPID

        let applications = aggregates.map { identity, aggregate in
            ApplicationMetrics(
                identity: identity,
                processIDs: aggregate.processes.map(\.pid),
                processes: aggregate.processes.sorted { left, right in
                    if left.cpuPercent == right.cpuPercent {
                        return left.memoryBytes > right.memoryBytes
                    }
                    return left.cpuPercent > right.cpuPercent
                },
                cpuPercent: aggregate.cpuPercent,
                memoryBytes: aggregate.memoryBytes,
                diskReadBytesPerSecond: aggregate.diskReadBytesPerSecond,
                diskWriteBytesPerSecond: aggregate.diskWriteBytesPerSecond
            )
        }
        .sorted { left, right in
            if left.cpuPercent == right.cpuPercent {
                return left.memoryBytes > right.memoryBytes
            }
            return left.cpuPercent > right.cpuPercent
        }

        return MetricSnapshot(applications: applications)
    }

    func applicationIdentitiesByPID() -> [pid_t: ApplicationIdentity] {
        latestIdentityByPID
    }

    nonisolated static func appRootPath(for executablePath: String) -> String? {
        if executablePath.hasSuffix(".app") {
            return executablePath
        }
        guard let range = executablePath.range(of: ".app/") else {
            return nil
        }
        return String(executablePath[..<range.upperBound].dropLast())
    }

    nonisolated static func positiveDelta(_ current: UInt64, _ previous: UInt64) -> UInt64 {
        current >= previous ? current - previous : 0
    }

    private func processRecords() -> [ProcessRecord] {
        allProcessIDs().compactMap { pid in
            guard pid > 0, let counters = counters(for: pid) else {
                return nil
            }
            let executablePath = executablePath(for: pid)
            let name = executablePath.map { URL(fileURLWithPath: $0).lastPathComponent }
                ?? processName(for: pid)
                ?? "PID \(pid)"
            return ProcessRecord(
                pid: pid,
                parentPID: parentPID(for: pid),
                name: name,
                executablePath: executablePath,
                counters: counters
            )
        }
    }

    private func identity(
        for record: ProcessRecord,
        recordsByPID: [pid_t: ProcessRecord]
    ) -> ApplicationIdentity {
        if let rootPath = resolvedAppRoot(for: record, recordsByPID: recordsByPID) {
            let key = "app:\(rootPath)"
            if let cached = identityCache[key] {
                return cached
            }
            let appURL = URL(fileURLWithPath: rootPath)
            let bundle = Bundle(url: appURL)
            let displayName = bundle?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
                ?? bundle?.object(forInfoDictionaryKey: "CFBundleName") as? String
                ?? appURL.deletingPathExtension().lastPathComponent
            let identity = ApplicationIdentity(id: key, name: displayName, bundlePath: rootPath)
            identityCache[key] = identity
            return identity
        }

        let key = "process:\(record.name)"
        if let cached = identityCache[key] {
            return cached
        }
        let identity = ApplicationIdentity(id: key, name: record.name, bundlePath: nil)
        identityCache[key] = identity
        return identity
    }

    private func resolvedAppRoot(
        for record: ProcessRecord,
        recordsByPID: [pid_t: ProcessRecord]
    ) -> String? {
        var current: ProcessRecord? = record
        var visited = Set<pid_t>()

        while let candidate = current, visited.insert(candidate.pid).inserted {
            if let path = candidate.executablePath, let rootPath = Self.appRootPath(for: path) {
                return rootPath
            }
            guard candidate.parentPID > 1 else {
                break
            }
            current = recordsByPID[candidate.parentPID]
        }
        return nil
    }

    private func allProcessIDs() -> [pid_t] {
        let estimatedCount = max(64, Int(proc_listallpids(nil, 0)) + 64)
        var processIDs = [pid_t](repeating: 0, count: estimatedCount)
        let count = processIDs.withUnsafeMutableBytes { buffer in
            proc_listallpids(buffer.baseAddress, Int32(buffer.count))
        }
        guard count > 0 else {
            return []
        }
        return Array(processIDs.prefix(Int(count)))
    }

    private func counters(for pid: pid_t) -> Counters? {
        var info = rusage_info_v4()
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
                proc_pid_rusage(pid, RUSAGE_INFO_V4, $0)
            }
        }
        guard result == 0 else {
            return nil
        }
        return Counters(
            cpuNanoseconds: info.ri_user_time &+ info.ri_system_time,
            memoryBytes: info.ri_phys_footprint,
            diskReadBytes: info.ri_diskio_bytesread,
            diskWriteBytes: info.ri_diskio_byteswritten
        )
    }

    private func executablePath(for pid: pid_t) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXPATHLEN) * 4)
        let length = proc_pidpath(pid, &buffer, UInt32(buffer.count))
        guard length > 0 else {
            return nil
        }
        return Self.decode(buffer, length: Int(length))
    }

    private func processName(for pid: pid_t) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXCOMLEN) + 1)
        let length = proc_name(pid, &buffer, UInt32(buffer.count))
        guard length > 0 else {
            return nil
        }
        return Self.decode(buffer, length: Int(length))
    }

    private func parentPID(for pid: pid_t) -> pid_t {
        var info = proc_bsdinfo()
        let size = MemoryLayout<proc_bsdinfo>.size
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, pointer, Int32(size))
        }
        guard result == Int32(size) else {
            return 0
        }
        return pid_t(info.pbi_ppid)
    }

    private func seconds(
        from start: ContinuousClock.Instant,
        to end: ContinuousClock.Instant
    ) -> Double {
        let duration = start.duration(to: end)
        let components = duration.components
        return Double(components.seconds) + Double(components.attoseconds) / 1e18
    }

    nonisolated private static func decode(_ buffer: [CChar], length: Int) -> String {
        buffer.withUnsafeBytes { bytes in
            String(decoding: bytes.prefix(length), as: UTF8.self)
        }
    }
}
