import AppKit
import SwiftUI

struct ApplicationIconView: View {
    let identity: ApplicationIdentity
    var size: CGFloat = 30

    var body: some View {
        Group {
            if let image = ApplicationIconCache.shared.image(for: identity) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
            } else {
                Image(systemName: fallbackSymbol)
                    .resizable()
                    .scaledToFit()
                    .padding(size * 0.22)
                    .foregroundStyle(.secondary)
                    .background(Color(nsColor: .controlBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: size * 0.2))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private var fallbackSymbol: String {
        let name = identity.name.lowercased()
        if name.contains("window") { return "macwindow" }
        if name.contains("kernel") { return "cpu" }
        if name.contains("spotlight") || name.contains("mds") { return "magnifyingglass" }
        return "gearshape.2"
    }
}

@MainActor
private final class ApplicationIconCache {
    static let shared = ApplicationIconCache()

    private let cache = NSCache<NSString, NSImage>()

    func image(for identity: ApplicationIdentity) -> NSImage? {
        guard let bundlePath = identity.bundlePath else {
            return nil
        }
        let key = bundlePath as NSString
        if let cached = cache.object(forKey: key) {
            return cached
        }
        let image = NSWorkspace.shared.icon(forFile: bundlePath)
        cache.setObject(image, forKey: key)
        return image
    }
}

enum ApplicationPalette {
    static let colors: [Color] = [
        Color(red: 0.03, green: 0.53, blue: 0.72),
        Color(red: 0.94, green: 0.39, blue: 0.24),
        Color(red: 0.46, green: 0.41, blue: 0.85),
        Color(red: 0.89, green: 0.64, blue: 0.00),
        Color(red: 0.13, green: 0.64, blue: 0.40),
        Color(red: 0.70, green: 0.41, blue: 0.21),
        Color(red: 0.28, green: 0.63, blue: 0.66),
        Color(red: 0.83, green: 0.31, blue: 0.51),
        Color(red: 0.31, green: 0.50, blue: 0.82),
        Color(red: 0.42, green: 0.52, blue: 0.53),
    ]

    static func color(for identity: ApplicationIdentity) -> Color {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in identity.id.utf8 {
            hash ^= UInt64(byte)
            hash &*= 1_099_511_628_211
        }
        return colors[Int(hash % UInt64(colors.count))]
    }
}
