import SwiftUI
import SpotuifyKit

/// Render styles for the spectrum visualizer.
enum VizStyle: String, CaseIterable, Identifiable {
    case bars, circular, wave
    var id: String { rawValue }
    var icon: String {
        switch self {
        case .bars: "waveform"
        case .circular: "circle.dotted"
        case .wave: "wave.3.right"
        }
    }
}

/// A spectrum visualizer driven by the daemon's `spectrum-frame` events, in one
/// of several styles. Bars settle to flat when playback is paused. The palette
/// accent drives a gradient + glow so it reads as part of the album-themed UI
/// rather than a generic meter.
struct VisualizerView: View {
    @Environment(AppModel.self) private var model
    var style: VizStyle = .bars
    /// Concrete tint (Canvas styles can't read `.tint`); defaults to the accent.
    var tint: Color = Color("AccentColor")

    private var barCount: Int { VizStore.bandCount }

    var body: some View {
        let bands = model.viz.bands
        let live = model.player.isPlaying
        let values = (0..<barCount).map { index in
            live ? min(1.0, max(0.02, Double(bands[safe: index] ?? 0))) : 0.02
        }
        switch style {
        case .bars: BarsViz(values: values, tint: tint)
        case .circular: CircularViz(values: values, tint: tint)
        case .wave: WaveViz(values: values, tint: tint)
        }
    }
}

/// Mirrored gradient bars with rounded caps, a soft accent glow, and a glassy
/// highlight along the top — springy so levels feel alive.
private struct BarsViz: View {
    let values: [Double]
    let tint: Color

    var body: some View {
        GeometryReader { geo in
            let spacing: CGFloat = 6
            let count = max(values.count, 1)
            let barWidth = min(
                max(3, (geo.size.width - spacing * CGFloat(count - 1)) / CGFloat(count)), 22)
            // Height is driven by the band level only (NOT bar width) so a wider
            // window can't make the bars grow taller.
            HStack(alignment: .center, spacing: spacing) {
                ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                    Capsule()
                        .fill(
                            LinearGradient(
                                colors: [tint, tint.opacity(OpacityTokens.level45)],
                                startPoint: .top, endPoint: .bottom)
                        )
                        .overlay(alignment: .top) {
                            // Glassy sheen on the cap.
                            Capsule()
                                .fill(.white.opacity(OpacityTokens.level35))
                                .frame(height: barWidth)
                                .blur(radius: 1)
                        }
                        .frame(width: barWidth, height: max(5, CGFloat(value) * geo.size.height))
                        .animation(.spring(response: 0.34, dampingFraction: 0.62), value: value)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
            .shadow(color: tint.opacity(OpacityTokens.level45), radius: 12)
            .shadow(color: tint.opacity(OpacityTokens.level25), radius: 3)
        }
    }
}

/// Radiating gradient spokes with rounded tips, a glowing core orb, and a faint
/// guide ring — a polished take on the radial meter.
private struct CircularViz: View {
    let values: [Double]
    let tint: Color

    var body: some View {
        Canvas { ctx, size in
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            let baseRadius = min(size.width, size.height) * 0.20
            let maxLen = min(size.width, size.height) * 0.28
            let count = values.count
            guard count > 0 else { return }

            // Glowing core orb.
            let orb = Path(ellipseIn: CGRect(
                x: center.x - baseRadius * 0.55, y: center.y - baseRadius * 0.55,
                width: baseRadius * 1.1, height: baseRadius * 1.1))
            ctx.fill(orb, with: .radialGradient(
                Gradient(colors: [tint.opacity(OpacityTokens.level90), tint.opacity(0.0)]),
                center: center, startRadius: 0, endRadius: baseRadius * 0.8))

            // Faint guide ring.
            let ring = Path(ellipseIn: CGRect(
                x: center.x - baseRadius, y: center.y - baseRadius,
                width: baseRadius * 2, height: baseRadius * 2))
            ctx.stroke(ring, with: .color(tint.opacity(OpacityTokens.level22)), lineWidth: 1.5)

            for (index, value) in values.enumerated() {
                let angle = (Double(index) / Double(count)) * 2 * .pi - .pi / 2
                let inner = baseRadius + 4
                let outer = baseRadius + 4 + maxLen * value
                var spoke = Path()
                spoke.move(to: CGPoint(
                    x: center.x + cos(angle) * inner, y: center.y + sin(angle) * inner))
                spoke.addLine(to: CGPoint(
                    x: center.x + cos(angle) * outer, y: center.y + sin(angle) * outer))
                ctx.stroke(
                    spoke,
                    with: .linearGradient(
                        Gradient(colors: [tint.opacity(OpacityTokens.level50), tint]),
                        startPoint: CGPoint(x: center.x + cos(angle) * inner, y: center.y + sin(angle) * inner),
                        endPoint: CGPoint(x: center.x + cos(angle) * outer, y: center.y + sin(angle) * outer)),
                    style: StrokeStyle(lineWidth: 4, lineCap: .round))
                // Bright tip dot.
                let tip = CGPoint(x: center.x + cos(angle) * outer, y: center.y + sin(angle) * outer)
                let dot = Path(ellipseIn: CGRect(x: tip.x - 2, y: tip.y - 2, width: 4, height: 4))
                ctx.fill(dot, with: .color(.white.opacity(OpacityTokens.level90)))
            }
        }
    }
}

/// A smooth, mirrored ribbon: a gradient-filled body between the top and bottom
/// curves with a bright accent stroke — flowing rather than a thin trace.
private struct WaveViz: View {
    let values: [Double]
    let tint: Color

    var body: some View {
        Canvas { ctx, size in
            let count = values.count
            guard count > 1 else { return }
            let mid = size.height / 2
            let step = size.width / CGFloat(count - 1)
            func amp(_ i: Int) -> CGFloat { CGFloat(values[i]) * size.height * 0.42 }

            // Smooth top + bottom curves via quad curves through midpoints.
            func curve(sign: CGFloat) -> Path {
                var p = Path()
                p.move(to: CGPoint(x: 0, y: mid - sign * amp(0)))
                for i in 1..<count {
                    let x = CGFloat(i) * step
                    let y = mid - sign * amp(i)
                    let px = CGFloat(i - 1) * step
                    let py = mid - sign * amp(i - 1)
                    p.addQuadCurve(to: CGPoint(x: x, y: y),
                                   control: CGPoint(x: (px + x) / 2, y: py))
                }
                return p
            }
            let top = curve(sign: 1)
            let bottom = curve(sign: -1)

            // Filled ribbon between the two curves.
            var fill = top
            fill.addLine(to: CGPoint(x: size.width, y: mid + amp(count - 1)))
            for i in stride(from: count - 2, through: 0, by: -1) {
                fill.addLine(to: CGPoint(x: CGFloat(i) * step, y: mid + amp(i)))
            }
            fill.closeSubpath()
            ctx.fill(fill, with: .linearGradient(
                Gradient(colors: [tint.opacity(OpacityTokens.level45), tint.opacity(OpacityTokens.level08)]),
                startPoint: CGPoint(x: 0, y: 0), endPoint: CGPoint(x: 0, y: size.height)))

            // Bright edges.
            let stroke = StrokeStyle(lineWidth: 2.5, lineCap: .round, lineJoin: .round)
            ctx.stroke(top, with: .color(tint), style: stroke)
            ctx.stroke(bottom, with: .color(tint.opacity(OpacityTokens.level60)), style: stroke)
        }
        .shadow(color: tint.opacity(OpacityTokens.level40), radius: 10)
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
