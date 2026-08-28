import AppKit
import AwayRawCore

/// 編輯風格檔 — the list of presets (built-in plus custom) on the left, the same
/// 基本調整 / 色彩 / 細節 panels used in the main window on the right.
///
/// Edits save back to PresetStore automatically when the selection changes or the window
/// closes. A built-in edited back to exactly its default has its override *removed*
/// rather than stored, so "unchanged" and "explicitly set to the default" stay the same
/// thing. 預設時設定 is a full reset and never appears in this list.
final class PresetEditorWindowController: DialogController {

    private let listView = DarkListView()
    private let nameField = NSTextField(frame: .zero)
    private let basic = BasicAdjustPanel()
    private let color = ColorPanel()
    private let detail = DetailPanel()

    private var names: [String] = []
    private var currentName: String?
    private var working = ImageAdjustments()
    private var baseline = ImageAdjustments()

    var onClosed: (() -> Void)?

    init() {
        super.init(title: "編輯風格檔", width: 660, height: 760)

        addLabel("編輯風格檔", x: 16, y: 14, w: 300, font: Theme.dialogTitle)

        // ---- left: the preset list
        listView.frame = NSRect(x: 16, y: 50, width: 240, height: 468)
        listView.onSelect = { [weak self] i in
            guard let self, i >= 0, i < self.names.count, self.names[i] != self.currentName
            else { return }
            self.select(name: self.names[i])
        }
        content.addSubview(listView)

        addLabel("新增自訂風格檔", x: 16, y: 528, w: 240, color: Theme.textDim)
        nameField.font = Theme.normal
        nameField.backgroundColor = Theme.panelBg3
        nameField.textColor = Theme.text
        nameField.isBordered = true
        nameField.bezelStyle = .squareBezel
        nameField.focusRingType = .none
        nameField.frame = NSRect(x: 16, y: 552, width: 152, height: 28)
        content.addSubview(nameField)
        addButton("新增", x: 174, y: 551, w: 82, h: 28) { [weak self] in self?.addCustom() }

        let hint = addLabel("「新增」以目前顯示的設定建立\n修改會自動儲存",
                            x: 16, y: 588, w: 240, font: Theme.small,
                            color: Theme.textFaint, lines: 2)
        _ = hint

        // ---- right: the three adjust panels
        basic.frame = NSRect(x: 272, y: 50, width: 370, height: 245)
        color.frame = NSRect(x: 272, y: 305, width: 370, height: 210)
        detail.frame = NSRect(x: 272, y: 525, width: 370, height: 145)
        // No target photo here, so the eyedropper and 拍攝時設定 do not apply. The
        // temperature/tint sliders still show — the user asked for the layout to stay put —
        // with a note in their place saying a preset leaves white balance alone.
        color.showPresetWhiteBalanceNote()
        for p in [basic, color, detail] as [AdjustPanelBase] {
            p.onChanged = { [weak self] in
                guard let self else { return }
                self.working = p.adjustments ?? self.working
            }
            content.addSubview(p)
        }

        // ---- bottom row
        addButton("備份全部", x: 16, y: 690, w: 116, h: 32) { [weak self] in self?.backup() }
        addButton("還原備份", x: 140, y: 690, w: 116, h: 32) { [weak self] in self?.restore() }
        addButton("恢復預設", x: 272, y: 690, w: 116, h: 32) { [weak self] in self?.resetAll() }
        addButton("關閉", x: 546, y: 690, w: 96, h: 32, primary: true) { [weak self] in
            self?.saveCurrent()
            self?.close()
            self?.onClosed?()
        }

        reloadList()
        if let first = names.first { select(name: first) }
    }

    // ---- list ------------------------------------------------------------

    private func reloadList() {
        // 預設時設定 is a full reset, not a set of values: it is not editable and its
        // name may not be reused for a custom preset.
        names = PresetStore.allNames().filter { $0 != PresetProfile.defaultName }
        listView.rows = names.map { name in
            // Built-ins carry a marker when the user has overridden them.
            let overridden = PresetProfile.builtIn[name] != nil && PresetStore.hasOverride(name: name)
            return DarkListView.Row(title: L.t(name), marked: overridden)
        }
    }

    private func select(name: String) {
        saveCurrent()
        currentName = name
        working = PresetStore.resolvedValues(name: name) ?? ImageAdjustments()
        baseline = working
        basic.bind(working)
        color.bind(working)
        detail.bind(working)
        basic.adjustments = working
        color.adjustments = working
        detail.adjustments = working
        listView.selectedIndex = names.firstIndex(of: name) ?? -1
    }

    /// Persist the preset being edited, unless nothing changed.
    private func saveCurrent() {
        guard let name = currentName else { return }
        guard !working.valueEquals(baseline) else { return }
        if let builtIn = PresetProfile.builtInValues(name), working.valueEquals(builtIn) {
            // Edited back to exactly the built-in default → drop the override.
            PresetStore.remove(name: name)
        } else {
            PresetStore.save(name: name, source: working)
        }
        baseline = working
    }

    private func addCustom() {
        let name = nameField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        guard name != PresetProfile.defaultName else {
            warn(L.f("「{0}」是保留名稱，請換一個。", name))
            return
        }
        guard !names.contains(name) else {
            warn(L.f("已經有名為「{0}」的風格檔。", name))
            return
        }
        saveCurrent()
        // "新增" creates the preset from whatever is currently displayed.
        PresetStore.save(name: name, source: working)
        nameField.stringValue = ""
        reloadList()
        select(name: name)
    }

    private func warn(_ text: String) {
        let a = NSAlert()
        a.messageText = L.t("無法新增")
        a.informativeText = text
        a.addButton(withTitle: L.t("確定"))
        a.beginSheetModal(for: panel, completionHandler: nil)
    }

    // ---- backup / restore ------------------------------------------------

    private func backup() {
        saveCurrent()
        let p = NSSavePanel()
        p.nameFieldStringValue = "presets.xml"
        p.beginSheetModal(for: panel) { r in
            guard r == .OK, let url = p.url else { return }
            try? PresetStore.exportTo(path: url.path)
        }
    }

    private func restore() {
        let p = NSOpenPanel()
        p.allowedContentTypes = [.xml]
        p.beginSheetModal(for: panel) { [weak self] r in
            guard r == .OK, let url = p.url, let self else { return }
            do {
                guard try PresetStore.importFrom(path: url.path) else {
                    self.warn(L.t("這個檔案不是風格檔備份。"))
                    return
                }
                self.currentName = nil
                self.reloadList()
                if let first = self.names.first { self.select(name: first) }
            } catch {
                self.warn(error.localizedDescription)
            }
        }
    }

    private func resetAll() {
        let a = NSAlert()
        a.alertStyle = .warning
        a.messageText = L.t("恢復預設")
        a.informativeText = L.t("將刪除所有自訂風格檔，並把內建風格檔恢復為預設值。")
        a.addButton(withTitle: L.t("確定"))
        a.addButton(withTitle: L.t("取消"))
        a.beginSheetModal(for: panel) { [weak self] r in
            guard r == .alertFirstButtonReturn, let self else { return }
            PresetStore.resetAllToDefaults()
            self.currentName = nil
            self.reloadList()
            if let first = self.names.first { self.select(name: first) }
        }
    }
}
