import AppKit
import AwayRawCore

/// The label + value + track slider used throughout the adjustment panels.
///
/// Behaviour matches the Windows control: drag or click the track to set, double-click to
/// return to the default, scroll to nudge, and click the value text to type a number.
/// `editBegin` fires once at the start of a gesture so the owner can push an undo step
/// and capture the multi-selection sync targets *before* the value moves.
final class AdjustmentSlider: NSView {

    var label = "" { didSet { needsDisplay = true } }
    var minValue: Double = -100
    var maxValue: Double = 100
    var defaultValue: Double = 0
    /// Scroll increment; nil derives it from the range, as the Windows control does.
    var wheelStep: Double?
    /// Decimal places shown (0 = integer), mirroring the C# format strings.
    var decimals = 0 { didSet { needsDisplay = true } }
    var bipolar = false
    /// Draw the track right-to-left. Used by the crop angle, whose UI value is the
    /// negative of the stored one.
    var reverse = false
    var gradient: SliderGradient = .none

    var onEditBegin: (() -> Void)?
    var onValueChanged: ((Double) -> Void)?

    private var _value: Double = 0
    var value: Double {
        get { _value }
        set { setValue(newValue, notify: true) }
    }

    /// Set without firing `onValueChanged` — used when rebinding to a different photo.
    func setValueSilent(_ v: Double) {
        _value = min(max(v, minValue), maxValue)
        needsDisplay = true
    }

    /// Change the range without letting the old value be clamped by the previous bounds.
    /// The exposure slider swaps between ±2 (legacy) and ±5 (v1), and on Windows this had
    /// to happen *before* binding or the value came back clamped.
    func setRange(min lo: Double, max hi: Double, default d: Double? = nil) {
        minValue = lo
        maxValue = hi
        if let d { defaultValue = d }
        _value = min(max(_value, lo), hi)
        needsDisplay = true
    }

    private var dragging = false
    private var editor: NSTextField?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    private var trackTop: CGFloat { 22 }
    private var valueWidth: CGFloat { 70 }
    private var trackRect: NSRect {
        NSRect(x: 2, y: trackTop, width: bounds.width - 4, height: 6)
    }

    private var step: Double { wheelStep ?? ((maxValue - minValue) / 200.0) }

    private func setValue(_ v: Double, notify: Bool) {
        let clamped = min(max(v, minValue), maxValue)
        if abs(clamped - _value) < 1e-9 { return }
        _value = clamped
        needsDisplay = true
        if notify { onValueChanged?(clamped) }
    }

    private func valueToX(_ v: Double) -> CGFloat {
        var t = (v - minValue) / (maxValue - minValue)
        if reverse { t = 1 - t }
        let tr = trackRect
        return tr.minX + CGFloat(t) * tr.width
    }

    private func xToValue(_ x: CGFloat) -> Double {
        let tr = trackRect
        var t = Double((x - tr.minX) / tr.width)
        t = min(max(t, 0), 1)
        if reverse { t = 1 - t }
        return minValue + t * (maxValue - minValue)
    }

    var displayText: String {
        String(format: "%.\(decimals)f", _value)
    }

    // ---- interaction -----------------------------------------------------

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)

        if event.clickCount >= 2, !(p.y < trackTop && p.x > bounds.width - valueWidth) {
            onEditBegin?()
            setValue(defaultValue, notify: true)
            return
        }
        // Clicking the value text (top-right) opens an inline numeric field.
        if p.y < trackTop && p.x > bounds.width - valueWidth {
            beginEdit()
            return
        }
        dragging = true
        onEditBegin?()
        setValue(xToValue(p.x), notify: true)
    }

    override func mouseDragged(with event: NSEvent) {
        guard dragging else { return }
        let p = convert(event.locationInWindow, from: nil)
        setValue(xToValue(p.x), notify: true)
    }

    override func mouseUp(with event: NSEvent) { dragging = false }

    override func scrollWheel(with event: NSEvent) {
        // A trackpad sends many small deltas; a mouse wheel sends discrete lines.
        let delta = event.hasPreciseScrollingDeltas
            ? Double(event.scrollingDeltaY) / 10.0
            : Double(event.scrollingDeltaY)
        guard delta != 0 else { return }
        onEditBegin?()
        setValue(_value + delta * step, notify: true)
    }

    // ---- inline numeric edit ---------------------------------------------

    private func beginEdit() {
        endEdit()
        let f = NSTextField(frame: NSRect(x: bounds.width - 68, y: 1, width: 64, height: 20))
        f.stringValue = displayText
        f.alignment = .right
        f.font = Theme.normal
        f.backgroundColor = Theme.panelBg3
        f.textColor = Theme.text
        f.isBordered = true
        f.bezelStyle = .squareBezel
        f.focusRingType = .none
        f.target = self
        f.action = #selector(commitEdit)
        f.delegate = self
        addSubview(f)
        editor = f
        window?.makeFirstResponder(f)
        f.currentEditor()?.selectAll(nil)
        needsDisplay = true
    }

    @objc private func commitEdit() {
        guard let editor else { return }
        if let v = Double(editor.stringValue.trimmingCharacters(in: .whitespaces)) {
            onEditBegin?()
            setValue(v, notify: true)
        }
        endEdit()
    }

    private func endEdit() {
        guard let e = editor else { return }
        editor = nil
        e.removeFromSuperview()
        needsDisplay = true
    }

    // ---- drawing ---------------------------------------------------------

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg)

        Theme.drawLeft(L.t(label), x: 0, midY: 11, font: Theme.normal, color: Theme.textDim)
        if editor == nil {
            Theme.drawRight(displayText, maxX: bounds.width, midY: 11,
                            font: Theme.normal, color: Theme.text)
        }

        let tr = trackRect
        let knobX = valueToX(_value)
        var stops = Theme.gradientStops(gradient)

        if !stops.isEmpty {
            // The ramp runs min→max along the track, so flip it when the track is reversed.
            if reverse { stops.reverse() }
            let path = Theme.roundedRect(tr, 3)
            NSGraphicsContext.saveGraphicsState()
            path.addClip()
            let g = NSGradient(colors: stops)
            g?.draw(in: tr, angle: 0)
            NSGraphicsContext.restoreGraphicsState()

            // Bipolar sliders lose their "how far from neutral" cue without a fill —
            // put it back as a faint tick at the default value.
            if bipolar {
                let tick = valueToX(defaultValue)
                Theme.sliderKnob.withAlphaComponent(0.35).setStroke()
                let p = NSBezierPath()
                p.move(to: NSPoint(x: tick, y: tr.minY))
                p.line(to: NSPoint(x: tick, y: tr.maxY))
                p.lineWidth = 1
                p.stroke()
            }
        } else {
            Theme.fill(tr, Theme.sliderTrack, radius: 3)
            let x0: CGFloat, x1: CGFloat
            if bipolar {
                let midX = valueToX(0)
                x0 = min(midX, knobX); x1 = max(midX, knobX)
            } else {
                let a = valueToX(minValue), b = knobX
                x0 = min(a, b); x1 = max(a, b)
            }
            Theme.fill(NSRect(x: x0, y: tr.minY, width: max(1, x1 - x0), height: tr.height),
                       Theme.sliderFill, radius: 3)
        }

        // knob
        let ky = tr.midY
        let kr: CGFloat = 6
        let knobRect = NSRect(x: knobX - kr, y: ky - kr, width: kr * 2, height: kr * 2)
        Theme.sliderKnob.setFill()
        NSBezierPath(ovalIn: knobRect).fill()
        Theme.windowBg.setStroke()
        let ring = NSBezierPath(ovalIn: knobRect)
        ring.lineWidth = 1.5
        ring.stroke()
    }
}

extension AdjustmentSlider: NSTextFieldDelegate {
    func control(_ control: NSControl, textView: NSTextView,
                 doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            endEdit()
            return true
        }
        if commandSelector == #selector(NSResponder.insertNewline(_:)) {
            commitEdit()
            return true
        }
        return false
    }

    func controlTextDidEndEditing(_ obj: Notification) {
        commitEdit()
    }
}
