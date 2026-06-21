#!/usr/bin/env swift
import AppKit
import CoreGraphics

// Renders the Spotuify app icon: a pastel-green squircle (`#A6E3A1` → `#7BC97F`)
// with two beamed eighth notes (♫) — the universal music symbol — punched in
// white. Writes pixel-accurate PNGs and the AppIcon.appiconset Contents.json.
// Run:
//   swift scripts/make_icon.swift Sources/Spotuify/Assets.xcassets/AppIcon.appiconset

let outDir = CommandLine.arguments.count > 1
    ? CommandLine.arguments[1]
    : "Sources/Spotuify/Assets.xcassets/AppIcon.appiconset"

func render(_ px: Int) -> Data {
    let s = CGFloat(px)
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else {
        fatalError("rep")
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let ctx = NSGraphicsContext.current!.cgContext

    ctx.clear(CGRect(x: 0, y: 0, width: s, height: s))

    // Squircle body
    let margin = s * 0.085
    let rect = CGRect(x: margin, y: margin, width: s - 2 * margin, height: s - 2 * margin)
    let radius = rect.width * 0.2237
    let body = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.saveGState()
    ctx.addPath(body)
    ctx.clip()
    let space = CGColorSpaceCreateDeviceRGB()
    // Pastel green gradient: #A6E3A1 (top-left) → #7BC97F (bottom-right)
    let gradient = CGGradient(
        colorsSpace: space,
        colors: [
            CGColor(red: 0.651, green: 0.890, blue: 0.631, alpha: 1),
            CGColor(red: 0.482, green: 0.788, blue: 0.498, alpha: 1),
        ] as CFArray,
        locations: [0, 1])!
    ctx.drawLinearGradient(
        gradient,
        start: CGPoint(x: rect.minX, y: rect.maxY),
        end: CGPoint(x: rect.maxX, y: rect.minY),
        options: [])
    ctx.restoreGState()

    // Beamed eighth notes (♫) — two tilted heads, two stems, one beam.
    let headRx = s * 0.105
    let headRy = s * 0.078
    let note1Cx = s * 0.33
    let note2Cx = s * 0.57
    let noteCy = s * 0.66
    let stemW = s * 0.024
    let beamTopY = s * 0.24
    let beamBotY = s * 0.31

    ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.96))

    // Note heads (tilted ellipses, like a real music note)
    for cx in [note1Cx, note2Cx] {
        ctx.saveGState()
        ctx.translateBy(x: cx, y: noteCy)
        ctx.rotate(by: -0.32)
        ctx.fillEllipse(in: CGRect(x: -headRx, y: -headRy, width: headRx * 2, height: headRy * 2))
        ctx.restoreGState()
    }

    // Stems connect each head's top-right to the beam's bottom.
    let stemBotY = noteCy - headRy * 0.25
    let stemX1 = note1Cx + headRx * 0.72
    let stemX2 = note2Cx + headRx * 0.72
    ctx.fill(CGRect(x: stemX1 - stemW / 2, y: beamBotY, width: stemW, height: stemBotY - beamBotY))
    ctx.fill(CGRect(x: stemX2 - stemW / 2, y: beamBotY, width: stemW, height: stemBotY - beamBotY))

    // Beam (thick line connecting the two stem tops)
    let beamLeft = stemX1
    let beamRight = stemX2 + stemW / 2
    let beam = CGRect(x: beamLeft, y: beamTopY, width: beamRight - beamLeft, height: beamBotY - beamTopY)
    ctx.addPath(CGPath(roundedRect: beam, cornerWidth: (beamBotY - beamTopY) * 0.35, cornerHeight: (beamBotY - beamTopY) * 0.35, transform: nil))
    ctx.fillPath()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let uniqueSizes = [16, 32, 64, 128, 256, 512, 1024]
for px in uniqueSizes {
    let data = render(px)
    let path = "\(outDir)/icon_\(px).png"
    try! data.write(to: URL(fileURLWithPath: path))
    print("wrote \(path)")
}

// Map (size pt, scale) -> pixel file
let entries: [(Int, Int)] = [
    (16, 1), (16, 2), (32, 1), (32, 2),
    (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2),
]
var images = "["
images += entries.map { size, scale in
    let px = size * scale
    return """

    {
      "size" : "\(size)x\(size)",
      "idiom" : "mac",
      "filename" : "icon_\(px).png",
      "scale" : "\(scale)x"
    }
"""
}.joined(separator: ",")
images += "\n  ]"

let contents = """
{
  "images" : \(images),
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
"""
try! contents.write(to: URL(fileURLWithPath: "\(outDir)/Contents.json"), atomically: true, encoding: .utf8)
print("wrote \(outDir)/Contents.json")
