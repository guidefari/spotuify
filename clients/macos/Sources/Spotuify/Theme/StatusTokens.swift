import SwiftUI

/// Connection / lifecycle status colors. Apple convention is green / yellow
/// / red / gray; keeping them named keeps that intent explicit and lets a
/// future redesign (e.g. a custom status palette) change in one place.
struct StatusTokens: Equatable {
    var ready: Color
    var warning: Color
    var failed: Color
    var idle: Color

    static let `default` = StatusTokens(
        ready: .green,
        warning: .yellow,
        failed: .red,
        idle: .gray)
}
