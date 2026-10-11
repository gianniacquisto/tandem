// Renders the Tandem app icon at 1024px (ticket 03, 2026-10-11): the
// tandem-bike photo — the whole frame stays visible, no crop — letterboxed
// on the day panel's paper, in Apple's icon-grid proportions. Companion to
// `make-app-icon.sh`, which turns the result into an .iconset and .icns.
//
// Usage: swift Scripts/make-app-icon.swift <output-dir>
// Writes <output-dir>/1024.png

import AppKit

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write("usage: swift Scripts/make-app-icon.swift <output-dir>\n".data(using: .utf8)!)
    exit(2)
}
let outDir = URL(fileURLWithPath: args[1], isDirectory: true)
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

let canvas: CGFloat = 1024

// The brand photo, whole frame (no crop).
let photoURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()   // Scripts/
    .deletingLastPathComponent()   // repo root
    .appendingPathComponent("Resources/TandemBike.jpg")
guard let photo = NSImage(contentsOf: photoURL) else {
    FileHandle.standardError.write("error: missing \(photoURL.path)\n".data(using: .utf8)!)
    exit(1)
}
// Pixel dimensions (DPI metadata must not skew the fit).
let pw = CGFloat(photo.representations.first?.pixelsWide ?? 2400)
let ph = CGFloat(photo.representations.first?.pixelsHigh ?? 1792)

let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

// Paper plate: ~81% of the canvas, ~22.4% corner radius (Apple icon grid).
// Day-panel paper (0xF5F0E6): an .icns is a single bitmap, so the light
// plate is the one that reads in both appearances.
let inset = canvas * 0.095
let plate = NSRect(x: inset, y: inset, width: canvas - 2 * inset, height: canvas - 2 * inset)
NSColor(srgbRed: 0xF5 / 255, green: 0xF0 / 255, blue: 0xE6 / 255, alpha: 1).setFill()
NSBezierPath(roundedRect: plate, xRadius: plate.width * 0.2237, yRadius: plate.height * 0.2237).fill()

// Letterbox the photo inside the plate, centred, with a quiet paper margin.
let pad = canvas * 0.055
let scale = min((plate.width - 2 * pad) / pw, (plate.height - 2 * pad) / ph)
let w = pw * scale
let h = ph * scale
photo.draw(in: NSRect(x: plate.midX - w / 2, y: plate.midY - h / 2, width: w, height: h),
           from: .zero, operation: .sourceOver, fraction: 1)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else {
    FileHandle.standardError.write("error: failed to render icon\n".data(using: .utf8)!)
    exit(1)
}
try png.write(to: outDir.appendingPathComponent("1024.png"))
print("wrote \(outDir.appendingPathComponent("1024.png").path)")
