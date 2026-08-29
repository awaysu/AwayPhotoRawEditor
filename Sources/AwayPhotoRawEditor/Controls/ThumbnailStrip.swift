import AppKit
import AwayRawCore

/// The horizontal preview strip. Thumbnails scale by *width* (about 172×115 landscape)
/// with the height hugging the image so there is no dead space, and a deliberately thick
/// (16 pt) horizontal scrollbar underneath because a thin one is hard to grab.
final class ThumbnailStrip: NSView {

    struct Entry {
        var item: PhotoItem
        var image: CGImage?
    }

    private(set) var entries: [Entry] = []
    private(set) var selectedIndices: Set<Int> = []
    /// The item that anchors shift-click ranges and is the one actually being edited.
    private(set) var currentIndex = -1

    var onSelectionChanged: (() -> Void)?
    var onContextMenu: ((Int, NSEvent) -> Void)?
    /// Double-click.
    var onActivate: ((Int) -> Void)?

    var showNumbers = true { didSet { needsDisplay = true } }

    private var scrollX: CGFloat = 0
    private var hoverIndex = -1 { didSet { needsDisplay = true } }
    private var trackingArea: NSTrackingArea?
    private var draggingBar = false
    private var dragBarStartX: CGFloat = 0
    private var dragBarStartScroll: CGFloat = 0

    private let thumbW: CGFloat = 172
    private let gap: CGFloat = 6
    private let labelH: CGFloat = 20
    private let barH: CGFloat = 16

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var isOpaque: Bool { true }

    private var cellW: CGFloat { thumbW + gap }
    private var contentW: CGFloat { CGFloat(entries.count) * cellW + gap }
    private var thumbAreaH: CGFloat { bounds.height - barH - labelH }

    // ---- content ---------------------------------------------------------

    func setItems(_ items: [PhotoItem]) {
        entries = items.map { Entry(item: $0, image: nil) }
        selectedIndices = []
        currentIndex = -1
        scrollX = 0
        needsDisplay = true
    }

    func setImage(_ image: CGImage?, at index: Int) {
        guard index >= 0, index < entries.count else { return }
        entries[index].image = image
        needsDisplay = true
    }

    func setImage(_ image: CGImage?, forKey key: String) {
        guard let i = entries.firstIndex(where: { $0.item.key == key }) else { return }
        entries[i].image = image
        needsDisplay = true
    }

    func index(forKey key: String) -> Int? {
        entries.firstIndex { $0.item.key == key }
    }

    var selectedItems: [PhotoItem] {
        selectedIndices.sorted().compactMap { $0 < entries.count ? entries[$0].item : nil }
    }

    var currentItem: PhotoItem? {
        currentIndex >= 0 && currentIndex < entries.count ? entries[currentIndex].item : nil
    }

    func select(index: Int, extend: Bool = false, toggle: Bool = false) {
        guard index >= 0, index < entries.count else { return }
        if toggle {
            if selectedIndices.contains(index) { selectedIndices.remove(index) }
            else { selectedIndices.insert(index) }
            currentIndex = index
        } else if extend, currentIndex >= 0 {
            let lo = min(currentIndex, index), hi = max(currentIndex, index)
            selectedIndices = Set(lo...hi)
        } else {
            selectedIndices = [index]
            currentIndex = index
        }
        scrollToVisible(index)
        needsDisplay = true
        onSelectionChanged?()
    }

    func selectAll() {
        guard !entries.isEmpty else { return }
        selectedIndices = Set(0..<entries.count)
        needsDisplay = true
        onSelectionChanged?()
    }

    func invertSelection() {
        guard !entries.isEmpty else { return }
        selectedIndices = Set(0..<entries.count).subtracting(selectedIndices)
        needsDisplay = true
        onSelectionChanged?()
    }

    /// Clears the multi-selection but keeps the current photo in the editor, so the
    /// panels never point at nothing.
    func deselectAll() {
        if currentIndex >= 0 { selectedIndices = [currentIndex] } else { selectedIndices = [] }
        needsDisplay = true
        onSelectionChanged?()
    }

    func refreshBadges() { needsDisplay = true }

    private func scrollToVisible(_ index: Int) {
        let x = CGFloat(index) * cellW
        if x < scrollX { scrollX = x }
        else if x + cellW > scrollX + bounds.width { scrollX = x + cellW - bounds.width }
        clampScroll()
    }

    private func clampScroll() {
        scrollX = min(max(0, scrollX), max(0, contentW - bounds.width))
    }

    // ---- hit testing -----------------------------------------------------

    private func indexAt(_ p: NSPoint) -> Int {
        guard p.y < thumbAreaH + labelH else { return -1 }
        let i = Int((p.x + scrollX - gap) / cellW)
        return (i >= 0 && i < entries.count) ? i : -1
    }

    private var barRect: NSRect {
        NSRect(x: 0, y: bounds.height - barH, width: bounds.width, height: barH)
    }

    private var barThumbRect: NSRect {
        guard contentW > bounds.width else { return .zero }
        let frac = bounds.width / contentW
        let w = max(40, bounds.width * frac)
        let travel = bounds.width - w
        let x = travel * (scrollX / max(1, contentW - bounds.width))
        return NSRect(x: x, y: bounds.height - barH + 3, width: w, height: barH - 6)
    }

    // ---- events ----------------------------------------------------------

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

    override func mouseMoved(with event: NSEvent) {
        hoverIndex = indexAt(convert(event.locationInWindow, from: nil))
    }
    override func mouseExited(with event: NSEvent) { hoverIndex = -1 }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        if barRect.contains(p) {
            draggingBar = true
            dragBarStartX = p.x
            dragBarStartScroll = scrollX
            // Clicking outside the bar thumb jumps there.
            if !barThumbRect.contains(p) {
                let frac = p.x / max(1, bounds.width)
                scrollX = frac * max(0, contentW - bounds.width)
                clampScroll()
                dragBarStartScroll = scrollX
                needsDisplay = true
            }
            return
        }
        let i = indexAt(p)
        guard i >= 0 else { return }
        if event.clickCount == 2 {
            onActivate?(i)
            return
        }
        select(index: i,
               extend: event.modifierFlags.contains(.shift),
               toggle: event.modifierFlags.contains(.command))
    }

    override func mouseDragged(with event: NSEvent) {
        guard draggingBar, contentW > bounds.width else { return }
        let p = convert(event.locationInWindow, from: nil)
        let travel = bounds.width - barThumbRect.width
        guard travel > 0 else { return }
        let k = (contentW - bounds.width) / travel
        scrollX = dragBarStartScroll + (p.x - dragBarStartX) * k
        clampScroll()
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) { draggingBar = false }

    override func rightMouseDown(with event: NSEvent) {
        let i = indexAt(convert(event.locationInWindow, from: nil))
        guard i >= 0 else { return }
        // Right-clicking outside the selection moves the selection there first, so the
        // menu always acts on what the user is pointing at.
        if !selectedIndices.contains(i) { select(index: i) }
        onContextMenu?(i, event)
    }

    override func scrollWheel(with event: NSEvent) {
        // Both axes scroll the strip: a trackpad's horizontal swipe and a mouse wheel.
        let dx = event.scrollingDeltaX != 0 ? event.scrollingDeltaX : event.scrollingDeltaY
        guard dx != 0 else { return }
        scrollX -= dx * (event.hasPreciseScrollingDeltas ? 1 : 6)
        clampScroll()
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 123:                                    // left arrow
            if currentIndex > 0 { select(index: currentIndex - 1) }
        case 124:                                    // right arrow
            if currentIndex >= 0 && currentIndex < entries.count - 1 { select(index: currentIndex + 1) }
        default:
            super.keyDown(with: event)
        }
    }

    // ---- drawing ---------------------------------------------------------

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.panelBg2)

        let th = thumbAreaH
        for (i, e) in entries.enumerated() {
            let x = gap + CGFloat(i) * cellW - scrollX
            if x + thumbW < 0 || x > bounds.width { continue }
            let cell = NSRect(x: x, y: 4, width: thumbW, height: th)
            let selected = selectedIndices.contains(i)

            if selected {
                Theme.fill(cell.insetBy(dx: -2, dy: -2), Theme.accent, radius: 3)
            } else if i == hoverIndex {
                Theme.fill(cell.insetBy(dx: -2, dy: -2), Theme.panelBg3, radius: 3)
            }
            Theme.fill(cell, Theme.viewerBg)

            if let img = e.image, let ctx = NSGraphicsContext.current?.cgContext {
                // Fit inside the cell, hugging the image so no dead space shows.
                let s = min(cell.width / CGFloat(img.width), cell.height / CGFloat(img.height))
                let w = CGFloat(img.width) * s, h = CGFloat(img.height) * s
                let r = NSRect(x: cell.midX - w / 2, y: cell.midY - h / 2, width: w, height: h)
                ctx.saveGState()
                ctx.translateBy(x: 0, y: r.maxY + r.minY)
                ctx.scaleBy(x: 1, y: -1)
                ctx.interpolationQuality = .high
                ctx.draw(img, in: r)
                ctx.restoreGState()
            }

            // #index badge, top-left. Hidden photos still hold their number, so a hidden
            // #2 leaves the strip showing #1, #3.
            if showNumbers {
                let n = e.item.displayNumber > 0 ? e.item.displayNumber : i + 1
                let text = "#\(n)"
                let size = Theme.measure(text, font: Theme.small)
                let br = NSRect(x: cell.minX + 3, y: cell.minY + 3,
                                width: size.width + 8, height: size.height + 2)
                Theme.fill(br, NSColor(white: 0, alpha: 0.55), radius: 2)
                Theme.drawCentered(text, in: br, font: Theme.small, color: .white)
            }

            // edited badge, top-right
            if e.item.isEdited {
                let d: CGFloat = 8
                let r = NSRect(x: cell.maxX - d - 4, y: cell.minY + 4, width: d, height: d)
                Theme.editedBadge.setFill()
                NSBezierPath(ovalIn: r).fill()
            }
            // virtual-copy badge
            if e.item.isVirtualCopy {
                let text = "copy \(e.item.virtualCopyIndex)"
                let size = Theme.measure(text, font: Theme.small)
                let br = NSRect(x: cell.maxX - size.width - 10, y: cell.maxY - size.height - 5,
                                width: size.width + 6, height: size.height + 2)
                Theme.fill(br, Theme.copyBadge.withAlphaComponent(0.85), radius: 2)
                Theme.drawCentered(text, in: br, font: Theme.small, color: .black)
            }
            // hidden badge
            if e.item.isHidden {
                let br = NSRect(x: cell.minX + 4, y: cell.maxY - 18, width: 18, height: 14)
                Theme.fill(br, NSColor(white: 0, alpha: 0.55), radius: 2)
                Theme.drawCentered("🚫", in: br, font: Theme.small, color: .white)
            }
            // copy-settings source marker
            if e.item.isCopySettingsSource {
                Theme.stroke(cell, Theme.copyBadge, width: 2)
            }

            // filename under the cell, clipped to the cell so long names cannot run into
            // the neighbouring thumbnails
            let labelRect = NSRect(x: cell.minX, y: cell.maxY + 2, width: cell.width, height: labelH - 4)
            let name = Theme.truncate(e.item.fileName, font: Theme.small, maxWidth: cell.width - 4)
            Theme.drawCentered(name, in: labelRect, font: Theme.small,
                               color: selected ? Theme.text : Theme.textDim)
        }

        // horizontal scrollbar
        if contentW > bounds.width {
            Theme.fill(barRect, Theme.windowBg)
            Theme.fill(barThumbRect, Theme.borderLight, radius: 4)
        }
    }
}
