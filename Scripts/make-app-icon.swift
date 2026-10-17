// Renders the Tandem app icon at 1024px: a bicycle wheel — the brand mark — in
// the Ligne Claire register (flat fills, uniform ink linework, no gradients) on
// the day panel's paper, in Apple's icon-grid proportions. The mark is drawn as
// a vector, so it stays crisp at every size in the iconset.
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

// The Ligne Claire palette (sampled in LigneTheme.swift from the bike art):
// day-panel paper, near-black ink, the sage frame accent, the rust saddle pop.
let paper = NSColor(srgbRed: 0xF1 / 255, green: 0xE9 / 255, blue: 0xD7 / 255, alpha: 1)
let ink   = NSColor(srgbRed: 0x21 / 255, green: 0x1D / 255, blue: 0x18 / 255, alpha: 1)
let sage  = NSColor(srgbRed: 0x6E / 255, green: 0x7D / 255, blue: 0x6E / 255, alpha: 1)
let rust  = NSColor(srgbRed: 0xA9 / 255, green: 0x74 / 255, blue: 0x5C / 255, alpha: 1)

/// A stroked ring of line width `w` whose outer edge sits at `radius`.
func ring(_ center: NSPoint, radius: CGFloat, width: CGFloat) {
    let r = radius - width / 2
    let path = NSBezierPath(ovalIn: NSRect(x: center.x - r, y: center.y - r,
                                           width: 2 * r, height: 2 * r))
    path.lineWidth = width
    path.stroke()
}

let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

// Paper plate: ~81% of the canvas, ~22.4% corner radius (Apple icon grid).
// Day-panel paper: an .icns is a single bitmap, so the light plate is the one
// that reads in both appearances.
let inset = canvas * 0.095
let plate = NSRect(x: inset, y: inset, width: canvas - 2 * inset, height: canvas - 2 * inset)
paper.setFill()
NSBezierPath(roundedRect: plate, xRadius: plate.width * 0.2237, yRadius: plate.height * 0.2237).fill()

// The wheel, centred on the plate.
let center = NSPoint(x: plate.midX, y: plate.midY)
let tireR  = plate.width * 0.335      // outer (tread) radius
let tireW  = canvas * 0.058           // tire thickness
let rimW   = canvas * 0.019           // rim thickness
let spokeW = canvas * 0.013           // spoke stroke
let hubR   = plate.width * 0.062      // hub radius

// Tire: one bold ink ring — the silhouette that reads first.
ink.setStroke()
ring(center, radius: tireR, width: tireW)

// Rim: a sage ring tucked just inside the tire (the frame accent).
sage.setStroke()
ring(center, radius: tireR - tireW - rimW / 2, width: rimW)

// Spokes: thin ink lines from the hub out to the rim, offset so none sits
// dead-vertical (a wheel, not a clock face).
let spokeOuter = tireR - tireW - rimW
let spokes = 10
let spoke = NSBezierPath()
spoke.lineWidth = spokeW
for i in 0..<spokes {
    let a = (CGFloat(i) / CGFloat(spokes)) * 2 * .pi + .pi / CGFloat(spokes)
    spoke.move(to: NSPoint(x: center.x + hubR * cos(a), y: center.y + hubR * sin(a)))
    spoke.line(to: NSPoint(x: center.x + spokeOuter * cos(a), y: center.y + spokeOuter * sin(a)))
}
ink.setStroke()
spoke.stroke()

// Hub: an ink disc with a paper axle, and a rust collar nut for the brand pop.
let hub = NSBezierPath(ovalIn: NSRect(x: center.x - hubR, y: center.y - hubR,
                                      width: 2 * hubR, height: 2 * hubR))
ink.setFill()
hub.fill()
let axleR = hubR * 0.40
let axle = NSBezierPath(ovalIn: NSRect(x: center.x - axleR, y: center.y - axleR,
                                       width: 2 * axleR, height: 2 * axleR))
paper.setFill()
axle.fill()
// Collar nut: a small rust ring around the axle.
rust.setStroke()
ring(center, radius: hubR * 0.72, width: canvas * 0.012)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else {
    FileHandle.standardError.write("error: failed to render icon\n".data(using: .utf8)!)
    exit(1)
}
try png.write(to: outDir.appendingPathComponent("1024.png"))
print("wrote \(outDir.appendingPathComponent("1024.png").path)")
