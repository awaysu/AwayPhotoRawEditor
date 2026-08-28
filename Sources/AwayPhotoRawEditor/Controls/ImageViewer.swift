import AppKit
import AwayRawCore

/// The central preview: pans, zooms, and hosts the crop / gradient / heal overlays.
///
/// Coordinates come in three flavours — normalised image space (0..1, what the
/// adjustments store), image pixels, and view points. Everything the overlays do is
/// expressed in normalised space so a proxy-resolution preview and a full-resolution
/// export agree.
final class ImageViewer: NSView {

    private enum Drag {
        case none, pan
        case cropMove, cropL, cropR, cropT, cropB, cropTL, cropTR, cropBL, cropBR
        case gradCenter, gradRange, gradRotate
        case healTarget, healSource
    }

    // ---- public state ----------------------------------------------------

    var tool: ToolMode = .none { didSet { needsDisplay = true } }
    var healMode: HealMode = .clone
    var healBrushSize: Double = 10
    var whiteBalancePickerActive = false {
        didSet { needsDisplay = true; window?.invalidateCursorRects(for: self) }
    }

    var onEditBegin: (() -> Void)?
    var onEditChanged: (() -> Void)?
    var onWhiteBalancePicked: ((Double, Double) -> Void)?
    var onViewChanged: (() -> Void)?
    var onGradientSelectionChanged: (() -> Void)?

    /// The adjustments the overlays edit in place. The owner re-reads them after every
    /// `onEditChanged`.
    var adjustments: ImageAdjustments? {
        didSet { needsDisplay = true }
    }

    private(set) var zoomMode: ZoomMode = .fit
    var zoomPercent: Double { scale * 100 }

    // ---- internals -------------------------------------------------------

    private var image: CGImage?
    private var scale: Double = 1
    private var offset: NSPoint = .zero

    private var drag: Drag = .none
    private var dragSpot = -1
    private var dragStart: NSPoint = .zero
    private var offsetStart: NSPoint = .zero
    private var dragMoved = false
    private var activeSpot = -1

    /// Below this many points of movement a left-drag counts as a click, so a click can
    /// cycle the zoom without the picture twitching.
    private let panThreshold: CGFloat = 4
    private var handleHitRadius: CGFloat { 10 }
    private var rotHandleDist: CGFloat { 64 }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var isOpaque: Bool { true }

    // ---- image / zoom ----------------------------------------------------

    func setImage(_ img: CGImage?, resetView: Bool) {
        image = img
        if resetView || image == nil {
            zoomMode = .fit
            zoomToFit()
        } else {
            clampOffset()
        }
        needsDisplay = true
        onViewChanged?()
    }

    var imageSize: NSSize {
        guard let image else { return .zero }
        return NSSize(width: image.width, height: image.height)
    }

    func zoomToFit() {
        guard let image, bounds.width > 0, bounds.height > 0 else { scale = 1; offset = .zero; return }
        let s = min(bounds.width / CGFloat(image.width), bounds.height / CGFloat(image.height))
        scale = Double(min(s, 1000))
        centerImage()
        zoomMode = .fit
        needsDisplay = true
        onViewChanged?()
    }

    func zoom100() { setZoom(1.0, mode: .actual100) }
    func zoom200() { setZoom(2.0, mode: .actual200) }

    private func setZoom(_ s: Double, mode: ZoomMode) {
        guard image != nil else { return }
        zoomAround(NSPoint(x: bounds.midX, y: bounds.midY), s)
        zoomMode = mode
        onViewChanged?()
    }

    private func centerImage() {
        guard let image else { return }
        let w = CGFloat(Double(image.width) * scale)
        let h = CGFloat(Double(image.height) * scale)
        offset = NSPoint(x: (bounds.width - w) / 2, y: (bounds.height - h) / 2)
    }

    private func zoomAround(_ p: NSPoint, _ newScale: Double) {
        guard image != nil else { return }
        let old = scale
        scale = min(max(newScale, 0.02), 16)
        // Keep the image point under the cursor pinned.
        let k = CGFloat(scale / old)
        offset = NSPoint(x: p.x - (p.x - offset.x) * k,
                         y: p.y - (p.y - offset.y) * k)
        clampOffset()
        needsDisplay = true
    }

    private func clampOffset() {
        guard let image else { return }
        let w = CGFloat(Double(image.width) * scale)
        let h = CGFloat(Double(image.height) * scale)
        var o = offset
        if w <= bounds.width { o.x = (bounds.width - w) / 2 }
        else { o.x = min(0, max(bounds.width - w, o.x)) }
        if h <= bounds.height { o.y = (bounds.height - h) / 2 }
        else { o.y = min(0, max(bounds.height - h, o.y)) }
        offset = o
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if zoomMode == .fit { zoomToFit() } else { clampOffset() }
    }

    // ---- coordinate transforms -------------------------------------------

    private func normToView(_ nx: Double, _ ny: Double) -> NSPoint {
        guard let image else { return .zero }
        return NSPoint(x: offset.x + CGFloat(nx * Double(image.width) * scale),
                       y: offset.y + CGFloat(ny * Double(image.height) * scale))
    }

    private func viewToNorm(_ p: NSPoint) -> (Double, Double) {
        guard let image, image.width > 0, image.height > 0 else { return (0, 0) }
        return (Double(p.x - offset.x) / (Double(image.width) * scale),
                Double(p.y - offset.y) / (Double(image.height) * scale))
    }

    private static func dist(_ a: NSPoint, _ b: NSPoint) -> CGFloat {
        ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).squareRoot()
    }

    // ---- heal helpers ----------------------------------------------------

    /// Change the brush size and, when a spot is selected, resize that spot too.
    @discardableResult
    func setHealBrushSize(_ size: Double) -> Bool {
        healBrushSize = size
        guard var adj = adjustments, activeSpot >= 0, activeSpot < adj.healSpots.count else { return false }
        onEditBegin?()
        adj.healSpots[activeSpot].radiusNorm = max(0.005, size / 500.0)
        adj.healSpots[activeSpot].radius = size
        adjustments = adj
        needsDisplay = true
        onEditChanged?()
        return true
    }

    /// Switch clone/inpaint. As well as setting the mode for *new* spots this converts
    /// the currently selected one and re-renders — setting only the property leaves the
    /// picture unchanged when the button is pressed, which reads as a bug.
    @discardableResult
    func setHealMode(_ mode: HealMode) -> Bool {
        healMode = mode
        guard var adj = adjustments, activeSpot >= 0, activeSpot < adj.healSpots.count else { return false }
        onEditBegin?()
        adj.healSpots[activeSpot].useInpaint = (mode == .inpaint)
        adjustments = adj
        needsDisplay = true
        onEditChanged?()
        return true
    }

    // ---- mouse -----------------------------------------------------------

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        dragStart = p
        offsetStart = offset
        dragMoved = false
        guard image != nil else { return }

        if whiteBalancePickerActive {
            let (nx, ny) = viewToNorm(p)
            onWhiteBalancePicked?(nx, ny)
            return
        }

        if adjustments != nil {
            switch tool {
            case .crop:     if beginCropDrag(p) { return }
            case .gradient: if beginGradientDrag(p) { return }
            case .heal:     if beginHealDrag(p) { return }
            case .none:     break
            }
        }

        // default: pan with left when no tool is active
        if tool == .none {
            drag = .pan
            offsetStart = offset
        }
    }

    override func rightMouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        guard adjustments != nil else { return }
        if tool == .heal { deleteHealAt(p); return }
        if tool == .gradient {
            let idx = gradientAtPoint(p)
            if idx >= 0 { showGradientDeleteMenu(idx, at: event) }
        }
    }

    override func otherMouseDown(with event: NSEvent) {
        // Middle button pans regardless of the active tool.
        dragStart = convert(event.locationInWindow, from: nil)
        offsetStart = offset
        dragMoved = false
        drag = .pan
    }

    override func otherMouseDragged(with event: NSEvent) { mouseDragged(with: event) }
    override func otherMouseUp(with event: NSEvent) { drag = .none }

    override func mouseDragged(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if drag == .pan {
            let dx = p.x - dragStart.x, dy = p.y - dragStart.y
            if !dragMoved && dx * dx + dy * dy < panThreshold * panThreshold { return }
            dragMoved = true
            offset = NSPoint(x: offsetStart.x + dx, y: offsetStart.y + dy)
            // Keep the current zoom level (fit/100%/200%) so a following click continues
            // the cycle; only the wheel switches to a free zoom.
            clampOffset()
            needsDisplay = true
            return
        }
        if drag != .none, adjustments != nil { updateDrag(p) }
    }

    override func mouseUp(with event: NSEvent) {
        let wasPan = (drag == .pan)
        drag = .none
        dragSpot = -1
        // A left click without dragging cycles the zoom (fit → 100% → 200% → fit).
        if wasPan && !dragMoved && !whiteBalancePickerActive {
            cycleZoom(at: convert(event.locationInWindow, from: nil))
        }
    }

    override func scrollWheel(with event: NSEvent) {
        guard image != nil else { return }
        let p = convert(event.locationInWindow, from: nil)
        let delta = event.hasPreciseScrollingDeltas
            ? Double(event.scrollingDeltaY) * 0.005
            : Double(event.scrollingDeltaY) * 0.06
        guard delta != 0 else { return }
        zoomAround(p, scale * (1 + delta))
        zoomMode = .custom          // only the wheel produces a free zoom
        onViewChanged?()
    }

    private func cycleZoom(at p: NSPoint) {
        guard image != nil else { return }
        switch zoomMode {
        case .fit:
            zoomMode = .actual100
            zoomAround(p, 1.0)
        case .actual100:
            zoomMode = .actual200
            zoomAround(p, 2.0)
        default:
            zoomToFit()
        }
        onViewChanged?()
    }

    override func resetCursorRects() {
        let c: NSCursor = whiteBalancePickerActive ? .crosshair
                        : (tool == .none ? .openHand : .arrow)
        addCursorRect(bounds, cursor: c)
    }

    // ---- crop interaction ------------------------------------------------

    private func cropViewRect() -> NSRect {
        guard let adj = adjustments else { return bounds }
        let tl = normToView(adj.cropX, adj.cropY)
        let br = normToView(adj.cropX + adj.cropWidth, adj.cropY + adj.cropHeight)
        return NSRect(x: tl.x, y: tl.y, width: br.x - tl.x, height: br.y - tl.y)
    }

    private func beginCropDrag(_ p: NSPoint) -> Bool {
        let r = cropViewRect()
        let g: CGFloat = 10
        let l = abs(p.x - r.minX) < g, rr = abs(p.x - r.maxX) < g
        let t = abs(p.y - r.minY) < g, b = abs(p.y - r.maxY) < g
        let inX = p.x > r.minX - g && p.x < r.maxX + g
        let inY = p.y > r.minY - g && p.y < r.maxY + g

        var d: Drag = .none
        if l && t { d = .cropTL } else if rr && t { d = .cropTR }
        else if l && b { d = .cropBL } else if rr && b { d = .cropBR }
        else if l && inY { d = .cropL } else if rr && inY { d = .cropR }
        else if t && inX { d = .cropT } else if b && inX { d = .cropB }
        else if r.contains(p) { d = .cropMove }
        guard d != .none else { return false }
        drag = d
        onEditBegin?()
        return true
    }

    /// The aspect ratio the crop must keep, or nil for a free crop.
    private var cropAspect: Double? {
        guard let adj = adjustments, let image else { return nil }
        let s = adj.cropAspectRatio
        if s == "Original" {
            return Double(image.width) / Double(image.height)
        }
        let parts = s.split(separator: ":")
        guard parts.count == 2, let w = Double(parts[0]), let h = Double(parts[1]),
              w > 0, h > 0, image.height > 0 else { return nil }
        // Ratios are expressed against the image's own pixels, so convert to normalised.
        return (w / h) * (Double(image.height) / Double(image.width))
    }

    private func updateDrag(_ p: NSPoint) {
        guard var adj = adjustments, let image else { return }
        let (nx0, ny0) = viewToNorm(p)
        let nx = min(max(nx0, 0), 1), ny = min(max(ny0, 0), 1)
        let minN = 0.02

        switch drag {
        case .cropMove:
            let (sx, sy) = viewToNorm(dragStart)
            let dx = nx0 - sx, dy = ny0 - sy
            dragStart = p
            adj.cropX = min(max(adj.cropX + dx, 0), 1 - adj.cropWidth)
            adj.cropY = min(max(adj.cropY + dy, 0), 1 - adj.cropHeight)

        case .cropL, .cropR, .cropT, .cropB, .cropTL, .cropTR, .cropBL, .cropBR:
            let x0 = adj.cropX, y0 = adj.cropY
            let x1 = adj.cropX + adj.cropWidth, y1 = adj.cropY + adj.cropHeight
            switch drag {
            case .cropL:
                let w = max(minN, min(x1, x1 - nx))
                adj.cropX = x1 - w; adj.cropWidth = w
            case .cropR:
                adj.cropWidth = max(minN, min(1 - x0, nx - x0))
            case .cropT:
                let h = max(minN, min(y1, y1 - ny))
                adj.cropY = y1 - h; adj.cropHeight = h
            case .cropB:
                adj.cropHeight = max(minN, min(1 - y0, ny - y0))
            case .cropTL:
                let w = max(minN, min(x1, x1 - nx)), h = max(minN, min(y1, y1 - ny))
                adj.cropX = x1 - w; adj.cropWidth = w
                adj.cropY = y1 - h; adj.cropHeight = h
            case .cropTR:
                let w = max(minN, min(1 - x0, nx - x0)), h = max(minN, min(y1, y1 - ny))
                adj.cropWidth = w
                adj.cropY = y1 - h; adj.cropHeight = h
            case .cropBL:
                let w = max(minN, min(x1, x1 - nx)), h = max(minN, min(1 - y0, ny - y0))
                adj.cropX = x1 - w; adj.cropWidth = w
                adj.cropHeight = h
            case .cropBR:
                adj.cropWidth = max(minN, min(1 - x0, nx - x0))
                adj.cropHeight = max(minN, min(1 - y0, ny - y0))
            default: break
            }
            applyCropAspect(&adj, anchor: drag)

        case .gradCenter:
            var g = adj.gradients[adj.resolvedGradientIndex]
            g.centerX = nx; g.centerY = ny
            adj.gradients[adj.resolvedGradientIndex] = g

        case .gradRange:
            var g = adj.gradients[adj.resolvedGradientIndex]
            let center = normToView(g.centerX, g.centerY)
            let d = Double(Self.dist(p, center)) / (Double(image.height) * scale)
            g.range = min(max(d, 0.01), 2.0)
            adj.gradients[adj.resolvedGradientIndex] = g

        case .gradRotate:
            var g = adj.gradients[adj.resolvedGradientIndex]
            let center = normToView(g.centerX, g.centerY)
            // View Y runs downward, so negate to get a mathematical angle.
            let a = atan2(Double(center.y - p.y), Double(p.x - center.x))
            g.angle = a * 180 / Double.pi
            adj.gradients[adj.resolvedGradientIndex] = g

        case .healTarget where dragSpot >= 0:
            adj.healSpots[dragSpot].targetX = nx
            adj.healSpots[dragSpot].targetY = ny

        case .healSource where dragSpot >= 0:
            adj.healSpots[dragSpot].sourceX = nx
            adj.healSpots[dragSpot].sourceY = ny

        default:
            return
        }

        adjustments = adj
        needsDisplay = true
        onEditChanged?()
    }

    /// Force the crop box back onto the selected aspect ratio, holding the edge the user
    /// is not dragging.
    private func applyCropAspect(_ adj: inout ImageAdjustments, anchor: Drag) {
        guard let aspect = cropAspect else { return }
        let x0 = adj.cropX, y0 = adj.cropY
        var w = adj.cropWidth, h = adj.cropHeight
        // Derive height from width, then correct if it does not fit.
        h = w / aspect
        if y0 + h > 1 { h = 1 - y0; w = h * aspect }
        if x0 + w > 1 { w = 1 - x0; h = w / aspect }
        switch anchor {
        case .cropL, .cropTL, .cropBL:
            adj.cropX = min(max(x0 + adj.cropWidth - w, 0), 1 - w)
        default:
            adj.cropX = min(max(x0, 0), 1 - w)
        }
        switch anchor {
        case .cropT, .cropTL, .cropTR:
            adj.cropY = min(max(y0 + adj.cropHeight - h, 0), 1 - h)
        default:
            adj.cropY = min(max(y0, 0), 1 - h)
        }
        adj.cropWidth = w
        adj.cropHeight = h
    }

    // ---- gradient interaction -------------------------------------------

    private static func gradAxis(_ g: LinearGradient) -> (ux: Double, uy: Double) {
        let a = g.angle * Double.pi / 180
        return (sin(a), cos(a))     // axis of variation (view coords, Y down)
    }

    /// Blue rotate handle: a fixed screen offset to the right of the white handle,
    /// rotating with the gradient.
    private func rotateHandlePos(_ center: NSPoint, _ g: LinearGradient) -> NSPoint {
        let a = g.angle * Double.pi / 180
        return NSPoint(x: center.x + CGFloat(cos(a)) * rotHandleDist,
                       y: center.y - CGFloat(sin(a)) * rotHandleDist)
    }

    private func beginGradientDrag(_ p: NSPoint) -> Bool {
        guard let adj = adjustments, let image else { return false }
        // On the active gradient, grab its yellow (range) or blue (rotate) handle first.
        if let g = adj.activeGradient {
            let center = normToView(g.centerX, g.centerY)
            let (ux, uy) = Self.gradAxis(g)
            let rangePt = NSPoint(x: center.x + CGFloat(ux * g.range * Double(image.height) * scale),
                                  y: center.y + CGFloat(uy * g.range * Double(image.height) * scale))
            let rotatePt = rotateHandlePos(center, g)
            var d: Drag = .none
            if Self.dist(p, rotatePt) < handleHitRadius { d = .gradRotate }
            else if Self.dist(p, rangePt) < handleHitRadius { d = .gradRange }
            if d != .none { drag = d; onEditBegin?(); return true }
        }
        // Otherwise, clicking any gradient's white dot selects it (and starts a move).
        let idx = gradientAtPoint(p)
        if idx >= 0 {
            if idx != adj.resolvedGradientIndex {
                adjustments?.activeGradientIndex = idx
                onGradientSelectionChanged?()
            }
            drag = .gradCenter
            onEditBegin?()
            return true
        }
        // Nothing hit — a gradient is only created by the "新增線性漸層" button.
        return false
    }

    /// Index of the gradient whose white centre handle is under `p`, or -1.
    private func gradientAtPoint(_ p: NSPoint) -> Int {
        guard let adj = adjustments else { return -1 }
        for (i, g) in adj.gradients.enumerated()
        where Self.dist(p, normToView(g.centerX, g.centerY)) < handleHitRadius {
            return i
        }
        return -1
    }

    private func showGradientDeleteMenu(_ index: Int, at event: NSEvent) {
        let menu = NSMenu()
        let item = NSMenuItem(title: L.t("刪除此線性漸層"), action: #selector(deleteGradient(_:)), keyEquivalent: "")
        item.target = self
        item.tag = index
        menu.addItem(item)
        NSMenu.popUpContextMenu(menu, with: event, for: self)
    }

    @objc private func deleteGradient(_ sender: NSMenuItem) {
        guard var adj = adjustments, sender.tag >= 0, sender.tag < adj.gradients.count else { return }
        onEditBegin?()
        adj.gradients.remove(at: sender.tag)
        adj.activeGradientIndex = adj.gradients.count - 1
        adjustments = adj
        onGradientSelectionChanged?()
        needsDisplay = true
        onEditChanged?()
    }

    /// Add a gradient at the centre of the frame and select it.
    func addGradient() {
        guard var adj = adjustments else { return }
        onEditBegin?()
        var g = LinearGradient()
        g.centerX = 0.5
        g.centerY = 0.15
        adj.gradients.append(g)
        adj.activeGradientIndex = adj.gradients.count - 1
        adjustments = adj
        onGradientSelectionChanged?()
        needsDisplay = true
        onEditChanged?()
    }

    // ---- heal interaction ------------------------------------------------

    private func healRadiusPx(_ s: HealSpot) -> CGFloat {
        guard let image else { return 0 }
        return CGFloat(s.radiusNorm * Double(max(image.width, image.height)) * scale)
    }

    private func beginHealDrag(_ p: NSPoint) -> Bool {
        guard var adj = adjustments else { return false }
        for (i, s) in adj.healSpots.enumerated() {
            let rpx = max(8, healRadiusPx(s))
            if Self.dist(p, normToView(s.targetX, s.targetY)) < rpx {
                drag = .healTarget; dragSpot = i; activeSpot = i
                onEditBegin?()
                return true
            }
            if !s.useInpaint, Self.dist(p, normToView(s.sourceX, s.sourceY)) < rpx {
                drag = .healSource; dragSpot = i; activeSpot = i
                onEditBegin?()
                return true
            }
        }
        // add a new spot
        let (nx, ny) = viewToNorm(p)
        var spot = HealSpot()
        spot.targetX = nx
        spot.targetY = ny
        spot.sourceX = min(max(nx - 0.06, 0), 1)
        spot.sourceY = ny
        spot.radiusNorm = max(0.005, healBrushSize / 500.0)
        spot.radius = healBrushSize
        spot.useInpaint = (healMode == .inpaint)
        onEditBegin?()
        adj.healSpots.append(spot)
        adjustments = adj
        drag = .healTarget
        dragSpot = adj.healSpots.count - 1
        activeSpot = dragSpot
        needsDisplay = true
        onEditChanged?()
        return true
    }

    private func deleteHealAt(_ p: NSPoint) {
        guard var adj = adjustments else { return }
        for (i, s) in adj.healSpots.enumerated() {
            let rpx = max(8, healRadiusPx(s))
            if Self.dist(p, normToView(s.targetX, s.targetY)) < rpx {
                onEditBegin?()
                adj.healSpots.remove(at: i)
                activeSpot = adj.healSpots.count - 1   // fall back to the previous spot
                adjustments = adj
                needsDisplay = true
                onEditChanged?()
                return
            }
        }
    }

    // ---- paint -----------------------------------------------------------

    override func draw(_ dirtyRect: NSRect) {
        Theme.fill(bounds, Theme.viewerBg)
        guard let image, let ctx = NSGraphicsContext.current?.cgContext else {
            Theme.drawCentered(L.t("選擇一張相片開始編輯"), in: bounds,
                               font: Theme.normal, color: Theme.textFaint)
            return
        }

        let dst = NSRect(x: offset.x, y: offset.y,
                         width: CGFloat(Double(image.width) * scale),
                         height: CGFloat(Double(image.height) * scale))
        ctx.saveGState()
        // Above 200% show the real pixels rather than a smoothed guess.
        ctx.interpolationQuality = scale >= 2 ? .none : .high
        // The view is flipped; draw the image the right way up inside it.
        ctx.translateBy(x: 0, y: dst.maxY + dst.minY)
        ctx.scaleBy(x: 1, y: -1)
        ctx.draw(image, in: dst)
        ctx.restoreGState()

        guard adjustments != nil else { return }
        switch tool {
        case .crop:     drawCropOverlay()
        case .gradient: drawGradientOverlay()
        case .heal:     drawHealOverlay()
        case .none:     break
        }
        if whiteBalancePickerActive {
            let r = NSRect(x: 0, y: 8, width: bounds.width, height: 20)
            Theme.drawCentered(L.t("點擊中性灰色區域設定白平衡"), in: r,
                               font: Theme.normal, color: Theme.text)
        }
    }

    private func imageViewRect() -> NSRect {
        guard let image else { return .zero }
        return NSRect(x: offset.x, y: offset.y,
                      width: CGFloat(Double(image.width) * scale),
                      height: CGFloat(Double(image.height) * scale))
    }

    private func drawCropOverlay() {
        let r = cropViewRect()
        let full = imageViewRect()

        // Dim everything outside the crop box.
        NSColor(white: 0, alpha: 0.55).setFill()
        let dim = NSBezierPath(rect: full)
        dim.append(NSBezierPath(rect: r).reversed)
        dim.windingRule = .evenOdd
        dim.fill()

        NSColor.white.setStroke()
        let box = NSBezierPath(rect: r)
        box.lineWidth = 1.5
        box.stroke()

        // rule of thirds
        NSColor(white: 1, alpha: 0.47).setStroke()
        let thin = NSBezierPath()
        for i in 1..<3 {
            let x = r.minX + r.width * CGFloat(i) / 3
            let y = r.minY + r.height * CGFloat(i) / 3
            thin.move(to: NSPoint(x: x, y: r.minY)); thin.line(to: NSPoint(x: x, y: r.maxY))
            thin.move(to: NSPoint(x: r.minX, y: y)); thin.line(to: NSPoint(x: r.maxX, y: y))
        }
        thin.lineWidth = 1
        thin.stroke()

        // handles
        NSColor.white.setFill()
        for pt in [NSPoint(x: r.minX, y: r.minY), NSPoint(x: r.maxX, y: r.minY),
                   NSPoint(x: r.minX, y: r.maxY), NSPoint(x: r.maxX, y: r.maxY),
                   NSPoint(x: r.midX, y: r.minY), NSPoint(x: r.midX, y: r.maxY),
                   NSPoint(x: r.minX, y: r.midY), NSPoint(x: r.maxX, y: r.midY)] {
            NSRect(x: pt.x - 3, y: pt.y - 3, width: 6, height: 6).fill()
        }
    }

    private func drawGradientOverlay() {
        guard let adj = adjustments else { return }
        let ai = adj.resolvedGradientIndex
        // Inactive gradients first, the active one on top.
        for (i, g) in adj.gradients.enumerated() where i != ai {
            drawOneGradient(g, isActive: false)
        }
        if ai >= 0, ai < adj.gradients.count {
            drawOneGradient(adj.gradients[ai], isActive: true)
        }
    }

    private func drawOneGradient(_ gr: LinearGradient, isActive: Bool) {
        guard let image else { return }
        let center = normToView(gr.centerX, gr.centerY)
        let (ux, uy) = Self.gradAxis(gr)
        let len = max(bounds.width, bounds.height)
        // The drawn line runs perpendicular to the axis of variation.
        let v = NSPoint(x: CGFloat(-uy), y: CGFloat(ux))
        func P(_ t: CGFloat) -> NSPoint { NSPoint(x: center.x + v.x * t, y: center.y + v.y * t) }

        let yellow = NSColor(srgbRed: 1, green: 0.86, blue: 0.24, alpha: isActive ? 0.90 : 0.39)
        yellow.setStroke()
        let line = NSBezierPath()
        line.move(to: P(-len)); line.line(to: P(len))
        line.lineWidth = isActive ? 1.6 : 1.2
        line.stroke()

        let hr: CGFloat = 5
        if !isActive {
            // A small white dot so an inactive gradient can be clicked to select it.
            NSColor(white: 1, alpha: 0.78).setFill()
            NSBezierPath(ovalIn: NSRect(x: center.x - hr, y: center.y - hr,
                                        width: hr * 2, height: hr * 2)).fill()
            return
        }

        // Yellow range band: dashed edges plus a handle marking the falloff distance.
        let rangePx = CGFloat(gr.range * Double(image.height) * scale)
        let c1 = NSPoint(x: center.x + CGFloat(ux) * rangePx, y: center.y + CGFloat(uy) * rangePx)
        let c2 = NSPoint(x: center.x - CGFloat(ux) * rangePx, y: center.y - CGFloat(uy) * rangePx)
        NSColor(srgbRed: 1, green: 0.86, blue: 0.24, alpha: 0.47).setStroke()
        for c in [c1, c2] {
            let p = NSBezierPath()
            p.move(to: NSPoint(x: c.x + v.x * -len, y: c.y + v.y * -len))
            p.line(to: NSPoint(x: c.x + v.x * len, y: c.y + v.y * len))
            p.lineWidth = 1
            p.setLineDash([4, 4], count: 2, phase: 0)
            p.stroke()
        }
        NSColor(srgbRed: 1, green: 0.86, blue: 0.24, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: c1.x - hr, y: c1.y - hr, width: hr * 2, height: hr * 2)).fill()

        // Blue rotate handle.
        let rot = rotateHandlePos(center, gr)
        NSColor(srgbRed: 0.47, green: 0.78, blue: 1, alpha: 0.78).setStroke()
        let rl = NSBezierPath()
        rl.move(to: center); rl.line(to: rot)
        rl.lineWidth = 1
        rl.stroke()
        NSColor(srgbRed: 0.47, green: 0.78, blue: 1, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: rot.x - hr, y: rot.y - hr, width: hr * 2, height: hr * 2)).fill()

        // White position handle on top.
        let wr: CGFloat = 6
        NSColor.white.setFill()
        NSBezierPath(ovalIn: NSRect(x: center.x - wr, y: center.y - wr,
                                    width: wr * 2, height: wr * 2)).fill()
    }

    private func drawHealOverlay() {
        guard let adj = adjustments else { return }
        for s in adj.healSpots {
            let rpx = healRadiusPx(s)
            let tc = normToView(s.targetX, s.targetY)
            NSColor(srgbRed: 0.35, green: 0.78, blue: 0.47, alpha: 0.90).setStroke()
            let t = NSBezierPath(ovalIn: NSRect(x: tc.x - rpx, y: tc.y - rpx,
                                                width: rpx * 2, height: rpx * 2))
            t.lineWidth = 1.6
            t.stroke()

            if !s.useInpaint {
                let sc = normToView(s.sourceX, s.sourceY)
                NSColor(srgbRed: 0.47, green: 0.71, blue: 0.94, alpha: 0.78).setStroke()
                let src = NSBezierPath(ovalIn: NSRect(x: sc.x - rpx, y: sc.y - rpx,
                                                      width: rpx * 2, height: rpx * 2))
                src.lineWidth = 1.4
                src.setLineDash([4, 4], count: 2, phase: 0)
                src.stroke()
                let link = NSBezierPath()
                link.move(to: sc); link.line(to: tc)
                link.lineWidth = 1.4
                link.setLineDash([4, 4], count: 2, phase: 0)
                link.stroke()
            }
        }
    }
}
