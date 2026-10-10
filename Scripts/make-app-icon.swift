// Renders the Tandem app icon at 1024px: a white `checklist` SF Symbol (the
// same glyph the menu bar item uses) on a rounded-square blue gradient, in
// Apple's icon-grid proportions. Companion to `make-app-icon.sh`, which turns
// the result into an .iconset and .icns.
//
// Usage: swift Scripts/make-app-icon.swift <output-dir>
// Writes <output-dir>/1024.png

import AppKit

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write("usage: make-app-icon.swift <output-dir>\n".data(using: .utf8)!)
    exit(2)
}
let outDir = URL(fileURLWithPath: args[1], isDirectory: true)
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

let canvas: CGFloat = 1024

let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

// Rounded-square plate: ~81% of the canvas, ~22.4% corner radius (Apple icon grid).
let inset = canvas * 0.095
let plate = NSRect(x: inset, y: inset, width: canvas - 2 * inset, height: canvas - 2 * inset)
let path = NSBezierPath(roundedRect: plate, xRadius: plate.width * 0.2237, yRadius: plate.height * 0.2237)
let gradient = NSGradient(colors: [
    NSColor(calibratedRed: 0.36, green: 0.56, blue: 0.98, alpha: 1.0),
    NSColor(calibratedRed: 0.16, green: 0.33, blue: 0.83, alpha: 1.0),
])!
gradient.draw(in: path, angle: -90)

// White `checklist` glyph, centered on the plate.
func tinted(_ source: NSImage, _ color: NSColor) -> NSImage {
    let out = NSImage(size: source.size)
    out.lockFocus()
    color.set()
    let rect = NSRect(origin: .zero, size: source.size)
    source.draw(in: rect)
    rect.fill(using: .sourceAtop)
    out.unlockFocus()
    return out
}

let config = NSImage.SymbolConfiguration(pointSize: plate.height * 0.62, weight: .semibold)
if let symbol = NSImage(systemSymbolName: "checklist", accessibilityDescription: nil)?
    .withSymbolConfiguration(config) {
    let glyph = tinted(symbol, .white)
    let size = glyph.size
    let origin = NSPoint(x: plate.midX - size.width / 2, y: plate.midY - size.height / 2)
    glyph.draw(in: NSRect(origin: origin, size: size))
}

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else {
    FileHandle.standardError.write("error: failed to render icon\n".data(using: .utf8)!)
    exit(1)
}
try png.write(to: outDir.appendingPathComponent("1024.png"))
print("wrote \(outDir.appendingPathComponent("1024.png").path)")
