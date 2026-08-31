import AppKit
import AwayRawCore

/// 工具 — a ribbon with three pages (裁切 / 漸層 / 修護). Only the selected page's
/// controls are visible; each page ends with its own reset button.
final class ToolsPanel: SectionPanel {

    let tabs = TopTab()

    var adjustments: ImageAdjustments?

    var onEditBegin: (() -> Void)?
    var onChanged: (() -> Void)?
    var onToolChanged: ((ToolMode) -> Void)?
    var onAddGradient: (() -> Void)?
    var onHealModeChanged: ((HealMode) -> Void)?
    var onHealBrushSize: ((Double) -> Void)?
    var onRotate: ((Bool) -> Void)?          // true = clockwise
    /// The 比例 popup changed; the owner reshapes the crop box.
    var onCropAspectChanged: ((String) -> Void)?

    /// Starts as `.none`, like Windows: the crop page shows as a placeholder but is
    /// locked until a tab is picked. Clicking the active tab returns to `.none`.
    private(set) var tool: ToolMode = .none

    /// Sits over the page controls while no tool is selected and swallows the mouse.
    private let ribbonLock = RibbonLockView()

    // ---- crop page -------------------------------------------------------
    private let ratioLabel = NSTextField(labelWithString: "")
    private let ratioCombo = DarkComboBox(frame: .zero)
    private let ratioW = DarkNumberField(frame: .zero)
    private let ratioH = DarkNumberField(frame: .zero)
    private let angleSlider = AdjustmentSlider()
    private let distortSlider = AdjustmentSlider()
    private let rotateLeft = FlatButton(title: "照片左轉90度")
    private let rotateRight = FlatButton(title: "照片右轉90度")
    private let cropReset = FlatButton(title: "裁切重設")

    private let ratioNames = ["Original", "3:2", "4:3", "16:9", "1:1", "Custom"]

    // ---- gradient page ---------------------------------------------------
    private let gradAdd = FlatButton(title: "新增線性漸層")
    private let gradExposure = AdjustmentSlider()
    private let gradContrast = AdjustmentSlider()
    private let gradHighlights = AdjustmentSlider()
    private let gradShadows = AdjustmentSlider()
    private let gradSaturation = AdjustmentSlider()
    private let gradHint = NSTextField(labelWithString: "")
    private let gradReset = FlatButton(title: "漸層重設（清除全部）")

    // ---- heal page -------------------------------------------------------
    private let healClone = FlatButton(title: "仿製")
    private let healInpaint = FlatButton(title: "修補")
    private let healSize = AdjustmentSlider()
    private let healHint = NSTextField(labelWithString: "")
    private let healReset = FlatButton(title: "修護重設")

    private var binding = false

    convenience init() {
        self.init(title: "工具")
        buildCropPage()
        buildGradientPage()
        buildHealPage()

        tabs.tabs = ["裁切", "漸層", "修護"]
        tabs.allowDeselect = true
        tabs.selectedIndex = -1        // no tool until the user picks one
        tabs.onSelect = { [weak self] i in
            guard let self else { return }
            self.tool = Self.mode(forTab: i)
            self.updateVisibility()
            self.onToolChanged?(self.tool)
        }
        addSubview(tabs)
        addSubview(ribbonLock)         // last, so it is on top of the page controls
        updateVisibility()
    }

    private static func mode(forTab i: Int) -> ToolMode {
        switch i { case 0: return .crop; case 1: return .gradient; case 2: return .heal; default: return .none }
    }
    private static func tab(for mode: ToolMode) -> Int {
        switch mode { case .crop: return 0; case .gradient: return 1; case .heal: return 2; case .none: return -1 }
    }

    /// Select a tool programmatically (Esc, the c/g/h keys); fires `onToolChanged`.
    func selectTool(_ mode: ToolMode) {
        tabs.selectedIndex = Self.tab(for: mode)
        tool = mode
        updateVisibility()
        onToolChanged?(mode)
    }

    // ---- construction ----------------------------------------------------

    private func slider(_ s: AdjustmentSlider, _ label: String,
                        min lo: Double = -100, max hi: Double = 100,
                        default d: Double = 0, decimals: Int = 0,
                        bipolar: Bool = true,
                        apply: @escaping (Double) -> Void) {
        s.label = label
        s.minValue = lo; s.maxValue = hi
        s.defaultValue = d
        s.decimals = decimals
        s.bipolar = bipolar
        s.onEditBegin = { [weak self] in
            guard let self, !self.binding else { return }
            self.onEditBegin?()
        }
        s.onValueChanged = { [weak self] v in
            guard let self, !self.binding else { return }
            apply(v)
            self.onChanged?()
        }
        addSubview(s)
    }

    private func buildCropPage() {
        ratioLabel.stringValue = L.t("比例")
        ratioLabel.font = Theme.normal
        ratioLabel.textColor = Theme.textDim
        addSubview(ratioLabel)

        // Display names are translated; "Original"/"Custom" are the storage keys.
        ratioCombo.setItems(ratioNames.map { $0 == "Original" ? L.t("原始") : ($0 == "Custom" ? L.t("自訂") : $0) })
        ratioCombo.onChange = { [weak self] i in
            guard let self, !self.binding else { return }
            self.onEditBegin?()
            let aspect = self.currentRatioString(i)
            self.adjustments?.cropAspectRatio = aspect
            self.updateRatioFieldsEnabled()
            self.onChanged?()
            self.onCropAspectChanged?(aspect)
        }
        addSubview(ratioCombo)

        ratioW.setRange(min: 1, max: 999); ratioW.intValue = 3
        ratioH.setRange(min: 1, max: 999); ratioH.intValue = 2
        let custom: (Int) -> Void = { [weak self] _ in
            guard let self, !self.binding, self.ratioCombo.selectedIndex == 5 else { return }
            self.onEditBegin?()
            let aspect = "\(self.ratioW.intValue):\(self.ratioH.intValue)"
            self.adjustments?.cropAspectRatio = aspect
            self.onChanged?()
            self.onCropAspectChanged?(aspect)
        }
        ratioW.onChange = custom
        ratioH.onChange = custom
        addSubview(ratioW)
        addSubview(ratioH)

        // The angle slider runs the opposite way to the stored value: the user asked for
        // the reversed direction in 2026-07, and only this binding negates — the stored
        // meaning of CropAngle is unchanged.
        slider(angleSlider, "角度", min: -45, max: 45, decimals: 1) { [weak self] v in
            self?.adjustments?.cropAngle = -v
        }
        slider(distortSlider, "廣角變形") { [weak self] v in
            self?.adjustments?.distortion = v
        }

        rotateLeft.onClick = { [weak self] in self?.onRotate?(false) }
        rotateRight.onClick = { [weak self] in self?.onRotate?(true) }
        addSubview(rotateLeft)
        addSubview(rotateRight)

        cropReset.onClick = { [weak self] in
            guard let self else { return }
            self.onEditBegin?()
            self.adjustments?.cropX = 0
            self.adjustments?.cropY = 0
            self.adjustments?.cropWidth = 1
            self.adjustments?.cropHeight = 1
            self.adjustments?.cropAngle = 0
            self.adjustments?.distortion = 0
            self.adjustments?.rotation = .r0
            self.adjustments?.cropAspectRatio = "Original"
            self.rebindCrop()
            self.onChanged?()
        }
        addSubview(cropReset)
    }

    private func buildGradientPage() {
        gradAdd.onClick = { [weak self] in self?.onAddGradient?() }
        addSubview(gradAdd)

        slider(gradExposure, "曝光", min: -5, max: 5, decimals: 2) { [weak self] v in
            self?.mutateActiveGradient { $0.exposure = v }
        }
        slider(gradContrast, "對比") { [weak self] v in
            self?.mutateActiveGradient { $0.contrast = v }
        }
        slider(gradHighlights, "亮部") { [weak self] v in
            self?.mutateActiveGradient { $0.highlights = v }
        }
        slider(gradShadows, "暗部") { [weak self] v in
            self?.mutateActiveGradient { $0.shadows = v }
        }
        slider(gradSaturation, "飽和度") { [weak self] v in
            self?.mutateActiveGradient { $0.saturation = v }
        }

        gradHint.stringValue = L.t("拖曳白點移動、黃點調範圍、藍點旋轉；右鍵刪除")
        gradHint.font = Theme.small
        gradHint.textColor = Theme.textFaint
        gradHint.lineBreakMode = .byWordWrapping
        gradHint.maximumNumberOfLines = 2
        gradHint.usesSingleLineMode = false
        gradHint.cell?.wraps = true
        gradHint.cell?.isScrollable = false
        gradHint.preferredMaxLayoutWidth = 266
        addSubview(gradHint)

        gradReset.onClick = { [weak self] in
            guard let self else { return }
            self.onEditBegin?()
            self.adjustments?.gradients.removeAll()
            self.adjustments?.activeGradientIndex = -1
            self.rebindGradient()
            self.onChanged?()
        }
        addSubview(gradReset)
    }

    private func buildHealPage() {
        healClone.onClick = { [weak self] in self?.setHealMode(.clone) }
        healInpaint.onClick = { [weak self] in self?.setHealMode(.inpaint) }
        healClone.isPrimary = true
        addSubview(healClone)
        addSubview(healInpaint)

        slider(healSize, "大小", min: 1, max: 50, default: 10, bipolar: false) { [weak self] v in
            self?.adjustments?.healSize = v
            self?.onHealBrushSize?(v)
        }

        healHint.stringValue = L.t("點擊畫面新增修護點；拖曳移動、右鍵刪除")
        healHint.font = Theme.small
        healHint.textColor = Theme.textFaint
        healHint.lineBreakMode = .byWordWrapping
        healHint.maximumNumberOfLines = 2
        healHint.usesSingleLineMode = false
        healHint.cell?.wraps = true
        healHint.cell?.isScrollable = false
        healHint.preferredMaxLayoutWidth = 266
        addSubview(healHint)

        healReset.onClick = { [weak self] in
            guard let self else { return }
            self.onEditBegin?()
            self.adjustments?.healSpots.removeAll()
            self.onChanged?()
        }
        addSubview(healReset)
    }

    private func setHealMode(_ m: HealMode) {
        healClone.isPrimary = (m == .clone)
        healInpaint.isPrimary = (m == .inpaint)
        onHealModeChanged?(m)
    }

    private func mutateActiveGradient(_ body: (inout LinearGradient) -> Void) {
        guard var adj = adjustments else { return }
        let i = adj.resolvedGradientIndex
        guard i >= 0, i < adj.gradients.count else { return }
        var g = adj.gradients[i]
        body(&g)
        adj.gradients[i] = g
        adjustments = adj
    }

    private func currentRatioString(_ index: Int) -> String {
        switch index {
        case 0: return "Original"
        case 5: return "\(ratioW.intValue):\(ratioH.intValue)"
        default: return ratioNames[index]
        }
    }

    private func updateRatioFieldsEnabled() {
        let custom = ratioCombo.selectedIndex == 5
        ratioW.alphaValue = custom ? 1 : 0.45
        ratioH.alphaValue = custom ? 1 : 0.45
    }

    // ---- binding ---------------------------------------------------------

    func bind(_ a: ImageAdjustments) {
        adjustments = a
        binding = true
        rebindCrop()
        rebindGradient()
        healSize.setValueSilent(a.healSize)
        binding = false
    }

    private func rebindCrop() {
        guard let a = adjustments else { return }
        let wasBinding = binding
        binding = true
        angleSlider.setValueSilent(-a.cropAngle)
        distortSlider.setValueSilent(a.distortion)
        if let i = ratioNames.firstIndex(of: a.cropAspectRatio) {
            ratioCombo.selectedIndex = i
        } else {
            // A stored "W:H" that is not one of the presets is the custom entry.
            ratioCombo.selectedIndex = 5
            let parts = a.cropAspectRatio.split(separator: ":")
            if parts.count == 2, let w = Int(parts[0]), let h = Int(parts[1]) {
                ratioW.intValue = w
                ratioH.intValue = h
            }
        }
        updateRatioFieldsEnabled()
        binding = wasBinding
    }

    /// Refresh the gradient sliders from whichever gradient is selected.
    func rebindGradient() {
        let wasBinding = binding
        binding = true
        let g = adjustments?.activeGradient
        let has = (g != nil)
        for s in [gradExposure, gradContrast, gradHighlights, gradShadows, gradSaturation] {
            s.alphaValue = has ? 1 : 0.45
        }
        gradExposure.setValueSilent(g?.exposure ?? 0)
        gradContrast.setValueSilent(g?.contrast ?? 0)
        gradHighlights.setValueSilent(g?.highlights ?? 0)
        gradShadows.setValueSilent(g?.shadows ?? 0)
        gradSaturation.setValueSilent(g?.saturation ?? 0)
        binding = wasBinding
    }

    // ---- layout ----------------------------------------------------------

    private func updateVisibility() {
        let cropViews: [NSView] = [ratioLabel, ratioCombo, ratioW, ratioH, angleSlider,
                                   distortSlider, rotateLeft, rotateRight, cropReset]
        let gradViews: [NSView] = [gradAdd, gradExposure, gradContrast, gradHighlights,
                                   gradShadows, gradSaturation, gradHint, gradReset]
        let healViews: [NSView] = [healClone, healInpaint, healSize, healHint, healReset]
        // With no tool the crop page stays visible as a placeholder, dimmed and locked.
        let locked = (tool == .none)
        for v in cropViews { v.isHidden = !(tool == .crop || locked); v.alphaValue = locked ? 0.45 : 1 }
        for v in gradViews { v.isHidden = (tool != .gradient) }
        for v in healViews { v.isHidden = (tool != .heal) }
        ribbonLock.isHidden = !locked
        needsLayout = true
    }

    override func relayout() {
        let x: CGFloat = 12
        let w = bounds.width - 24
        tabs.frame = NSRect(x: 6, y: titleHeight, width: bounds.width - 12, height: 28)
        var y = titleHeight + 34
        let resetY = bounds.height - 36

        ribbonLock.frame = NSRect(x: 0, y: y - 6, width: bounds.width, height: bounds.height - y + 6)

        switch tool {
        case .crop, .none:              // .none lays out the crop page as the placeholder
            // The popup keeps at least 110 pt; a long label (Seitenverhältnis) is clipped.
            let lw = min(Theme.measure(ratioLabel.stringValue, font: Theme.normal).width + 4,
                         w - 96 - 110 - 8)
            ratioLabel.frame = NSRect(x: x, y: y + 4, width: lw, height: 18)
            ratioCombo.frame = NSRect(x: x + lw + 4, y: y, width: w - lw - 4 - 96, height: 26)
            ratioW.frame = NSRect(x: bounds.width - 12 - 92, y: y, width: 44, height: 26)
            ratioH.frame = NSRect(x: bounds.width - 12 - 44, y: y, width: 44, height: 26)
            y += 34
            angleSlider.frame = NSRect(x: x, y: y, width: w, height: 30); y += 36
            distortSlider.frame = NSRect(x: x, y: y, width: w, height: 30); y += 40
            rotateLeft.frame = NSRect(x: x, y: y, width: (w - 8) / 2, height: 28)
            rotateRight.frame = NSRect(x: x + (w - 8) / 2 + 8, y: y, width: (w - 8) / 2, height: 28)
            cropReset.frame = NSRect(x: x, y: resetY, width: w, height: 28)

        case .gradient:
            gradAdd.frame = NSRect(x: x, y: y, width: w, height: 28); y += 36
            for s in [gradExposure, gradContrast, gradHighlights, gradShadows, gradSaturation] {
                s.frame = NSRect(x: x, y: y, width: w, height: 30)
                y += 34
            }
            gradHint.frame = NSRect(x: x, y: y + 2, width: w, height: 30)
            gradReset.frame = NSRect(x: x, y: resetY, width: w, height: 28)

        case .heal:
            healClone.frame = NSRect(x: x, y: y, width: (w - 8) / 2, height: 28)
            healInpaint.frame = NSRect(x: x + (w - 8) / 2 + 8, y: y, width: (w - 8) / 2, height: 28)
            y += 36
            healSize.frame = NSRect(x: x, y: y, width: w, height: 30); y += 36
            healHint.frame = NSRect(x: x, y: y + 2, width: w, height: 30)
            healReset.frame = NSRect(x: x, y: resetY, width: w, height: 28)
        }
    }
}

/// Transparent view that eats mouse events, so the placeholder page cannot be edited
/// while no tool is selected (the Windows build disables the ribbon host instead).
private final class RibbonLockView: NSView {
    // `point` is in the superview's coordinates; answering `self` for every point
    // swallowed the tab strip above as well, so no tool could ever be picked.
    override func hitTest(_ point: NSPoint) -> NSView? {
        (!isHidden && frame.contains(point)) ? self : nil
    }
    override func mouseDown(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) {}
    override func rightMouseDown(with event: NSEvent) {}
    override func scrollWheel(with event: NSEvent) { nextResponder?.scrollWheel(with: event) }
}
