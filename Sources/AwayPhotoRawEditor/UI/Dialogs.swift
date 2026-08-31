import AppKit
import AwayRawCore

/// A small helper for the simpler dialogs: a titled panel with a flipped, palette-coloured
/// content view and the usual sheet plumbing.
class DialogController: NSObject {
    let panel: NSPanel
    let content: FlippedView
    private weak var host: NSWindow?
    /// Alive from show() to close(). Every caller creates the controller as a local and
    /// lets it go after show(); the buttons' closures capture self weakly, so without
    /// this the sheet stayed up with every button dead (the same fault the progress
    /// window had, found by a user in the export dialog).
    private var retainedWhileShown: DialogController?
    /// Dialogs currently on screen (for --uitest).
    private(set) static var shownCount = 0

    init(title: String, width: CGFloat, height: CGFloat) {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: width, height: height),
                        styleMask: [.titled, .closable], backing: .buffered, defer: false)
        content = FlippedView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        super.init()
        panel.title = L.t(title)
        panel.appearance = Theme.appearance
        content.wantsLayer = true
        content.layer?.backgroundColor = Theme.panelBg.cgColor
        panel.contentView = content
    }

    func resizeContent(to height: CGFloat) {
        let w = panel.frame.width
        panel.setContentSize(NSSize(width: w, height: height))
        content.frame = NSRect(x: 0, y: 0, width: w, height: height)
    }

    func show(over window: NSWindow?) {
        host = window
        if retainedWhileShown == nil { Self.shownCount += 1 }
        retainedWhileShown = self
        guard let window else { panel.makeKeyAndOrderFront(nil); return }
        window.beginSheet(panel)
    }

    func close() {
        if let host, panel.isSheet { host.endSheet(panel) }
        panel.orderOut(nil)
        if retainedWhileShown != nil { Self.shownCount -= 1 }
        retainedWhileShown = nil
    }

    // ---- small builders --------------------------------------------------

    @discardableResult
    func addLabel(_ text: String, x: CGFloat, y: CGFloat, w: CGFloat,
                  font: NSFont? = nil, color: NSColor? = nil,
                  align: NSTextAlignment = .left, lines: Int = 1) -> NSTextField {
        let l = NSTextField(labelWithString: L.t(text))
        l.font = font ?? Theme.normal
        l.textColor = color ?? Theme.text
        l.alignment = align
        l.maximumNumberOfLines = lines
        l.lineBreakMode = lines > 1 ? .byWordWrapping : .byTruncatingTail
        if lines > 1 {
            l.usesSingleLineMode = false
            l.cell?.wraps = true
            l.cell?.isScrollable = false
            l.preferredMaxLayoutWidth = w
        }
        l.frame = NSRect(x: x, y: y, width: w, height: lines > 1 ? CGFloat(lines) * 18 : 20)
        content.addSubview(l)
        return l
    }

    @discardableResult
    func addButton(_ title: String, x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat = 30,
                   primary: Bool = false, action: @escaping () -> Void) -> FlatButton {
        let b = FlatButton(title: title, action: action)
        b.isPrimary = primary
        b.frame = NSRect(x: x, y: y, width: w, height: h)
        content.addSubview(b)
        return b
    }
}

// MARK: - 設定

/// Settings: RAW decoding, precision, interface style, language, thumbnail numbers.
final class SettingsWindowController: DialogController {
    private let useLibRaw = DarkCheckBox(title: "使用 LibRaw")
    private let highPrecision = DarkCheckBox(title: "高精度 RAW 處理流程 (16-bit / float)")
    private let useGpu = DarkCheckBox(title: "GPU 加速（Metal）")
    // Two single-line labels rather than one wrapping field: the text is Chinese, which
    // gives word-wrapping nothing to break on, and NSTextField's multi-line cell layout
    // is more trouble than a second label is worth.
    private let gpuStatus = NSTextField(labelWithString: "")
    private let gpuStatus2 = NSTextField(labelWithString: "")
    private let showNumbers = DarkCheckBox(title: "在縮圖左上顯示編號 (#1, #2 …)")
    private var fonts = AppSettings.current.fontSizes
    private let styleCombo = DarkComboBox(frame: .zero)
    private let langCombo = DarkComboBox(frame: .zero)
    private let libRawStatus = NSTextField(labelWithString: "")

    /// Called with true when a change needs a relaunch (language or interface style).
    var onSaved: ((Bool) -> Void)?

    init() {
        super.init(title: "設定", width: 460, height: 400)
        let s = AppSettings.current
        var y: CGFloat = 20

        addLabel("一般選項", x: 20, y: y, w: 420, font: Theme.dialogTitle); y += 30

        useLibRaw.isChecked = s.useLibRaw
        useLibRaw.frame = NSRect(x: 30, y: y, width: 400, height: 22); y += 26
        content.addSubview(useLibRaw)

        libRawStatus.stringValue = LibRawBridge.available
            ? "LibRaw \(LibRawBridge.version)"
            : L.t("未使用LibRaw讀取")
        libRawStatus.font = Theme.small
        libRawStatus.textColor = LibRawBridge.available ? Theme.textFaint : Theme.warn
        libRawStatus.frame = NSRect(x: 52, y: y, width: 390, height: 18); y += 26
        content.addSubview(libRawStatus)

        highPrecision.isChecked = s.useHighPrecisionRawPipeline
        highPrecision.frame = NSRect(x: 30, y: y, width: 340, height: 22)
        content.addSubview(highPrecision)
        addButton("說明", x: 380, y: y - 3, w: 60, h: 26) { [weak self] in self?.showPrecisionHelp() }
        y += 36

        // 字體大小…: per-size tuning; committed with the rest when 套用 is pressed.
        addButton("字體大小…", x: 30, y: y, w: 130, h: 28) { [weak self] in
            guard let self else { return }
            let d = FontSizeWindowController(initial: self.fonts)
            d.onPicked = { [weak self] f in self?.fonts = f }
            d.show(over: self.panel)
        }
        y += 40

        addLabel("算圖", x: 20, y: y, w: 420, font: Theme.dialogTitle); y += 30

        useGpu.isChecked = s.useGpu
        useGpu.frame = NSRect(x: 30, y: y, width: 400, height: 22); y += 26
        content.addSubview(useGpu)

        // Say plainly what the GPU will and will not be used for: above the size cap the
        // renderer falls back to the CPU, and a user watching an export should not be
        // wondering why the setting appears to do nothing.
        let metal = MetalPipeline.shared
        let mp = metal.maxPixels / 1_000_000
        for (i, label) in [gpuStatus, gpuStatus2].enumerated() {
            label.font = Theme.small
            label.textColor = metal.available ? Theme.textFaint : Theme.warn
            label.lineBreakMode = .byTruncatingTail
            label.frame = NSRect(x: 52, y: y + CGFloat(i) * 17, width: 390, height: 16)
            content.addSubview(label)
        }
        gpuStatus.stringValue = metal.available
            ? metal.statusText
            : L.t("此電腦無法使用 Metal，一律以 CPU 算圖")
        gpuStatus2.stringValue = metal.available
            ? L.f("{0} MP 以下的預覽使用，全解析度匯出走 CPU", mp) : ""
        y += 44

        addLabel("介面風格", x: 20, y: y, w: 420, font: Theme.dialogTitle); y += 30

        addLabel("介面風格", x: 30, y: y + 4, w: 80, color: Theme.textDim)
        styleCombo.setItems([L.t("經典深色"), L.t("暖白相紙")])
        styleCombo.selectedIndex = (s.interfaceStyle == .warmPaper) ? 1 : 0
        styleCombo.frame = NSRect(x: 120, y: y, width: 200, height: 26)
        content.addSubview(styleCombo)
        y += 34

        addLabel("語言", x: 30, y: y + 4, w: 80, color: Theme.textDim)
        langCombo.setItems(AppLanguage.allCases.map { L.languageDisplayName($0) })
        langCombo.selectedIndex = AppLanguage.allCases.firstIndex(of: s.uiLanguage) ?? 0
        langCombo.frame = NSRect(x: 120, y: y, width: 260, height: 26)
        content.addSubview(langCombo)
        y += 34

        showNumbers.isChecked = s.showThumbnailNumber
        showNumbers.frame = NSRect(x: 30, y: y, width: 400, height: 22)
        content.addSubview(showNumbers)
        y += 40

        addButton("恢復預設", x: 20, y: y, w: 130) { [weak self] in self?.restoreDefaults() }
        addButton("套用", x: 340, y: y, w: 100, primary: true) { [weak self] in self?.save() }
        addButton("取消", x: 230, y: y, w: 100) { [weak self] in self?.close() }
        y += 44
        resizeContent(to: y)
    }

    /// Every option back to its default; written only when 套用 is pressed.
    private func restoreDefaults() {
        let d = AppSettings()
        useLibRaw.isChecked = d.useLibRaw
        highPrecision.isChecked = d.useHighPrecisionRawPipeline
        useGpu.isChecked = d.useGpu
        showNumbers.isChecked = d.showThumbnailNumber
        styleCombo.selectedIndex = 0
        langCombo.selectedIndex = AppLanguage.allCases.firstIndex(of: d.uiLanguage) ?? 0
        fonts = FontSizes()
    }

    private func showPrecisionHelp() {
        let a = NSAlert()
        a.messageText = L.t("RAW 處理精度")
        a.informativeText = L.rawPrecisionHelp
        a.addButton(withTitle: L.t("確定"))
        a.beginSheetModal(for: panel, completionHandler: nil)
    }

    private func save() {
        let s = AppSettings.current
        let oldStyle = s.interfaceStyle
        let oldLang = s.uiLanguage

        s.useLibRaw = useLibRaw.isChecked
        s.useHighPrecisionRawPipeline = highPrecision.isChecked
        s.useGpu = useGpu.isChecked
        if !s.useGpu { MetalPipeline.shared.flushPool() }
        let oldFonts = s.fontSizes
        s.fontSizes = fonts
        s.showThumbnailNumber = showNumbers.isChecked
        s.interfaceStyle = styleCombo.selectedIndex == 1 ? .warmPaper : .classicDark
        s.uiLanguage = AppLanguage.allCases[max(0, langCombo.selectedIndex)]
        s.save()

        // The palette and every translated caption are baked in at build time, so a
        // change to either goes through a relaunch — the same as on Windows.
        let needsRestart = (oldStyle != s.interfaceStyle) || (oldLang != s.uiLanguage)
                        || (oldFonts != s.fontSizes)
        close()
        onSaved?(needsRestart)
    }
}

// MARK: - 字體大小

/// Per-item font size tuning, with a restore-defaults button at the bottom.
final class FontSizeWindowController: DialogController {
    private var fields: [(path: WritableKeyPath<FontSizes, Int>, stepper: NSStepper, label: NSTextField)] = []
    private var sizes: FontSizes
    /// Standalone use (the app menu): sizes are written and a relaunch offered.
    var onSaved: (() -> Void)?
    /// Embedded use (from Settings): the chosen sizes are handed back, nothing is written.
    var onPicked: ((FontSizes) -> Void)?

    init(initial: FontSizes = AppSettings.current.fontSizes) {
        sizes = initial
        super.init(title: "字體大小…", width: 420, height: 100)
        var y: CGFloat = 16
        addLabel("字體", x: 20, y: y, w: 380, font: Theme.dialogTitle); y += 30

        for (key, path) in FontSizes.fields {
            addLabel(key, x: 24, y: y + 3, w: 190, color: Theme.textDim)
            let value = NSTextField(labelWithString: String(sizes[keyPath: path]))
            value.font = Theme.normal
            value.textColor = Theme.text
            value.alignment = .right
            value.frame = NSRect(x: 300, y: y + 3, width: 40, height: 20)
            content.addSubview(value)

            let st = NSStepper(frame: NSRect(x: 348, y: y, width: 20, height: 26))
            st.minValue = Double(FontSizes.minPt)
            st.maxValue = Double(FontSizes.maxPt)
            st.increment = 1
            st.integerValue = sizes[keyPath: path]
            st.target = self
            st.action = #selector(stepped(_:))
            st.tag = fields.count
            content.addSubview(st)

            fields.append((path, st, value))
            y += 30
        }
        y += 10
        addButton("恢復預設", x: 24, y: y, w: 110) { [weak self] in self?.restoreDefaults() }
        addButton("確定", x: 300, y: y, w: 96, primary: true) { [weak self] in self?.save() }
        y += 44
        resizeContent(to: y)
    }

    @objc private func stepped(_ sender: NSStepper) {
        let f = fields[sender.tag]
        sizes[keyPath: f.path] = sender.integerValue
        f.label.stringValue = String(sender.integerValue)
    }

    private func restoreDefaults() {
        sizes = FontSizes()
        for f in fields {
            f.stepper.integerValue = sizes[keyPath: f.path]
            f.label.stringValue = String(sizes[keyPath: f.path])
        }
    }

    private func save() {
        sizes.clamp()
        close()
        if let onPicked { onPicked(sizes); return }
        AppSettings.current.fontSizes = sizes
        AppSettings.current.save()
        onSaved?()
    }
}

// MARK: - 關於

final class AboutWindowController: DialogController {
    private let updateStatus = NSTextField(labelWithString: "")
    private var checkButton: FlatButton!

    init() {
        super.init(title: "關於", width: 600, height: 100)
        var y: CGFloat = 24
        addLabel("AwayPhotoRawEditor", x: 20, y: y, w: 560,
                 font: Theme.aboutTitle, align: .center); y += 34
        y += 6
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "—"
        // One line per fact, in the Windows About dialog's order.
        let rows: [(String, String, String?)] = [
            (L.t("版本："), AppVersion.version, nil),
            (L.t("編譯時間："), build, nil),
            (L.t("作者:"), " Chih-Wei Su (Awaysu)  awaysu@gmail.com", nil),
            (L.t("下載:"), " " + UpdateCheck.pageUrl, UpdateCheck.pageUrl),
            ("Source Code:", " https://github.com/awaysu/AwayPhotoRawEditor", "https://github.com/awaysu/AwayPhotoRawEditor"),
            (L.t("第三方元件:"), " LibRaw \(LibRawBridge.available ? LibRawBridge.version : "—") (LGPL 2.1) · Apple ImageIO / Core Graphics / Metal", nil),
            (L.t("授權："), "BSD 3-Clause　© 2026 Chih-Wei Su (Awaysu)", nil),
        ]
        for (cap, val, link) in rows {
            let l = addLabel(cap + val, x: 30, y: y, w: 540, font: Theme.aboutBody, color: Theme.text)
            l.stringValue = cap + val
            l.lineBreakMode = .byTruncatingMiddle
            if let link {
                l.textColor = Theme.accentHover
                l.toolTip = link
                let click = NSClickGestureRecognizer(target: self, action: #selector(openLink(_:)))
                l.addGestureRecognizer(click)
                l.identifier = NSUserInterfaceItemIdentifier(link)
            }
            y += 22
        }
        y += 6
        let note = addLabel("歡迎自由修改成你自己的版本，只希望你能在你的「關於」視窗中提及來源是這裡（AwayPhotoRawEditor / Awaysu）。",
                            x: 30, y: y, w: 540, font: Theme.small, color: Theme.textDim, lines: 3)
        note.usesSingleLineMode = false
        note.cell?.wraps = true
        note.cell?.isScrollable = false
        note.preferredMaxLayoutWidth = 540
        y += 3 * 16 + 10

        updateStatus.font = Theme.small
        updateStatus.textColor = Theme.textDim
        updateStatus.alignment = .center
        updateStatus.lineBreakMode = .byWordWrapping
        updateStatus.maximumNumberOfLines = 3
        updateStatus.usesSingleLineMode = false
        updateStatus.cell?.wraps = true
        updateStatus.cell?.isScrollable = false
        updateStatus.preferredMaxLayoutWidth = 540
        updateStatus.frame = NSRect(x: 30, y: y, width: 540, height: 4)
        content.addSubview(updateStatus)
        y += 6

        checkButton = addButton("檢查更新", x: 40, y: y, w: 140) { [weak self] in
            self?.checkForUpdate()
        }
        addButton("確定", x: 440, y: y, w: 120, primary: true) { [weak self] in self?.close() }
        y += 44
        resizeContent(to: y)
    }

    @objc private func openLink(_ g: NSClickGestureRecognizer) {
        guard let id = g.view?.identifier?.rawValue, let url = URL(string: id) else { return }
        NSWorkspace.shared.open(url)
    }

    /// The Windows flow: a message for each outcome, and when there is a newer version an
    /// offer to open the download page.
    private func checkForUpdate() {
        checkButton.isEnabledButton = false
        checkButton.title = "檢查中…"
        updateStatus.stringValue = ""
        Task { @MainActor in
            let info = await UpdateCheck.fetch()
            self.checkButton.isEnabledButton = true
            self.checkButton.title = "檢查更新"
            let a = NSAlert()
            a.messageText = L.t("檢查更新")
            guard let info else {
                a.alertStyle = .warning
                a.informativeText = L.t("無法連線到更新伺服器，請稍後再試。")
                a.beginSheetModal(for: self.panel) { _ in }
                return
            }
            guard info.updateAvailable else {
                a.informativeText = L.f("目前已是最新版本（{0}）。", AppVersion.version)
                a.beginSheetModal(for: self.panel) { _ in }
                return
            }
            var msg = L.f("有新版本可以下載。\n\n目前版本：{0}\n最新版本：v{1}",
                          AppVersion.version, info.latestVersion)
            if !info.notes.isEmpty { msg += "\n\n" + info.notes }
            msg += "\n\n" + L.t("要開啟下載頁面嗎？")
            a.informativeText = msg
            a.addButton(withTitle: L.t("確定"))
            a.addButton(withTitle: L.t("取消"))
            a.beginSheetModal(for: self.panel) { r in
                guard r == .alertFirstButtonReturn, let url = URL(string: info.pageUrl) else { return }
                NSWorkspace.shared.open(url)
            }
        }
    }
}

/// The app's version string, shown in the About window. The number itself lives in
/// AwayRawCore (AppVersionInfo) because the update check sends it to the API.
enum AppVersion {
    static let version = "v" + AppVersionInfo.version
    static var display: String {
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String
        return build.map { "\(version) (\($0))" } ?? version
    }
}

// MARK: - 第一次執行的語言選擇

/// Shown once, when there is no settings.xml yet. Pre-selects the system's language and
/// falls back to English for anything unrecognised.
final class FirstRunLanguageController: DialogController {
    private let group = RadioGroup()
    private var chosen: AppLanguage = .traditionalChinese
    var onChosen: ((AppLanguage) -> Void)?

    init() {
        super.init(title: "語言 / Language", width: 380, height: 100)
        var y: CGFloat = 20
        let head = addLabel("語言", x: 20, y: y, w: 340, font: Theme.dialogTitle, align: .center)
        head.stringValue = "請選擇語言 / Choose your language"
        y += 34

        let guess = AppLanguage.guessFromSystem()
        chosen = guess
        for (i, lang) in AppLanguage.allCases.enumerated() {
            let r = DarkRadioButton(title: L.languageDisplayName(lang))
            r.frame = NSRect(x: 40, y: y, width: 300, height: 24)
            r.isSelected = (lang == guess)
            content.addSubview(r)
            group.add(r)
            y += 28
            _ = i
        }
        group.onChange = { [weak self] i in self?.chosen = AppLanguage.allCases[i] }
        y += 12
        let ok = addButton("確定", x: 130, y: y, w: 120, primary: true) { [weak self] in
            guard let self else { return }
            self.close()
            self.onChosen?(self.chosen)
        }
        ok.title = "確定 / OK"
        y += 44
        resizeContent(to: y)
    }
}
