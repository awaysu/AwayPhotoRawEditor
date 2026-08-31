import AppKit
import AwayRawCore

/// The ☰ application menu, the thumbnail context menu, and export.
extension MainViewController {

    // ---- app menu --------------------------------------------------------

    func showAppMenu() {
        let menu = NSMenu()
        func add(_ title: String, _ action: Selector, enabled: Bool = true) {
            let item = NSMenuItem(title: L.t(title), action: action, keyEquivalent: "")
            item.target = self
            item.isEnabled = enabled
            menu.addItem(item)
        }
        add("開啟資料夾…", #selector(menuOpenFolder))
        add("重新整理資料夾  (F5)", #selector(menuRefreshFolder), enabled: !folder.isEmpty)
        add("關閉資料夾", #selector(menuCloseFolder), enabled: !folder.isEmpty)
        add("關閉資料夾並刪除快取縮圖", #selector(menuCloseAndClearCache), enabled: !folder.isEmpty)

        // 紀錄: every folder opened before, most recent first; pick one to open it.
        let recentItem = NSMenuItem(title: L.t("紀錄"), action: nil, keyEquivalent: "")
        let recentMenu = NSMenu()
        let recents = settings.recentFolders
        if recents.isEmpty {
            let none = NSMenuItem(title: L.t("（尚無開啟紀錄）"), action: nil, keyEquivalent: "")
            none.isEnabled = false
            recentMenu.addItem(none)
        } else {
            for path in recents {
                let it = NSMenuItem(title: path, action: #selector(menuOpenRecent(_:)), keyEquivalent: "")
                it.target = self
                it.representedObject = path
                it.toolTip = path
                recentMenu.addItem(it)
            }
            recentMenu.addItem(.separator())
            let clear = NSMenuItem(title: L.t("清除紀錄"), action: #selector(menuClearRecent), keyEquivalent: "")
            clear.target = self
            recentMenu.addItem(clear)
        }
        recentItem.submenu = recentMenu
        menu.addItem(recentItem)

        // 2026-08-31：☰ 不再放「匯出目前照片／匯出照片／匯出全部照片」——右下的匯出按鈕
        // 與縮圖右鍵選單已涵蓋（使用者要求，與 Windows 版選單刻意不同）。
        menu.addItem(.separator())
        add("設定…", #selector(menuSettings))
        add("編輯風格檔…", #selector(menuPresetEditor))
        // 2026-08-31：「還原已隱藏的照片」也移除（使用者要求）——縮圖右鍵的
        // 顯示全部／取消隱藏已涵蓋。
        menu.addItem(.separator())
        add("支援RAW檔相機列表", #selector(menuCameraList))
        add("關於", #selector(menuAbout))

        let p = NSPoint(x: menuButton.bounds.minX, y: menuButton.bounds.maxY)
        menu.popUp(positioning: nil, at: p, in: menuButton)
    }

    @objc func menuOpenFolder() { pickFolder() }
    @objc func menuRefreshFolder() { refreshFolder() }
    @objc func menuCloseFolder() { closeFolder() }
    @objc func menuCloseAndClearCache() { closeFolderAndClearCache() }
    @objc func menuExportSelection() { exportPhotos(strip.selectedItems) }
    @objc func menuPresetEditor() { showPresetEditor() }
    @objc func menuSettings() { showSettings() }
    @objc func menuFontSize() { showFontSizeDialog() }
    @objc func menuAbout() { showAbout() }


    @objc func menuOpenRecent(_ sender: NSMenuItem) {
        guard let path = sender.representedObject as? String else { return }
        var isDir: ObjCBool = false
        if FileManager.default.fileExists(atPath: path, isDirectory: &isDir), isDir.boolValue {
            openFolder(path)
        } else {
            let a = NSAlert()
            a.alertStyle = .warning
            a.messageText = L.t("紀錄")
            a.informativeText = L.f("資料夾已不存在：\n{0}", path)
            a.runModal()
        }
    }

    @objc func menuClearRecent() {
        settings.recentFolders.removeAll()
        settings.save()
    }

    @objc func menuCameraList() {
        if let url = URL(string: "https://www.libraw.org/supported-cameras") {
            NSWorkspace.shared.open(url)
        }
    }

    @objc func menuShowHiddenMode(_ sender: NSMenuItem) { setShowHiddenMode(sender.tag == 1) }
    @objc func menuHideSelected() { hideSelected() }
    @objc func menuUnhideSelected() { unhideSelected() }
    @objc func menuSelectAll() { strip.selectAll() }
    @objc func menuInvertSelection() { strip.invertSelection() }
    @objc func menuDeselectAll() { strip.deselectAll() }
    @objc func menuApplyPreset(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        applyPresetToSelection(name)
    }

    // ---- thumbnail context menu -----------------------------------------

    /// The thumbnail context menu, in the Windows build's order.
    func showThumbnailMenu(index: Int, event: NSEvent) {
        guard index >= 0, index < items.count else { return }
        let item = items[index]
        let menu = NSMenu()
        func add(_ title: String, _ action: Selector, enabled: Bool = true) {
            let m = NSMenuItem(title: L.t(title), action: action, keyEquivalent: "")
            m.target = self
            m.representedObject = item
            m.isEnabled = enabled
            menu.addItem(m)
        }
        let selected = strip.selectedItems

        add("全選", #selector(menuSelectAll))
        add("反向選擇", #selector(menuInvertSelection))
        add("取消全選", #selector(menuDeselectAll))
        menu.addItem(.separator())

        // 套用風格檔 ▸ every preset; built-in names are translated, custom ones shown as is.
        let presetItem = NSMenuItem(title: L.t("套用風格檔"), action: nil, keyEquivalent: "")
        let presetMenu = NSMenu()
        for name in PresetStore.allNames() {
            let it = NSMenuItem(title: PresetProfile.builtIn[name] != nil ? L.t(name) : name,
                                action: #selector(menuApplyPreset(_:)), keyEquivalent: "")
            it.target = self
            it.representedObject = name
            presetMenu.addItem(it)
        }
        presetItem.submenu = presetMenu
        menu.addItem(presetItem)

        // Copying only makes sense for one photo; pasting goes to the whole selection.
        add("複製照片設定", #selector(menuCopySettings(_:)), enabled: selected.count <= 1)
        add("貼上照片設定", #selector(menuPasteSettings(_:)), enabled: copiedSettings != nil)
        add("升級處理版本", #selector(menuUpgradePipeline(_:)))
        menu.addItem(.separator())
        add("建立副本", #selector(menuCreateVirtualCopy(_:)))
        add("隱藏且不輸出", #selector(menuHideSelected), enabled: selected.contains { !$0.isHidden })
        add("取消隱藏", #selector(menuUnhideSelected), enabled: selected.contains { $0.isHidden })
        add("刪除檔案", #selector(menuDeleteFile(_:)))
        menu.addItem(.separator())

        // Display mode: a two-way check on whichever is in effect.
        let showAll = settings.showHiddenPhotos
        let hideMode = NSMenuItem(title: L.t("不顯示隱藏"), action: #selector(menuShowHiddenMode(_:)), keyEquivalent: "")
        hideMode.target = self; hideMode.tag = 0; hideMode.state = showAll ? .off : .on
        let showMode = NSMenuItem(title: L.t("顯示全部"), action: #selector(menuShowHiddenMode(_:)), keyEquivalent: "")
        showMode.target = self; showMode.tag = 1; showMode.state = showAll ? .on : .off
        menu.addItem(hideMode)
        menu.addItem(showMode)
        menu.addItem(.separator())
        add("匯出照片", #selector(menuExportSelection))

        NSMenu.popUpContextMenu(menu, with: event, for: strip)
    }

    @objc func menuCopySettings(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        // Take the live values when it is the photo being edited, so an unsaved change
        // is what gets copied.
        copiedSettings = (item.key == current?.key)
            ? adj
            : AdjustmentXmlStore.load(imagePath: item.sourcePath, copyIndex: item.virtualCopyIndex)
        copySourceKey = item.key
        for it in items { it.isCopySettingsSource = (it.key == copySourceKey) }
        strip.refreshBadges()
        statusLabel.stringValue = L.t("已複製相片設定")
    }

    @objc func menuPasteSettings(_ sender: NSMenuItem) {
        guard let source = copiedSettings else {
            statusLabel.stringValue = L.t("尚未複製任何設定")
            return
        }
        let targets = strip.selectedItems
        guard !targets.isEmpty else { return }
        for t in targets {
            var a = source
            // Heal spots are position-specific; they never travel between photos.
            a.healSpots = []
            if t.key == current?.key {
                pushUndo()
                let keepVersion = adj.pipelineVersion
                adj = a
                adj.pipelineVersion = keepVersion
                rebindAll()
                onAdjustmentChanged(immediate: true)
            } else {
                let existing = AdjustmentXmlStore.load(imagePath: t.sourcePath,
                                                       copyIndex: t.virtualCopyIndex)
                a.pipelineVersion = existing?.pipelineVersion ?? ImageAdjustments.currentPipelineVersion
                AdjustmentXmlStore.save(imagePath: t.sourcePath, adjustments: a,
                                        copyIndex: t.virtualCopyIndex)
                t.isEdited = !a.isDefault
            }
        }
        refreshThumbnails(targets.filter { $0.key != current?.key })
        strip.refreshBadges()
        statusLabel.stringValue = L.t("已貼上相片設定")
    }

    @objc func menuCreateVirtualCopy(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        saveCurrentIfDirty()
        // Copy indices are per source file; take the next free one.
        let used = previewList.virtualCopies.filter { $0.path == item.sourcePath }.map(\.index)
        let next = (used.max() ?? 0) + 1
        previewList.virtualCopies.append(VirtualCopyEntry(path: item.sourcePath, index: next))
        savePreviewList()

        // Seed the copy with the source's current settings.
        let source = AdjustmentXmlStore.load(imagePath: item.sourcePath,
                                             copyIndex: item.virtualCopyIndex)
                     ?? ImageAdjustments()
        let e = AdjustmentXmlStore.loadExif(imagePath: item.sourcePath,
                                            copyIndex: item.virtualCopyIndex)
        AdjustmentXmlStore.save(imagePath: item.sourcePath, adjustments: source,
                                copyIndex: next, exif: e)

        refreshStripKeepSelection()
        if let i = strip.index(forKey: "\(item.sourcePath)|copy:\(next)") {
            strip.select(index: i)
        }
        statusLabel.stringValue = L.t("已建立虛擬副本")
    }

    @objc func menuDeleteFile(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        deletePhotoFile(item)
    }

    /// Delete one photo. A virtual copy has no file of its own, so it simply leaves the
    /// list (with its sidecar) without asking; a real file goes to the Trash after
    /// confirmation, and its caches are hard-deleted because they regenerate.
    func deletePhotoFile(_ item: PhotoItem) {
        // Whatever is being edited must be written before the list is rebuilt — even
        // when it is not the photo being deleted.
        saveCurrentIfDirty()

        if item.isVirtualCopy {
            previewList.virtualCopies.removeAll {
                $0.path == item.sourcePath && $0.index == item.virtualCopyIndex
            }
            previewList.hidden.removeAll { $0 == item.key }
            savePreviewList()
            AdjustmentXmlStore.delete(imagePath: item.sourcePath, copyIndex: item.virtualCopyIndex)
            if item.key == current?.key { loadedKey = nil; current = nil }
            refreshStripKeepSelection()
            return
        }

        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = L.t("刪除照片檔案")
        alert.informativeText = L.f("確定刪除檔案？（會移到資源回收桶）\n{0}", item.fileName)
        alert.addButton(withTitle: L.t("刪除檔案"))
        alert.addButton(withTitle: L.t("取消"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        do {
            try FileManager.default.trashItem(at: URL(fileURLWithPath: item.sourcePath),
                                             resultingItemURL: nil)
        } catch {
            let a = NSAlert()
            a.alertStyle = .critical
            a.messageText = "AwayPhotoRawEditor"
            a.informativeText = L.t("刪除失敗：") + error.localizedDescription
            a.runModal()
            return
        }
        CacheManager.deleteCacheFiles(item.sourcePath)
        AdjustmentXmlStore.delete(imagePath: item.sourcePath)
        // Its virtual copies have nothing to be copies of any more.
        previewList.virtualCopies.removeAll { $0.path == item.sourcePath }
        previewList.hidden.removeAll { PhotoItem.parseKey($0).path == item.sourcePath }
        savePreviewList()
        if current?.sourcePath == item.sourcePath { loadedKey = nil; current = nil }
        refreshStripKeepSelection()
        statusLabel.stringValue = L.t("已刪除檔案")
    }

    @objc func menuUpgradePipeline(_ sender: NSMenuItem) { upgradeSelectedPipeline() }

    // ---- export ----------------------------------------------------------

    func exportCurrent() {
        guard let item = current else { return }
        exportPhotos([item])
    }

    func exportAll() { exportPhotos(items) }

    func exportPhotos(_ all: [PhotoItem]) {
        // 隱藏且不輸出: hidden photos never export, whichever entry point was used.
        let targets = all.filter { !$0.isHidden }
        guard !targets.isEmpty else {
            let a = NSAlert()
            a.messageText = L.t("匯出")
            a.informativeText = L.t("沒有可匯出的相片。")
            a.runModal()
            return
        }
        saveCurrentIfDirty()

        let dialog = ExportWindowController(settings: exportSettings, count: targets.count)
        // The watermark is edited live: toggling it in the dialog updates the preview.
        dialog.onWatermarkChanged = { [weak self] s in
            guard let self else { return }
            self.exportSettings = s
            if self.current != nil { self.scheduler.schedule(immediate: true) }
        }
        dialog.onSave = { [weak self] s in
            self?.exportSettings = s
            s.save()
            if self?.current != nil { self?.scheduler.schedule(immediate: true) }
        }
        dialog.onConfirm = { [weak self] settings in
            guard let self else { return }
            self.exportSettings = settings
            settings.save()
            self.runExport(targets, settings)
        }
        dialog.show(over: view.window)
    }

    private func runExport(_ targets: [PhotoItem], _ settings: ExportSettings) {
        let progress = ProgressWindowController(title: L.t("匯出照片"))
        progress.show(over: view.window)
        let token = CancelToken()
        progress.onCancel = { token.cancel() }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            do {
                let written = try Exporter.export(items: targets, settings: settings,
                                                  loader: self.loader,
                                                  progress: { p in
                    DispatchQueue.main.async {
                        progress.update(fraction: Double(p.done) / Double(max(1, p.total)),
                                        message: p.message)
                    }
                }, token: token)
                DispatchQueue.main.async {
                    progress.finish(message: L.f("已匯出 {0} 張相片", written.count))
                    self.statusLabel.stringValue = L.f("已匯出 {0} 張相片", written.count)
                }
            } catch is CancellationError {
                DispatchQueue.main.async {
                    progress.close()
                    self.statusLabel.stringValue = L.t("匯出已取消")
                }
            } catch {
                DispatchQueue.main.async {
                    progress.close()
                    let alert = NSAlert()
                    alert.alertStyle = .warning
                    alert.messageText = L.t("匯出失敗")
                    alert.informativeText = error.localizedDescription
                    alert.runModal()
                }
            }
        }
    }
}
