import AppKit
import SwiftUI

struct StackedMetricChart: View {
    let snapshots: [MetricSnapshot]
    let metric: MetricKind

    @State private var hoverLocation: CGPoint?
    @State private var selectedIndex: Int?

    private let leftInset: CGFloat = 42
    private let rightInset: CGFloat = 8
    private let topInset: CGFloat = 8
    private let bottomInset: CGFloat = 24

    var body: some View {
        VStack(spacing: 9) {
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
                            hoverLocation = nil
                            selectedIndex = nil
                        }
                    }

                    if let selectedIndex,
                       snapshots.indices.contains(selectedIndex),
                       let hoverLocation {
                        MetricTooltip(
                            snapshot: snapshots[selectedIndex],
                            metric: metric
                        )
                        .frame(width: 212)
                        .fixedSize(horizontal: false, vertical: true)
                        .offset(tooltipOffset(for: hoverLocation, in: geometry.size))
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
            .frame(minHeight: 250)

            HStack(spacing: 8) {
                Text("最高")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: 34, alignment: .trailing)
                DominanceStrip(snapshots: snapshots, metric: metric)
                    .frame(height: 10)
                    .clipShape(RoundedRectangle(cornerRadius: 2))
            }
        }
    }

    private var series: [ChartSeries] {
        ChartSeries.make(from: snapshots, metric: metric)
    }

    private var maximumYValue: Double {
        let observed = snapshots.map { metric.total(in: $0) }.max() ?? 0
        switch metric {
        case .cpu:
            return max(25, min(100, Self.niceMaximum(observed * 1.12)))
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
        drawGrid(context: context, plot: plot)
        drawAreas(context: context, plot: plot)
        drawTimeLabels(context: context, plot: plot)

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

    private func drawGrid(context: GraphicsContext, plot: CGRect) {
        for tick in 0...4 {
            let fraction = Double(tick) / 4
            let value = maximumYValue * fraction
            let y = plot.maxY - plot.height * CGFloat(fraction)
            var path = Path()
            path.move(to: CGPoint(x: plot.minX, y: y))
            path.addLine(to: CGPoint(x: plot.maxX, y: y))
            context.stroke(
                path,
                with: .color(Color.secondary.opacity(tick == 0 ? 0.24 : 0.13)),
                lineWidth: 1
            )

            let label = Text(metric.axisLabel(value))
                .font(.system(size: 9, design: .rounded))
                .foregroundStyle(.secondary)
            context.draw(label, at: CGPoint(x: plot.minX - 8, y: y), anchor: .trailing)
        }
    }

    private func drawAreas(context: GraphicsContext, plot: CGRect) {
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
                    y: yPosition(for: cumulative[index], in: plot)
                )
                index == 0 ? area.move(to: point) : area.addLine(to: point)
            }
            for index in snapshots.indices.reversed() {
                area.addLine(to: CGPoint(
                    x: xPosition(for: index, in: plot),
                    y: yPosition(for: bottoms[index], in: plot)
                ))
            }
            area.closeSubpath()
            let color = ApplicationPalette.color(for: item.identity)
            context.fill(area, with: .color(color.opacity(0.86)))

            var topLine = Path()
            for index in snapshots.indices {
                let point = CGPoint(
                    x: xPosition(for: index, in: plot),
                    y: yPosition(for: cumulative[index], in: plot)
                )
                index == 0 ? topLine.move(to: point) : topLine.addLine(to: point)
            }
            context.stroke(topLine, with: .color(color.opacity(0.95)), lineWidth: 0.7)
        }
    }

    private func drawTimeLabels(context: GraphicsContext, plot: CGRect) {
        guard snapshots.count > 1 else {
            return
        }
        let indexes = [0, 1, 2, 3, 4].map {
            Int((Double(snapshots.count - 1) * Double($0) / 4).rounded())
        }
        for (position, index) in indexes.enumerated() {
            let date = snapshots[index].timestamp.formatted(.dateTime.hour().minute())
            let text = Text(date)
                .font(.system(size: 9, design: .rounded))
                .foregroundStyle(.secondary)
            let anchor: UnitPoint = position == 0 ? .topLeading : position == 4 ? .topTrailing : .top
            context.draw(text, at: CGPoint(x: xPosition(for: index, in: plot), y: plot.maxY + 7), anchor: anchor)
        }
    }

    private func updateSelection(location: CGPoint, size: CGSize) {
        let plotWidth = size.width - leftInset - rightInset
        guard snapshots.count > 1,
              location.x >= leftInset,
              location.x <= size.width - rightInset,
              location.y >= topInset,
              location.y <= size.height - bottomInset else {
            hoverLocation = nil
            selectedIndex = nil
            return
        }
        let fraction = (location.x - leftInset) / plotWidth
        selectedIndex = min(
            snapshots.count - 1,
            max(0, Int((fraction * CGFloat(snapshots.count - 1)).rounded()))
        )
        hoverLocation = location
    }

    private func tooltipOffset(for location: CGPoint, in size: CGSize) -> CGSize {
        let width: CGFloat = 212
        let x = location.x + width + 18 > size.width ? location.x - width - 10 : location.x + 10
        let y = min(max(topInset, location.y - 28), max(topInset, size.height - 174))
        return CGSize(width: x, height: y)
    }

    private func xPosition(for index: Int, in plot: CGRect) -> CGFloat {
        guard snapshots.count > 1 else {
            return plot.minX
        }
        return plot.minX + plot.width * CGFloat(index) / CGFloat(snapshots.count - 1)
    }

    private func yPosition(for value: Double, in plot: CGRect) -> CGFloat {
        plot.maxY - plot.height * CGFloat(min(1, max(0, value / maximumYValue)))
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

private struct ChartSeries: Identifiable {
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
        let topIDs = Set(topIdentities.map(\.id))
        var series = topIdentities.map { identity in
            ChartSeries(
                identity: identity,
                values: snapshots.map { snapshot in
                    snapshot.applications.first { $0.identity.id == identity.id }
                        .map(metric.value(for:)) ?? 0
                }
            )
        }

        let otherValues = snapshots.map { snapshot in
            snapshot.applications
                .filter { !topIDs.contains($0.identity.id) }
                .reduce(0) { $0 + metric.value(for: $1) }
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

private struct DominanceStrip: View {
    let snapshots: [MetricSnapshot]
    let metric: MetricKind

    var body: some View {
        Canvas { context, size in
            guard !snapshots.isEmpty else {
                return
            }
            let width = size.width / CGFloat(snapshots.count)
            for (index, snapshot) in snapshots.enumerated() {
                guard let application = snapshot.applications.max(by: {
                    metric.value(for: $0) < metric.value(for: $1)
                }) else {
                    continue
                }
                context.fill(
                    Path(CGRect(
                        x: CGFloat(index) * width,
                        y: 0,
                        width: ceil(width) + 0.5,
                        height: size.height
                    )),
                    with: .color(ApplicationPalette.color(for: application.identity))
                )
            }
        }
        .background(Color.secondary.opacity(0.12))
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
