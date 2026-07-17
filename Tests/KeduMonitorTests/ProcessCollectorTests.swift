import Foundation
import Testing
@testable import KeduMonitor

@Suite("ProcessCollector")
struct ProcessCollectorTests {
    @Test("samples running processes")
    func samplesRunningProcesses() async {
        let snapshot = await ProcessCollector().sample()
        #expect(!snapshot.applications.isEmpty)
        #expect(snapshot.totalMemoryBytes > 0)
        #expect(snapshot.applications.allSatisfy { !$0.processes.isEmpty })
        #expect(snapshot.applications.allSatisfy { application in
            application.memoryBytes == application.processes.reduce(0) { $0 + $1.memoryBytes }
        })
    }

    @Test("extracts the outermost app bundle")
    func extractsAppRoot() {
        let helper = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper"
        #expect(ProcessCollector.appRootPath(for: helper) == "/Applications/Google Chrome.app")
    }

    @Test("ignores executables outside app bundles")
    func ignoresNonAppPath() {
        #expect(ProcessCollector.appRootPath(for: "/usr/libexec/WindowServer") == nil)
    }

    @Test("counter reset does not underflow")
    func handlesCounterReset() {
        #expect(ProcessCollector.positiveDelta(4, 9) == 0)
        #expect(ProcessCollector.positiveDelta(12, 9) == 3)
    }

    @Test("parses process arguments from kern procargs data")
    func parsesArguments() {
        var count = Int32(3)
        var data = withUnsafeBytes(of: &count) { Data($0) }
        data.append(contentsOf: "/usr/bin/node\0\0node\0server.js\0--port=3000\0PATH=/bin\0".utf8)

        #expect(ProcessCollector.parseArguments(data) == ["node", "server.js", "--port=3000"])
    }
}
