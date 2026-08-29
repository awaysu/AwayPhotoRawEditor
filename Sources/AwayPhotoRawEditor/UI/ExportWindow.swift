import AppKit
import AwayRawCore

/// 匯出照片 — destination, rename rule, size/DPI, format, and the watermark.
final class ExportWindowController: NSObject {

    let panel: NSPanel
    private var settings: ExportSettings
    private let count: Int
    private weak var host: NSWindow?

    var onConfirm: ((ExportSettings) -> Void)?
    /// 儲存設定 without starting the export.
    var onSave: ((ExportSettings) -> Void)?
    /// Any watermark field changed — the caller re-renders the preview live.
    var onWatermarkChanged: ((ExportSettings) -> Void)?

    // destination
    private let locGroup = RadioGroup()
    private let customPathField = NSTextField(frame: .zero)
    private let browseButton = FlatButton(title: "瀏覽")
    private let subFolderCheck = DarkCheckBox(title: "儲存至次資料夾")
    private let subFolderField = NSTextField(frame: .zero)

    // naming
    private let renameCombo = DarkComboBox(frame: .zero)
    private let conflictCombo = DarkComboBox(frame: .zero)

    // image
    private let formatCombo = DarkComboBox(frame: .zero)
    private let longEdgeField = NSTextField(frame: .zero)
    private let dpiField = NSTextField(frame: .zero)
    private let qualitySlider = AdjustmentSlider()
    private let preserveExifCheck = DarkCheckBox(title: "保存 EXIF（相機 / 鏡頭 / 拍攝資訊）")
    private let openAfterCheck = DarkCheckBox(title: "轉檔完成後開啟檔案總管顯示")

    // watermark
    private let wmEnabled = DarkCheckBox(title: "啟用浮水印")
    private let wmText = NSTextField(frame: .zero)
    private let wmFont = NSTextField(frame: .zero)
    private let wmSize = AdjustmentSlider()
    private let wmAlpha = AdjustmentSlider()
    private let wmColor = DarkComboBox(frame: .zero)
    private let wmPosition = DarkComboBox(frame: .zero)
    private let wmMargin = NSTextField(frame: .zero)

    private let okButton = FlatButton(title: "儲存設定並開始轉存")
    private let saveButton = FlatButton(title: "儲存設定")
    private let cancelButton = FlatButton(title: "取消")

    private let colorNames: [WatermarkColor] = WatermarkColor.allCases
    private let positionNames: [WatermarkPosition] = [.topLeft, .topRight, .bottomLeft, .bottomRight]

    init(settings: ExportSettings, count: Int) {
        self.settings = settings
        self.count = count
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 560, height: 700),
                        styleMask: [.titled, .closable], backing: .buffered, defer: false)
        super.init()
        panel.title = L.t("匯出設定")
        panel.appearance = Theme.appearance
        build()
        load()
    }

    private func build() {
        let content = FlippedView(frame: NSRect(x: 0, y: 0, width: 560, height: 700))
        content.wantsLayer = true
        content.layer?.backgroundColor = Theme.panelBg.cgColor
        panel.contentView = content

        var y: CGFloat = 16
        func header(_ t: String) {
            let l = NSTextField(labelWithString: L.t(t))
            l.font = Theme.dialogTitle
            l.textColor = Theme.text
            l.frame = NSRect(x: 20, y: y, width: 520, height: 22)
            content.addSubview(l)
            y += 30
        }
        func label(_ t: String, x: CGFloat = 20, w: CGFloat = 110) -> NSTextField {
            let l = NSTextField(labelWithString: L.t(t))
            l.font = Theme.normal
            l.textColor = Theme.textDim
            l.frame = NSRect(x: x, y: y + 4, width: w, height: 18)
            content.addSubview(l)
            return l
        }
        func style(_ f: NSTextField) {
            f.font = Theme.normal
            f.backgroundColor = Theme.panelBg3
            f.textColor = Theme.text
            f.isBordered = true
            f.bezelStyle = .squareBezel
            f.focusRingType = .none
        }

        header("匯出照片")
        y -= 8
        let sub = NSTextField(labelWithString: L.f("共 {0} 張相片將被轉存", count))
        sub.font = Theme.small; sub.textColor = Theme.textDim
        sub.frame = NSRect(x: 20, y: y, width: 520, height: 18)
        content.addSubview(sub)
        y += 26

        // ---- destination
        header("儲存位置")
        for (i, t) in ["桌面", "同原始照片目錄", "自己選擇"].enumerated() {
            let r = DarkRadioButton(title: t)
            r.frame = NSRect(x: 30, y: y, width: 260, height: 22)
            content.addSubview(r)
            locGroup.add(r)
            y += 26
            _ = i
        }
        locGroup.onChange = { [weak self] i in
            self?.settings.location = [.desktop, .sameAsSource, .custom][i]
            self?.updateEnabled()
        }
        style(customPathField)
        customPathField.frame = NSRect(x: 30, y: y, width: 420, height: 24)
        content.addSubview(customPathField)
        browseButton.frame = NSRect(x: 458, y: y, width: 82, height: 24)
        browseButton.onClick = { [weak self] in self?.browse() }
        content.addSubview(browseButton)
        y += 32

        subFolderCheck.frame = NSRect(x: 30, y: y, width: 180, height: 22)
        subFolderCheck.onToggle = { [weak self] v in
            self?.settings.useSubFolder = v; self?.updateEnabled()
        }
        content.addSubview(subFolderCheck)
        style(subFolderField)
        subFolderField.frame = NSRect(x: 220, y: y, width: 200, height: 24)
        content.addSubview(subFolderField)
        y += 34

        // ---- naming
        header("重新命名")
        _ = label("重新命名規則")
        renameCombo.setItems(["按照原始檔案", "日期時間（IMG 年月日時分秒＋序號）", "數字開始（IMG00001）"].map { L.t($0) })
        renameCombo.frame = NSRect(x: 140, y: y, width: 400, height: 26)
        renameCombo.onChange = { [weak self] i in
            self?.settings.rename = [.original, .dateTime, .sequence][i]
        }
        content.addSubview(renameCombo)
        y += 34

        let conflictLabelW = min(Theme.measure(L.t("存檔遇到相同檔名"), font: Theme.normal).width + 8, 230)
        _ = label("存檔遇到相同檔名", w: conflictLabelW)
        conflictCombo.setItems(["檔名接續 \"_數字\"，例如 _1, _2...", "直接覆蓋"].map { L.t($0) })
        conflictCombo.frame = NSRect(x: 20 + conflictLabelW + 6, y: y, width: 540 - 20 - conflictLabelW - 6, height: 26)
        conflictCombo.onChange = { [weak self] i in
            self?.settings.conflict = [.appendNumber, .overwrite][i]
        }
        content.addSubview(conflictCombo)
        y += 34

        // ---- image
        header("格式與尺寸")
        _ = label("格式")
        formatCombo.setItems(["JPEG", "PNG", "TIFF", "BMP"])
        formatCombo.frame = NSRect(x: 140, y: y, width: 120, height: 26)
        formatCombo.onChange = { [weak self] i in
            self?.settings.format = [.jpeg, .png, .tiff, .bmp][i]
            self?.updateEnabled()
        }
        content.addSubview(formatCombo)

        let edgeW = min(Theme.measure(L.t("寬長最大"), font: Theme.normal).width + 6, 100)
        _ = label("寬長最大", x: 262, w: edgeW)
        style(longEdgeField)
        longEdgeField.frame = NSRect(x: 262 + edgeW + 4, y: y, width: 64, height: 24)
        content.addSubview(longEdgeField)

        _ = label("解析度", x: 424, w: 56)
        style(dpiField)
        dpiField.frame = NSRect(x: 482, y: y, width: 58, height: 24)
        content.addSubview(dpiField)
        y += 34

        qualitySlider.label = "JPEG 品質"
        qualitySlider.minValue = 50
        qualitySlider.maxValue = 100
        qualitySlider.defaultValue = 100
        qualitySlider.bipolar = false
        qualitySlider.frame = NSRect(x: 20, y: y, width: 520, height: 30)
        qualitySlider.onValueChanged = { [weak self] v in self?.settings.jpegQuality = Int(v) }
        content.addSubview(qualitySlider)
        y += 36

        preserveExifCheck.frame = NSRect(x: 20, y: y, width: 520, height: 22)
        preserveExifCheck.onToggle = { [weak self] v in self?.settings.preserveExif = v }
        content.addSubview(preserveExifCheck)
        y += 28
        openAfterCheck.frame = NSRect(x: 20, y: y, width: 520, height: 22)
        openAfterCheck.onToggle = { [weak self] v in self?.settings.openFinderAfter = v }
        content.addSubview(openAfterCheck)
        y += 34

        // ---- watermark
        header("標誌")
        wmEnabled.frame = NSRect(x: 20, y: y, width: 180, height: 22)
        wmEnabled.onToggle = { [weak self] v in
            self?.settings.watermarkEnabled = v; self?.updateEnabled()
        }
        content.addSubview(wmEnabled)
        style(wmText)
        wmText.frame = NSRect(x: 210, y: y - 1, width: 330, height: 24)
        content.addSubview(wmText)
        y += 32

        _ = label("字體")
        style(wmFont)
        wmFont.frame = NSRect(x: 140, y: y, width: 180, height: 24)
        content.addSubview(wmFont)

        _ = label("位置", x: 330, w: 64)
        wmPosition.setItems(["左上", "右上", "左下", "右下"].map { L.t($0) })
        wmPosition.frame = NSRect(x: 396, y: y - 1, width: 144, height: 26)
        wmPosition.onChange = { [weak self] i in
            guard let self else { return }
            self.settings.watermarkPosition = self.positionNames[i]
        }
        content.addSubview(wmPosition)
        y += 34

        _ = label("顏色")
        wmColor.setItems(["白", "黑", "藍", "黃", "綠", "紅", "灰", "橘"].map { L.t($0) })
        wmColor.frame = NSRect(x: 140, y: y, width: 120, height: 26)
        wmColor.onChange = { [weak self] i in
            guard let self else { return }
            self.settings.watermarkColor = self.colorNames[i]
        }
        content.addSubview(wmColor)

        _ = label("邊距", x: 280, w: 64)
        style(wmMargin)
        wmMargin.frame = NSRect(x: 346, y: y, width: 64, height: 24)
        content.addSubview(wmMargin)
        y += 34

        wmSize.label = "大小"
        wmSize.minValue = 6; wmSize.maxValue = 300; wmSize.defaultValue = 150
        wmSize.bipolar = false
        wmSize.frame = NSRect(x: 20, y: y, width: 520, height: 30)
        wmSize.onValueChanged = { [weak self] v in self?.settings.watermarkFontSize = v }
        content.addSubview(wmSize)
        y += 34

        wmAlpha.label = "透明度"
        wmAlpha.minValue = 0; wmAlpha.maxValue = 100; wmAlpha.defaultValue = 20
        wmAlpha.bipolar = false
        wmAlpha.frame = NSRect(x: 20, y: y, width: 520, height: 30)
        wmAlpha.onValueChanged = { [weak self] v in self?.settings.watermarkTransparency = Int(v) }
        content.addSubview(wmAlpha)
        y += 44

        saveButton.frame = NSRect(x: 20, y: y, width: 110, height: 32)
        saveButton.onClick = { [weak self] in self?.saveOnly() }
        content.addSubview(saveButton)
        okButton.isPrimary = true
        okButton.frame = NSRect(x: 300, y: y, width: 240, height: 32)
        okButton.onClick = { [weak self] in self?.confirm() }
        content.addSubview(okButton)
        cancelButton.frame = NSRect(x: 200, y: y, width: 88, height: 32)
        cancelButton.onClick = { [weak self] in self?.close() }
        content.addSubview(cancelButton)

        // Live watermark: every watermark control reports through here.
        wmText.delegate = self
        wmFont.delegate = self
        wmMargin.delegate = self
        let wmToggle = wmEnabled.onToggle
        wmEnabled.onToggle = { [weak self] v in wmToggle?(v); self?.watermarkChanged() }
        let wmSizeChanged = wmSize.onValueChanged
        wmSize.onValueChanged = { [weak self] v in wmSizeChanged?(v); self?.watermarkChanged() }
        let wmAlphaChanged = wmAlpha.onValueChanged
        wmAlpha.onValueChanged = { [weak self] v in wmAlphaChanged?(v); self?.watermarkChanged() }
        let wmColorChanged = wmColor.onChange
        wmColor.onChange = { [weak self] i in wmColorChanged?(i); self?.watermarkChanged() }
        let wmPosChanged = wmPosition.onChange
        wmPosition.onChange = { [weak self] i in wmPosChanged?(i); self?.watermarkChanged() }
        y += 44

        panel.setContentSize(NSSize(width: 560, height: y))
        content.frame = NSRect(x: 0, y: 0, width: 560, height: y)
    }

    private func load() {
        let s = settings
        locGroup.buttons[[ExportLocation.desktop, .sameAsSource, .custom]
            .firstIndex(of: s.location) ?? 0].isSelected = true
        locGroup.select([ExportLocation.desktop, .sameAsSource, .custom]
            .firstIndex(of: s.location) ?? 0)
        customPathField.stringValue = s.customPath
        subFolderCheck.isChecked = s.useSubFolder
        subFolderField.stringValue = s.subFolder
        renameCombo.selectedIndex = [RenameMode.original, .dateTime, .sequence].firstIndex(of: s.rename) ?? 0
        conflictCombo.selectedIndex = [ConflictMode.appendNumber, .overwrite].firstIndex(of: s.conflict) ?? 0
        formatCombo.selectedIndex = [ExportFormat.jpeg, .png, .tiff, .bmp].firstIndex(of: s.format) ?? 0
        longEdgeField.stringValue = String(s.maxLongEdge)
        dpiField.stringValue = String(s.resolution)
        qualitySlider.setValueSilent(Double(s.jpegQuality))
        preserveExifCheck.isChecked = s.preserveExif
        openAfterCheck.isChecked = s.openFinderAfter
        wmEnabled.isChecked = s.watermarkEnabled
        wmText.stringValue = s.watermarkText
        wmFont.stringValue = s.watermarkFontName
        wmSize.setValueSilent(s.watermarkFontSize)
        wmAlpha.setValueSilent(Double(s.watermarkTransparency))
        wmColor.selectedIndex = colorNames.firstIndex(of: s.watermarkColor) ?? 0
        wmPosition.selectedIndex = positionNames.firstIndex(of: s.watermarkPosition) ?? 3
        wmMargin.stringValue = String(s.watermarkMargin)
        updateEnabled()
    }

    private func updateEnabled() {
        let custom = settings.location == .custom
        customPathField.isEnabled = custom
        browseButton.isEnabledButton = custom
        subFolderField.isEnabled = settings.useSubFolder
        qualitySlider.alphaValue = settings.format == .jpeg ? 1 : 0.45
        // BMP cannot carry EXIF at all.
        preserveExifCheck.alphaValue = settings.format == .bmp ? 0.45 : 1
        let wm = settings.watermarkEnabled
        for v in [wmText, wmFont, wmMargin] as [NSTextField] { v.isEnabled = wm }
        for v in [wmSize, wmAlpha] as [NSView] { v.alphaValue = wm ? 1 : 0.45 }
        for v in [wmColor, wmPosition] as [NSView] { v.alphaValue = wm ? 1 : 0.45 }
    }

    private func browse() {
        let p = NSOpenPanel()
        p.canChooseDirectories = true
        p.canChooseFiles = false
        p.beginSheetModal(for: panel) { [weak self] r in
            guard r == .OK, let url = p.url else { return }
            self?.customPathField.stringValue = url.path
        }
    }

    private func collect() {
        settings.customPath = customPathField.stringValue
        settings.subFolder = subFolderField.stringValue
        settings.maxLongEdge = max(0, Int(longEdgeField.stringValue) ?? settings.maxLongEdge)
        settings.resolution = max(0, Int(dpiField.stringValue) ?? settings.resolution)
        settings.watermarkText = wmText.stringValue
        settings.watermarkFontName = wmFont.stringValue.isEmpty ? "Helvetica" : wmFont.stringValue
        settings.watermarkMargin = max(0, Int(wmMargin.stringValue) ?? settings.watermarkMargin)
    }

    private func confirm() {
        collect()
        let s = settings
        close()
        onConfirm?(s)
    }

    private func saveOnly() {
        collect()
        let s = settings
        close()
        onSave?(s)
    }

    private func watermarkChanged() {
        collect()
        onWatermarkChanged?(settings)
    }

    func show(over window: NSWindow?) {
        host = window
        guard let window else { panel.makeKeyAndOrderFront(nil); return }
        window.beginSheet(panel)
    }

    func close() {
        if let host, panel.isSheet { host.endSheet(panel) }
        panel.orderOut(nil)
    }
}

extension ExportWindowController: NSTextFieldDelegate {
    func controlTextDidChange(_ obj: Notification) {
        guard let f = obj.object as? NSTextField, f === wmText || f === wmFont || f === wmMargin
        else { return }
        watermarkChanged()
    }
}
