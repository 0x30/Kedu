import SwiftUI

@main
struct KeduMonitorApp: App {
    @State private var store = MonitorStore()

    var body: some Scene {
        WindowGroup("刻度") {
            ContentView()
                .environment(store)
                .frame(minWidth: 720, minHeight: 460)
        }
        .defaultSize(width: 900, height: 560)
        .windowResizability(.contentMinSize)
    }
}
