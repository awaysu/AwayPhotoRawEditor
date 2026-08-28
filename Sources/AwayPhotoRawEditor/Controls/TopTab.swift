import AppKit
import AwayRawCore

/// The ribbon tab strip at the top of the Tools panel (裁切 / 漸層 / 修護).
final class TopTab: NSView {
    var tabs: [String] = [] { didSet { needsDisplay = true } }
    var selectedIndex = 0 { didSet { needsDisplay = true } }
    var onSelect: ((Int) -> Void)?

    private var hoverIndex = -1 { didSet { needsDisplay = true } }
    private var trackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let t = trackingArea { removeTrackingArea(t) }
        let t = NSTrackingArea(rect: bounds,
                               options: [.mouseEnteredAndExited, .mouseMoved,
                                         .activeInKeyWindow, .inVisibleRect],
                               owner: self)
        addTrackingArea(t)
        trackingArea = t
    }

    private func indexAt(_ p: NSPoint) -> Int {
        guard !tabs.isEmpty else { return -1 }
        let w = bounds.width / CGFloat(tabs.count)
        let i = Int(p.x / w)
        return (i >= 0 && i < tabs.count) ? i : -1
    }

    override func mouseMoved(with event: NSEvent) {
        hoverIndex = indexAt(convert(event.locationInWindow, from: nil))
    }
    override func mouseExited(with event: NSEvent) { hoverIndex = -1 }

    override func mouseUp(with event: NSEvent) {
        let i = indexAt(convert(event.locationInWindow, from: nil))
        guard i >= 0 else { return }
        selectedIndex = i
        onSelect?(i)
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg)
        guard !tabs.isEmpty else { return }
        let w = bounds.width / CGFloat(tabs.count)
        for (i, t) in tabs.enumerated() {
            let r = NSRect(x: CGFloat(i) * w, y: 0, width: w, height: bounds.height)
            if i == selectedIndex {
                Theme.fill(r.insetBy(dx: 2, dy: 2), Theme.panelBg2, radius: 3)
            } else if i == hoverIndex {
                Theme.fill(r.insetBy(dx: 2, dy: 2), Theme.panelBg2.withAlphaComponent(0.5), radius: 3)
            }
            Theme.drawCentered(L.t(t), in: r, font: Theme.normal,
                               color: i == selectedIndex ? Theme.text : Theme.textDim)
            if i == selectedIndex {
                Theme.fill(NSRect(x: r.minX + 8, y: r.maxY - 2, width: r.width - 16, height: 2),
                           Theme.accent)
            }
        }
    }
}

/// The RGB histogram, with the channel means underneath.
final class HistogramView: NSView {
    var histogram: Histogram? { didSet { needsDisplay = true } }
    var mean: (r: Double, g: Double, b: Double)? { didSet { needsDisplay = true } }

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg)

        let meanH: CGFloat = 18
        let plot = NSRect(x: 0, y: 0, width: bounds.width, height: bounds.height - meanH)
        Theme.fill(plot, Theme.viewerBg, radius: 3)

        if let h = histogram {
            // Additive blending so overlapping channels read as white, the way every
            // photo editor draws this.
            NSGraphicsContext.current?.compositingOperation = .plusLighter
            drawChannel(h.r, h.max, in: plot, color: NSColor(srgbRed: 0.85, green: 0.20, blue: 0.20, alpha: 0.75))
            drawChannel(h.g, h.max, in: plot, color: NSColor(srgbRed: 0.20, green: 0.80, blue: 0.30, alpha: 0.75))
            drawChannel(h.b, h.max, in: plot, color: NSColor(srgbRed: 0.25, green: 0.45, blue: 0.95, alpha: 0.75))
            NSGraphicsContext.current?.compositingOperation = .sourceOver
        }
        Theme.stroke(plot, Theme.border, radius: 3)

        if let m = mean {
            let y = bounds.height - meanH / 2
            let text = String(format: "R %.1f   G %.1f   B %.1f", m.r, m.g, m.b)
            Theme.drawLeft(text, x: 2, midY: y, font: Theme.mono, color: Theme.textDim)
        }
    }

    private func drawChannel(_ bins: [Int], _ maxValue: Int, in r: NSRect, color: NSColor) {
        guard maxValue > 0 else { return }
        let path = NSBezierPath()
        path.move(to: NSPoint(x: r.minX, y: r.maxY))
        let step = r.width / 255.0
        for i in 0..<256 {
            let v = min(1.0, Double(bins[i]) / Double(maxValue))
            let x = r.minX + CGFloat(i) * step
            let y = r.maxY - CGFloat(v) * (r.height - 2)
            path.line(to: NSPoint(x: x, y: y))
        }
        path.line(to: NSPoint(x: r.maxX, y: r.maxY))
        path.close()
        color.setFill()
        path.fill()
    }
}
