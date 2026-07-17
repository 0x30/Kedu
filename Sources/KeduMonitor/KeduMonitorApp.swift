import SwiftUI

@main
@MainActor
struct KeduMonitorApp: App {
    @State private var store: MonitorStore

    init() {
        let store = MonitorStore()
        store.start()
        _store = State(initialValue: store)
    }

    var body: some Scene {
        WindowGroup("刻度", id: "main") {
            ContentView()
                .environment(store)
                .frame(minWidth: 720, minHeight: 460)
        }
        .defaultSize(width: 900, height: 560)
        .windowResizability(.contentMinSize)

        MenuBarExtra("刻度", systemImage: "gauge.with.dots.needle.67percent") {
            MenuBarLauncher()
                .environment(store)
        }
        .menuBarExtraStyle(.window)
    }
}

private struct MenuBarLauncher: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(spacing: 10) {
            Button("显示刻度", systemImage: "macwindow") {
                openWindow(id: "main")
                NSApp.activate(ignoringOtherApps: true)
            }
            .buttonStyle(.borderedProminent)
            .tint(.teal)

            Button("退出", systemImage: "power") {
                NSApp.terminate(nil)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
        }
        .padding(12)
        .frame(width: 180)
    }
}
