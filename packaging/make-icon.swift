import Foundation

guard CommandLine.arguments.count == 3 else {
    fputs("usage: make-icon.swift SOURCE_APPICONSET OUTPUT_ICONSET\n", stderr)
    exit(64)
}

let source = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let output = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
let files = [
    ("icon_16.png", "icon_16x16.png"),
    ("icon_32.png", "icon_16x16@2x.png"),
    ("icon_32.png", "icon_32x32.png"),
    ("icon_64.png", "icon_32x32@2x.png"),
    ("icon_128.png", "icon_128x128.png"),
    ("icon_256.png", "icon_128x128@2x.png"),
    ("icon_256.png", "icon_256x256.png"),
    ("icon_512.png", "icon_256x256@2x.png"),
    ("icon_512.png", "icon_512x512.png"),
    ("icon_1024.png", "icon_512x512@2x.png"),
]

let manager = FileManager.default
try manager.createDirectory(at: output, withIntermediateDirectories: true)
for (sourceName, outputName) in files {
    let sourceFile = source.appendingPathComponent(sourceName)
    let outputFile = output.appendingPathComponent(outputName)
    guard manager.fileExists(atPath: sourceFile.path) else {
        fputs("missing icon asset: \(sourceFile.path)\n", stderr)
        exit(66)
    }
    try manager.copyItem(at: sourceFile, to: outputFile)
}
