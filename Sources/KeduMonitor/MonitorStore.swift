import Foundation
import Observation

@MainActor
@Observable
final class MonitorStore {
    private(set) var snapshots: [MetricSnapshot] = []
    private(set) var isPaused = false
    private(set) var isCollecting = false
    private(set) var errorMessage: String?
    var samplingInterval: TimeInterval = 15 {
        didSet {
            guard samplingInterval != oldValue, collectionTask != nil else {
                return
            }
            restartCollection()
        }
    }
    var retentionDuration: TimeInterval = 30 * 60 {
        didSet {
            guard retentionDuration != oldValue else {
                return
            }
            trimSnapshots()
        }
    }

    @ObservationIgnored private let processCollector = ProcessCollector()
    @ObservationIgnored private let networkCollector = NetworkCollector()
    @ObservationIgnored private var collectionTask: Task<Void, Never>?

    var latestSnapshot: MetricSnapshot? {
        snapshots.last
    }

    var estimatedStorageBytes: Int {
        snapshots.reduce(0) { partial, snapshot in
            partial + snapshot.applications.count * 152
        }
    }

    func start() {
        guard collectionTask == nil else {
            return
        }
        collectionTask = Task { [weak self] in
            guard let self else {
                return
            }
            await capture()
            try? await Task.sleep(for: .seconds(1))
            await capture()

            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: .seconds(samplingInterval))
                } catch {
                    break
                }
                if !isPaused {
                    await capture()
                }
            }
        }
    }

    func stop() {
        collectionTask?.cancel()
        collectionTask = nil
        Task {
            await networkCollector.stop()
        }
    }

    func togglePause() {
        isPaused.toggle()
    }

    func clear() {
        snapshots.removeAll(keepingCapacity: true)
    }

    func restartCollection() {
        stop()
        start()
    }

    func displaySnapshots(maximumCount: Int = 240) -> [MetricSnapshot] {
        guard snapshots.count > maximumCount, maximumCount > 1 else {
            return snapshots
        }
        let step = Double(snapshots.count - 1) / Double(maximumCount - 1)
        return (0..<maximumCount).map { index in
            snapshots[min(snapshots.count - 1, Int((Double(index) * step).rounded()))]
        }
    }

    private func capture() async {
        isCollecting = true
        let processSnapshot = await processCollector.sample()
        let identitiesByPID = await processCollector.applicationIdentitiesByPID()

        do {
            let rates = try await networkCollector.sampleRates(groupedBy: identitiesByPID)
            append(processSnapshot.merging(networkRates: rates))
            errorMessage = nil
        } catch {
            append(processSnapshot)
            errorMessage = "网络流量暂时不可用"
        }
        isCollecting = false
    }

    private func append(_ snapshot: MetricSnapshot) {
        snapshots.append(snapshot)
        trimSnapshots(referenceDate: snapshot.timestamp)
    }

    private func trimSnapshots(referenceDate: Date = .now) {
        let cutoff = referenceDate.addingTimeInterval(-retentionDuration)
        if let firstRetainedIndex = snapshots.firstIndex(where: { $0.timestamp >= cutoff }),
           firstRetainedIndex > snapshots.startIndex {
            snapshots.removeFirst(firstRetainedIndex)
        }
    }
}
