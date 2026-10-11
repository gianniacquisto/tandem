import AppKit
import SwiftUI

/// The brand asset (ticket 03, 2026-10-11): the tandem-bike photo — the
/// app's logo. It ships as `Contents/Resources/TandemBike.jpg` (copied in by
/// `Scripts/make-app-bundle.sh`); from a source checkout it is found in the
/// repo's `Resources/`.
enum BrandAsset {
    /// The tandem-bike photo, or nil when the asset is missing — callers fall
    /// back to the vector bike, so no surface ever renders empty.
    static let bikePhoto: Image? = {
        if let url = Bundle.main.url(forResource: "TandemBike", withExtension: "jpg"),
           let nsImage = NSImage(contentsOf: url) {
            return Image(nsImage: nsImage)
        }
        // Dev fallback (swift run): walk up from this source file to the repo root.
        var dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        for _ in 0..<6 {
            let candidate = dir.appendingPathComponent("Resources/TandemBike.jpg")
            if let nsImage = NSImage(contentsOf: candidate) {
                return Image(nsImage: nsImage)
            }
            dir = dir.deletingLastPathComponent()
        }
        return nil
    }()

    /// The photo's aspect (2400×1792): rendered height = width × this.
    static let bikePhotoAspect: CGFloat = 1792.0 / 2400.0
}
