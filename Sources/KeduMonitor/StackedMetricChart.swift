import AppKit
import SwiftUI

struct StackedMetricChart: View {
    let snapshots: [MetricSnapshot]
    let metric: MetricKind
    let onSelectSnapshot: (MetricSnapshot) -> Void
    private let renderData: ChartRenderData

    @State private var selectedIndex: Int?

    private let leftInset: CGFloat = 24
    private let rightInset: CGFloat = 6
    private let topInset: CGFloat = 4
    private let bottomInset: CGFloat = 17

    init(
        snapshots: [MetricSnapshot],
        metric: MetricKind,
        onSelectSnapshot: @escaping (MetricSnapshot) -> Void
    ) {
        self.snapshots = snapshots
        self.metric = metric
        self.onSelectSnapshot = onSelectSnapshot
        renderData = ChartRenderData.make(from: snapshots, metric: metric)
    }

    var body: some View {
        VStack(spacing: 3) {
            GeometryReader { geometry in
                ZStack(alignment: .topLeading) {
                    Canvas { context, size in
                        drawChart(context: context, size: size)
                    }
                    .contentShape(Rectangle())
                    .onContinuousHover { phase in
                        switch phase {
                        case .active(let location):
                            updateSelection(location: location, size: geometry.size)
                        case .ended:
                            selectedIndex = nil
                        }
                    }
                    .onTapGesture {
                        guard let selectedIndex, snapshots.indices.contains(selectedIndex) else {
                            return
                        }
                        onSelectSnapshot(snapshots[selectedIndex])
                    }

                    if let selectedIndex,
                       snapshots.indices.contains(selectedIndex) {
                        MetricTooltip(
                            snapshot: snapshots[selectedIndex],
                            metric: metric
                        )
                        .frame(width: 212)
                        .fixedSize(horizontal: false, vertical: true)
                        .offset(tooltipOffset(for: selectedIndex, in: geometry.size))
                        .allowsHitTesting(false)
                    }

                    if snapshots.count < 2 {
                        ProgressView("正在建立采样基线")
                            .controlSize(.small)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
                .clipped()
            }
            .frame(minHeight: 180)
        }
    }

    fileprivate static func maximumYValue(for snapshots: [MetricSnapshot], metric: MetricKind) -> Double {
        let observed = snapshots.map { metric.total(in: $0) }.max() ?? 0
        switch metric {
        case .cpu:
            return max(1, Self.niceMaximum(observed * 1.2))
        case .memory:
            return max(1, Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824)
        case .diskRead, .diskWrite, .networkDownload, .networkUpload:
            return max(0.1, Self.niceMaximum(observed * 1.12))
        }
    }

    private func drawChart(context: GraphicsContext, size: CGSize) {
        guard size.width > leftInset + rightInset, size.height > topInset + bottomInset else {
            return
        }
        let plot = CGRect(
            x: leftInset,
            y: topInset,
            width: size.width - leftInset - rightInset,
            height: size.height - topInset - bottomInset
        )
        drawGrid(context: context, plot: plot, maximum: renderData.maximum)
        drawAreas(
            context: context,
            plot: plot,
            series: renderData.series,
            maximum: renderData.maximum
        )
        drawAxisLabels(context: context, plot: plot, maximum: renderData.maximum)
        if let selectedIndex, snapshots.indices.contains(selectedIndex) {
            let x = xPosition(for: selectedIndex, in: plot)
            var line = Path()
            line.move(to: CGPoint(x: x, y: plot.minY))
            line.addLine(to: CGPoint(x: x, y: plot.maxY))
            context.stroke(
                line,
                with: .color(Color.primary.opacity(0.48)),
                style: StrokeStyle(lineWidth: 1, dash: [3, 3])
            )
        }
    }

    private func drawGrid(context: GraphicsContext, plot: CGRect, maximum: Double) {
        for tick in 0...4 {
            let fraction = Double(tick) / 4
            let y = plot.maxY - plot.height * CGFloat(fraction)
            var path = Path()
            path.move(to: CGPoint(x: plot.minX, y: y))
            path.addLine(to: CGPoint(x: plot.maxX, y: y))
            context.stroke(
                path,
                with: .color(Color.secondary.opacity(tick == 0 ? 0.24 : 0.13)),
                lineWidth: 1
            )
        }
    }

    private func drawAxisLabels(context: GraphicsContext, plot: CGRect, maximum: Double) {
        let labelFont = Font.system(size: 8, design: .rounded)
        for tick in 0...4 {
            let fraction = Double(tick) / 4
            let value = maximum * fraction
            let y = plot.maxY - plot.height * CGFloat(fraction)
            let label = Text(metric.axisLabel(value))
                .font(labelFont)
                .foregroundStyle(Color.secondary.opacity(0.46))
            context.draw(label, at: CGPoint(x: plot.minX - 5, y: y), anchor: .trailing)
        }

        guard snapshots.count > 1 else {
            return
        }
        let indexes = [0, (snapshots.count - 1) / 2, snapshots.count - 1]
        for (position, index) in indexes.enumerated() {
            let label = Text(snapshots[index].timestamp.formatted(.dateTime.hour().minute()))
                .font(labelFont)
                .foregroundStyle(Color.secondary.opacity(0.46))
            let anchor: UnitPoint = position == 0 ? .topLeading : position == 2 ? .topTrailing : .top
            context.draw(label, at: CGPoint(x: xPosition(for: index, in: plot), y: plot.maxY + 5), anchor: anchor)
        }
    }

    private func drawAreas(
        context: GraphicsContext,
        plot: CGRect,
        series: [ChartSeries],
        maximum: Double
    ) {
        guard !snapshots.isEmpty else {
            return
        }
        var cumulative = [Double](repeating: 0, count: snapshots.count)

        for item in series {
            let bottoms = cumulative
            for index in cumulative.indices {
                cumulative[index] += item.values[index]
            }

            var area = Path()
            for index in snapshots.indices {
                let point = CGPoint(
                    x: xPosition(for: index, in: plot),
                    y: yPosition(for: cumulative[index], in: plot, maximum: maximum)
                )
                index == 0 ? area.move(to: point) : area.addLine(to: point)
            }
            for index in snapshots.indices.reversed() {
                area.addLine(to: CGPoint(
                    x: xPosition(for: index, in: plot),
                    y: yPosition(for: bottoms[index], in: plot, maximum: maximum)
                ))
            }
            area.closeSubpath()
            let color = ApplicationPalette.color(for: item.identity)
            context.fill(area, with: .color(color.opacity(0.86)))

            var topLine = Path()
            for index in snapshots.indices {
                let point = CGPoint(
                    x: xPosition(for: index, in: plot),
                    y: yPosition(for: cumulative[index], in: plot, maximum: maximum)
                )
                index == 0 ? topLine.move(to: point) : topLine.addLine(to: point)
            }
            context.stroke(topLine, with: .color(color.opacity(0.95)), lineWidth: 0.7)
        }
    }

    private func updateSelection(location: CGPoint, size: CGSize) {
        let plotWidth = size.width - leftInset - rightInset
        guard snapshots.count > 1,
              location.x >= leftInset,
              location.x <= size.width - rightInset,
              location.y >= topInset,
              location.y <= size.height - bottomInset else {
            selectedIndex = nil
            return
        }
        let fraction = (location.x - leftInset) / plotWidth
        let index = min(
            snapshots.count - 1,
            max(0, Int((fraction * CGFloat(snapshots.count - 1)).rounded()))
        )
        if selectedIndex != index {
            selectedIndex = index
        }
    }

    private func tooltipOffset(for index: Int, in size: CGSize) -> CGSize {
        let width: CGFloat = 212
        let plot = CGRect(
            x: leftInset,
            y: topInset,
            width: size.width - leftInset - rightInset,
            height: size.height - topInset - bottomInset
        )
        let locationX = xPosition(for: index, in: plot)
        let x = locationX + width + 18 > size.width ? locationX - width - 10 : locationX + 10
        return CGSize(width: x, height: topInset + 4)
    }

    private func xPosition(for index: Int, in plot: CGRect) -> CGFloat {
        guard snapshots.count > 1 else {
            return plot.minX
        }
        return plot.minX + plot.width * CGFloat(index) / CGFloat(snapshots.count - 1)
    }

    private func yPosition(for value: Double, in plot: CGRect, maximum: Double) -> CGFloat {
        plot.maxY - plot.height * CGFloat(min(1, max(0, value / maximum)))
    }

    private static func niceMaximum(_ value: Double) -> Double {
        guard value > 0 else {
            return 1
        }
        let exponent = floor(log10(value))
        let scale = pow(10, exponent)
        let fraction = value / scale
        let rounded = fraction <= 1 ? 1.0 : fraction <= 2 ? 2.0 : fraction <= 5 ? 5.0 : 10.0
        return rounded * scale
    }
}

struct ChartSeries: Identifiable {
    let identity: ApplicationIdentity
    let values: [Double]

    var id: String { identity.id }

    static func make(from snapshots: [MetricSnapshot], metric: MetricKind) -> [ChartSeries] {
        let totals = snapshots.reduce(into: [ApplicationIdentity: Double]()) { result, snapshot in
            for application in snapshot.applications {
                result[application.identity, default: 0] += metric.value(for: application)
            }
        }
        let topIdentities = totals.sorted { $0.value > $1.value }.prefix(7).map(\.key)
        let indexByID = Dictionary(uniqueKeysWithValues: topIdentities.enumerated().map { ($0.element.id, $0.offset) })
        var values = topIdentities.map { _ in
            [Double](repeating: 0, count: snapshots.count)
        }
        var otherValues = [Double](repeating: 0, count: snapshots.count)

        for (snapshotIndex, snapshot) in snapshots.enumerated() {
            for application in snapshot.applications {
                let value = metric.value(for: application)
                if let seriesIndex = indexByID[application.identity.id] {
                    values[seriesIndex][snapshotIndex] = value
                } else {
                    otherValues[snapshotIndex] += value
                }
            }
        }

        var series = zip(topIdentities, values).map { identity, values in
            ChartSeries(identity: identity, values: values)
        }
        if otherValues.contains(where: { $0 > 0 }) {
            series.append(ChartSeries(
                identity: ApplicationIdentity(id: "other", name: "其他", bundlePath: nil),
                values: otherValues
            ))
        }
        return series
    }
}

struct ChartRenderData {
    let series: [ChartSeries]
    let maximum: Double

    @MainActor
    static func make(from snapshots: [MetricSnapshot], metric: MetricKind) -> ChartRenderData {
        ChartRenderData(
            series: ChartSeries.make(from: snapshots, metric: metric),
            maximum: StackedMetricChart.maximumYValue(for: snapshots, metric: metric)
        )
    }
}

private struct MetricTooltip: View {
    let snapshot: MetricSnapshot
    let metric: MetricKind

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(snapshot.timestamp, format: .dateTime.hour().minute().second())
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            ForEach(sortedApplications.prefix(5)) { application in
                HStack(spacing: 7) {
                    ApplicationIconView(identity: application.identity, size: 20)
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(ApplicationPalette.color(for: application.identity))
                        .frame(width: 6, height: 14)
                    Text(application.identity.name)
                        .font(.caption)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(metric.formatted(metric.value(for: application)))
                        .font(.caption2.monospacedDigit())
                }
            }
        }
        .padding(9)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 7))
        .overlay {
            RoundedRectangle(cornerRadius: 7)
                .stroke(Color.primary.opacity(0.1))
        }
        .shadow(color: .black.opacity(0.16), radius: 12, y: 6)
    }

    private var sortedApplications: [ApplicationMetrics] {
        snapshot.applications.sorted {
            metric.value(for: $0) > metric.value(for: $1)
        }
    }
}
