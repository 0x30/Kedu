import Foundation

enum SessionExporter {
    static func csvData(for snapshots: [MetricSnapshot]) -> Data {
        var rows = [
            [
                "timestamp",
                "application",
                "bundle_path",
                "pids",
                "cpu_percent",
                "memory_bytes",
                "disk_read_bytes_per_second",
                "disk_write_bytes_per_second",
                "network_download_bytes_per_second",
                "network_upload_bytes_per_second",
            ],
        ]
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

        for snapshot in snapshots {
            for application in snapshot.applications {
                rows.append([
                    formatter.string(from: snapshot.timestamp),
                    application.identity.name,
                    application.identity.bundlePath ?? "",
                    application.processIDs.map(String.init).joined(separator: ";"),
                    String(format: "%.4f", application.cpuPercent),
                    String(application.memoryBytes),
                    String(format: "%.2f", application.diskReadBytesPerSecond),
                    String(format: "%.2f", application.diskWriteBytesPerSecond),
                    String(format: "%.2f", application.networkDownloadBytesPerSecond),
                    String(format: "%.2f", application.networkUploadBytesPerSecond),
                ])
            }
        }

        let csv = rows.map { row in
            row.map(escape).joined(separator: ",")
        }
        .joined(separator: "\n")
        return Data(("\u{FEFF}" + csv + "\n").utf8)
    }

    private static func escape(_ value: String) -> String {
        "\"\(value.replacingOccurrences(of: "\"", with: "\"\""))\""
    }
}
