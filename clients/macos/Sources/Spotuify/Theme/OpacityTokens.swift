import SwiftUI

/// Numeric scale of opacities used to tint semantic colors (`.tint`,
/// `.primary`, `.black`, `.white`, etc.). Centralised so the visual rhythm
/// stays consistent across views and so a future redesign only has to change
/// the scale, not every call site.
///
/// The naming is the value as a percentage (`level40` = 0.40) so the call
/// site reads naturally without a second lookup table. Values are rounded to
/// two decimal places to match what the codebase already used.
enum OpacityTokens {
    static let level04: Double = 0.04
    static let level05: Double = 0.05
    static let level06: Double = 0.06
    static let level08: Double = 0.08
    static let level12: Double = 0.12
    static let level15: Double = 0.15
    static let level18: Double = 0.18
    static let level20: Double = 0.20
    static let level22: Double = 0.22
    static let level35: Double = 0.35
    static let level38: Double = 0.38
    static let level40: Double = 0.40
    static let level45: Double = 0.45
    static let level50: Double = 0.50
    static let level55: Double = 0.55
    static let level60: Double = 0.60
    static let level70: Double = 0.70
    static let level78: Double = 0.78
    static let level80: Double = 0.80
    static let level85: Double = 0.85
    static let level90: Double = 0.90
    static let level92: Double = 0.92
    static let level96: Double = 0.96
}
