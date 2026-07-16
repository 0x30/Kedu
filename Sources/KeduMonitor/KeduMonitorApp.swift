import SwiftUI

@main
struct KeduMonitorApp: App {
    var body: some Scene {
        WindowGroup("刻度") {
            ContentView()
                .frame(minWidth: 760, minHeight: 480)
        }
        .defaultSize(width: 980, height: 620)
        .windowResizability(.contentMinSize)
    }
}

private struct ContentView: View {
    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "gauge.with.dots.needle.67percent")
                .font(.system(size: 34, weight: .light))
                .foregroundStyle(.teal)
            Text("刻度")
                .font(.title2.weight(.semibold))
            Text("系统监控正在初始化")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}
