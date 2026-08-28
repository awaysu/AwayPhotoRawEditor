import AppKit
import AwayRawCore

/// A flat, fully custom-drawn button. `isPrimary` is a stored property with an explicit
/// redraw in its observer — an early Windows version had it as an auto-property and the
/// previously-selected tool button never repainted, so two buttons looked selected at once.
final class FlatButton: NSView {
    var title = "" { didSet { needsDisplay = true } }
    var isPrimary = false { didSet { needsDisplay = true } }
    var isEnabledButton = true { didSet { needsDisplay = true } }
    var font: NSFont?
    var onClick: (() -> Void)?

    private var hovering = false { didSet { needsDisplay = true } }
    private var pressed = false { didSet { needsDisplay = true } }
    private var trackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }

    convenience init(title: String, action: (() -> Void)? = nil) {
        self.init(frame: .zero)
        self.title = title
        self.onClick = action
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let t = trackingArea { removeTrackingArea(t) }
        let t = NSTrackingArea(rect: bounds,
                               options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                               owner: self)
        addTrackingArea(t)
        trackingArea = t
    }

    override func mouseEntered(with event: NSEvent) { hovering = true }
    override func mouseExited(with event: NSEvent) { hovering = false }

    override func mouseDown(with event: NSEvent) {
        guard isEnabledButton else { return }
        pressed = true
    }

    override func mouseUp(with event: NSEvent) {
        guard isEnabledButton else { return }
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        pressed = false
        if inside { onClick?() }
    }

    override func draw(_ dirtyRect: NSRect) {
        let bg: NSColor
        if !isEnabledButton {
            bg = Theme.panelBg2
        } else if isPrimary {
            bg = pressed ? Theme.accentDim : (hovering ? Theme.accentHover : Theme.accent)
        } else {
            bg = pressed ? Theme.panelBg : (hovering ? Theme.panelBg3 : Theme.panelBg2)
        }
        Theme.fill(bounds, bg, radius: 3)
        Theme.stroke(bounds, isPrimary ? Theme.accentDim : Theme.border, radius: 3)

        let color: NSColor = !isEnabledButton ? Theme.textFaint
                           : (isPrimary ? .white : Theme.text)
        Theme.drawCentered(L.t(title), in: bounds, font: font ?? Theme.normal, color: color)
    }
}

/// A square glyph button (the white-balance eyedropper, the rotate buttons, ☰).
final class IconButton: NSView {
    var glyph = "" { didSet { needsDisplay = true } }
    var isActive = false { didSet { needsDisplay = true } }
    var tooltip: String? { didSet { toolTip = tooltip.map { L.t($0) } } }
    var font: NSFont?
    var drawsBorder = true
    var onClick: (() -> Void)?

    private var hovering = false { didSet { needsDisplay = true } }
    private var trackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }

    convenience init(glyph: String, action: (() -> Void)? = nil) {
        self.init(frame: .zero)
        self.glyph = glyph
        self.onClick = action
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let t = trackingArea { removeTrackingArea(t) }
        let t = NSTrackingArea(rect: bounds,
                               options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                               owner: self)
        addTrackingArea(t)
        trackingArea = t
    }

    override func mouseEntered(with event: NSEvent) { hovering = true }
    override func mouseExited(with event: NSEvent) { hovering = false }
    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }

    override func draw(_ dirtyRect: NSRect) {
        let bg = isActive ? Theme.accent : (hovering ? Theme.panelBg3 : Theme.panelBg2)
        Theme.fill(bounds, bg, radius: 3)
        if drawsBorder { Theme.stroke(bounds, Theme.border, radius: 3) }
        Theme.drawCentered(glyph, in: bounds, font: font ?? Theme.iconGlyph,
                           color: isActive ? .white : Theme.text)
    }
}

/// A fixed-size panel with a bold title and absolutely positioned content — the
/// 基本調整 / 色彩 / 細節 / 直方圖 / 照片資訊 / 工具 boxes.
class SectionPanel: NSView {
    var title = "" { didSet { needsDisplay = true } }
    /// Height reserved for the title before content starts.
    var titleHeight: CGFloat = 26

    override var isFlipped: Bool { true }

    convenience init(title: String) {
        self.init(frame: .zero)
        self.title = title
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg, radius: 4)
        Theme.stroke(bounds, Theme.border, radius: 4)
        if !title.isEmpty {
            Theme.drawLeft(L.t(title), x: 12, midY: titleHeight / 2 + 3,
                           font: Theme.sectionTitle, color: Theme.text)
        }
    }

    /// Lay the section's contents out. Subclasses override; called whenever the panel
    /// is resized. A re-entrancy guard mirrors the Windows control, where setting a
    /// child's height inside layout could loop back into layout.
    private var inRelayout = false
    override func layout() {
        super.layout()
        guard !inRelayout else { return }
        inRelayout = true
        relayout()
        inRelayout = false
    }

    func relayout() {}
}

/// A checkbox drawn to match the palette.
final class DarkCheckBox: NSView {
    var title = "" { didSet { needsDisplay = true } }
    var isChecked = false { didSet { needsDisplay = true } }
    var onToggle: ((Bool) -> Void)?

    override var isFlipped: Bool { true }

    convenience init(title: String, checked: Bool = false, action: ((Bool) -> Void)? = nil) {
        self.init(frame: .zero)
        self.title = title
        self.isChecked = checked
        self.onToggle = action
    }

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        isChecked.toggle()
        onToggle?(isChecked)
    }

    override func draw(_ dirtyRect: NSRect) {
        let box = NSRect(x: 0, y: (bounds.height - 14) / 2, width: 14, height: 14)
        Theme.fill(box, isChecked ? Theme.accent : Theme.panelBg3, radius: 2)
        Theme.stroke(box, isChecked ? Theme.accent : Theme.borderLight, radius: 2)
        if isChecked {
            NSColor.white.setStroke()
            let p = NSBezierPath()
            p.move(to: NSPoint(x: box.minX + 3, y: box.midY))
            p.line(to: NSPoint(x: box.minX + 6, y: box.maxY - 4))
            p.line(to: NSPoint(x: box.maxX - 3, y: box.minY + 4))
            p.lineWidth = 1.8
            p.lineCapStyle = .round
            p.lineJoinStyle = .round
            p.stroke()
        }
        Theme.drawLeft(L.t(title), x: box.maxX + 8, midY: bounds.midY,
                       font: Theme.normal, color: Theme.text)
    }
}

/// A radio button drawn to match the palette. Group members share a `group` string and
/// deselect each other through `RadioGroup`.
final class DarkRadioButton: NSView {
    var title = "" { didSet { needsDisplay = true } }
    var isSelected = false { didSet { needsDisplay = true } }
    var onSelect: (() -> Void)?

    override var isFlipped: Bool { true }

    convenience init(title: String, action: (() -> Void)? = nil) {
        self.init(frame: .zero)
        self.title = title
        self.onSelect = action
    }

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        onSelect?()
    }

    override func draw(_ dirtyRect: NSRect) {
        let d: CGFloat = 14
        let box = NSRect(x: 0, y: (bounds.height - d) / 2, width: d, height: d)
        Theme.panelBg3.setFill()
        NSBezierPath(ovalIn: box).fill()
        (isSelected ? Theme.accent : Theme.borderLight).setStroke()
        let ring = NSBezierPath(ovalIn: box.insetBy(dx: 0.5, dy: 0.5))
        ring.lineWidth = 1
        ring.stroke()
        if isSelected {
            Theme.accent.setFill()
            NSBezierPath(ovalIn: box.insetBy(dx: 4, dy: 4)).fill()
        }
        Theme.drawLeft(L.t(title), x: box.maxX + 8, midY: bounds.midY,
                       font: Theme.normal, color: Theme.text)
    }
}

/// Keeps a set of radio buttons mutually exclusive.
final class RadioGroup {
    private(set) var buttons: [DarkRadioButton] = []
    var onChange: ((Int) -> Void)?

    func add(_ b: DarkRadioButton) {
        let index = buttons.count
        buttons.append(b)
        b.onSelect = { [weak self] in self?.select(index) }
    }

    func select(_ index: Int) {
        for (i, b) in buttons.enumerated() { b.isSelected = (i == index) }
        onChange?(index)
    }

    var selectedIndex: Int { buttons.firstIndex { $0.isSelected } ?? -1 }
}

/// A popup drawn to match the palette. Wraps NSPopUpButton so keyboard and accessibility
/// behaviour stays native, but paints its own chrome.
final class DarkComboBox: NSView {
    private let popup = NSPopUpButton(frame: .zero, pullsDown: false)
    var onChange: ((Int) -> Void)?

    override var isFlipped: Bool { true }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        popup.isBordered = false
        popup.font = Theme.normal
        popup.target = self
        popup.action = #selector(changed)
        popup.autoresizingMask = [.width, .height]
        addSubview(popup)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layout() {
        super.layout()
        popup.frame = bounds.insetBy(dx: 4, dy: 2)
    }

    @objc private func changed() { onChange?(popup.indexOfSelectedItem) }

    /// Items are set as display strings; the caller keeps the mapping back to storage keys.
    func setItems(_ items: [String], selected: Int = 0) {
        popup.removeAllItems()
        popup.addItems(withTitles: items.isEmpty ? [" "] : items)
        if selected >= 0 && selected < popup.numberOfItems { popup.selectItem(at: selected) }
        restyle()
    }

    var selectedIndex: Int {
        get { popup.indexOfSelectedItem }
        set { if newValue >= 0 && newValue < popup.numberOfItems { popup.selectItem(at: newValue) } }
    }

    var selectedTitle: String { popup.titleOfSelectedItem ?? "" }

    func selectTitle(_ t: String) {
        if popup.itemTitles.contains(t) { popup.selectItem(withTitle: t) }
    }

    private func restyle() {
        for item in popup.itemArray {
            item.attributedTitle = NSAttributedString(
                string: item.title,
                attributes: [.font: Theme.normal, .foregroundColor: Theme.text])
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg3, radius: 3)
        Theme.stroke(bounds, Theme.border, radius: 3)
    }
}

/// A numeric stepper field (the crop ratio W:H boxes).
final class DarkNumberField: NSView {
    private let field = NSTextField(frame: .zero)
    private let stepper = NSStepper(frame: .zero)
    var onChange: ((Int) -> Void)?

    override var isFlipped: Bool { true }

    var intValue: Int {
        get { field.integerValue }
        set { field.integerValue = newValue; stepper.integerValue = newValue }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        field.font = Theme.normal
        field.backgroundColor = Theme.panelBg3
        field.textColor = Theme.text
        field.isBordered = false
        field.drawsBackground = true
        field.alignment = .center
        field.focusRingType = .none
        field.target = self
        field.action = #selector(fieldChanged)
        addSubview(field)

        stepper.minValue = 1
        stepper.maxValue = 999
        stepper.increment = 1
        stepper.valueWraps = false
        stepper.target = self
        stepper.action = #selector(stepperChanged)
        addSubview(stepper)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func setRange(min lo: Int, max hi: Int) {
        stepper.minValue = Double(lo)
        stepper.maxValue = Double(hi)
    }

    override func layout() {
        super.layout()
        let sw: CGFloat = 13
        field.frame = NSRect(x: 2, y: 2, width: bounds.width - sw - 4, height: bounds.height - 4)
        stepper.frame = NSRect(x: bounds.width - sw, y: 0, width: sw, height: bounds.height)
    }

    @objc private func fieldChanged() {
        stepper.integerValue = field.integerValue
        onChange?(field.integerValue)
    }

    @objc private func stepperChanged() {
        field.integerValue = stepper.integerValue
        onChange?(stepper.integerValue)
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg3, radius: 3)
        Theme.stroke(bounds, Theme.border, radius: 3)
    }
}

/// A simple custom-drawn single-selection list. Used for the preset editor's list —
/// every other control here is custom-drawn, and this avoids NSTableView's lazy row
/// lifecycle for what is a dozen fixed rows.
final class DarkListView: NSView {
    struct Row {
        var title: String
        /// Drawn in the accent colour with a marker — a built-in that has been overridden.
        var marked: Bool = false
    }

    var rows: [Row] = [] { didSet { needsDisplay = true } }
    var selectedIndex = -1 { didSet { needsDisplay = true } }
    var rowHeight: CGFloat = 28
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
        let i = Int(p.y / rowHeight)
        return (i >= 0 && i < rows.count) ? i : -1
    }

    override func mouseMoved(with event: NSEvent) {
        hoverIndex = indexAt(convert(event.locationInWindow, from: nil))
    }
    override func mouseExited(with event: NSEvent) { hoverIndex = -1 }

    override func mouseDown(with event: NSEvent) {
        let i = indexAt(convert(event.locationInWindow, from: nil))
        guard i >= 0 else { return }
        selectedIndex = i
        onSelect?(i)
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg2)
        Theme.stroke(bounds, Theme.border)
        for (i, row) in rows.enumerated() {
            let r = NSRect(x: 0, y: CGFloat(i) * rowHeight, width: bounds.width, height: rowHeight)
            if i == selectedIndex {
                Theme.fill(r.insetBy(dx: 2, dy: 1), Theme.accent, radius: 3)
            } else if i == hoverIndex {
                Theme.fill(r.insetBy(dx: 2, dy: 1), Theme.panelBg3, radius: 3)
            }
            let color: NSColor = i == selectedIndex ? .white
                               : (row.marked ? Theme.copyBadge : Theme.text)
            Theme.drawLeft(row.title + (row.marked ? " ●" : ""),
                           x: 10, midY: r.midY, font: Theme.normal, color: color)
        }
    }

    /// Content height, so the owner can size a scroll view's document.
    var contentHeight: CGFloat { CGFloat(rows.count) * rowHeight }
}
