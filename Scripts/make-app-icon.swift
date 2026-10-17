// Renders the Tandem app icon at 1024px: the bicycle wheel from the brand
// illustration (Resources/TandemBike.jpg) — the app's logo. The illustration's
// wheel is stylized: a dark charcoal tyre, a slim silver rim, an *empty* (paper)
// interior with no drawn spokes, and a small dark hub ringed by a starburst of
// short ink ticks. Redrawn as clean flat vector onto the day-panel's paper, in
// Apple's icon-grid proportions.
//
// Companion to `make-app-icon.sh`, which turns the result into an .iconset and
// .icns.
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

// Colors sampled from the illustration: day-panel paper, the charcoal tyre, the
// silver rim, and the near-black ink of the hub and ticks.
let paper = NSColor(srgbRed: 0xF1 / 255, green: 0xE9 / 255, blue: 0xD7 / 255, alpha: 1)
let tyre  = NSColor(srgbRed: 0x3A / 255, green: 0x38 / 255, blue: 0x32 / 255, alpha: 1)
let rim   = NSColor(srgbRed: 0xB0 / 255, green: 0xB4 / 255, blue: 0xA9 / 255, alpha: 1)
let ink   = NSColor(srgbRed: 0x21 / 255, green: 0x1D / 255, blue: 0x18 / 255, alpha: 1)

/// A stroked ring of line width `w` whose outer edge sits at `radius`.
func ring(_ center: NSPoint, radius: CGFloat, width: CGFloat) {
    let r = radius - width / 2
    let path = NSBezierPath(ovalIn: NSRect(x: center.x - r, y: center.y - r,
                                           width: 2 * r, height: 2 * r))
    path.lineWidth = width
    path.stroke()
}

/// A filled disc of `radius`.
func disc(_ center: NSPoint, radius: CGFloat) {
    let path = NSBezierPath(ovalIn: NSRect(x: center.x - radius, y: center.y - radius,
                                           width: 2 * radius, height: 2 * radius))
    path.fill()
}

let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

// Paper plate: ~81% of the canvas, ~22.4% corner radius (Apple icon grid).
let inset = canvas * 0.095
let plate = NSRect(x: inset, y: inset, width: canvas - 2 * inset, height: canvas - 2 * inset)
paper.setFill()
NSBezierPath(roundedRect: plate, xRadius: plate.width * 0.2237, yRadius: plate.height * 0.2237).fill()

// The wheel, centred on the plate.
let center = NSPoint(x: plate.midX, y: plate.midY)
let radius = plate.width * 0.365                   // outer (tread) radius
let tyreW = radius * 0.165                         // tyre thickness
let rimW = radius * 0.060                          // rim thickness
let hubR = radius * 0.105                          // hub radius

// Tyre: the bold charcoal silhouette. Its inner edge bounds the rim.
tyre.setStroke()
ring(center, radius: radius, width: tyreW)

// Rim: a slim silver band tucked just inside the tyre.
rim.setStroke()
ring(center, radius: radius - tyreW - rimW / 2, width: rimW)

// Interior stays paper — the illustration draws no spokes.

// Hub: a charcoal disc, a silver collar, and an ink bolt at the centre.
tyre.setFill()
disc(center, radius: hubR)
rim.setFill()
disc(center, radius: hubR * 0.62)
ink.setFill()
disc(center, radius: hubR * 0.26)

// The motion flourish: short ink ticks scattered around the hub (the
// illustration's stand-in for spokes). Fixed values keep renders reproducible;
// the angles, radii, and lengths vary so it reads as a sketch, not a cog.
ink.setStroke()
let ticks: [(deg: CGFloat, r: CGFloat, len: CGFloat)] = [
    (18, 2.35, 0.62), (52, 2.05, 0.48), (86, 2.45, 0.70), (120, 2.15, 0.55),
    (158, 2.40, 0.66), (196, 2.10, 0.50), (230, 2.50, 0.72), (264, 2.20, 0.58),
    (300, 2.05, 0.46), (338, 2.30, 0.64),
]
for t in ticks {
    let a = t.deg * .pi / 180
    let inner = hubR * t.r
    let outer = inner + hubR * t.len
    let start = NSPoint(x: center.x + inner * cos(a), y: center.y + inner * sin(a))
    let end = NSPoint(x: center.x + outer * cos(a), y: center.y + outer * sin(a))
    let p = NSBezierPath()
    p.lineWidth = canvas * 0.006
    p.lineCapStyle = .round
    p.move(to: start)
    p.line(to: end)
    p.stroke()
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
