import AppKit
import AwayRawCore

/// Shared plumbing for the panels that are just a stack of sliders.
///
/// `onEditBegin` fires once when a gesture starts — the owner uses it to push an undo
/// step and, critically, to capture the multi-selection sync targets *before* the value
/// changes. `onChanged` fires for every value update.
class AdjustPanelBase: SectionPanel {
    /// The panel edits this copy in place; the owner reads it back on every `onChanged`.
    var adjustments: ImageAdjustments?

    var onEditBegin: (() -> Void)?
    var onChanged: (() -> Void)?

    private(set) var sliders: [AdjustmentSlider] = []

    /// Suppresses `onChanged` while the panel is being re-bound to another photo.
    private(set) var binding = false

    @discardableResult
    func addSlider(_ label: String, min lo: Double = -100, max hi: Double = 100,
                   default d: Double = 0, decimals: Int = 0,
                   bipolar: Bool = true, gradient: SliderGradient = .none,
                   apply: @escaping (Double) -> Void) -> AdjustmentSlider {
        let s = AdjustmentSlider()
        s.label = label
        s.minValue = lo
        s.maxValue = hi
        s.defaultValue = d
        s.decimals = decimals
        s.bipolar = bipolar
        s.gradient = gradient
        s.onEditBegin = { [weak self] in
            guard let self, !self.binding else { return }
            self.onEditBegin?()
        }
        s.onValueChanged = { [weak self] v in
            guard let self, !self.binding else { return }
            apply(v)
            self.onChanged?()
        }
        sliders.append(s)
        addSubview(s)
        return s
    }

    /// Run `body` with change notifications suppressed.
    func withoutNotifying(_ body: () -> Void) {
        binding = true
        body()
        binding = false
    }

    /// Stack the sliders under the title at a fixed pitch.
    override func relayout() {
        let x: CGFloat = 12
        let w = bounds.width - 24
        var y = titleHeight + 2
        for s in sliders {
            s.frame = NSRect(x: x, y: y, width: w, height: 30)
            y += 34
        }
    }
}

/// 基本調整 — exposure through blacks.
final class BasicAdjustPanel: AdjustPanelBase {
    private var exposureSlider: AdjustmentSlider!

    convenience init() {
        self.init(title: "基本調整")
        exposureSlider = addSlider("曝光", min: -5, max: 5, decimals: 2) { [weak self] v in
            self?.adjustments?.exposure = v
        }
        addSlider("對比") { [weak self] v in self?.adjustments?.contrast = v }
        addSlider("亮部") { [weak self] v in self?.adjustments?.highlights = v }
        addSlider("暗部") { [weak self] v in self?.adjustments?.shadows = v }
        addSlider("白色") { [weak self] v in self?.adjustments?.whites = v }
        addSlider("黑色") { [weak self] v in self?.adjustments?.blacks = v }
    }

    func bind(_ a: ImageAdjustments) {
        adjustments = a
        withoutNotifying {
            // The exposure range depends on the pipeline version — legacy sliders were ±2
            // (so old slider positions still mean what they meant), v1 is ±5 true EV.
            // The range has to change *before* the value is set, or the value gets clamped
            // by the previous bounds.
            let limit = a.isLegacyPipeline ? 2.0 : 5.0
            exposureSlider.setRange(min: -limit, max: limit)
            exposureSlider.setValueSilent(a.exposure)
            sliders[1].setValueSilent(a.contrast)
            sliders[2].setValueSilent(a.highlights)
            sliders[3].setValueSilent(a.shadows)
            sliders[4].setValueSilent(a.whites)
            sliders[5].setValueSilent(a.blacks)
        }
    }
}

/// 色彩 — the white-balance row plus temperature/tint/vibrance/saturation.
final class ColorPanel: AdjustPanelBase {
    private let pickerLabel = NSTextField(labelWithString: "")
    private let pickerButton = IconButton(glyph: "💧")
    private let asShotButton = FlatButton(title: "拍攝時設定")
    private let presetNote = NSTextField(labelWithString: "")

    var onPickerToggled: ((Bool) -> Void)?
    var onAsShot: (() -> Void)?

    private(set) var pickerActive = false {
        didSet { pickerButton.isActive = pickerActive }
    }

    /// Slider ±100 ↔ 5200 ± 3000 K for photos that have no camera Kelvin scale.
    static let nonRawScale = 30.0
    static func clampToNonRawRange(_ kelvin: Double) -> Double {
        min(max(kelvin, 5200 - 100 * nonRawScale), 5200 + 100 * nonRawScale)
    }

    /// RAW photos edit in Kelvin; everything else on a 0-centred ±100 scale (there is no
    /// as-shot Kelvin to anchor to). Call before `bind` so the value loads in the right
    /// scale — the Windows build does the same in its RebindAll.
    private var tempIsRaw = true
    func setTemperatureMode(isRaw: Bool) {
        tempIsRaw = isRaw
        let t = sliders[0]
        if isRaw {
            t.setRange(min: ColorScience.minKelvin, max: ColorScience.maxKelvin, default: 5200)
            t.bipolar = false
            t.wheelStep = 50
        } else {
            t.setRange(min: -100, max: 100, default: 0)
            t.bipolar = true
            t.wheelStep = 1
        }
        t.needsDisplay = true
    }

    private func tempToSlider(_ k: Double) -> Double { tempIsRaw ? k : (k - 5200) / Self.nonRawScale }
    private func sliderToTemp(_ v: Double) -> Double { tempIsRaw ? v : 5200 + v * Self.nonRawScale }

    convenience init() {
        self.init(title: "色彩")

        pickerLabel.stringValue = L.t("白平衡選擇器")
        pickerLabel.font = Theme.normal
        pickerLabel.textColor = Theme.textDim
        addSubview(pickerLabel)

        pickerButton.tooltip = "點擊畫面中的中性灰設定白平衡"
        pickerButton.onClick = { [weak self] in
            guard let self else { return }
            self.pickerActive.toggle()
            self.onPickerToggled?(self.pickerActive)
        }
        addSubview(pickerButton)

        asShotButton.onClick = { [weak self] in self?.onAsShot?() }
        addSubview(asShotButton)

        // Sits where the white-balance row is hidden in the preset editor, explaining
        // that a preset leaves white balance alone.
        presetNote.stringValue = L.t("套用風格檔時維持照片目前的色溫／色調")
        presetNote.font = Theme.small
        presetNote.textColor = Theme.textFaint
        presetNote.isHidden = true
        addSubview(presetNote)

        addSlider("色溫", min: ColorScience.minKelvin, max: ColorScience.maxKelvin,
                  default: 5200, bipolar: false, gradient: .temperature) { [weak self] v in
            guard let self else { return }
            self.adjustments?.temperature = self.sliderToTemp(v)
        }
        addSlider("色調", gradient: .tint) { [weak self] v in self?.adjustments?.tint = v }
        addSlider("鮮豔度", gradient: .saturation) { [weak self] v in self?.adjustments?.vibrance = v }
        addSlider("飽和度", gradient: .saturation) { [weak self] v in self?.adjustments?.saturation = v }
    }

    func bind(_ a: ImageAdjustments) {
        adjustments = a
        withoutNotifying {
            sliders[0].setValueSilent(tempToSlider(a.temperature))
            sliders[1].setValueSilent(a.tint)
            sliders[2].setValueSilent(a.vibrance)
            sliders[3].setValueSilent(a.saturation)
        }
    }

    func setPickerActive(_ on: Bool) { pickerActive = on }

    /// Used by the preset editor: hide the white-balance row and show the note instead.
    func showPresetWhiteBalanceNote() {
        pickerLabel.isHidden = true
        pickerButton.isHidden = true
        asShotButton.isHidden = true
        presetNote.isHidden = false
    }

    override func relayout() {
        let x: CGFloat = 12
        let w = bounds.width - 24
        let rowY = titleHeight + 2
        // Measured, not fixed: the label changes with language and font size, and a fixed
        // 110 clips the last character of 白平衡選擇器 in large fonts or long translations.
        let asShotW = max(96, Theme.measure(L.t(asShotButton.title), font: Theme.normal).width + 20)
        let labelW = min(Theme.measure(pickerLabel.stringValue, font: Theme.normal).width + 4,
                         bounds.width - 24 - 26 - asShotW - 12)
        pickerLabel.frame = NSRect(x: x, y: rowY + 3, width: labelW, height: 18)
        pickerButton.frame = NSRect(x: x + labelW + 6, y: rowY, width: 26, height: 24)
        asShotButton.frame = NSRect(x: bounds.width - 12 - asShotW, y: rowY, width: asShotW, height: 24)
        presetNote.frame = NSRect(x: x, y: rowY + 4, width: w, height: 16)

        var y = rowY + 32
        for s in sliders {
            s.frame = NSRect(x: x, y: y, width: w, height: 30)
            y += 34
        }
    }
}

/// 細節 — sharpening, vignette, noise reduction.
final class DetailPanel: AdjustPanelBase {
    convenience init() {
        self.init(title: "細節")
        addSlider("銳利度") { [weak self] v in self?.adjustments?.sharpening = v }
        addSlider("暗角") { [weak self] v in self?.adjustments?.vignette = v }
        addSlider("降噪", min: 0, max: 100, bipolar: false) { [weak self] v in
            self?.adjustments?.noiseReduction = v
        }
    }

    func bind(_ a: ImageAdjustments) {
        adjustments = a
        withoutNotifying {
            sliders[0].setValueSilent(a.sharpening)
            sliders[1].setValueSilent(a.vignette)
            sliders[2].setValueSilent(a.noiseReduction)
        }
    }
}

/// 套用風格檔 — the preset picker and its apply button.
final class PresetPanel: SectionPanel {
    private let combo = DarkComboBox(frame: .zero)
    private let applyButton = FlatButton(title: "套用該風格檔")

    var onApply: ((String) -> Void)?

    /// Storage keys, parallel to the combo's translated display strings.
    private var names: [String] = []

    convenience init() {
        self.init(title: "風格檔種類")
        addSubview(combo)
        applyButton.isPrimary = true
        applyButton.onClick = { [weak self] in
            guard let self, self.combo.selectedIndex >= 0,
                  self.combo.selectedIndex < self.names.count else { return }
            self.onApply?(self.names[self.combo.selectedIndex])
        }
        addSubview(applyButton)
        reload()
    }

    /// Rebuild the list — called at startup and whenever the preset editor saves.
    func reload() {
        names = PresetStore.allNames()
        combo.setItems(names.map { L.t($0) }, selected: 0)
    }

    var selectedName: String {
        combo.selectedIndex >= 0 && combo.selectedIndex < names.count
            ? names[combo.selectedIndex] : PresetProfile.defaultName
    }

    override func relayout() {
        let x: CGFloat = 12
        let w = bounds.width - 24
        combo.frame = NSRect(x: x, y: titleHeight + 2, width: w, height: 26)
        applyButton.frame = NSRect(x: x, y: titleHeight + 34, width: w, height: 28)
    }
}
